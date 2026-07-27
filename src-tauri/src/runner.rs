//! Running the StackVo CLI and `docker compose`, streaming their output.
//!
//! Two things this fixes relative to the web UI:
//!
//!   1. **No shell.** `ProjectService` built command strings and handed them to
//!      `execAsync`, which spawns `/bin/sh -c`. That is why the codebase needed
//!      an `assertSafeName` regex guarding every interpolated value. Here every
//!      argument is a separate array element, so a project named `a; rm -rf ~`
//!      is a name that does not exist, not a command that runs.
//!
//!   2. **No buffering.** `execAsync` resolves once the process exits, so a
//!      ten-minute build produced nothing until it finished — hence the 600s
//!      proxy timeout in the old nginx config. Here stdout and stderr are read
//!      line by line and emitted as they arrive.

use crate::error::{Code, Error, Result};
use crate::events::{self, FinishedEvent, ProgressEvent};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// A finished process run.
pub struct Output {
    pub success: bool,
    pub code: Option<i32>,
    pub lines: Vec<String>,
}

/// Spawn a command and stream every output line to `on_line`.
///
/// stdout and stderr are merged in arrival order: `docker compose` writes its
/// progress to stderr and its results to stdout, and splitting them makes the
/// log read out of sequence.
pub async fn stream<F>(program: &str, args: &[String], cwd: &Path, mut on_line: F) -> Result<Output>
where
    F: FnMut(&str) + Send,
{
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            Error::new(
                Code::GenerateFailed,
                format!("could not run {program}: {e}"),
            )
            .with_hint(if e.kind() == std::io::ErrorKind::NotFound {
                format!("`{program}` is not on PATH.")
            } else {
                String::new()
            })
        })?;

    let stdout = child.stdout.take().expect("stdout piped above");
    let stderr = child.stderr.take().expect("stderr piped above");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);
    let tx_err = tx.clone();

    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_err.send(line).await.is_err() {
                break;
            }
        }
    });

    let mut collected = Vec::new();
    while let Some(line) = rx.recv().await {
        on_line(&line);
        // Bounded so a runaway build cannot grow this without limit; the live
        // stream already went to the UI, this is only the tail for the summary.
        if collected.len() < 2000 {
            collected.push(line);
        }
    }

    let status = child.wait().await.map_err(|e| {
        Error::new(
            Code::GenerateFailed,
            format!("{program} did not exit cleanly: {e}"),
        )
    })?;

    Ok(Output {
        success: status.success(),
        code: status.code(),
        lines: collected,
    })
}

/// What to run and how to report it. A struct rather than eight positional
/// arguments: at that count, two adjacent `&str` parameters swapped by mistake
/// compile cleanly and emit the wrong event name.
pub struct Operation<'a> {
    pub operation_id: &'a str,
    pub subject: &'a str,
    pub progress_event: &'a str,
    pub finished_event: &'a str,
    pub program: &'a str,
    pub args: &'a [String],
    pub cwd: &'a Path,
}

/// Run a command and report it as an operation: progress events per line, one
/// terminal event carrying success or failure.
pub async fn run_operation(app: &AppHandle, op: Operation<'_>) -> Result<()> {
    let Operation {
        operation_id,
        subject,
        progress_event,
        finished_event,
        program,
        args,
        cwd,
    } = op;

    let started = std::time::Instant::now();

    let result = stream(program, args, cwd, |line| {
        events::emit(
            app,
            progress_event,
            ProgressEvent {
                operation_id: operation_id.to_string(),
                subject: subject.to_string(),
                line: line.to_string(),
            },
        );
    })
    .await;

    let duration_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(output) if output.success => {
            tracing::info!(
                operation_id,
                subject,
                program,
                duration_ms,
                "operation succeeded"
            );
            events::emit(
                app,
                finished_event,
                FinishedEvent {
                    operation_id: operation_id.to_string(),
                    subject: subject.to_string(),
                    success: true,
                    duration_ms,
                    error: None,
                    log_path: None,
                },
            );
            Ok(())
        }
        Ok(output) => {
            // The last few lines are almost always the actual error; the rest
            // is progress noise the user already watched scroll past.
            let tail = output
                .lines
                .iter()
                .rev()
                .take(5)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let message = format!(
                "{program} exited with code {}",
                output
                    .code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into())
            );

            // The tail is what makes a failed build diagnosable after the fact,
            // and it is subprocess output — the one source that can carry a
            // secret into the log without anyone deciding to put it there.
            tracing::error!(
                operation_id,
                subject,
                program,
                exit_code = output.code,
                duration_ms,
                tail = %crate::logging::redact(&tail),
                "operation failed"
            );

            events::emit(
                app,
                finished_event,
                FinishedEvent {
                    operation_id: operation_id.to_string(),
                    subject: subject.to_string(),
                    success: false,
                    duration_ms,
                    error: Some(if tail.is_empty() {
                        message.clone()
                    } else {
                        tail
                    }),
                    log_path: None,
                },
            );
            Err(Error::new(Code::GenerateFailed, message))
        }
        Err(e) => {
            // Could not even start: a missing binary, a bad cwd, no permission.
            tracing::error!(operation_id, subject, program, error = %e, "operation could not start");
            events::emit(
                app,
                finished_event,
                FinishedEvent {
                    operation_id: operation_id.to_string(),
                    subject: subject.to_string(),
                    success: false,
                    duration_ms,
                    error: Some(e.message.clone()),
                    log_path: None,
                },
            );
            Err(e)
        }
    }
}

