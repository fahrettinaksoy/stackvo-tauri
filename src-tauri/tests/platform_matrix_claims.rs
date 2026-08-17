//! The measurement table in `docs/durum.md`, held to the same standard as
//! `README.md`.
//!
//! That table answers with counts — how many commands there are, how many
//! front-end files reach for Tauri, how many places the data path goes through.
//! Counts taken once and then left behind are how a document came to say 142
//! commands against 149, 47 front-end files against 95, and 32,515 lines of
//! Rust against 37,969.
//!
//! Nothing was wrong when it was written. That is the point: a number in prose
//! has no way of aging, so it stops being a measurement and becomes a memory of
//! one, and the reader cannot tell which they are looking at.
//!
//! `readme_claims.rs` makes this argument for `README.md` and
//! `architecture_claims.rs` for `ARCHITECTURE.md`. This is the same gate for
//! the status document, which absorbed the platform matrix's numbers when the
//! five documents under `docs/` became one.
//!
//! ## What is checked, and what deliberately is not
//!
//! Checked: the counts a parser can settle — commands, files, wrappers, lines,
//! and the four commands the document names as having no web meaning.
//!
//! Not checked: the four classification counts the document itself marks as
//! manual and prints the method for. Those are judgements about what code
//! *means*, and a test that pretended to settle them would be a worse lie than
//! the stale number this file exists to prevent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn document() -> String {
    read(&repo_root().join("docs/durum.md"))
}

/// Every `.js` and `.vue` under `src/`, tests excluded.
fn front_end_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![repo_root().join("src")];

    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if (name.ends_with(".js") || name.ends_with(".vue")) && !name.ends_with(".spec.js") {
                found.push(path);
            }
        }
    }

    found
}

/// The document has to say this number, in this many places.
///
/// Asserting on `contains` rather than parsing the table: the counts appear in
/// prose as well, and a document that fixed the table while leaving "142
/// commands" three paragraphs down would have passed a stricter-looking test.
/// The §1 table, which is where the machine-checked counts live.
///
/// Scoped rather than searched whole, and the reason is not hypothetical. This
/// gate used to ask `doc.contains("96")` of the entire document, so it passed
/// while the table said **95** front-end files against a tree of 96 — because
/// §7's account of an *earlier* miscount says 37.969, and "96" is in there. A
/// stale number survived the gate built to catch stale numbers, hidden by the
/// paragraph describing the last time that happened.
///
/// The second `| | Sayı |` table is the manual classification, and stays out on
/// purpose: the module doc above says why a test must not pretend to settle it.
fn measurement_table(doc: &str) -> &str {
    let start = doc
        .find("| | Sayı | Nasıl sayıldı |")
        .expect("docs/durum.md still has its §7 measurement table");
    let table = &doc[start..];
    &table[..table.find("\n\n").unwrap_or(table.len())]
}

/// Is `needle` stated as a number here, rather than buried inside a longer one?
///
/// `219` is a substring of `38.219` and `14` of `149`; a plain `contains` reads
/// either as a claim that was made. The neighbours have to be non-numeric for
/// it to count.
fn states_number(section: &str, needle: &str) -> bool {
    let numeric = |b: u8| b.is_ascii_digit() || b == b'.' || b == b',';
    let bytes = section.as_bytes();
    let mut from = 0;

    while let Some(offset) = section[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        let clear_before = start == 0 || !numeric(bytes[start - 1]);
        let clear_after = end >= bytes.len() || !numeric(bytes[end]);
        if clear_before && clear_after {
            return true;
        }
        from = start + 1;
    }
    false
}

fn assert_states(doc: &str, number: usize, what: &str) {
    let table = measurement_table(doc);
    assert!(
        states_number(table, &number.to_string()),
        "the measurement table in docs/durum.md does not state \
         {number}, which is the current count of {what}. Re-measure the \
         document — every number in it is a claim about this tree."
    );
}

#[test]
fn the_command_counts_are_the_contract_and_the_code() {
    let doc = document();

    let contract: serde_json::Value =
        serde_json::from_str(&read(&repo_root().join("contracts/ipc.json")))
            .expect("the contract is valid JSON");
    let commands = contract["commands"]
        .as_object()
        .expect("commands object")
        .len();

    assert_states(&doc, commands, "commands in the contract");

    // The Rust half of that surface. Counted the way `readme_claims.rs` counts
    // it — attribute lines outside `#[cfg(test)]` — because the document
    // distinguishes the two numbers and a reader comparing them needs both to
    // mean what they say.
    let commands_rs = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands.rs"));
    let implemented = production_regions(&commands_rs)
        .lines()
        .filter(|line| {
            line.trim_start() == "#[tauri::command]"
                || line.trim_start() == "#[tauri::command(async)]"
        })
        .count();

    assert!(
        implemented > 100,
        "only {implemented} `#[tauri::command]` attributes found — the scan has \
         stopped matching, and a scan that finds nothing agrees with any document"
    );
    assert_states(&doc, implemented, "`#[tauri::command]` functions");
}

