//! The contract has to spell a field the way the wire spells it.
//!
//! `contract_agreement.rs` checks that the *set* of commands in
//! `contracts/ipc.json` matches the set the app registers.
//! `contract_version.rs` checks the contract against the last released copy of
//! itself. Between them there was no gate at all on the one thing the front end
//! actually reads: the **names of the fields**.
//!
//! Nothing being wrong about that would have been luck. It was not luck, and it
//! was not close: **106** field names were written in Rust's spelling —
//! `managed_by_stackvo`, `operation_id`, `cpu_percent`, `session_id` — for a
//! wire that has always carried `managedByStackvo`, `operationId`,
//! `cpuPercent`, `sessionId`. Every struct a command returns derives
//! `#[serde(rename_all = "camelCase")]`, and Tauri camel-cases arguments on the
//! way in, so not one of those hundred and six names ever appeared in a
//! payload. The front end had it right all along; the document describing the
//! front end had it wrong.
//!
//! It went unnoticed for exactly as long as nothing consumed the type table.
//! The moment `tools/generate-types.mjs` turned it into `src/lib/ipc.d.ts` and
//! `npm run types:tsc` compiled the result, the contract's spelling started
//! telling an editor that correct code was wrong — which is worse than no types
//! at all, because it is wrong with authority.
//!
//! ## Two claims, and the second is the one that matters
//!
//! Spelling every name in camelCase is only right *because* every serialisable
//! shape is renamed. A struct added without `rename_all` would make the first
//! rule false without changing a line of the contract, so both are checked
//! here: the document's spelling, and the reason that spelling is correct.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Keys that are prose wherever they appear.
///
/// The same list `contract_version.rs` uses, and for the same reason: a `note`
/// inside a type is documentation that happens to live in an object. `_note`
/// and the `*Note` suffix are the two spellings this contract grew.
fn is_prose(key: &str) -> bool {
    matches!(
        key,
        "why" | "notes" | "note" | "_note" | "new" | "$schema" | "$ref" | "..."
    ) || key.ends_with("Note")
}

fn is_snake_case(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    key.contains('_')
        && first.is_ascii_lowercase()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Every field name below `node`, with the path that reaches it.
fn field_names(node: &serde_json::Value, path: &str, out: &mut Vec<(String, String)>) {
    match node {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_prose(key) {
                    continue;
                }
                if is_snake_case(key) {
                    out.push((path.to_string(), key.clone()));
                }
                field_names(value, &format!("{path}.{key}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                field_names(value, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// The document's spelling.
///
/// Command names and event names are deliberately **not** checked: `pty_open`
/// and `build:start` are the identifiers Tauri and the emitter use, and they are
/// snake_case and colon-separated on purpose. What is checked is everything a
/// payload carries — the `types` table, each command's `args`, and each event's
/// `payload`.
#[test]
fn no_field_name_in_the_contract_is_spelled_the_way_rust_spells_it() {
    let contract: serde_json::Value =
        serde_json::from_str(&read("contracts/ipc.json")).expect("contracts/ipc.json parses");

    let mut found = Vec::new();
    field_names(&contract["types"], "types", &mut found);

    for (name, command) in contract["commands"]
        .as_object()
        .expect("the contract has commands")
    {
        field_names(
            &command["args"],
            &format!("commands.{name}.args"),
            &mut found,
        );
    }

    for (name, event) in contract["events"]
        .as_object()
        .expect("the contract has events")
    {
        field_names(
            &event["payload"],
            &format!("events.{name}.payload"),
            &mut found,
        );
    }

    assert!(
        found.is_empty(),
        "{} field name(s) in contracts/ipc.json are spelled in snake_case, and \
         nothing on this wire is:\n{}\n\nEvery shape a command returns derives \
         `#[serde(rename_all = \"camelCase\")]` and Tauri camel-cases arguments, \
         so a name written this way describes a field no payload carries. \
         `src/lib/ipc.d.ts` is generated from these names, which makes a wrong \
         one worse than a missing one — an editor repeats it as fact.",
        found.len(),
        found
            .iter()
            .map(|(path, key)| format!("  {path}  →  {key}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `ExtensionSpec` mirrors a FILE, not a payload.
///
/// `contracts/php-extensions.json` is written in snake_case and this struct
/// deserialises it; renaming its fields would stop it reading the file it
/// exists to read. It never reaches the wire — `catalog_get` answers with
/// `ExtensionOption`, which is renamed like everything else. Named here rather
/// than pattern-matched away, so the exception is a sentence somebody can
/// disagree with.
const FILE_SHAPES: [&str; 1] = ["ExtensionSpec"];

/// Why the rule above is true.
///
/// Scans every `pub struct` in the crate that derives `Serialize`. One carrying
/// a snake_case field without `rename_all` would put that field on the wire in
/// Rust's spelling — and the contract, which this repository now generates the
/// front end's types from, would be right to describe it that way and wrong
/// everywhere else.
#[test]
fn every_shape_this_app_serialises_is_renamed_to_camel_case() {
    let mut offenders: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let exempt: BTreeSet<&str> = FILE_SHAPES.into_iter().collect();

    let src = repo_root().join("src-tauri/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("src-tauri/src is readable")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();

    for path in &files {
        let text = std::fs::read_to_string(path).expect("a source file is readable");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

        for (index, _) in text.match_indices("pub struct ") {
            // The attributes are the lines immediately above, back to the first
            // that is neither an attribute nor a doc comment.
            let head = &text[..index];
            let attrs: String = head
                .lines()
                .rev()
                .take_while(|line| {
                    let t = line.trim();
                    t.starts_with('#') || t.starts_with("///") || t.starts_with("//")
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !attrs.contains("Serialize") {
                continue;
            }

            let rest = &text[index..];
            let Some(open) = rest.find('{') else { continue };
            let Some(close) = rest.find("\n}") else {
                continue;
            };
            if close < open {
                continue;
            }
            let struct_name = rest["pub struct ".len()..open]
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .find(|s| !s.is_empty())
                .unwrap_or("?")
                .to_string();
            if exempt.contains(struct_name.as_str()) {
                continue;
            }
            checked += 1;
            if attrs.contains("rename_all") {
                continue;
            }

            for line in rest[open..close].lines() {
                let trimmed = line.trim();
                let Some(field) = trimmed.strip_prefix("pub ") else {
                    continue;
                };
                let Some(field) = field.split(':').next() else {
                    continue;
                };
                let field = field.trim();
                if is_snake_case(field) {
                    offenders.push(format!("  {name}::{struct_name}  →  {field}"));
                }
            }
        }
    }

    // 204 today. The floor is the point, not the number: a scan that quietly
    // stopped matching would pass this test by looking at nothing, which is the
    // failure mode a gate over source text actually has.
    assert!(
        checked > 150,
        "only {checked} serialisable struct(s) were found — the scan stopped \
         matching, which would make this test pass by looking at nothing"
    );
    assert!(
        offenders.is_empty(),
        "{} field(s) would go over the wire in Rust's spelling:\n{}\n\nAdd \
         `#[serde(rename_all = \"camelCase\")]`, or — if the shape mirrors a \
         file rather than a payload — name it in FILE_SHAPES with the reason. \
         `no_field_name_in_the_contract_is_spelled_the_way_rust_spells_it` is \
         only correct while this one is.",
        offenders.len(),
        offenders.join("\n")
    );
}
