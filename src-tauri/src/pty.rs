//! Interactive terminals.
//!
//! `TerminalService.js` spawned `docker exec -it <container> bash` through
//! node-pty — from inside a container. That worked for reaching sibling
//! containers and nothing else: opening a shell on the *host* was impossible,
//! so `POST /api/terminal/:name/open` returned HTTP 400 with "Terminal cannot
//! be opened from containerized environment" in every shipped configuration.
//!
//! On the host both targets are ordinary child processes.

use crate::error::{Code, Error, Result};
use crate::events;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PtyTarget {
    /// A shell inside a running container.
    Container {
        name: String,
        #[serde(default)]
        shell: Option<String>,
    },
    /// A shell on the host, typically in a project directory. New in v1 — the
    /// containerised UI had no way to offer this.
    Host {
        #[serde(default)]
        cwd: Option<String>,
    },
}

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

#[derive(Default)]
pub struct Registry {
    sessions: Mutex<HashMap<String, Session>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Build the command for a target.
///
/// Container shells try bash and fall back to sh: alpine-based images (which
/// StackVo uses for several services) have no bash, and the old code hardcoded
/// `bash -l`, so those terminals simply failed to open.
fn command_for(target: &PtyTarget) -> Result<CommandBuilder> {
    match target {
        PtyTarget::Container { name, shell } => {
            let container = crate::engine::container_name(name);
            let shell = shell.clone().unwrap_or_else(|| "bash".into());

            let mut cmd = CommandBuilder::new("docker");
            cmd.args([
                "exec",
                "-it",
                &container,
                "sh",
                "-c",
                // The inner string is passed to `sh -c` inside the container,
                // and `shell` is the only variable part — validated below.
                &format!("command -v {shell} >/dev/null 2>&1 && exec {shell} -l || exec sh"),
            ]);
            Ok(cmd)
        }
        PtyTarget::Host { cwd } => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                if cfg!(target_os = "windows") {
                    "powershell".into()
                } else {
                    "/bin/sh".into()
                }
            });
            let mut cmd = CommandBuilder::new(shell);
            if let Some(dir) = cwd {
                cmd.cwd(dir);
            }
            Ok(cmd)
        }
    }
}

/// A shell name goes into a `sh -c` string, so it must not be able to carry
/// anything but a command name.
fn validate(target: &PtyTarget) -> Result<()> {
    if let PtyTarget::Container {
        shell: Some(shell), ..
    } = target
    {
        let plain = shell
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'));
        if !plain || shell.is_empty() {
            return Err(Error::new(
                Code::InvalidInput,
                format!("invalid shell: {shell}"),
            ));
        }
    }
    Ok(())
}

/// Open a PTY and stream its output as `terminal:output` events.
pub fn open(
    app: &AppHandle,
    registry: &Registry,
    target: PtyTarget,
    cols: u16,
    rows: u16,
) -> Result<String> {
    validate(&target)?;

    let session_id = events::next_operation_id("pty");
    let size = PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| Error::new(Code::IoError, format!("could not allocate a terminal: {e}")))?;

    let child = pair
        .slave
        .spawn_command(command_for(&target)?)
        .map_err(|e| Error::new(Code::IoError, format!("could not start the shell: {e}")))?;

    // Drop the slave handle: while it is open the master never sees EOF, so the
    // reader thread below would hang forever after the shell exits.
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| Error::new(Code::IoError, format!("could not read the terminal: {e}")))?;
    let writer = pair.master.take_writer().map_err(|e| {
        Error::new(
            Code::IoError,
            format!("could not write to the terminal: {e}"),
        )
    })?;

    // The PTY reader blocks, so it gets a thread rather than a tokio task.
    {
        let app = app.clone();
        let session_id = session_id.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // Lossy: a read can split a multi-byte character, and
                        // xterm.js reassembles the pieces on its side.
                        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                        events::emit(
                            &app,
                            "terminal:output",
                            serde_json::json!({ "sessionId": session_id, "data": chunk }),
                        );
                    }
                }
            }
            events::emit(
                &app,
                "terminal:closed",
                serde_json::json!({ "sessionId": session_id, "exitCode": 0 }),
            );
        });
    }

    registry
        .sessions
        .lock()
        .map_err(|_| Error::new(Code::IoError, "terminal registry lock poisoned"))?
        .insert(
            session_id.clone(),
            Session {
                master: pair.master,
                writer,
                child,
            },
        );

    events::emit(
        app,
        "terminal:ready",
        serde_json::json!({ "sessionId": session_id }),
    );
    Ok(session_id)
}

