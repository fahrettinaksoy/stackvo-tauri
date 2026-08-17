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

/// The decisions carry the three parts that make one worth reading.
///
/// `docs/durum.md` §6 replaced `docs/adr/`, one file per decision, when the
/// five documents under `docs/` became one. The numbering survived the move on
/// purpose — comments through the codebase say "ADR 0005" and "ADR 0009", and a
/// reference that resolves to nothing is worse than no reference.
///
/// A decision without a status is a draft somebody forgot; without a decision
/// it is a description of a problem; without consequences it is the half that
/// reads well and the half nobody needs is missing.
#[test]
fn every_decision_carries_a_status_a_decision_and_its_consequences() {
    let text = read(&repo_root().join("docs/durum.md"));

    let numbers: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("### "))
        .filter(|rest| rest.starts_with("00"))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();

    assert!(
        numbers.len() >= 10,
        "only {} decisions found in §6 — the heading format has changed and this \
         gate has stopped reading them",
        numbers.len()
    );

    // Each block runs from its own heading to the next `###` or `---`.
    for number in &numbers {
        let start = text
            .find(&format!("### {number} "))
            .expect("the heading was just read out of this text");
        let rest = &text[start..];
        let end = rest[4..]
            .find("\n### ")
            .map(|i| i + 4)
            .or_else(|| rest.find("\n---"))
            .unwrap_or(rest.len());
        let block = &rest[..end];

        for part in ["**Status:**", "**Decision:**", "**Consequences:**"] {
            assert!(block.contains(part), "decision {number} has no {part} line");
        }
    }

    // And ARCHITECTURE.md points at them rather than carrying a second table
    // that can disagree.
    assert!(
        architecture().contains("docs/durum.md"),
        "ARCHITECTURE.md no longer points at the decisions"
    );
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

/// A clock is not an identity, and a test fixture must not use one as one.
///
/// Four test helpers built a temp directory out of
/// `SystemTime::now().as_nanos()`, on the reading that a nanosecond clock
/// cannot hand out the same value twice. It can. macOS quantises the reading to
/// a microsecond — every value ends in `000` — and `cargo test` runs test
/// functions on parallel threads, so two of them inside the same microsecond
/// got the same directory. The second `fs::write` replaced the first's fixture
/// and whichever test read afterwards asserted against the other one's data.
///
/// It cost more to find than to fix, because of how it presented: `cargo test`
/// failed, `cargo test <that test>` passed, and which test failed moved between
/// runs. That is the signature of shared state, and the shared state was a name
/// that looked unique.
///
/// The replacement is what `market::tests::scratch` already did: name the
/// directory after the test, scope it to the pid, and remove it first. A stray
/// directory then says which test left it, which a timestamp never could.
///
/// Scoped to the fixture idiom rather than banning the call: `as_nanos` is a
/// duration measurement in plenty of places and this is not about those. What
/// is refused is a **path** built from a clock.
#[test]
fn no_test_fixture_builds_its_directory_out_of_a_clock() {
    let mut offenders = Vec::new();

    for dir in ["src", "tests", "examples"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            // This file, because the needles are in it as string literals — it
            // is the only place in the tree where `temp_dir()` and `as_nanos()`
            // appear together and mean nothing. Excluded by name rather than by
            // being clever about how the literals are spelled: a scanner that
            // hides its own needles is one nobody can grep for.
            if path
                .file_name()
                .is_some_and(|n| n == "architecture_claims.rs")
            {
                continue;
            }
            // Comment lines dropped first, and `policy_claims.rs` already paid
            // for this lesson: its first version searched a whole file and was
            // satisfied by a path sitting in the documentation twenty lines
            // above the parser. Here it is the reverse and worse — the note
            // explaining *why* this rule exists sits directly above the code it
            // fixed, so a scanner that reads prose reports every file that has
            // been fixed.
            let text: String = read(&path)
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");

            // The two have to appear in the same `format!` for this to be the
            // bug: a file that measures a duration somewhere and builds a temp
            // path somewhere else is not doing anything wrong. Read per
            // statement rather than per file, splitting on the `;` that ends
            // one — cheap, and precise enough that it has no false positive in
            // this tree.
            for statement in text.split(';') {
                if statement.contains("temp_dir()")
                    && (statement.contains("as_nanos()")
                        || statement.contains("as_micros()")
                        || statement.contains("as_millis()"))
                {
                    offenders.push(path.file_name().unwrap().to_string_lossy().to_string());
                    break;
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these build a temp path from a clock, which is not unique across \
         parallel test threads — name the directory after the test and scope \
         it to the pid, as `market::tests::scratch` does: {offenders:?}"
    );
}