#[test]
fn the_front_end_counts_are_the_tree() {
    let doc = document();
    let files = front_end_files();

    assert!(
        files.len() > 20,
        "only {} front-end files found",
        files.len()
    );
    assert_states(&doc, files.len(), "front-end source files");

    let with_tauri = files
        .iter()
        .filter(|path| read(path).contains("@tauri-apps"))
        .count();
    assert_states(&doc, with_tauri, "front-end files importing @tauri-apps");

    assert_states(&doc, ipc_wrapper_count(), "wrappers on the `api` object");
}

/// Members of the `api` object in `src/lib/ipc.js`.
///
/// This row of the table was the one number in it that nothing checked. It said
/// 142 against an object of 143, and every test passed — which is precisely the
/// failure the document's own §7 warns about, surviving inside the file built
/// to prevent it. The count is only meaningful because `api` is a flat object
/// literal: one member per line, two spaces in, and nothing nested.
fn ipc_wrapper_count() -> usize {
    let source = read(&repo_root().join("src/lib/ipc.js"));
    let body = source
        .split_once("export const api = {")
        .expect("ipc.js still exports an `api` object")
        .1;
    let body = &body[..body.find("\n};").expect("the object is closed")];

    body.lines()
        .filter(|line| {
            // `  name: ` at exactly one level of indentation. Doc comments and
            // the bodies of multi-line wrappers are indented further or start
            // with a comment marker.
            let Some(rest) = line.strip_prefix("  ") else {
                return false;
            };
            let mut chars = rest.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                && rest
                    .split_once(':')
                    .is_some_and(|(name, _)| name.chars().all(|c| c.is_ascii_alphanumeric()))
        })
        .count()
}

/// The document's central finding: one function is the whole data path.
///
/// This is the claim the entire web-version argument rests on — "change the
/// body of `call()` and the other 94 files do not move" — so it is checked as a
/// property of the tree rather than as a number. A second `invoke(` anywhere
/// makes the finding false, and the finding is why the document exists.
#[test]
fn invoke_appears_in_exactly_one_file() {
    let offenders: Vec<String> = front_end_files()
        .into_iter()
        .filter(|path| !path.ends_with("lib/ipc.js"))
        .filter(|path| read(path).contains("invoke("))
        .map(|path| path.display().to_string())
        .collect();

    assert!(
        offenders.is_empty(),
        "`invoke(` is supposed to appear only in src/lib/ipc.js — the whole \
         transport argument in docs/durum.md depends on it. It also \
         appears in: {offenders:?}"
    );
}

/// The four commands the document names as having no meaning in a browser.
///
/// Named rather than counted, because "roughly four of them" was the previous
/// version of this sentence and nobody could check it. If one of these is
/// renamed, the paragraph that lists them becomes wrong here rather than
/// quietly.
///
/// This checks the four are still there. [`no_fifth_command_has_quietly_become_desktop_only`]
/// checks that a fifth has not appeared, which is the direction that actually
/// goes wrong.
#[test]
fn the_desktop_only_commands_are_still_called_that() {
    let doc = document();
    let contract: serde_json::Value =
        serde_json::from_str(&read(&repo_root().join("contracts/ipc.json")))
            .expect("the contract is valid JSON");
    let commands = contract["commands"].as_object().expect("commands object");

    for name in [
        "tray_relabel",
        "window_close_action",
        "updater_status",
        "updates_check",
    ] {
        assert!(
            commands.contains_key(name),
            "docs/durum.md names `{name}` as one of the four commands a \
             web build cannot have, and the contract no longer declares it"
        );
        assert!(
            doc.contains(name),
            "`{name}` is no longer named in docs/durum.md"
        );
    }
}

#[test]
fn the_rust_source_size_is_current() {
    let doc = document();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut modules = 0;
    let mut lines = 0;
    for entry in std::fs::read_dir(&dir).expect("src/ is readable").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        modules += 1;
        lines += read(&path).lines().count();
    }

    assert_states(&doc, modules, "Rust modules");

    // Written with a thousands separator in the document, as Turkish prose
    // does: 38.219. A raw `38219` would never be found.
    let grouped = format!("{}.{:03}", lines / 1000, lines % 1000);
    assert!(
        states_number(measurement_table(&doc), &grouped),
        "the measurement table in docs/durum.md does not state \
         {grouped} lines of Rust, which is what `src-tauri/src/*.rs` holds"
    );
}

/// The size of the deletion §3 #36 is waiting to make.
///
/// In §7 rather than beside the item, and the placement is the argument §8
/// makes: §3 cannot be gated, because "not done" is not a property of the code.
/// A *count* is, and this one had already drifted — the row said "roughly half
/// of 186" for three rounds while the real figure was 150, four fifths. The
/// difference is the difference between a tidy-up and a fifth of the constant
/// table, and it is exactly the kind of number that only stays right if
/// something recomputes it.
#[test]
fn the_two_halves_of_the_embedded_defaults_are_current() {
    let doc = document();

    assert_states(
        &doc,
        stackvo_desktop_lib::config::SETTINGS.len(),
        "embedded settings that stay",
    );
    assert_states(
        &doc,
        stackvo_desktop_lib::config::LEGACY_SERVICES.len(),
        "embedded defaults that exist only for the migration",
    );
    assert_states(
        &doc,
        stackvo_desktop_lib::config::EMBEDDED.len(),
        "embedded defaults in total",
    );
}

