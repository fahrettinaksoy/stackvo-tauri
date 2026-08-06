//! The terminal and editor the user actually uses.
//!
//! Both surfaces were half-built. The editor had a preference and a fallback
//! chain but no way to see what was installed, so choosing meant typing a
//! command and hoping. The external terminal was worse: hardcoded to
//! Terminal.app and gated behind `#[cfg(target_os = "macos")]`, so on Windows
//! and Linux the button existed and returned `Unsupported`.
//!
//! Detection rather than a free-text box. A list of what is actually on the
//! machine is the difference between a setting someone can use and one they
//! have to research.

use crate::error::{Code, Error, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// Stable key stored in preferences. Never the display name — those are
    /// localised and change between versions.
    pub id: String,
    pub name: String,
    /// What the UI shows beside it.
    pub icon: String,
    /// Present on this machine.
    pub available: bool,
    /// The one this app would use when the user has chosen nothing.
    ///
    /// Without it the picker was blank on a fresh install and said nothing
    /// about what "Open in terminal" would actually start — the fallback lives
    /// in `resolve_terminal` and the editor loop, where nobody can see it. Set
    /// on exactly one entry per list, and computed by the same rule those use.
    pub default: bool,
}

/// The entry that wins when no preference is stored: the first installed one.
fn mark_default(mut apps: Vec<App>) -> Vec<App> {
    if let Some(first) = apps.iter_mut().find(|a| a.available) {
        first.default = true;
    }
    apps
}

/// Terminals worth offering, in the order a user is likely to prefer them.
#[cfg(target_os = "macos")]
const TERMINALS: &[(&str, &str, &str, &str)] = &[
    // (id, display name, icon, probe)
    (
        "terminal",
        "Terminal",
        "mdi-apple",
        "/System/Applications/Utilities/Terminal.app",
    ),
    ("iterm2", "iTerm2", "mdi-console", "/Applications/iTerm.app"),
    ("warp", "Warp", "mdi-console-line", "/Applications/Warp.app"),
    (
        "ghostty",
        "Ghostty",
        "mdi-ghost",
        "/Applications/Ghostty.app",
    ),
    (
        "alacritty",
        "Alacritty",
        "mdi-console",
        "/Applications/Alacritty.app",
    ),
    ("kitty", "kitty", "mdi-cat", "/Applications/kitty.app"),
];

#[cfg(target_os = "windows")]
const TERMINALS: &[(&str, &str, &str, &str)] = &[
    ("wt", "Windows Terminal", "mdi-microsoft-windows", "wt.exe"),
    ("pwsh", "PowerShell", "mdi-powershell", "pwsh.exe"),
    (
        "powershell",
        "Windows PowerShell",
        "mdi-powershell",
        "powershell.exe",
    ),
    ("cmd", "Command Prompt", "mdi-console", "cmd.exe"),
];

#[cfg(all(unix, not(target_os = "macos")))]
const TERMINALS: &[(&str, &str, &str, &str)] = &[
    (
        "gnome-terminal",
        "GNOME Terminal",
        "mdi-console",
        "gnome-terminal",
    ),
    ("konsole", "Konsole", "mdi-console", "konsole"),
    ("alacritty", "Alacritty", "mdi-console", "alacritty"),
    ("kitty", "kitty", "mdi-cat", "kitty"),
    ("wezterm", "WezTerm", "mdi-console-line", "wezterm"),
    (
        "xfce4-terminal",
        "Xfce Terminal",
        "mdi-console",
        "xfce4-terminal",
    ),
    ("xterm", "xterm", "mdi-console", "xterm"),
];

