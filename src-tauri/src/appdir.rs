//! The folders this app owns inside the OS's own directories.
//!
//! Two of them — where preferences are written, and where the log is written —
//! and until now each was named where it was used, with the bundle identifier
//! spelled out by hand in both. That is how two directories that are supposed
//! to carry the same name stop carrying the same name.
//!
//! ## Why not the bundle identifier
//!
//! The identifier is what the *operating system* calls this app: the
//! `Preferences` plist, the code signature, the privacy prompts. macOS gives no
//! choice there. These two folders are different — they are ours to name, and
//! they are the ones a person is asked to open when something goes wrong.
//!
//! Three comparable apps on the machine this was written on, checked rather
//! than recalled:
//!
//! | app           | identifier                      | Application Support | Logs           |
//! | ------------- | ------------------------------- | ------------------- | -------------- |
//! | Postman       | `com.postmanlabs.mac`           | `Postman`           | —              |
//! | Termius       | `com.termius-dmg.mac`           | `Termius`           | —              |
//! | Redis Insight | `org.RedisLabs.RedisInsight-V2` | `RedisInsight`      | `RedisInsight` |
//!
//! All three put the identifier in the plist, which they must, and the readable
//! name in the folders they chose, which they need not have. None does the
//! reverse.
//!
//! ## Why not `~/.stackvo`
//!
//! That directory is the *stack's* state: the user picks what it points at,
//! `STACKVO_ROOT` can move it, and deleting it is a supported way to start
//! over. A log that moves with the root fragments across roots and disappears
//! along with the workspace whose failure it was recording — and the preference
//! for which editor to open is not a fact about a stack.
//!
//! Redis Insight, the closest analogue here — a desktop shell in front of a
//! service it manages — splits it the same way, and both halves are in use:
//! `~/Library/Logs/RedisInsight/main.log` for the app, `~/.redis-insight/logs/`
//! for the backend it drives.

use std::path::PathBuf;

/// The name this app takes in the OS's directories.
///
/// Title case where the platform's own folders are title case, lower case on
/// Linux where every XDG directory is.
pub fn name() -> &'static str {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        "StackVo"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "stackvo"
    }
}

/// Where `preferences.json` lives.
///
/// macOS: `~/Library/Application Support/StackVo`, Windows: `%APPDATA%\StackVo`,
/// Linux: `~/.config/stackvo`.
pub fn config() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(name()))
}

/// Where the rotating log files live.
///
/// Deliberately not `config()`: those are things the user chose and would want
/// to keep, and a log is neither. On macOS that distinction needs the
/// platform's own convention, because `config_dir()` and `data_local_dir()` are
/// the same folder there (`~/Library/Application Support`) — measured, not
/// assumed. Apple puts logs in `~/Library/Logs`, which is also where Console.app
/// looks for them.
pub fn logs() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(dirs::home_dir()?.join("Library").join("Logs").join(name()))
    }
    #[cfg(target_os = "windows")]
    {
        Some(dirs::data_local_dir()?.join(name()).join("logs"))
    }
    // Linux and the BSDs. `XDG_STATE_HOME` (`~/.local/state`) rather than
    // `XDG_DATA_HOME`, which is where this used to write: the specification adds
    // state for data that should persist but is not worth backing up, and names
    // log files as the example. `state_dir` answers None off Linux, so the old
    // location stays as the fallback rather than losing the log entirely.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Some(
            dirs::state_dir()
                .or_else(dirs::data_local_dir)?
                .join(name())
                .join("logs"),
        )
    }
}

/// The folder name every one of these used before the rename.
///
/// Read-only, and only by the migration below and by `workspace`'s reader for a
/// pre-split `workspace.txt`. It is a historical fact about installs that
/// already exist, so it must never be re-derived from the current identifier —
/// changing the identifier again would silently stop finding what the old one
/// left behind.
pub const LEGACY_DIR: &str = "dev.stackvo.desktop";

/// Bring `preferences.json` across from the folder the old identifier named.
///
/// Only the one file. `workspace.txt` is deliberately left where it is: it
/// belongs to a layout two migrations ago and `workspace::migrate_single_root`
/// is still the thing that reads and retires it.
///
/// Best effort, and silent about a failure beyond the log: the worst outcome is
/// a user whose theme is back to default, which is not a reason to refuse to
/// start. Never overwrites — a preferences file at the new path means this
/// already ran, or the user wrote one, and either way it wins.
pub fn migrate_config() {
    let (Some(new_dir), Some(old_dir)) = (config(), dirs::config_dir().map(|d| d.join(LEGACY_DIR)))
    else {
        return;
    };
    move_preferences(&new_dir, &old_dir);
}

