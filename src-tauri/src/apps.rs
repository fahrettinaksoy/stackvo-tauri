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
//!
//! The database clients at the bottom arrived the same way and for the same
//! reason. `connect.rs` had been producing the correct URI and offering to copy
//! it since it was written; what nobody had written was the twenty lines that
//! hand that string to the application it was built for.

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

// ------------------------------------------------------- database clients

/// Desktop database clients worth offering, by bundle.
///
/// Only the identity is in this table. Which *protocols* an entry can open is
/// deliberately absent, and asking the application itself instead is the whole
/// design: this machine has Redis Insight installed, and Redis Insight declares
/// exactly one URL scheme — `redisinsight`. A hand-written table would have put
/// it beside `redis://` on the strength of its name, and the result would be a
/// button that launches an application which then ignores the address it was
/// given. `mdi-` icons and display names cannot be got wrong that way; scheme
/// support can, so it is read rather than claimed.
///
/// macOS only, and the reason is not laziness about the other two. A bundle
/// carries `CFBundleURLTypes`, so what it opens is answerable from disk;
/// Windows keeps the same fact in the registry under a different key per app
/// and Linux in `.desktop` files spread over three directories. On those the
/// system handler below is the whole offer, which is still the thing G-3 was
/// missing — something that opens.
#[cfg(target_os = "macos")]
const DB_CLIENTS: &[(&str, &str, &str)] = &[
    // (id, display name, bundle)
    ("tableplus", "TablePlus", "/Applications/TablePlus.app"),
    ("dbeaver", "DBeaver", "/Applications/DBeaver.app"),
    ("datagrip", "DataGrip", "/Applications/DataGrip.app"),
    (
        "compass",
        "MongoDB Compass",
        "/Applications/MongoDB Compass.app",
    ),
    ("sequel-ace", "Sequel Ace", "/Applications/Sequel Ace.app"),
    (
        "beekeeper",
        "Beekeeper Studio",
        "/Applications/Beekeeper Studio.app",
    ),
    ("postico", "Postico", "/Applications/Postico 2.app"),
    (
        "redis-insight",
        "Redis Insight",
        "/Applications/Redis Insight.app",
    ),
    ("medis", "Medis", "/Applications/Medis.app"),
];

#[cfg(not(target_os = "macos"))]
const DB_CLIENTS: &[(&str, &str, &str)] = &[];

/// The spellings an application may register for one of our schemes.
///
/// `connect::scheme_of` names a protocol once, for a manifest; the desktop world
/// names the same protocol more than once. Postgres is `postgres` about as often
/// as `postgresql`, and MariaDB clients register `mariadb` for what this app
/// calls `mysql`. Matching on the exact string would hide a client that handles
/// the service perfectly well.
fn aliases(scheme: &str) -> &'static [&'static str] {
    match scheme {
        "mysql" => &["mysql", "mariadb"],
        "postgresql" => &["postgresql", "postgres"],
        "mongodb" => &["mongodb", "mongodb+srv"],
        "redis" => &["redis", "rediss"],
        _ => &[],
    }
}

/// The URL schemes an installed bundle says it opens.
///
/// `defaults` rather than a plist crate: an `Info.plist` inside a bundle is
/// usually the binary form, `defaults read` is the reader macOS ships for it,
/// and the alternative is a dependency to answer a question asked about at most
/// nine paths on one platform.
///
/// The output is old-style plist text and what is wanted from it is a set of
/// bare words, so it is scanned for tokens rather than parsed. A scheme name is
/// matched whole — `redis` must not match `redisinsight`, which is the exact
/// pair this function was written against.
#[cfg(target_os = "macos")]
fn declared_schemes(bundle: &str) -> Vec<String> {
    let Ok(out) = std::process::Command::new("defaults")
        .arg("read")
        .arg(format!("{bundle}/Contents/Info"))
        .arg("CFBundleURLTypes")
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&out.stdout)
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn declared_schemes(_bundle: &str) -> Vec<String> {
    Vec::new()
}

/// Every client that can open `scheme`, installed or not, plus the system
/// handler.
///
/// "Installed or not" is the same rule the editor list follows — a greyed row
/// says this app knows about TablePlus, an absent one says it does not. But an
/// installed client that does not declare the scheme is left out altogether
/// rather than greyed: it is not missing, it is not applicable, and a Redis
/// Insight greyed out under a MySQL service would read as a broken install.
///
/// The system handler heads the list and is always available, exactly as the
/// system default browser does, and for the same reason: it is the absence of a
/// choice rather than a choice, and something answers a registered scheme.
pub fn db_clients(scheme: &str) -> Vec<App> {
    let wanted = aliases(scheme);
    if wanted.is_empty() {
        // Not a protocol a desktop client opens — AMQP, SMTP, a bare host and
        // port. The caller shows nothing rather than an empty picker.
        return Vec::new();
    }

    let mut apps = vec![App {
        id: String::new(),
        name: "System default".to_string(),
        icon: "mdi-open-in-app".to_string(),
        available: true,
        default: false,
    }];

    for (id, name, bundle) in DB_CLIENTS {
        if !std::path::Path::new(bundle).exists() {
            apps.push(App {
                id: (*id).to_string(),
                name: (*name).to_string(),
                icon: "mdi-database".to_string(),
                available: false,
                default: false,
            });
            continue;
        }
        let declared = declared_schemes(bundle);
        if !wanted.iter().any(|w| declared.iter().any(|d| d == w)) {
            continue;
        }
        apps.push(App {
            id: (*id).to_string(),
            name: (*name).to_string(),
            icon: "mdi-database".to_string(),
            available: true,
            default: false,
        });
    }

    mark_default(apps)
}