/// Editors: the `PATH` launcher, and on macOS the application bundle too.
///
/// Probing only the launcher was wrong, and measurably so — on this machine VS
/// Code is installed at `/Applications/Visual Studio Code.app` while `code` is
/// not on `PATH`, because its "Install 'code' command in PATH" step is opt-in
/// and most people never run it. Detection said "not installed" about an editor
/// the user was looking at.
///
/// The bundle is launchable without the helper: `open -a <bundle> <path>` is
/// what Finder does. So a missing launcher is a reason to use a different
/// launch mechanism, not a reason to hide the editor.
const EDITORS: &[(&str, &str, &str, &str)] = &[
    // (id / PATH launcher, display name, icon, macOS bundle — "" when none)
    (
        "code",
        "VS Code",
        "mdi-microsoft-visual-studio-code",
        "/Applications/Visual Studio Code.app",
    ),
    (
        "cursor",
        "Cursor",
        "mdi-cursor-default",
        "/Applications/Cursor.app",
    ),
    (
        "subl",
        "Sublime Text",
        "mdi-file-code",
        "/Applications/Sublime Text.app",
    ),
    ("zed", "Zed", "mdi-lightning-bolt", "/Applications/Zed.app"),
    (
        "webstorm",
        "WebStorm",
        "mdi-alpha-w-box",
        "/Applications/WebStorm.app",
    ),
    (
        "phpstorm",
        "PhpStorm",
        "mdi-alpha-p-box",
        "/Applications/PhpStorm.app",
    ),
    // Terminal editors have no bundle to fall back to.
    ("nvim", "Neovim", "mdi-vim", ""),
    ("vim", "Vim", "mdi-vim", ""),
];

/// Browsers, by the same rule as editors: a `PATH` launcher when there is one,
/// the macOS bundle otherwise. `open -a <bundle> <url>` is what clicking a link
/// in Finder does, so a browser without a CLI shim is still launchable.
///
/// The empty id is the system default — not a browser, an *absence* of a
/// choice, and the one entry that always works.
#[cfg(target_os = "macos")]
const BROWSERS: &[(&str, &str, &str, &str)] = &[
    ("", "System default", "mdi-web", ""),
    (
        "google chrome",
        "Chrome",
        "mdi-google-chrome",
        "/Applications/Google Chrome.app",
    ),
    (
        "safari",
        "Safari",
        "mdi-apple-safari",
        "/Applications/Safari.app",
    ),
    (
        "firefox",
        "Firefox",
        "mdi-firefox",
        "/Applications/Firefox.app",
    ),
    (
        "microsoft edge",
        "Edge",
        "mdi-microsoft-edge",
        "/Applications/Microsoft Edge.app",
    ),
    (
        "brave browser",
        "Brave",
        "mdi-shield-check",
        "/Applications/Brave Browser.app",
    ),
    ("arc", "Arc", "mdi-alpha-a-circle", "/Applications/Arc.app"),
    (
        "chromium",
        "Chromium",
        "mdi-google-chrome",
        "/Applications/Chromium.app",
    ),
];

#[cfg(target_os = "linux")]
const BROWSERS: &[(&str, &str, &str, &str)] = &[
    ("", "System default", "mdi-web", ""),
    ("google-chrome", "Chrome", "mdi-google-chrome", ""),
    ("firefox", "Firefox", "mdi-firefox", ""),
    ("microsoft-edge", "Edge", "mdi-microsoft-edge", ""),
    ("brave-browser", "Brave", "mdi-shield-check", ""),
    ("chromium", "Chromium", "mdi-google-chrome", ""),
];

#[cfg(target_os = "windows")]
const BROWSERS: &[(&str, &str, &str, &str)] = &[
    ("", "System default", "mdi-web", ""),
    ("chrome", "Chrome", "mdi-google-chrome", ""),
    ("firefox", "Firefox", "mdi-firefox", ""),
    ("msedge", "Edge", "mdi-microsoft-edge", ""),
    ("brave", "Brave", "mdi-shield-check", ""),
];

pub fn browsers() -> Vec<App> {
    // The system default heads the list and is always available, so it is also
    // the entry `mark_default` lands on — which is exactly right: an unset
    // browserCommand means `resolve_browser` returns None and the OS decides.
    mark_default(
        BROWSERS
            .iter()
            .map(|(id, name, icon, bundle)| App {
                id: (*id).to_string(),
                name: (*name).to_string(),
                icon: (*icon).to_string(),
                // The system default is always available — it is the absence of a
                // choice, and something always answers a URL.
                available: id.is_empty()
                    || is_available(id)
                    || (cfg!(target_os = "macos")
                        && !bundle.is_empty()
                        && std::path::Path::new(bundle).exists()),
                default: false,
            })
            .collect(),
    )
}

