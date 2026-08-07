//! One IPC surface, three places that describe it.
//!
//! `contracts/ipc.json` is the boundary's specification: 148 entries naming
//! every command, its arguments, its return type and — for most of them — why
//! it exists at all. The front end's `src/lib/ipc.js` is generated against it,
//! the readiness review is argued from it, and `tests/ipc.spec.js` checks the
//! JavaScript wrappers against it.
//!
//! Nothing checked it against the *Rust*.
//!
//! The three descriptions are:
//!
//!   * the contract file — what the boundary is documented to be;
//!   * `#[tauri::command]` in `src/*.rs` — what is actually implemented;
//!   * `generate_handler!` in `lib.rs` — what is actually reachable.
//!
//! All three agree today. The way they stop agreeing is quiet in both
//! directions: implement a command and forget the handler, and the front end
//! gets "command not found" as a bare string at runtime (which
//! `tests/ipc.spec.js` proves the client survives, and nothing else notices);
//! delete a command and leave it in the contract, and the review keeps counting
//! a boundary that is not there.
//!
//! Neither shows up in a build, a clippy run, or any other test. The compiler
//! cannot see the JSON, and the JSON cannot see the compiler.

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

/// Every `.rs` file directly under `src/`, concatenated.
///
/// Flat on purpose: `lib.rs` declares every module with `mod x;`, so a file in
/// a subdirectory would not be part of the crate and could not carry a command.
fn all_sources() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = String::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("src/ is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    for file in files {
        out.push_str(&read(&file));
        out.push('\n');
    }
    out
}

/// The function name a `#[tauri::command]` attribute belongs to.
///
/// Scanned line by line rather than with a regex over the whole file, because
/// what sits between the attribute and the `fn` is unbounded: doc comments,
/// further attributes, and in two cases a paragraph of them. The rule is simply
/// "the next `fn` after the attribute", which is what Rust itself applies.
///
/// Both spellings count. `#[tauri::command(async)]` is the same attribute with
/// an argument, and it is not decoration: it moves the body onto a blocking
/// task so a native panel does not freeze the window behind it. A scanner that
/// only matched the bare form would have missed `workspace_pick` and
/// `hosts_apply` — the first version of this one did.
fn implemented(source: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut armed = false;
    let mut countdown = 0usize;

    for line in source.lines() {
        let trimmed = line.trim_start();

        // The whole line, not a prefix. `commands.rs` carries a unit test
        // that scans its own source for this attribute, so the text appears
        // there inside a string literal — and a prefix match arms on it, then
        // reports the next test helper as a command. The first run of this
        // file did exactly that and named `generated_workspace`.
        if trimmed == "#[tauri::command]" || trimmed == "#[tauri::command(async)]" {
            armed = true;
            countdown = 200;
            continue;
        }
        if !armed {
            continue;
        }
        // Doc comments between the attribute and the `fn` run to a paragraph in
        // places, but not to two hundred lines. Giving up rather than scanning
        // on keeps a missed match local instead of attaching the attribute to
        // something far below it.
        countdown -= 1;
        if countdown == 0 {
            armed = false;
            continue;
        }
        if let Some(name) = function_name(trimmed) {
            found.insert(name);
            armed = false;
        }
    }
    found
}

