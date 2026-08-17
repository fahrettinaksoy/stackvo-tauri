//! What has to change on the day `LEGACY_SERVICES` is deleted.
//!
//! §3 #36 of `docs/durum.md` is not blocked on code. `config::LEGACY_SERVICES`
//! — 150 of the 186 embedded defaults — exists so [`handover`] can read a
//! pre-market `.env` and turn `SERVICE_MYSQL_ENABLE=true` into an instance with
//! a version, a port and a volume. It goes when no supported workspace still
//! needs migrating, and *that* is a release decision rather than an engineering
//! one.
//!
//! What is engineering, and what this file is: the deletion must not be an
//! archaeology exercise six months from now. Two things make it mechanical —
//! the keys are one constant instead of 150 lines mixed into 36 others, and the
//! modules that read one are named here. A new reader is a change that makes
//! the eventual deletion bigger, so it has to be written down rather than
//! discovered by whoever attempts it.
//!
//! ## Why file granularity
//!
//! `commands.rs` is twelve thousand lines with several test modules scattered
//! through it, so "which function" cannot be answered by reading text. The
//! honest unit is the file, and the claim each row makes is a sentence about
//! why that file reads a legacy default at all — which is the thing a reader on
//! deletion day actually needs.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Everything that reads a `SERVICE_*` default out of an [`Env`].
///
/// `service_prefix` is in the list even though it only builds a string: a
/// caller that formats a key by hand and passes it to `get` is doing the same
/// thing one indirection later, and this is the spelling every such caller in
/// this crate happens to use.
const ACCESSORS: [&str; 7] = [
    "service_enabled",
    "service_version",
    "service_versions",
    "service_url",
    "service_host_port",
    "service_credentials",
    "service_prefix",
];

/// The declared readers, and why each one is a reader.
///
/// Ordered by what deletion day does to them: `config.rs` loses the constant
/// and the accessors, `handover.rs` loses its reason to exist, and the other
/// two lose a branch each.
const READERS: [(&str, &str); 4] = [
    (
        "config.rs",
        "defines the constant and the accessors; the deletion starts here",
    ),
    (
        "handover.rs",
        "the migration itself — this is the reason the constant is still here",
    ),
    (
        "commands.rs",
        "the pre-migration branches: `list_services` falls back to `.env` when \
         there is no instance table, and the traefik routes are rendered from \
         the same fallback",
    ),
    (
        "preset.rs",
        "`export` describes a stack, and an unmigrated stack is still described \
         by `.env`",
    ),
];

/// Production code only.
///
/// The same indentation-based scan as `platform_matrix_claims.rs`,
/// `readme_claims.rs` and `privacy_claims.rs`, for the same reason: brace
/// counting breaks on a test that writes an unmatched `{` inside a string
/// literal, while `cargo fmt --check` guarantees a top-level item closes with a
/// `}` in column zero.
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

/// A line that calls one of the accessors, rather than one that mentions it.
///
/// Doc comments name these functions in several modules — `connect.rs` points
/// at `service_credentials` to explain its own masking — and counting those
/// would put files on the list that read nothing. Prose is not a reader.
fn calls_an_accessor(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("//!") {
        return false;
    }
    ACCESSORS.iter().any(|name| line.contains(name))
}

fn readers_in_tree() -> Vec<String> {
    let dir = repo_root().join("src");
    let mut found = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("src/ is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name")
            .to_string();
        let source = production_regions(&read(&format!("src/{name}")));
        if source.lines().any(calls_an_accessor) {
            found.push(name);
        }
    }

    found.sort();
    found
}

/// The list is the whole list.
///
/// Fails in both directions on purpose. A new reader is the case this exists
/// for; a reader that has *gone* matters just as much, because a row nobody
/// removed is a row somebody will go looking for on deletion day and not find.
#[test]
fn only_the_declared_modules_read_a_legacy_service_default() {
    let found = readers_in_tree();
    let mut declared: Vec<String> = READERS
        .iter()
        .map(|(file, _)| (*file).to_string())
        .collect();
    declared.sort();

    assert_eq!(
        found, declared,
        "the set of modules reading a `SERVICE_*` default has changed.\n\
         Every one of them is work on the day `config::LEGACY_SERVICES` is \
         deleted (§3 #36), so the list in this file is the checklist for that \
         day — add or remove the row, with the sentence that says why."
    );
}

/// The constant's stated reason is true.
///
/// `LEGACY_SERVICES` says in prose that the migration is why it survives. If
/// `handover.rs` stopped reading it, that sentence would be the last thing
/// standing between the constant and its deletion — and it would be false.
#[test]
fn the_migration_is_still_the_reason_the_constant_exists() {
    let handover = production_regions(&read("src/handover.rs"));

    for accessor in ["service_enabled", "service_version"] {
        assert!(
            handover.lines().filter(|l| l.contains(accessor)).any(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("///")
            }),
            "handover.rs no longer calls `{accessor}`. If the migration has \
             stopped reading `.env`, `config::LEGACY_SERVICES` has no reader \
             that needs it and §3 #36 is no longer blocked on anything."
        );
    }
}

/// The two halves are named where the deletion will be planned.
///
/// The measurement itself is §7's and `platform_matrix_claims.rs` holds the
/// numbers. This holds something smaller and easier to lose: that §3's row
/// names the constant, so a reader who arrives at the item can find the code
/// without grepping for a phrase.
#[test]
fn the_document_names_the_constant_it_is_waiting_to_delete() {
    let doc = std::fs::read_to_string(repo_root().join("../docs/durum.md"))
        .expect("docs/durum.md is readable");

    assert!(
        doc.contains("LEGACY_SERVICES"),
        "docs/durum.md never names `config::LEGACY_SERVICES`, which is the \
         constant §3 #36 is about"
    );
}