// ---------------------------------------------------------------- commands

/// The generated compose files, in the order the CLI layers them.
pub fn compose_files(root: &Path) -> Vec<PathBuf> {
    [
        "stackvo.yml",
        "docker-compose.dynamic.yml",
        "docker-compose.projects.yml",
    ]
    .iter()
    .map(|f| root.join("generated").join(f))
    .collect()
}

/// `docker compose --env-file .env -f … -f … -f …` prefix.
pub fn compose_base_args(root: &Path) -> Vec<String> {
    let mut args = vec![
        "compose".to_string(),
        "--env-file".to_string(),
        root.join(".env").display().to_string(),
    ];
    for file in compose_files(root) {
        args.push("-f".to_string());
        args.push(file.display().to_string());
    }
    args
}

/// Compose profile arguments for a start mode, mirroring `stackvo up`.
pub fn profile_args(mode: &str, profiles: &[String]) -> Result<Vec<String>> {
    let mut args = vec!["--profile".to_string(), "core".to_string()];

    match mode {
        "minimal" => {}
        "services" => args.extend(["--profile".into(), "services".into()]),
        "projects" => args.extend(["--profile".into(), "projects".into()]),
        "all" => args.extend([
            "--profile".into(),
            "services".into(),
            "--profile".into(),
            "projects".into(),
        ]),
        "custom" => {
            if profiles.is_empty() {
                return Err(Error::new(
                    Code::InvalidInput,
                    "custom mode needs at least one profile",
                ));
            }
            for p in profiles {
                args.push("--profile".into());
                args.push(p.clone());
            }
        }
        other => {
            return Err(Error::new(
                Code::InvalidInput,
                format!("unknown start mode: {other}"),
            ))
        }
    }

    Ok(args)
}

/// Path to the CLI entrypoint, verified to exist.
pub fn cli_script(root: &Path) -> Result<PathBuf> {
    let path = root.join("core/cli/stackvo.sh");
    if !path.is_file() {
        return Err(
            Error::new(Code::NoWorkspace, format!("{} is missing", path.display()))
                .with_hint("The selected folder no longer looks like a StackVo checkout."),
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_args_match_the_cli_modes() {
        assert_eq!(
            profile_args("minimal", &[]).unwrap(),
            vec!["--profile", "core"]
        );
        assert_eq!(
            profile_args("all", &[]).unwrap(),
            vec![
                "--profile",
                "core",
                "--profile",
                "services",
                "--profile",
                "projects"
            ]
        );
        assert!(profile_args("nonsense", &[]).is_err());
        assert!(
            profile_args("custom", &[]).is_err(),
            "custom with no profiles is invalid"
        );
        assert_eq!(
            profile_args("custom", &["mysql".into()]).unwrap(),
            vec!["--profile", "core", "--profile", "mysql"]
        );
    }

    #[test]
    fn compose_args_reference_all_three_generated_files() {
        let args = compose_base_args(Path::new("/tmp/stackvo"));
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 3);
        assert!(args
            .iter()
            .any(|a| a.ends_with("docker-compose.projects.yml")));
        assert!(args.iter().any(|a| a.ends_with("/.env")));
    }

    #[tokio::test]
    async fn streams_lines_as_they_arrive_and_reports_exit_code() {
        let mut seen = Vec::new();
        let out = stream(
            "sh",
            &["-c".into(), "echo one; echo two >&2; exit 3".into()],
            Path::new("/tmp"),
            |line| seen.push(line.to_string()),
        )
        .await
        .unwrap();

        assert!(!out.success);
        assert_eq!(out.code, Some(3));
        // Both streams are captured; order between them is arrival order.
        assert!(seen.contains(&"one".to_string()));
        assert!(seen.contains(&"two".to_string()));
    }

    #[tokio::test]
    async fn arguments_are_never_shell_interpreted() {
        // The whole point of execFile-style spawning: this is one argument,
        // not a command separator. Under the old execAsync it would have run.
        let marker = std::env::temp_dir().join("stackvo-injection-canary");
        let _ = std::fs::remove_file(&marker);

        let out = stream(
            "echo",
            &[format!("hi; touch {}", marker.display())],
            Path::new("/tmp"),
            |_| {},
        )
        .await
        .unwrap();

        assert!(out.success);
        assert!(
            !marker.exists(),
            "the argument was executed as a shell command"
        );
    }

    #[test]
    fn cli_script_is_verified_to_exist() {
        assert!(cli_script(Path::new("/definitely/not/a/checkout")).is_err());
    }
}
