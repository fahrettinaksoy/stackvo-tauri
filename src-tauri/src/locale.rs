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

// ------------------------------------------------------- language packs

/// A language this app was not shipped with (M-7).
///
/// ## Why a file and not a third `locales/*.js`
///
/// "The app speaks N languages" is not a code problem, and until now it was
/// one: adding a third meant a new source file, a new entry in `SUPPORTED`
/// here, a new branch in the tray's fallback table and a rebuild. Nobody who
/// can actually translate this app can do any of that, which is why the item
/// sat on the list as "~2,000 strings" — the strings were never the blocker,
/// the rebuild was.
///
/// A pack is one JSON file with the same shape as `src/i18n/locales/en.js`,
/// dropped in the app's config directory. It is discovered at startup and
/// listed beside the two built-in languages.
///
/// ## Partial packs are the normal case, and they work
///
/// vue-i18n falls back to English key by key, so a pack that covers half the
/// app renders half in that language and half in English. That is deliberately
/// not treated as an error: the alternative is refusing a pack until it is
/// complete, which means nobody can ever start one. What the settings pane does
/// instead is **say how much of it is translated**, so a half-finished pack
/// looks half-finished rather than broken.
///
/// ## What is not here
///
/// No machine translation, and no seeding a new pack with English strings
/// silently relabelled as another language. A missing string that falls back to
/// English is honest; a fabricated one is a sentence somebody has to find and
/// disbelieve.
pub const PACK_DIR: &str = "locales";

/// Where packs live: `<config>/locales/`.
pub fn packs_dir() -> Option<std::path::PathBuf> {
    crate::appdir::config().map(|dir| dir.join(PACK_DIR))
}

/// A tag that can be a file name and a BCP 47 language.
///
/// Checked because the tag becomes a path segment. `../../etc/passwd` as a
/// locale is not a language somebody speaks, and a pack directory is a place
/// this app writes into.
pub fn is_valid_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    if !(2..=3).contains(&language.len()) || !language.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(region) => {
            parts.next().is_none()
                && (2..=8).contains(&region.len())
                && region.chars().all(|c| c.is_ascii_alphanumeric())
        }
    }
}

/// One installed pack, as the settings pane lists it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub tag: String,
    /// What to call it in the picker. From the pack's own `language.label`, or
    /// the tag when it does not say — a pack has to name itself, because this
    /// app cannot hold a name for a language it has never heard of.
    pub label: String,
    pub path: String,
    /// How many leaf strings it carries. The share of the app it covers is the
    /// front end's arithmetic — it is the side that holds the English catalogue.
    pub strings: usize,
    /// Set when the file is on disk and unreadable as JSON, so a typo in a
    /// hand-edited pack is reported rather than silently ignored.
    pub broken: Option<String>,
}

fn count_strings(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(_) => 1,
        serde_json::Value::Object(map) => map.values().map(count_strings).sum(),
        _ => 0,
    }
}

/// Every pack on this machine, in tag order.
pub fn packs() -> Vec<Pack> {
    let Some(dir) = packs_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // A file called `../evil.json` cannot exist, but one called `English
        // (draft).json` can, and it is not a language tag.
        if !is_valid_tag(tag) {
            continue;
        }

        let (label, strings, broken) = match std::fs::read_to_string(&path)
            .ok()
            .map(|text| serde_json::from_str::<serde_json::Value>(&text))
        {
            Some(Ok(value)) => (
                value
                    .get("language")
                    .and_then(|l| l.get("label"))
                    .and_then(|l| l.as_str())
                    .unwrap_or(tag)
                    .to_string(),
                count_strings(&value),
                None,
            ),
            Some(Err(e)) => (tag.to_string(), 0, Some(e.to_string())),
            None => (tag.to_string(), 0, Some("could not be read".to_string())),
        };

        out.push(Pack {
            tag: tag.to_string(),
            label,
            path: path.display().to_string(),
            strings,
            broken,
        });
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    out
}

/// One pack's messages.
pub fn read_pack(tag: &str) -> crate::error::Result<serde_json::Value> {
    use crate::error::{Code, Error};
    if !is_valid_tag(tag) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{tag}\" is not a language tag"),
        ));
    }
    let path = packs_dir()
        .ok_or_else(|| Error::new(Code::IoError, "no config directory"))?
        .join(format!("{tag}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;
    serde_json::from_str(&text).map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("{} is not valid JSON: {e}", path.display()),
        )
    })
}

/// Write a pack, creating the directory the first time.
pub fn write_pack(tag: &str, messages: &serde_json::Value) -> crate::error::Result<String> {
    use crate::error::{Code, Error};
    if !is_valid_tag(tag) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{tag}\" is not a language tag"),
        ));
    }
    let dir = packs_dir().ok_or_else(|| Error::new(Code::IoError, "no config directory"))?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(format!("making {}", dir.display()), e))?;

    let path = dir.join(format!("{tag}.json"));
    let text = serde_json::to_string_pretty(messages)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the pack: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))?;
    Ok(path.display().to_string())
}

/// Remove a pack. Removing one that is not there is success — the caller is
/// asking for it to be gone, and it is.
pub fn delete_pack(tag: &str) -> crate::error::Result<()> {
    use crate::error::{Code, Error};
    if !is_valid_tag(tag) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{tag}\" is not a language tag"),
        ));
    }
    let Some(dir) = packs_dir() else {
        return Ok(());
    };
    let path = dir.join(format!("{tag}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(format!("removing {}", path.display()), e)),
    }
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

    /// The tag becomes a file name in a directory this app writes into.
    #[test]
    fn a_pack_tag_is_a_language_and_not_a_path() {
        for good in ["de", "fr", "pt-BR", "zh-Hans", "nb", "fil"] {
            assert!(is_valid_tag(good), "{good} was refused");
        }
        for bad in [
            "",
            "e",
            "english",
            "../etc/passwd",
            "de/../..",
            "DE",
            "de-",
            "de-DE-x",
            "de_DE",
            ".",
            "..",
        ] {
            assert!(!is_valid_tag(bad), "{bad} was accepted");
        }
    }

    /// The count is of leaf strings, not of keys: a pack's progress is how many
    /// sentences it has, and counting objects would make a deeply nested but
    /// empty file look like progress.
    #[test]
    fn a_pack_is_measured_in_sentences() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"language":{"label":"Deutsch"},"app":{"title":"StackVo","sub":{"a":"x","b":"y"}},"n":3}"#,
        )
        .unwrap();
        // label, title, a, b — the number is not a string.
        assert_eq!(count_strings(&value), 4);
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
