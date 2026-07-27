//! A log the user can send you.
//!
//! Until now this app wrote nothing anywhere. When someone reports "it won't
//! start" or "the build hangs", the only evidence is their description of it —
//! and the interesting failures (a Docker socket that moved, an elevation
//! prompt the user cancelled, a compose run that died on exit 137) leave no
//! trace at all once the window is closed.
//!
//! Deliberately narrow in what it records: operations, state transitions and
//! errors. Never payloads. A log that captures `.env` contents or container
//! environment is a log nobody can safely attach to an issue, which makes it
//! worthless for the one job it has.

use std::path::{Path, PathBuf};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Kept alive for the process lifetime; dropping it stops the writer thread and
/// silently discards buffered lines.
pub struct Guard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

/// Where the log files live.
///
/// Not beside `workspace.txt` and `preferences.json` in the config directory:
/// those are things the user chose and would want to keep, and logs are
/// neither. On macOS that distinction needs the platform's own convention,
/// because `config_dir()` and `data_local_dir()` are the same folder there
/// (`~/Library/Application Support`) — measured, not assumed. Apple puts logs
/// in `~/Library/Logs`, which is also where Console.app looks for them.
pub fn dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(
            dirs::home_dir()?
                .join("Library")
                .join("Logs")
                .join("dev.stackvo.desktop"),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Linux: ~/.local/share, Windows: %LOCALAPPDATA% — both already distinct
        // from the config directory.
        Some(
            dirs::data_local_dir()?
                .join("dev.stackvo.desktop")
                .join("logs"),
        )
    }
}

/// Start writing. Returns None when there is no writable location, which is a
/// reason to run without a log, not a reason to refuse to start.
pub fn init() -> Option<Guard> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;

    // Daily files, capped. Without the cap this grows for as long as the app is
    // installed — a log that eventually costs a gigabyte is a bug, not a
    // feature, and nobody reads a file from eight months ago.
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("stackvo")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .ok()?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    // `STACKVO_LOG` rather than `RUST_LOG`: this is a desktop app, and a user
    // being talked through raising the log level should not have to be told
    // about a convention from a language they do not use.
    let filter = EnvFilter::try_from_env("STACKVO_LOG")
        .unwrap_or_else(|_| EnvFilter::new("stackvo_desktop=info,warn"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    // In a release build there is no console to print to, so stderr would go
    // nowhere; in development it is the fastest thing to read.
    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    registry.try_init().ok()?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        dir = %dir.display(),
        "StackVo Desktop starting"
    );

    Some(Guard(guard))
}

/// Mask secret-looking assignments in text that came from a subprocess.
///
/// The streamed output of `docker compose` and the StackVo CLI is the one place
/// a secret can reach a log without anyone deciding to put it there: compose
/// echoes interpolated variables, and a failing container prints its own
/// environment. Everything else this module records is written by us, and we do
/// not write values.
///
/// The key test is the same one `config::Env` uses, so a key that is redacted
/// in the UI is redacted here too rather than by a second, drifting rule.
pub fn redact(line: &str) -> std::borrow::Cow<'_, str> {
    // Cheap reject: the overwhelming majority of build output has no `=` at all.
    if !line.contains('=') {
        return std::borrow::Cow::Borrowed(line);
    }

    let mut out = String::with_capacity(line.len());
    let mut changed = false;
    let mut rest = line;

    while let Some(eq) = rest.find('=') {
        let (before, after) = rest.split_at(eq);
        let after = &after[1..];

        // The key is the run of key-ish characters immediately before the `=`.
        let key_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let key = &before[key_start..];

        let value_end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());

        if !key.is_empty() && crate::config::Env::is_secret(key) && value_end > 0 {
            out.push_str(before);
            out.push_str("=***");
            changed = true;
        } else {
            out.push_str(before);
            out.push('=');
            out.push_str(&after[..value_end]);
        }
        rest = &after[value_end..];
    }
    out.push_str(rest);

    if changed {
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(line)
    }
}

/// The newest log file, for the "open my log" button.
pub fn newest_file() -> Option<PathBuf> {
    let dir = dir()?;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries.last().map(|e| e.path())
}

/// Best-effort size of everything under the log directory, for Settings.
pub fn total_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_without_assignments_is_returned_unchanged() {
        let line = "Step 4/12 : RUN apt-get update";
        assert!(matches!(redact(line), std::borrow::Cow::Borrowed(_)));
        assert_eq!(redact(line), line);
    }

    #[test]
    fn secret_values_are_masked_but_the_key_survives() {
        // The key is what makes the line diagnosable; the value is what makes
        // it unsafe to attach to an issue.
        let out = redact("MYSQL_ROOT_PASSWORD=hunter2");
        assert_eq!(out, "MYSQL_ROOT_PASSWORD=***");
    }

    #[test]
    fn ordinary_configuration_is_left_readable() {
        for line in [
            "DEFAULT_TLD_SUFFIX=stackvo.loc",
            "SERVICE_REDIS_ENABLE=true",
            "PHP_VERSION=8.4",
        ] {
            assert_eq!(redact(line), line, "{line} should not be masked");
        }
    }

    #[test]
    fn several_assignments_on_one_line_are_handled_independently() {
        let out = redact("env: DEFAULT_TLD_SUFFIX=stackvo.loc MYSQL_PASSWORD=s3cret DEBUG=1");
        assert_eq!(
            out,
            "env: DEFAULT_TLD_SUFFIX=stackvo.loc MYSQL_PASSWORD=*** DEBUG=1"
        );
    }

    #[test]
    fn a_secret_key_with_an_empty_value_is_left_alone() {
        // Nothing to hide, and rewriting it to `=***` would suggest a value is
        // set when none is.
        assert_eq!(redact("MYSQL_PASSWORD="), "MYSQL_PASSWORD=");
    }

    #[test]
    fn a_prefixed_occurrence_is_still_caught() {
        // Compose prefixes its echoes; the key is still the run before the `=`.
        let out = redact("stackvo-mysql | MYSQL_ROOT_PASSWORD=hunter2");
        assert_eq!(out, "stackvo-mysql | MYSQL_ROOT_PASSWORD=***");
    }

    /// Logs sitting next to `workspace.txt` would be swept up by anyone
    /// clearing "settings" and kept by anyone backing them up. On macOS this
    /// only holds because of the `~/Library/Logs` branch — `config_dir()` and
    /// `data_local_dir()` are the same directory there.
    #[test]
    fn the_log_directory_is_not_the_config_directory() {
        let logs = dir().expect("a log directory on every supported platform");
        let config = dirs::config_dir().expect("a config directory");
        assert!(
            !logs.starts_with(config.join("dev.stackvo.desktop")),
            "logs at {} sit inside the config directory",
            logs.display()
        );
    }
}
