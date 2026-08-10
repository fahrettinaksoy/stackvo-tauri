//! Does `ARCHITECTURE.md` still describe this tree?
//!
//! The document exists because §12 of the readiness review measured this
//! repository's bus factor at one and named the absence of an architecture
//! document as the first reason. A map that no longer matches the ground is
//! worse than no map: the second person trusts it, and it sends them somewhere
//! that is not there.
//!
//! This repository has been wrong about itself before, in exactly this way. The
//! readiness review's own first draft named a module as weakly tested that was
//! 94% covered, and counted 33 of something there were 60 of. Both survived
//! review because a number in prose is not checked by anything.
//!
//! So the checkable claims are checked. `readme_claims.rs` does this for
//! `README.md`; this does it for `ARCHITECTURE.md` and the ADRs it points at.
//!
//! What is *not* checked is the prose, and that is not an oversight — "the
//! dependency arrows only ever point downward" is a claim about intent that a
//! parser cannot settle. Review settles it.

use std::collections::BTreeSet;
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

fn architecture() -> String {
    read(&repo_root().join("ARCHITECTURE.md"))
}

/// Every `[text](target)` whose target is a repository path.
///
/// Anchors (`#keeping-this-file-honest`) and absolute URLs are somebody else's
/// problem; a relative path is this repository's.
fn local_links(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = markdown.chars().collect();

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == ']' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            let start = i + 2;
            if let Some(offset) = bytes[start..].iter().position(|c| *c == ')') {
                let target: String = bytes[start..start + offset].iter().collect();
                if !target.starts_with('#') && !target.contains("://") {
                    out.push(target.split('#').next().unwrap_or(&target).to_string());
                }
                i = start + offset;
            }
        }
        i += 1;
    }
    out
}

/// `ARCHITECTURE.md` sits at the repository root, so its relative links are
/// resolved from there.
#[test]
fn every_link_points_at_a_file_that_exists() {
    let links = local_links(&architecture());
    assert!(
        links.len() > 5,
        "only {} local links found — the link parser has stopped matching",
        links.len()
    );

    let root = repo_root();
    let broken: Vec<_> = links
        .iter()
        .filter(|target| !root.join(target).exists())
        .collect();

    assert!(
        broken.is_empty(),
        "ARCHITECTURE.md points at files that do not exist: {broken:?}"
    );
}

/// The decision table and the directory have to be the same set.
///
/// Both failures are quiet. An ADR nobody links to is a decision nobody will
/// find; a row pointing at a file that was never written reads as a decision
/// that was made.
#[test]
fn the_decision_table_lists_every_adr_and_no_others() {
    let dir = repo_root().join("docs/adr");
    let on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
        .expect("docs/adr exists")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".md") && name != "README.md")
        .collect();

    let linked: BTreeSet<String> = local_links(&architecture())
        .into_iter()
        .filter(|t| t.starts_with("docs/adr/"))
        .map(|t| t.trim_start_matches("docs/adr/").to_string())
        .collect();

    assert_eq!(
        on_disk, linked,
        "the ADR directory and ARCHITECTURE.md's decision table disagree"
    );
}

/// An ADR without a status is a draft somebody forgot; without a decision it is
/// a description of a problem.
#[test]
fn every_adr_carries_a_status_and_a_decision() {
    let dir = repo_root().join("docs/adr");
    let mut checked = 0;

    for entry in std::fs::read_dir(&dir).expect("docs/adr exists").flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") || name == "README.md" {
            continue;
        }
        let text = read(&path);

        assert!(
            text.contains("**Status:**"),
            "{name} has no Status line — a decision with no status is a draft"
        );
        assert!(
            text.contains("## Decision"),
            "{name} has no Decision section"
        );
        assert!(
            text.contains("## Consequences"),
            "{name} has no Consequences section — the consequence nobody wanted \
             is the part a later reader needs"
        );
        checked += 1;
    }

    assert!(checked >= 7, "only {checked} ADRs found");
}

/// The counts in the document, against the tree.
///
/// Only the ones a parser can settle. `54 modules` and `144 commands` are
/// facts; "one subject each" is a judgement.
#[test]
fn the_counts_match_the_tree() {
    let doc = architecture();

    let modules = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .expect("src/ is readable")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .count();
    assert!(
        doc.contains(&format!("{modules} modules")),
        "ARCHITECTURE.md does not say `{modules} modules`, which is what src/ holds"
    );

    let contract = read(&repo_root().join("contracts/ipc.json"));
    let value: serde_json::Value = serde_json::from_str(&contract).expect("valid JSON");
    let commands = value["commands"]
        .as_object()
        .expect("commands object")
        .len();

    // `_note` and `_removed` are section comments, not events. Counting the
    // object's keys called them two — and the document said 59 events for
    // months while the contract declared 57, with this test agreeing because it
    // made the same mistake. A gate that shares the document's error is not a
    // second opinion.
    let events = value["events"]
        .as_object()
        .expect("events object")
        .keys()
        .filter(|name| !name.starts_with('_'))
        .count();

    assert!(
        doc.contains(&format!("{commands} commands")),
        "ARCHITECTURE.md does not say `{commands} commands`"
    );
    assert!(
        doc.contains(&format!("{events} events")),
        "ARCHITECTURE.md does not say `{events} events`"
    );
}

/// The one structural claim that *is* checkable, and the rule the whole layer
/// diagram exists to state: only `commands.rs` names a Tauri handle.
///
/// This is ADR 0001 with a test behind it. Without one the rule is a comment,
/// and comments do not fail builds.
#[test]
fn only_the_command_layer_names_a_tauri_handle() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // The entry band builds the app, so it holds the handle by definition, and
    // `events` is the Tauri-side implementation of the sink ADR 0005 defines.
    let allowed = [
        "commands.rs",
        "lib.rs",
        "main.rs",
        "menu.rs",
        "tray.rs",
        "events.rs",
    ];

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("src/ is readable").flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".rs") || allowed.contains(&name.as_str()) {
            continue;
        }
        let text = read(&entry.path());
        // Any managed state, however it is spelled — `State<'_, AppState>`,
        // `State<'_, pty::Registry>`, `State<'_, crate::watcher::Handle>`. A
        // narrower pattern passed while a deliberately broken module sat right
        // in front of it, which is how this one got widened.
        if text.contains("State<'_,") {
            offenders.push(name);
        }
    }

    assert!(
        offenders.is_empty(),
        "these modules take Tauri's managed state, which ADR 0001 puts in \
         `commands.rs` alone — a function holding it cannot be called from a \
         test, the `diagnose` example, or the MCP surface: {offenders:?}"
    );
}
