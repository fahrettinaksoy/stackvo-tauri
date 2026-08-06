//! The hint catalogue and the locale files, held equal.
//!
//! `src/hints.rs` declares every suggestion this app makes; `src/i18n/locales/`
//! translates them. Nothing joins the two — one is Rust, the other is
//! JavaScript, and they are read by different runtimes at different times. That
//! is the shape of gap this project keeps closing with a test rather than a
//! convention, for the reason `readme_claims.rs` exists: a number or a key that
//! is only correct because somebody remembered is correct until they do not.
//!
//! Three failures, all silent without this:
//!
//!   * a hint added to the catalogue and never translated — the Turkish user
//!     gets English, which is the exact defect this whole change removed;
//!   * a hint deleted from the catalogue and left in the locales — dead weight
//!     that reads as coverage;
//!   * the English drifting between `hints.rs` and `en.js` — the fallback and
//!     the translation quietly saying different things, and no user ever seeing
//!     both to notice.
//!
//! ## Why the locale files are parsed rather than imported
//!
//! They are ES modules and this is a Rust test. Running `node` to print them as
//! JSON would work and would make `cargo test` need a Node toolchain — which
//! `cargo test` does not need today and should not start needing for this. The
//! `errorHints` block is generated in a fixed shape (`key: '…',` one per line),
//! so a narrow reader over exactly that block is enough, and it fails loudly
//! rather than silently if the shape ever changes: `no_keys_parsed` below.

use std::collections::{BTreeMap, BTreeSet};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("a repository root above src-tauri")
        .to_path_buf()
}

/// `key: '…'` pairs out of an object body, whatever Prettier did to the layout.
///
/// Deliberately not line-based, which is what this was first and why it broke:
/// `prettier --write` runs on every commit and moves a value that overruns the
/// print width onto its own line. A line reader then found twelve fewer keys and
/// reported them as missing translations — a test failing for a reason that had
/// nothing to do with what it checks. Scanning for the *pair* rather than the
/// *line* is indifferent to where the newlines land.
fn parse_entries(body: &str) -> BTreeMap<String, String> {
    let bytes: Vec<char> = body.chars().collect();
    let mut out = BTreeMap::new();
    let mut i = 0;

    while i < bytes.len() {
        // An identifier followed by a colon.
        if !(bytes[i].is_ascii_alphabetic() || bytes[i] == '_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '_') {
            i += 1;
        }
        let key: String = bytes[start..i].iter().collect();

        // Whitespace, then a colon, then whitespace, then a quote. Anything
        // else and this was not an entry — a word inside a comment, most often.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != ':' {
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_whitespace() {
            j += 1;
        }
        // Either quote. Prettier picks whichever needs fewer escapes, so
        // `"Edit it from the project's Manifest tab instead."` is double-quoted
        // while its neighbours are single-quoted — a reader that knew only one
        // style dropped exactly the entries with an apostrophe in them.
        if j >= bytes.len() || (bytes[j] != '\'' && bytes[j] != '"') {
            continue;
        }
        let quote = bytes[j];
        j += 1;

        // The value, honouring backslash escapes so a `\'` does not end it.
        let mut value = String::new();
        while j < bytes.len() && bytes[j] != quote {
            if bytes[j] == '\\' && j + 1 < bytes.len() {
                j += 1;
            }
            value.push(bytes[j]);
            j += 1;
        }
        if j >= bytes.len() {
            break; // unterminated string; the caller's emptiness check reports it
        }

        out.insert(key, value);
        i = j + 1;
    }
    out
}