/// How to open a URL in the chosen browser, or `None` for the system default.
///
/// Falls back rather than failing, exactly as `resolve_terminal` does: a
/// preference outlives the app it names, and refusing to open a link because
/// someone uninstalled Brave would be unhelpful when Safari is right there.
pub fn resolve_browser(preferred: Option<&str>) -> Option<Launch> {
    let id = preferred.filter(|p| !p.is_empty())?;
    let entry = BROWSERS.iter().find(|(i, ..)| *i == id)?;

    if is_available(entry.0) {
        return Some(Launch::Command(entry.0));
    }
    #[cfg(target_os = "macos")]
    if !entry.3.is_empty() && std::path::Path::new(entry.3).exists() {
        return Some(Launch::Bundle(entry.3));
    }
    None
}

/// How an editor can be started, if at all.
pub enum Launch {
    /// A launcher on `PATH`; the path is passed as an argument.
    Command(&'static str),
    /// macOS only: `open -a <bundle> <path>`.
    Bundle(&'static str),
}

/// Resolve `id` to a way of starting it, preferring the `PATH` launcher because
/// it is the one that accepts editor flags and behaves the same everywhere.
pub fn resolve_editor(id: &str) -> Option<Launch> {
    let entry = EDITORS.iter().find(|(i, ..)| *i == id)?;
    if is_available(entry.0) {
        return Some(Launch::Command(entry.0));
    }
    if cfg!(target_os = "macos") && !entry.3.is_empty() && std::path::Path::new(entry.3).exists() {
        return Some(Launch::Bundle(entry.3));
    }
    None
}

/// Is this program reachable? An absolute path is checked directly; a bare name
/// is looked up on `PATH`, which is what spawning it would do.
pub fn is_available(probe: &str) -> bool {
    if probe.contains(std::path::MAIN_SEPARATOR) || probe.starts_with('/') {
        return std::path::Path::new(probe).exists();
    }

    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(probe);
        candidate.is_file() || {
            // Windows omits the extension in PATHEXT lookups.
            cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|e| candidate.with_extension(e).is_file())
        }
    })
}

pub fn terminals() -> Vec<App> {
    // First installed one, the same choice `resolve_terminal` makes.
    mark_default(
        TERMINALS
            .iter()
            .map(|(id, name, icon, probe)| App {
                id: (*id).to_string(),
                name: (*name).to_string(),
                icon: (*icon).to_string(),
                available: is_available(probe),
                default: false,
            })
            .collect(),
    )
}

pub fn editors() -> Vec<App> {
    // First installed one, the same order `open_editor` walks when no editor
    // is configured.
    mark_default(
        EDITORS
            .iter()
            .map(|(id, name, icon, bundle)| App {
                id: (*id).to_string(),
                name: (*name).to_string(),
                icon: (*icon).to_string(),
                available: is_available(id)
                    || (cfg!(target_os = "macos")
                        && !bundle.is_empty()
                        && std::path::Path::new(bundle).exists()),
                default: false,
            })
            .collect(),
    )
}

