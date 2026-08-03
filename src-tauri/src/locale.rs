//! Which language the app opens in.
//!
//! Two surfaces speak it — the window and the tray — and until now they worked
//! it out separately and disagreed. The front end read `localStorage` and fell
//! back to `navigator.language`; the tray read `preferences.json` and fell back
//! to `$LANG`. Neither fallback is the machine's language:
//!
//! - `$LANG` is set by a login shell. An app launched from Finder or the Dock
//!   has no login shell in its ancestry, so on macOS it is simply absent — and
//!   the tray came up English on a Turkish machine, every time, for everyone.
//! - `navigator.language` in a WKWebView answers from the *bundle's* localised
//!   resources, which this app has none of. It is not a reading of the system
//!   setting; it just usually resembles one.
//!
//! So the order is one order, decided here, and both surfaces ask for it:
//!
//! 1. what the user chose, from `preferences.json` — a choice outlives every
//!    guess, which is the whole point of having made it;
//! 2. what the OS says, read from the OS rather than from the environment;
//! 3. English.
//!
//! Step 2 spawns a process on macOS and Windows. That is affordable because it
//! only ever runs when step 1 came up empty — which is the first launch, once.

/// The languages this app actually has strings for.
///
/// A tag it cannot serve must not be returned: `pt-BR` resolving to `pt` would
/// leave vue-i18n falling back key by key, which renders as an English UI with
/// a Turkish menu rather than as an honest English one.
const SUPPORTED: [&str; 2] = ["en", "tr"];

/// The language part of a BCP 47 tag, when this app speaks it.
///
/// `tr_TR.UTF-8`, `tr-TR`, `tr` and `TR` all mean Turkish. Underscore because
/// that is what POSIX locales use and what `$LANG` therefore contains; the
/// codeset suffix because `$LANG` carries that too.
pub fn normalise(raw: &str) -> Option<&'static str> {
    let tag = raw
        .trim()
        .split(['.', '@']) // strip `.UTF-8`, `@euro`
        .next()
        .unwrap_or("")
        .split(['-', '_']) // `tr-TR` → `tr`
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    SUPPORTED.into_iter().find(|s| *s == tag)
}

/// What the operating system says the user's language is.
///
/// Deliberately not `$LANG` first. It is consulted last, and only because on
/// Linux it *is* the mechanism — there `LC_ALL`/`LANG` is what every other
/// program reads, and a desktop session sets it.
pub fn system() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        // `AppleLocale` is the region-formatted locale ("tr_TR"); `AppleLanguages`
        // is the ordered preference list ("(\n    tr-TR,\n    en-GB\n)") and is
        // the one that actually changes when someone reorders languages in
        // System Settings. First readable answer wins.
        if let Some(found) = defaults("AppleLanguages").as_deref().and_then(first_tag) {
            return Some(found);
        }
        if let Some(found) = defaults("AppleLocale").as_deref().and_then(normalise) {
            return Some(found);
        }
    }

    #[cfg(target_os = "windows")]
    {
        // The user's display language, not the machine's. `reg query` rather
        // than a crate: this is one value, read once, on the first launch only.
        if let Some(found) = registry_locale().as_deref().and_then(normalise) {
            return Some(found);
        }
    }

    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find_map(|v| normalise(&v))
}

/// The first tag in a `defaults`-printed array, normalised.
///
/// The output is a plist array across several lines:
/// `(\n    "tr-TR",\n    "en-GB"\n)`. Only the head matters — it is the
/// language the user put first.
#[cfg(any(target_os = "macos", test))]
fn first_tag(printed: &str) -> Option<&'static str> {
    printed
        .lines()
        .map(|l| l.trim().trim_end_matches(',').trim_matches('"'))
        .filter(|l| !l.is_empty() && *l != "(" && *l != ")")
        .find_map(normalise)
}

#[cfg(target_os = "macos")]
fn defaults(key: &str) -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", key])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(target_os = "windows")]
fn registry_locale() -> Option<String> {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Control Panel\International",
            "/v",
            "LocaleName",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // `    LocaleName    REG_SZ    tr-TR`
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("LocaleName"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

/// The language to open in: the user's choice, then the machine's, then
/// English.
///
/// `stored` is passed in rather than read so this stays a pure function of the
/// two inputs — the preference file is the caller's business, and a resolution
/// order is worth testing without one.
pub fn resolve(stored: Option<&str>) -> &'static str {
    stored
        .and_then(normalise)
        .or_else(system)
        .unwrap_or(SUPPORTED[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_a_locale_arrives_in_reads_as_turkish() {
        for raw in ["tr", "TR", "tr-TR", "tr_TR", "tr_TR.UTF-8", "  tr-tr  "] {
            assert_eq!(normalise(raw), Some("tr"), "{raw} was not read as Turkish");
        }
    }

    /// The bug this prevents: a near-miss resolving to a language with no
    /// strings, which renders as a half-translated window rather than as
    /// English.
    #[test]
    fn a_language_this_app_does_not_speak_is_not_invented() {
        for raw in ["de", "pt-BR", "fr_FR.UTF-8", "", "   ", "C", "POSIX"] {
            assert_eq!(normalise(raw), None, "{raw} was accepted");
        }
    }

    #[test]
    fn a_stored_choice_outranks_everything_else() {
        assert_eq!(resolve(Some("tr")), "tr");
        assert_eq!(resolve(Some("en")), "en");
    }

    /// A preference file can hold a language a later build dropped, or an empty
    /// string from a bad write. Neither is a reason to fail — they fall through
    /// to detection exactly as "nothing stored" does.
    #[test]
    fn an_unusable_stored_value_falls_through_rather_than_sticking() {
        for stored in [Some(""), Some("de"), Some("zz-ZZ"), None] {
            let out = resolve(stored);
            assert!(
                SUPPORTED.contains(&out),
                "{stored:?} resolved to {out}, which has no strings"
            );
        }
    }

    /// macOS prints `AppleLanguages` as a plist array, and the head of it is
    /// the answer. Parsed here rather than trusted, because the shape is a
    /// command's output and this is the only place that reads it.
    #[test]
    fn the_head_of_the_macos_language_list_wins() {
        let printed = "(\n    \"tr-TR\",\n    \"en-GB\"\n)\n";
        assert_eq!(first_tag(printed), Some("tr"));

        // An unsupported first choice is not a reason to answer nothing: the
        // list is ordered, and English second is still a real preference.
        let printed = "(\n    \"de-DE\",\n    \"en-GB\"\n)\n";
        assert_eq!(first_tag(printed), Some("en"));

        assert_eq!(first_tag("(\n)\n"), None);
    }

    /// Detection must never answer something the app cannot render, whatever
    /// this machine is set to.
    #[test]
    fn detection_only_ever_answers_a_language_with_strings() {
        if let Some(found) = system() {
            assert!(SUPPORTED.contains(&found), "{found} has no strings");
        }
    }
}