/// The `errorHints` block of a locale file, as key → text.
fn locale_hints(locale: &str) -> BTreeMap<String, String> {
    let path = repo_root()
        .join("src/i18n/locales")
        .join(format!("{locale}.js"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

    let start = text
        .find("\n  errorHints: {")
        .unwrap_or_else(|| panic!("{} has no errorHints block", path.display()));
    let body = &text[start..];
    let end = body
        .find("\n  },")
        .unwrap_or_else(|| panic!("{} has an unterminated errorHints block", path.display()));
    let body = &body[..end];

    let out = parse_entries(body);

    assert!(
        !out.is_empty(),
        "no keys parsed out of {} — the block's shape changed and this test \
         would otherwise pass by finding nothing to disagree with",
        path.display()
    );
    out
}

fn catalogue() -> BTreeMap<String, String> {
    stackvo_desktop_lib::hints::ALL
        .iter()
        .map(|h| (h.key.to_string(), h.english.to_string()))
        .collect()
}

/// The failure this whole change exists to prevent, in test form.
#[test]
fn every_hint_is_translated_in_every_locale() {
    let expected: BTreeSet<String> = catalogue().keys().cloned().collect();

    for locale in ["en", "tr"] {
        let actual: BTreeSet<String> = locale_hints(locale).keys().cloned().collect();

        let missing: Vec<&String> = expected.difference(&actual).collect();
        assert!(
            missing.is_empty(),
            "{locale}.js is missing {} hint translation(s): {missing:?}\n\
             A user in this locale would be shown English for these.",
            missing.len()
        );

        let stale: Vec<&String> = actual.difference(&expected).collect();
        assert!(
            stale.is_empty(),
            "{locale}.js translates {} hint(s) nothing raises any more: {stale:?}\n\
             Delete them — a translation with no hint reads as coverage that is not there.",
            stale.len()
        );
    }
}

/// The fallback and the English translation are the same sentence.
///
/// They are two copies by necessity — Rust cannot read the locale file and the
/// webview cannot read the catalogue — so the only question is whether anything
/// notices when they diverge. This does.
#[test]
fn the_english_locale_matches_the_catalogue_word_for_word() {
    let catalogue = catalogue();
    let english = locale_hints("en");

    let mut drifted = Vec::new();
    for (key, rust) in &catalogue {
        let Some(js) = english.get(key) else {
            continue; // reported by the test above, with a better message
        };
        if rust != js {
            drifted.push(format!("  {key}\n    hints.rs: {rust}\n    en.js:    {js}"));
        }
    }

    assert!(
        drifted.is_empty(),
        "{} hint(s) say different things in Rust and in en.js:\n{}",
        drifted.len(),
        drifted.join("\n")
    );
}

/// A translation that is the English text is almost always a placeholder
/// somebody meant to come back to. A handful are legitimately identical — a
/// bare list of database names has nothing to translate — so this reports
/// rather than forbids, and the threshold is what makes it useful.
#[test]
fn the_turkish_locale_is_actually_translated() {
    let english = locale_hints("en");
    let turkish = locale_hints("tr");

    let untranslated: Vec<&String> = turkish
        .iter()
        .filter(|(key, tr)| english.get(*key).is_some_and(|en| en == *tr))
        .map(|(key, _)| key)
        .collect();

    assert!(
        untranslated.len() <= 2,
        "{} Turkish hints are identical to the English: {untranslated:?}",
        untranslated.len()
    );
}

/// Every hint in the catalogue is reachable — a `Hint` const nothing passes to
/// `with_hint` is a translation being maintained for a message no user sees.
#[test]
fn every_hint_in_the_catalogue_is_actually_raised() {
    let src = repo_root().join("src-tauri/src");
    let mut used = BTreeSet::new();

    for entry in std::fs::read_dir(&src).expect("src/").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|n| n != "hints.rs")
        {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for hint in stackvo_desktop_lib::hints::ALL {
                // The constant's Rust name, which is what a call site writes.
                let name = constant_name(hint.key);
                if text.contains(&format!("hints::{name}")) {
                    used.insert(hint.key);
                }
            }
        }
    }

    let unused: Vec<&str> = stackvo_desktop_lib::hints::ALL
        .iter()
        .map(|h| h.key)
        .filter(|k| !used.contains(k))
        .collect();

    assert!(
        unused.is_empty(),
        "{} hint(s) are declared and never raised: {unused:?}",
        unused.len()
    );
}

/// `startDockerOrSetHost` → `START_DOCKER_OR_SET_HOST`.
fn constant_name(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}