/// `pub async fn name(` → `name`, in any order of the modifiers.
fn function_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("pub ")
        .unwrap_or(line)
        .strip_prefix("async ")
        .map(|r| r.to_string())
        .unwrap_or_else(|| line.strip_prefix("pub ").unwrap_or(line).to_string());
    let rest = rest.strip_prefix("fn ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// What `generate_handler!` actually registers, module path stripped.
fn registered() -> BTreeSet<String> {
    let lib = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"));
    let start = lib
        .find("generate_handler![")
        .expect("lib.rs registers a handler list");
    let body = &lib[start..];
    let end = body.find(']').expect("the handler list is closed");

    body[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            // Comments group the list into phases; they are not commands.
            if line.starts_with("//") {
                return None;
            }
            let entry = line.trim_end_matches(',');
            entry.rsplit("::").next().and_then(|name| {
                (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
                    .then(|| name.to_string())
            })
        })
        .collect()
}

/// The contract's commands, split by who implements them.
///
/// Three entries are `frontend-plugin`: `open_path`, `open_url` and
/// `pick_directory` are served by Tauri plugins from the JavaScript side and
/// have no Rust function by design. One is `deferred`: `updates_check` is
/// specified and deliberately not built (see §14.2 — it needs a signing key and
/// an endpoint that answers). Both exclusions are read from the file rather
/// than hardcoded here, so adding a fourth plugin command needs no edit to this
/// test — and un-deferring `updates_check` correctly starts demanding an
/// implementation.
fn contract() -> (BTreeSet<String>, BTreeSet<String>) {
    let text = read(&repo_root().join("contracts/ipc.json"));
    let value: serde_json::Value = serde_json::from_str(&text).expect("the contract is valid JSON");
    let commands = value
        .get("commands")
        .and_then(|c| c.as_object())
        .expect("the contract has a `commands` object");

    let mut rust = BTreeSet::new();
    let mut elsewhere = BTreeSet::new();

    for (name, spec) in commands {
        let kind = spec.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let deferred = spec.get("status").and_then(|s| s.as_str()) == Some("deferred");

        if kind == "frontend-plugin" || deferred {
            elsewhere.insert(name.clone());
        } else {
            rust.insert(name.clone());
        }
    }
    (rust, elsewhere)
}

#[test]
fn every_contracted_command_is_implemented_in_rust() {
    let (expected, _) = contract();
    let actual = implemented(&all_sources());

    let missing: Vec<_> = expected.difference(&actual).collect();
    assert!(
        missing.is_empty(),
        "the contract names commands nothing implements: {missing:?}\n\
         Either write them, or mark them `\"status\": \"deferred\"` the way \
         `updates_check` is."
    );
}

#[test]
fn every_implemented_command_is_in_the_contract() {
    let (expected, _) = contract();
    let actual = implemented(&all_sources());

    let undocumented: Vec<_> = actual.difference(&expected).collect();
    assert!(
        undocumented.is_empty(),
        "these commands exist but the contract does not describe them: {undocumented:?}\n\
         The contract is what the front end is generated against and what the \
         readiness review is argued from; a command missing from it is a \
         boundary nobody agreed to."
    );
}

/// The failure mode this one exists for is the quietest of the three: the
/// command compiles, the contract describes it, and calling it answers
/// "command not found" as a bare string at runtime.
#[test]
fn every_implemented_command_is_reachable() {
    let actual = implemented(&all_sources());
    let wired = registered();

    let orphans: Vec<_> = actual.difference(&wired).collect();
    assert!(
        orphans.is_empty(),
        "these commands are implemented but not in `generate_handler!`, \
         so calling one answers `command not found`: {orphans:?}"
    );
}

#[test]
fn the_handler_registers_nothing_that_does_not_exist() {
    let actual = implemented(&all_sources());
    let wired = registered();

    let ghosts: Vec<_> = wired.difference(&actual).collect();
    assert!(
        ghosts.is_empty(),
        "`generate_handler!` names things that are not commands: {ghosts:?}"
    );
}

/// A guard on the guard.
///
/// Every assertion above is a set difference, and a scanner that silently found
/// nothing would make all four pass while checking nothing at all. That is
/// exactly how the first version of this file behaved: its regex required the
/// attribute to sit immediately above the `fn`, and the two commands written as
/// `#[tauri::command(async)]` — with a paragraph of documentation between —
/// were simply invisible.
#[test]
fn the_scanner_finds_a_realistic_number_of_commands() {
    let found = implemented(&all_sources());
    assert!(
        found.len() > 100,
        "only {} commands found — the scanner has stopped matching, and the \
         other tests in this file are passing vacuously",
        found.len()
    );

    // The two that broke the first scanner, named so a regression is legible
    // rather than a count that drifted.
    for name in ["workspace_pick", "hosts_apply"] {
        assert!(
            found.contains(name),
            "`{name}` is written `#[tauri::command(async)]`; the scanner must \
             match the parameterised form too"
        );
    }
}