/// The source with every top-level `#[cfg(test)]` item removed.
///
/// The same indentation-based scan as `readme_claims.rs` and
/// `privacy_claims.rs`, for the same reason: brace counting breaks on a test
/// that writes an unmatched `{` inside a string literal, while `cargo fmt
/// --check` guarantees a top-level item closes with a `}` in column zero.
fn production_regions(src: &str) -> String {
    let mut kept = String::with_capacity(src.len());
    let mut from = 0;

    while let Some(offset) = src[from..].find("\n#[cfg(test)]") {
        let start = from + offset + 1;
        kept.push_str(&src[from..start]);
        match src[start..].find("\n}\n") {
            Some(end) => from = start + end + 3,
            None => return kept,
        }
    }

    kept.push_str(&src[from..]);
    kept
}

/// The desktop-only set is derived, not remembered. §3 #34.
///
/// The test above holds the four names the document lists. It cannot fail when
/// a **fifth** appears, and that is the only direction this ever goes: somebody
/// adds a command that relabels the tray or drives the window, and §7's
/// paragraph — the one a web version would be planned from — quietly becomes a
/// list of four out of five.
///
/// So the set is computed. A command has no web counterpart when it reaches the
/// tray, the updater or a window, either in its own body or through a helper in
/// this same file: `window_close_action` does nothing itself and hands off to
/// `apply_close`, so a scan that did not follow one call would have missed it.
///
/// ## Why three and not four
///
/// `updates_check` is one of the contract's three `frontend-plugin` commands —
/// it is `tauri-plugin-updater`'s, not this crate's, so it has no
/// `#[tauri::command]` here to find. Asserting it separately would be asserting
/// that a dependency still exists; §7 names it because a *reader* planning a
/// web build needs to know about it, and that is prose rather than a scan.
///
/// ## Comments are stripped first, and the first version did not
///
/// It reported `locale_get`, which does nothing of the kind: it calls
/// `preferred_locale`, whose comment explains that the tray reads the same
/// preference. Same lesson as `policy_claims.rs` — a gate that reads prose is
/// answering a question about the documentation.
#[test]
fn no_fifth_command_has_quietly_become_desktop_only() {
    let source = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands.rs"));
    let source = production_regions(&source);
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    // Every top-level function in the file, by name, so a command that hands
    // off can be followed one step.
    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    for (offset, _) in code
        .match_indices("\nfn ")
        .chain(code.match_indices(" fn "))
    {
        let after = &code[offset..];
        let Some(rest) = after.split_once("fn ").map(|(_, rest)| rest) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let start = offset + (after.len() - rest.len());
        let end = code[start..]
            .find("\n}\n")
            .map(|i| start + i)
            .unwrap_or(code.len());
        bodies
            .entry(name)
            .or_insert_with(|| code[start..end].to_string());
    }

    /// The window, the tray and the updater: the three things a browser tab is
    /// not. Spelled as they appear in this crate rather than as Tauri names
    /// them, because a command reaches them through this app's own modules.
    const DESKTOP: [&str; 6] = [
        "tray",
        "updater",
        "Updater",
        "TrayIcon",
        "WebviewWindow",
        "get_webview_window",
    ];

    fn reaches(name: &str, bodies: &BTreeMap<String, String>, seen: &mut BTreeSet<String>) -> bool {
        if !seen.insert(name.to_string()) {
            return false;
        }
        let Some(body) = bodies.get(name) else {
            return false;
        };
        if DESKTOP.iter().any(|needle| body.contains(needle)) {
            return true;
        }
        // One call deep, and then as deep as that goes. Cheap because the set
        // of functions in one file is small and `seen` stops the cycles.
        bodies
            .keys()
            .filter(|callee| callee.as_str() != name && body.contains(&format!("{callee}(")))
            .any(|callee| reaches(callee, bodies, seen))
    }

    let mut found = BTreeSet::new();
    for chunk in code.split("\n#[tauri::command").skip(1) {
        let Some(rest) = chunk.split_once("fn ").map(|(_, rest)| rest) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if reaches(&name, &bodies, &mut BTreeSet::new()) {
            found.insert(name);
        }
    }

    let declared: BTreeSet<String> = ["tray_relabel", "updater_status", "window_close_action"]
        .into_iter()
        .map(str::to_string)
        .collect();

    assert_eq!(
        found, declared,
        "the set of commands that reach the tray, the window or the updater has \
         changed. §7 of docs/durum.md lists the commands a web build cannot \
         have, and that list is what anybody planning §3 #34 reads — add the \
         name there and here, or find out why this one needs a window."
    );
}