/// The chosen terminal, or the first one that is actually installed.
///
/// Falling back rather than failing: a preference can outlive the app it names
/// — someone uninstalls iTerm2 — and refusing to open a terminal because of a
/// stale setting would be unhelpful when another one is right there.
pub fn resolve_terminal(
    preferred: Option<&str>,
) -> Result<&'static (&'static str, &'static str, &'static str, &'static str)> {
    if let Some(id) = preferred {
        if let Some(entry) = TERMINALS.iter().find(|(i, ..)| *i == id) {
            if is_available(entry.3) {
                return Ok(entry);
            }
        }
    }

    TERMINALS
        .iter()
        .find(|(.., probe)| is_available(probe))
        .ok_or_else(|| {
            Error::new(Code::NotFound, "No terminal application was found.")
                .with_hint(crate::hints::INSTALL_A_TERMINAL)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_stable_looking() {
        // The id is what lands in preferences.json; a duplicate would make one
        // of the two unselectable.
        for list in [
            TERMINALS.iter().map(|(id, ..)| *id).collect::<Vec<_>>(),
            EDITORS.iter().map(|(id, ..)| *id).collect::<Vec<_>>(),
        ] {
            let mut seen = std::collections::HashSet::new();
            for id in &list {
                assert!(seen.insert(*id), "duplicate id {id}");
                assert!(
                    id.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{id} is not a stable-looking key"
                );
            }
        }
    }

    #[test]
    fn a_program_that_cannot_exist_is_not_available() {
        assert!(!is_available("stackvo-no-such-program-9f3a"));
        assert!(!is_available("/nonexistent/path/to/nothing"));
    }

    /// Every machine running these tests has a shell somewhere on PATH, so this
    /// exercises the positive branch rather than only the negative one.
    #[cfg(unix)]
    #[test]
    fn a_program_that_does_exist_is_found() {
        assert!(is_available("sh"), "sh should be on PATH");
        assert!(
            is_available("/bin/sh"),
            "an absolute path is checked directly"
        );
    }

    /// The bug this guards: probing only the `PATH` launcher reported VS Code
    /// as missing on a machine that had it, because the `code` helper is opt-in
    /// on macOS and most people never enable it.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_installed_bundle_counts_even_without_its_path_launcher() {
        let bundle = "/Applications/Visual Studio Code.app";
        if !std::path::Path::new(bundle).exists() {
            return; // nothing to assert about on a machine without it
        }

        let code = editors().into_iter().find(|a| a.id == "code").unwrap();
        assert!(
            code.available,
            "an installed bundle must count as available"
        );
        assert!(
            resolve_editor("code").is_some(),
            "and must resolve to something launchable"
        );
    }

    #[test]
    fn detection_reports_every_candidate_not_only_the_installed_ones() {
        // The UI greys out what is missing rather than hiding it; a list that
        // silently omits entries reads as "this app does not support iTerm".
        assert_eq!(terminals().len(), TERMINALS.len());
        assert_eq!(editors().len(), EDITORS.len());
    }

    /// The picker showed nothing selected on a fresh install, while the app
    /// happily opened *some* terminal. Exactly one entry per list carries the
    /// flag, and it is one that exists.
    #[test]
    fn one_entry_per_list_is_the_default_and_it_is_installed() {
        for list in [terminals(), editors(), browsers()] {
            let defaults: Vec<_> = list.iter().filter(|a| a.default).collect();
            assert!(defaults.len() <= 1, "at most one default per list");
            if let Some(d) = defaults.first() {
                assert!(d.available, "{} is the default but is not installed", d.id);
            } else {
                assert!(
                    list.iter().all(|a| !a.available),
                    "a list with something installed must name a default"
                );
            }
        }
    }

    /// The flag has to describe what actually launches, or it is a label that
    /// lies. `resolve_terminal(None)` is the code path the button takes.
    #[test]
    fn the_default_terminal_is_the_one_resolution_picks() {
        let flagged = terminals().into_iter().find(|a| a.default);
        match (flagged, resolve_terminal(None)) {
            (Some(app), Ok(entry)) => assert_eq!(app.id, entry.0),
            (None, Err(e)) => assert_eq!(e.code, Code::NotFound),
            _ => panic!("the flagged default and the resolved terminal disagree"),
        }
    }

    /// An unset browser means the OS decides, so the default entry must be the
    /// one that stands for that — not whichever browser happens to be first.
    #[test]
    fn the_default_browser_is_the_system_default() {
        let flagged = browsers().into_iter().find(|a| a.default).unwrap();
        assert_eq!(flagged.id, "", "the system default entry has the empty id");
        assert!(resolve_browser(Some(&flagged.id)).is_none());
    }

    #[test]
    fn an_unknown_or_uninstalled_preference_falls_back() {
        // Resolution must not depend on what happens to be installed here, so
        // only the shape is asserted: either something was found, or the error
        // says none was.
        match resolve_terminal(Some("definitely-not-a-terminal")) {
            Ok(entry) => assert!(is_available(entry.3)),
            Err(e) => assert_eq!(e.code, Code::NotFound),
        }
    }
}