/// The move itself, with both directories passed in rather than read.
///
/// Split out for the same reason `workspace::migrate` is: reading them from the
/// OS config directory inside this function would make it testable only by
/// writing to the real one.
fn move_preferences(new_dir: &std::path::Path, old_dir: &std::path::Path) {
    if new_dir == old_dir {
        return;
    }

    let (new, old) = (
        new_dir.join("preferences.json"),
        old_dir.join("preferences.json"),
    );
    if new.exists() || !old.is_file() {
        return;
    }

    if std::fs::create_dir_all(new_dir).is_err() {
        return;
    }

    // Rename first: it is atomic within a filesystem, and both paths are under
    // the same config directory. The copy is for the case where they are not —
    // a home directory assembled out of mounts is unusual, not impossible.
    let moved = std::fs::rename(&old, &new).is_ok()
        || (std::fs::copy(&old, &new).is_ok() && {
            let _ = std::fs::remove_file(&old);
            true
        });

    if moved {
        tracing::info!(from = %old.display(), to = %new.display(), "moved preferences to the renamed config directory");
    } else {
        tracing::warn!(from = %old.display(), to = %new.display(), "could not move preferences; starting with defaults");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name is ours to choose, so the one thing worth asserting is that it
    /// is not the identifier — which is what this rename existed to undo.
    #[test]
    fn the_folder_name_is_not_the_bundle_identifier() {
        assert!(!name().contains('.'), "{} looks like an identifier", name());
        assert_ne!(name(), LEGACY_DIR);
    }

    /// Both folders carry the same name, which is the drift this module exists
    /// to prevent.
    #[test]
    fn config_and_logs_agree_on_the_name() {
        let config = config().expect("a config directory");
        let logs = logs().expect("a log directory");
        assert!(config.ends_with(name()));
        assert!(
            logs.components().any(|c| c.as_os_str() == name()),
            "{} does not carry {}",
            logs.display(),
            name()
        );
    }

    /// Logs sitting next to `preferences.json` would be swept up by anyone
    /// clearing "settings" and kept by anyone backing them up. On macOS this
    /// only holds because of the `~/Library/Logs` branch — `config_dir()` and
    /// `data_local_dir()` are the same directory there.
    #[test]
    fn the_log_directory_is_not_the_config_directory() {
        let logs = logs().expect("a log directory");
        let config = dirs::config_dir().expect("a config directory");
        assert!(
            !logs.starts_with(&config),
            "logs at {} sit inside the config directory {}",
            logs.display(),
            config.display()
        );
    }

    /// The other half of the question this module keeps being asked: both
    /// folders belong to the application, not to the stack, so neither may land
    /// under the app root — which `STACKVO_ROOT` can move and the user can
    /// delete.
    #[test]
    fn neither_directory_is_inside_the_app_root() {
        let app_root = crate::workspace::app_root();
        for dir in [config().unwrap(), logs().unwrap()] {
            assert!(
                !dir.starts_with(&app_root),
                "{} sits inside the app root {}",
                dir.display(),
                app_root.display()
            );
        }
    }

    /// XDG puts logs in `XDG_STATE_HOME`, and this wrote to `XDG_DATA_HOME`
    /// until it was noticed. The fallback branch is what keeps the assertion
    /// honest on a machine where `state_dir` answers None.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_logs_go_under_the_state_directory() {
        let logs = logs().expect("a log directory");
        match dirs::state_dir() {
            Some(state) => assert!(
                logs.starts_with(&state),
                "logs at {} are not under {}",
                logs.display(),
                state.display()
            ),
            None => assert!(logs.starts_with(dirs::data_local_dir().unwrap())),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-appdir-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn preferences_are_carried_over_and_the_old_copy_is_gone() {
        let base = scratch("carry");
        let (old, new) = (base.join(LEGACY_DIR), base.join(name()));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("preferences.json"), r#"{"theme":"dark"}"#).unwrap();

        move_preferences(&new, &old);

        assert_eq!(
            std::fs::read_to_string(new.join("preferences.json")).unwrap(),
            r#"{"theme":"dark"}"#
        );
        assert!(
            !old.join("preferences.json").exists(),
            "leaving it behind means the next launch has two files to disagree about"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A preferences file already at the new path is the user's, and a
    /// migration that overwrites it is a migration that loses settings.
    #[test]
    fn an_existing_file_at_the_new_path_wins() {
        let base = scratch("nooverwrite");
        let (old, new) = (base.join(LEGACY_DIR), base.join(name()));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("preferences.json"), r#"{"theme":"light"}"#).unwrap();
        std::fs::write(new.join("preferences.json"), r#"{"theme":"dark"}"#).unwrap();

        move_preferences(&new, &old);

        assert_eq!(
            std::fs::read_to_string(new.join("preferences.json")).unwrap(),
            r#"{"theme":"dark"}"#
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A fresh install has nothing to carry, and must not be left with an empty
    /// directory implying it does.
    #[test]
    fn nothing_to_migrate_creates_nothing() {
        let base = scratch("fresh");
        let (old, new) = (base.join(LEGACY_DIR), base.join(name()));

        move_preferences(&new, &old);

        assert!(
            !new.exists(),
            "{} should not have been created",
            new.display()
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
