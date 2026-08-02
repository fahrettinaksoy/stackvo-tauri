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
pub async fn run_operation(sink: &events::Sink, op: Operation<'_>) -> Result<()> {
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
        sink.emit(
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
            sink.emit(
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

            sink.emit(
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
            sink.emit(
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
///
/// The Xdebug overlay is re-derived here rather than merely appended if it
/// happens to exist. It is a pure function of the manifests, and the failure
/// mode of letting it persist as state is total: an overlay naming a deleted
/// project declares a service with neither an image nor a build context, and
/// compose then refuses every command — including the `down` that would have
/// cleared it. Rebuilding it costs a few small file reads on a path that is
/// about to spawn Docker.
pub fn compose_base_args(root: &Path) -> Vec<String> {
    // `--env-file` is passed only when there is a file: compose exits with
    // "couldn't find env file" rather than carrying on without it. Nothing is
    // lost when it is absent — every service value is already written out
    // literally by the generator, and the one variable left in the output,
    // `${PWD}`, comes from the process environment.
    let mut args = vec!["compose".to_string()];
    let env_file = root.join(".env");
    if env_file.is_file() {
        args.push("--env-file".to_string());
        args.push(env_file.display().to_string());
    }
    for file in compose_files(root) {
        args.push("-f".to_string());
        args.push(file.display().to_string());
    }

    // Layered last so their `environment:` and `volumes:` merge onto the
    // generated service rather than being merged over. Two files rather than
    // one: the overlays are independent — Xdebug adds environment, php.ini adds
    // a mount — and a fault in either must not take the other's projects down.
    if crate::xdebug::sync(root) {
        args.push("-f".to_string());
        args.push(crate::xdebug::overlay_path(root).display().to_string());
    }
    if crate::phpini::sync(root) {
        args.push("-f".to_string());
        args.push(crate::phpini::overlay_path(root).display().to_string());
    }
    // Two environment variables, and nothing else. Safe to layer for every
    // eligible project because a dump with no collector listening falls back to
    // rendering into the response — verified, not assumed.
    if crate::dumps::sync(root) {
        args.push("-f".to_string());
        args.push(crate::dumps::overlay_path(root).display().to_string());
    }
    // Last of the three, and it is the only one that overrides rather than
    // adds: it replaces the container's `command` with the dev server. Anything
    // layered after it would be merging onto a service already in a different
    // mode, so this is where the chain ends.
    if crate::devserver::sync(root) {
        args.push("-f".to_string());
        args.push(crate::devserver::overlay_path(root).display().to_string());
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
        // A path with no projects/ directory: no project asks for Xdebug, so
        // the overlay is not rendered and the three generated files are all
        // that is layered.
        let args = compose_base_args(Path::new("/tmp/stackvo-not-a-checkout"));
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 3);
        assert!(args
            .iter()
            .any(|a| a.ends_with("docker-compose.projects.yml")));

        // No `.env` at this path, so none is passed. Naming a file that is not
        // there is not a harmless extra flag: compose exits with "couldn't
        // find env file" and the command never runs.
        assert!(!args.iter().any(|a| a == "--env-file"));
    }

    #[test]
    fn the_env_file_is_passed_when_there_is_one() {
        let dir = std::env::temp_dir().join("stackvo-env-file-arg-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".env"), "SERVICE_REDIS_HOST_PORT=6380\n").unwrap();

        let args = compose_base_args(&dir);
        assert!(args.iter().any(|a| a == "--env-file"));
        assert!(args.iter().any(|a| a.ends_with("/.env")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The overlay has to come after the generated files. Compose merges later
    /// `-f` files onto earlier ones; reversed, the generated service would
    /// overwrite the environment the overlay exists to add.
    #[test]
    fn the_xdebug_overlay_is_layered_last_when_present() {
        let dir = std::env::temp_dir().join("stackvo-xdebug-order-test");
        let project = dir.join("projects").join("shop");
        std::fs::create_dir_all(&project).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        std::fs::create_dir_all(dir.join("generated")).unwrap();

        // The overlay may only name services the generator actually emitted,
        // so the test has to look like a generated checkout, not just a
        // directory with a manifest in it.
        std::fs::write(
            dir.join("generated").join("docker-compose.projects.yml"),
            "name: stackvo\n\nservices:\n  shop:\n    image: x\n\nnetworks:\n  stackvo-net:\n",
        )
        .unwrap();
        std::fs::write(
            project.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd","xdebug"]}}"#,
        )
        .unwrap();

        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 4);
        assert!(args.last().unwrap().ends_with("docker-compose.xdebug.yml"));

        // And it disappears again once nothing asks for it.
        std::fs::write(
            project.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd"]}}"#,
        )
        .unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 3);
        assert!(!crate::xdebug::overlay_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The php.ini overlay is a second, independent layer. Both can be present
    /// at once, and neither depends on the other — which is the reason they are
    /// two files and not two sections of one.
    #[test]
    fn the_php_ini_overlay_layers_alongside_the_xdebug_one() {
        let dir = std::env::temp_dir().join("stackvo-phpini-order-test");
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("projects").join("shop");
        std::fs::create_dir_all(project.join(".stackvo")).unwrap();
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        std::fs::write(
            dir.join("generated").join("docker-compose.projects.yml"),
            "name: stackvo\n\nservices:\n  shop:\n    image: x\n\nnetworks:\n  stackvo-net:\n",
        )
        .unwrap();
        std::fs::write(
            project.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd"]}}"#,
        )
        .unwrap();
        std::fs::write(
            project.join(".stackvo").join("php.ini"),
            "memory_limit = 512M\n",
        )
        .unwrap();

        // php.ini alone: four files, and the last one is the mount.
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 4);
        assert!(args.last().unwrap().ends_with("docker-compose.phpini.yml"));

        // Both: five, with php.ini still last.
        std::fs::write(
            project.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd","xdebug"]}}"#,
        )
        .unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 5);

        // And the mount goes when the file does — a stale overlay pointing at a
        // path that no longer exists mounts an empty directory into conf.d.
        std::fs::remove_file(project.join(".stackvo").join("php.ini")).unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 4);
        assert!(!crate::phpini::overlay_path(&dir).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The dumps overlay reaches a PHP project that has the collector in its
    /// vendor tree, and nothing else — a project with no `var-dump-server`
    /// would be pointed at a server it can never run.
    #[test]
    fn the_dumps_overlay_needs_the_collector_to_be_installed() {
        let dir = std::env::temp_dir().join("stackvo-dumps-overlay-test");
        let _ = std::fs::remove_dir_all(&dir);
        let shop = dir.join("projects").join("shop");
        std::fs::create_dir_all(&shop).unwrap();
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        std::fs::write(
            dir.join("generated").join("docker-compose.projects.yml"),
            "name: stackvo\n\nservices:\n  shop:\n    image: x\n\nnetworks:\n  stackvo-net:\n",
        )
        .unwrap();
        std::fs::write(
            shop.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd"]}}"#,
        )
        .unwrap();

        // No vendor tree yet: three files, no dumps overlay.
        assert_eq!(
            compose_base_args(&dir)
                .iter()
                .filter(|a| *a == "-f")
                .count(),
            3
        );

        std::fs::create_dir_all(shop.join("vendor").join("bin")).unwrap();
        std::fs::write(shop.join(crate::dumps::BINARY), "#!/usr/bin/env php\n").unwrap();

        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 4);
        assert!(args.iter().any(|a| a.ends_with("docker-compose.dumps.yml")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The dev-server overlay is the only one that *overrides* rather than
    /// adds — it replaces the container's command — so it has to be last, and
    /// it must not attach to a PHP project, whose `/app` does not exist and
    /// whose site would be replaced by an empty mount.
    #[test]
    fn the_dev_server_overlay_is_last_and_node_only() {
        let dir = std::env::temp_dir().join("stackvo-devserver-order-test");
        let _ = std::fs::remove_dir_all(&dir);
        let php = dir.join("projects").join("shop");
        let node = dir.join("projects").join("site");
        std::fs::create_dir_all(php.join(".stackvo")).unwrap();
        std::fs::create_dir_all(node.join(".stackvo")).unwrap();
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        std::fs::write(
            dir.join("generated").join("docker-compose.projects.yml"),
            "name: stackvo\n\nservices:\n  shop:\n    image: x\n  site:\n    image: y\n\nnetworks:\n  stackvo-net:\n",
        )
        .unwrap();
        std::fs::write(
            php.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd"]}}"#,
        )
        .unwrap();
        std::fs::write(
            node.join("stackvo.json"),
            r#"{"name":"site","domain":"site.loc","runtime":"node",
                "node":{"version":"22","install":"npm ci","start":"node server.js","port":3000}}"#,
        )
        .unwrap();

        // A PHP project asking for dev mode is refused by the renderer, not by
        // the UI: an overlay is derived from files, and a file can be dropped
        // in by hand.
        std::fs::write(
            php.join(".stackvo").join("devserver.json"),
            r#"{"command":"npm run dev"}"#,
        )
        .unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(
            args.iter().filter(|a| *a == "-f").count(),
            3,
            "a PHP project got a dev-server overlay"
        );

        std::fs::write(
            node.join(".stackvo").join("devserver.json"),
            r#"{"command":"npm run dev"}"#,
        )
        .unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 4);
        assert!(args
            .last()
            .unwrap()
            .ends_with("docker-compose.devserver.yml"));

        // And it stays last with the others present.
        std::fs::write(
            php.join(".stackvo").join("php.ini"),
            "memory_limit = 512M\n",
        )
        .unwrap();
        std::fs::write(
            php.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["gd","xdebug"]}}"#,
        )
        .unwrap();
        let args = compose_base_args(&dir);
        assert_eq!(args.iter().filter(|a| *a == "-f").count(), 6);
        assert!(args
            .last()
            .unwrap()
            .ends_with("docker-compose.devserver.yml"));

        std::fs::remove_dir_all(&dir).ok();
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
}
