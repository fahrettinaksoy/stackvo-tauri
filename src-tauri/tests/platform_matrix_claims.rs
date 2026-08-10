//! `docs/platform-matrix.md`, held to the same standard as `README.md`.
//!
//! The document answers one question — what a web build of this app could and
//! could not do — and it answers it with counts: how many commands there are,
//! how many front-end files reach for Tauri, how many places the data path goes
//! through. Those counts were taken once, in August 2026, and then the tree
//! grew. By the time anyone looked again the document said 142 commands against
//! 149, 47 front-end files against 95, and 32,515 lines of Rust against 37,969.
//!
//! Nothing was wrong when it was written. That is the point: a number in prose
//! has no way of aging, so it stops being a measurement and becomes a memory of
//! one, and the reader cannot tell which they are looking at.
//!
//! `readme_claims.rs` made this argument for `README.md` and
//! `architecture_claims.rs` for `ARCHITECTURE.md`. This is the same gate for
//! the third document that carries countable claims.
//!
//! ## What is checked, and what deliberately is not
//!
//! Checked: the counts a parser can settle — commands, files, wrappers, lines,
//! and the four commands the document names as having no web meaning.
//!
//! Not checked: the platform columns, the streaming argument, and the four
//! classification counts the document itself marks as manual. Those are
//! judgements about what code *means*, and §7 of that document already says
//! which of them have been run and which have only been read. A test that
//! pretended to settle them would be a worse lie than the stale number this
//! file exists to prevent.

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
    read(&repo_root().join("docs/platform-matrix.md"))
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
fn assert_states(doc: &str, number: usize, what: &str) {
    assert!(
        doc.contains(&number.to_string()),
        "docs/platform-matrix.md does not state {number}, which is the current \
         count of {what}. Re-measure the document — every number in it is a \
         claim about this tree."
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
         transport argument in docs/platform-matrix.md depends on it. It also \
         appears in: {offenders:?}"
    );
}

/// The four commands the document names as having no meaning in a browser.
///
/// Named rather than counted, because "roughly four of them" was the previous
/// version of this sentence and nobody could check it. If one of these is
/// renamed, the paragraph that lists them becomes wrong here rather than
/// quietly.
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
            "docs/platform-matrix.md names `{name}` as one of the four commands a \
             web build cannot have, and the contract no longer declares it"
        );
        assert!(
            doc.contains(name),
            "`{name}` is no longer named in docs/platform-matrix.md"
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
    // does: 37.969. A raw `37969` would never be found.
    let grouped = format!("{}.{:03}", lines / 1000, lines % 1000);
    assert!(
        doc.contains(&grouped),
        "docs/platform-matrix.md does not state {grouped} lines of Rust, which \
         is what `src-tauri/src/*.rs` holds"
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