pub fn write(registry: &Registry, session_id: &str, data: &str) -> Result<()> {
    let mut sessions = registry
        .sessions
        .lock()
        .map_err(|_| Error::new(Code::IoError, "terminal registry lock poisoned"))?;

    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| Error::not_found(format!("terminal session {session_id}")))?;

    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|_| session.writer.flush())
        .map_err(|e| Error::io("writing to the terminal", e))
}

pub fn resize(registry: &Registry, session_id: &str, cols: u16, rows: u16) -> Result<()> {
    let sessions = registry
        .sessions
        .lock()
        .map_err(|_| Error::new(Code::IoError, "terminal registry lock poisoned"))?;

    let session = sessions
        .get(session_id)
        .ok_or_else(|| Error::not_found(format!("terminal session {session_id}")))?;

    session
        .master
        .resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| Error::new(Code::IoError, format!("could not resize the terminal: {e}")))
}

pub fn close(registry: &Registry, session_id: &str) -> Result<()> {
    let mut sessions = registry
        .sessions
        .lock()
        .map_err(|_| Error::new(Code::IoError, "terminal registry lock poisoned"))?;

    if let Some(mut session) = sessions.remove(session_id) {
        // Killing the child closes the master, which ends the reader thread.
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
    Ok(())
}

/// Kill every session. Called on window close so shells do not outlive the app.
pub fn close_all(registry: &Registry) {
    if let Ok(mut sessions) = registry.sessions.lock() {
        for (_, mut session) in sessions.drain() {
            let _ = session.child.kill();
        }
    }
}

/// Open the user's own terminal application at a target.
///
/// The HTTP version of this returned 400 unconditionally in the shipped
/// container configuration. The first desktop version worked, but only on
/// macOS and only in Terminal.app — on Windows and Linux the button was there
/// and the command answered `Unsupported`.
pub fn open_external(target: &PtyTarget, preferred: Option<&str>) -> Result<()> {
    validate(target)?;

    let command = match target {
        PtyTarget::Container { name, shell } => {
            let container = crate::engine::container_name(name);
            let shell = shell.clone().unwrap_or_else(|| "bash".into());
            format!("docker exec -it {container} {shell}")
        }
        PtyTarget::Host { cwd } => match cwd {
            Some(dir) => format!("cd {dir}"),
            None => String::new(),
        },
    };

    let (id, ..) = crate::apps::resolve_terminal(preferred)?;
    spawn_terminal(id, &command)
}

/// Open the user's terminal running one of the catalog's commands.
///
/// Separate from [`open_external`] because the command is not a shell here: it
/// comes from [`crate::quickcmd::CATALOG`] by id, so the string assembled below
/// is built from compiled-in words plus a container name the workspace helper
/// has already validated. Nothing the frontend typed reaches it.
///
/// A string rather than argv only because that is what a terminal emulator
/// takes — every one of them is handed a command line, which is why
/// `spawn_terminal` exists in three platform flavours.
pub fn open_external_command(
    container: &str,
    spec: &crate::quickcmd::Spec,
    preferred: Option<&str>,
) -> Result<()> {
    let argv = crate::quickcmd::exec_argv(container, spec);
    let command = format!("docker {}", argv.join(" "));

    let (id, ..) = crate::apps::resolve_terminal(preferred)?;
    spawn_terminal(id, &command)
}

/// Launch `id` running `command`.
///
/// Each terminal wants the command a different way, and none of them takes it
/// the way the others do — this is the whole reason the first version only
/// supported one.
#[cfg(target_os = "macos")]
fn spawn_terminal(id: &str, command: &str) -> Result<()> {
    // AppleScript string literal: backslashes first, then quotes, or the
    // escaping of the first would be re-escaped by the second.
    let escaped = command.replace('\\', r"\\").replace('"', r#"\""#);

    let script = match id {
        "iterm2" => format!(
            r#"tell application "iTerm"
                activate
                set w to (create window with default profile)
                tell current session of w to write text "{escaped}"
            end tell"#
        ),
        // Terminal.app's dialect, which Warp, Ghostty, Alacritty and kitty do
        // not speak; for those, open the app and let the user paste. Better
        // than launching the wrong terminal silently.
        "terminal" => format!(
            r#"tell application "Terminal"
                activate
                do script "{escaped}"
            end tell"#
        ),
        other => {
            let app = match other {
                "warp" => "Warp",
                "ghostty" => "Ghostty",
                "alacritty" => "Alacritty",
                "kitty" => "kitty",
                _ => "Terminal",
            };
            format!(r#"tell application "{app}" to activate"#)
        }
    };

    std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn()
        .map_err(|e| Error::io("opening the terminal", e))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn spawn_terminal(id: &str, command: &str) -> Result<()> {
    // No shell here either: every argument is passed as its own element, so the
    // command text is data to the terminal rather than something cmd re-parses.
    let mut cmd = match id {
        "wt" => {
            let mut c = std::process::Command::new("wt.exe");
            c.args(["powershell", "-NoExit", "-Command", command]);
            c
        }
        "pwsh" => {
            let mut c = std::process::Command::new("pwsh.exe");
            c.args(["-NoExit", "-Command", command]);
            c
        }
        "powershell" => {
            let mut c = std::process::Command::new("powershell.exe");
            c.args(["-NoExit", "-Command", command]);
            c
        }
        _ => {
            let mut c = std::process::Command::new("cmd.exe");
            c.args(["/K", command]);
            c
        }
    };

    cmd.spawn()
        .map_err(|e| Error::io("opening the terminal", e))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_terminal(id: &str, command: &str) -> Result<()> {
    // `-e` is not universal: GNOME Terminal wants `--`, Konsole wants `-e` with
    // the rest as separate arguments, and the sh -c wrapper is what keeps the
    // window open long enough to matter.
    let shell_arg = format!("{command}; exec $SHELL");

    let mut cmd = match id {
        "gnome-terminal" => {
            let mut c = std::process::Command::new("gnome-terminal");
            c.args(["--", "sh", "-c", &shell_arg]);
            c
        }
        "konsole" => {
            let mut c = std::process::Command::new("konsole");
            c.args(["-e", "sh", "-c", &shell_arg]);
            c
        }
        "xfce4-terminal" => {
            let mut c = std::process::Command::new("xfce4-terminal");
            c.args(["--command", &format!("sh -c '{shell_arg}'")]);
            c
        }
        other => {
            let mut c = std::process::Command::new(other);
            c.args(["-e", "sh", "-c", &shell_arg]);
            c
        }
    };

    cmd.spawn()
        .map_err(|e| Error::io("opening the terminal", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_shell_carrying_shell_metacharacters() {
        let bad = PtyTarget::Container {
            name: "mysql".into(),
            shell: Some("bash; rm -rf /".into()),
        };
        assert!(validate(&bad).is_err());

        let ok = PtyTarget::Container {
            name: "mysql".into(),
            shell: Some("/bin/bash".into()),
        };
        assert!(validate(&ok).is_ok());
    }

    #[test]
    fn container_command_falls_back_to_sh() {
        // Several StackVo service images are alpine-based and have no bash;
        // the old hardcoded `bash -l` simply failed on those.
        let cmd = command_for(&PtyTarget::Container {
            name: "redis".into(),
            shell: None,
        })
        .unwrap();
        let rendered = format!("{cmd:?}");
        assert!(rendered.contains("stackvo-redis"));
        assert!(rendered.contains("exec sh"), "no fallback in: {rendered}");
    }

    #[test]
    fn host_target_is_a_plain_shell() {
        let cmd = command_for(&PtyTarget::Host {
            cwd: Some("/tmp".into()),
        })
        .unwrap();
        let rendered = format!("{cmd:?}");
        assert!(
            !rendered.contains("docker"),
            "host shells must not go through docker"
        );
    }

    #[test]
    fn target_deserialises_from_the_contract_shape() {
        let container: PtyTarget =
            serde_json::from_str(r#"{"kind":"container","name":"mysql"}"#).unwrap();
        assert!(matches!(container, PtyTarget::Container { .. }));

        let host: PtyTarget = serde_json::from_str(r#"{"kind":"host","cwd":"/w/p"}"#).unwrap();
        assert!(matches!(host, PtyTarget::Host { .. }));
    }
}