/// How to hand a URI to `id`, or `None` when nothing there can take it.
///
/// The empty id is the system handler and resolves to `None` on purpose — the
/// caller's fallback is already "let the OS decide", which is the same thing,
/// and giving it a `Launch` would mean two code paths for one behaviour.
pub fn resolve_db_client(id: &str) -> Option<Launch> {
    let entry = DB_CLIENTS.iter().find(|(i, ..)| *i == id)?;
    std::path::Path::new(entry.2)
        .exists()
        .then_some(Launch::Bundle(entry.2))
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
            DB_CLIENTS.iter().map(|(id, ..)| *id).collect::<Vec<_>>(),
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

    /// The bug the design exists to avoid, held as an assertion rather than a
    /// comment: `redis` must not match `redisinsight`. Whole-token matching is
    /// what separates a client that opens the address from one that launches
    /// and ignores it.
    #[test]
    fn a_scheme_is_matched_whole_and_not_as_a_prefix() {
        let declared = ["redisinsight".to_string()];
        let wanted = aliases("redis");
        assert!(
            !wanted.iter().any(|w| declared.iter().any(|d| d == w)),
            "redisinsight is not redis"
        );

        let declared = ["redis".to_string()];
        assert!(wanted.iter().any(|w| declared.iter().any(|d| d == w)));
    }

    /// The two spellings of Postgres and of MySQL/MariaDB both resolve, because
    /// a client registering the other one handles the service perfectly well.
    #[test]
    fn a_protocol_is_recognised_under_the_names_clients_actually_register() {
        assert!(aliases("postgresql").contains(&"postgres"));
        assert!(aliases("mysql").contains(&"mariadb"));
        assert!(aliases("mongodb").contains(&"mongodb+srv"));
    }

    /// A protocol no desktop client opens produces no picker at all, rather
    /// than one containing only the system handler — the caller keys the whole
    /// button on this list being non-empty.
    #[test]
    fn a_protocol_without_desktop_clients_offers_nothing() {
        for scheme in ["amqp", "smtp", "http", "host-port"] {
            assert!(
                db_clients(scheme).is_empty(),
                "{scheme} should offer no client"
            );
        }
    }

    /// Every scheme this list keys on has to be one `connect` can actually
    /// produce, or the picker is wired to a vocabulary nothing speaks.
    #[test]
    fn the_schemes_offered_are_ones_connect_names() {
        use crate::connect;
        let produced: Vec<&str> = [
            connect::Kind::Mysql,
            connect::Kind::Postgres,
            connect::Kind::Mongo,
            connect::Kind::Redis,
            connect::Kind::Memcached,
            connect::Kind::Amqp,
            connect::Kind::Http,
            connect::Kind::HostPort,
            connect::Kind::Smtp,
        ]
        .into_iter()
        .map(connect::scheme_of)
        .collect();

        for scheme in ["mysql", "postgresql", "mongodb", "redis"] {
            assert!(
                produced.contains(&scheme),
                "{scheme} is not a scheme connect::scheme_of returns"
            );
            assert!(!db_clients(scheme).is_empty());
        }
    }

    /// The system handler is always there and always first, so a machine with
    /// no client installed still has something to click.
    #[test]
    fn the_system_handler_heads_every_non_empty_list() {
        for scheme in ["mysql", "postgresql", "mongodb", "redis"] {
            let list = db_clients(scheme);
            assert_eq!(list[0].id, "", "{scheme}: the system handler comes first");
            assert!(list[0].available);
            assert!(list[0].default, "and is what an unset preference means");
        }
    }

    /// Read against the machine rather than against the table: whatever is
    /// installed here must agree with what `resolve_db_client` will launch.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_offered_client_is_one_that_can_be_launched() {
        for scheme in ["mysql", "postgresql", "mongodb", "redis"] {
            for app in db_clients(scheme).into_iter().filter(|a| a.available) {
                if app.id.is_empty() {
                    continue; // the system handler has no bundle
                }
                assert!(
                    resolve_db_client(&app.id).is_some(),
                    "{} is offered for {scheme} but resolves to nothing",
                    app.id
                );
            }
        }
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
