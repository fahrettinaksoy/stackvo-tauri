//! `contractVersion` means something, and this is what it means.
//!
//! The contract has carried a `contractVersion` since the first commit, and
//! nothing anywhere said what would make it go up. The readiness review found
//! that and put it plainly: the field exists, and *what counts as a major
//! change is undefined*. A version number nobody can derive is decoration — it
//! moves when someone remembers, which is the same as not moving.
//!
//! Decision 0008 in [`docs/durum.md`](../../docs/durum.md) §6 gives
//! the rule. This file is the rule as a build failure.
//!
//! ## How it works
//!
//! `contracts/surface.lock.json` is the **last released** call surface and the
//! version it was released as. Every run compares the working contract against
//! it, classifies the difference, and requires that `contractVersion` has
//! already been raised far enough to describe it:
//!
//! * something a client depended on is **gone or different** → major;
//! * something new is **available** → minor;
//! * nothing about the call surface changed → the version must not move.
//!
//! Prose is not the surface. `why` and `notes` can be rewritten freely, and a
//! `note` key inside a type is documentation that happens to live in an object.
//!
//! ## Refreshing the lock
//!
//! At release, and only then:
//!
//! ```text
//! UPDATE_CONTRACT_LOCK=1 cargo test --test contract_version
//! ```
//!
//! Refreshing it at any other time is how this gate would be defeated: the lock
//! is the memory of what other people were promised, and a memory that is
//! rewritten whenever it disagrees is not one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read_json(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn contract_path() -> PathBuf {
    repo_root().join("contracts/ipc.json")
}

fn lock_path() -> PathBuf {
    repo_root().join("contracts/surface.lock.json")
}

// ---------------------------------------------------------------- the surface

/// What a caller can rely on: names, argument names and types, return shapes,
/// event payloads, and the named types those refer to.
///
/// Deliberately **not** a struct per concept. Everything here is compared as
/// JSON against a file written by an earlier version of this same code, and a
/// typed model would have to be migrated in lockstep with a lock file that
/// already exists on disk — a second compatibility problem inside the tool
/// built to detect the first one.
#[derive(Debug, Clone, PartialEq)]
struct Surface {
    commands: BTreeMap<String, Value>,
    events: BTreeMap<String, Value>,
    types: BTreeMap<String, Value>,
}

/// Keys that are documentation wherever they appear.
fn is_prose(key: &str) -> bool {
    matches!(key, "why" | "notes" | "note" | "_note" | "new" | "$schema")
}

fn without_prose(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !is_prose(k))
                .map(|(k, v)| (k.clone(), without_prose(v)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

/// The call surface of a contract document.
fn surface_of(contract: &Value) -> Surface {
    let section = |name: &str| -> BTreeMap<String, Value> {
        contract[name]
            .as_object()
            .map(|map| {
                map.iter()
                    // `_note` is a section-level comment, not an entry.
                    .filter(|(k, _)| !is_prose(k))
                    .map(|(k, v)| (k.clone(), without_prose(v)))
                    .collect()
            })
            .unwrap_or_default()
    };

    Surface {
        commands: section("commands"),
        events: section("events"),
        types: section("types"),
    }
}

// ------------------------------------------------------------- classification

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    None,
    Minor,
    Major,
}

#[derive(Debug)]
struct Change {
    severity: Severity,
    what: String,
}

/// Is this argument one a caller may leave out?
///
/// The contract writes argument types as prose — `string?`, `u32 (default
/// 200)`, `string[]? (all when omitted)` — because a human reads them. So this
/// is a heuristic, and it is the conservative direction of one: anything not
/// recognisably optional is treated as required, which makes *adding* it a
/// breaking change. Being wrong that way costs a major version nobody needed.
/// Being wrong the other way ships a break as a minor.
fn is_optional(spec: &str) -> bool {
    spec.contains('?') || spec.contains("default") || spec.contains("omitted")
}

fn object_of<'a>(value: &'a Value, key: &str) -> BTreeMap<String, &'a Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .map(|map| map.iter().map(|(k, v)| (k.clone(), v)).collect())
        .unwrap_or_default()
}

fn status_of(command: &Value) -> &str {
    command
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("ok")
}

fn compare_commands(was: &Surface, now: &Surface, out: &mut Vec<Change>) {
    for (name, before) in &was.commands {
        let Some(after) = now.commands.get(name) else {
            out.push(Change {
                severity: Severity::Major,
                what: format!("command `{name}` was removed"),
            });
            continue;
        };

        // `updates_check` is documented as deferred; a command that goes back to
        // deferred stops answering, which from a caller's side is a removal.
        if status_of(before) == "ok" && status_of(after) != "ok" {
            out.push(Change {
                severity: Severity::Major,
                what: format!(
                    "command `{name}` became `{}` — it no longer answers",
                    status_of(after)
                ),
            });
        } else if status_of(before) != "ok" && status_of(after) == "ok" {
            out.push(Change {
                severity: Severity::Minor,
                what: format!("command `{name}` is no longer deferred"),
            });
        }

        for field in ["kind", "returns"] {
            if before.get(field) != after.get(field) {
                out.push(Change {
                    severity: Severity::Major,
                    what: format!(
                        "command `{name}`'s {field} changed: {} → {}",
                        before.get(field).unwrap_or(&Value::Null),
                        after.get(field).unwrap_or(&Value::Null)
                    ),
                });
            }
        }

        let old_args = object_of(before, "args");
        let new_args = object_of(after, "args");

        for (arg, spec) in &old_args {
            match new_args.get(arg) {
                None => out.push(Change {
                    severity: Severity::Major,
                    what: format!("command `{name}` lost the argument `{arg}`"),
                }),
                Some(now_spec) if now_spec != spec => out.push(Change {
                    severity: Severity::Major,
                    what: format!(
                        "command `{name}`'s argument `{arg}` changed: {spec} → {now_spec}"
                    ),
                }),
                _ => {}
            }
        }

        for (arg, spec) in &new_args {
            if old_args.contains_key(arg) {
                continue;
            }
            let optional = spec.as_str().is_some_and(is_optional);
            out.push(Change {
                severity: if optional {
                    Severity::Minor
                } else {
                    Severity::Major
                },
                what: format!(
                    "command `{name}` gained {} argument `{arg}`",
                    if optional {
                        "the optional"
                    } else {
                        "the required"
                    }
                ),
            });
        }

        // A command that stops emitting an event leaves whoever waits for it
        // waiting: the operation console never closes the row.
        let emitted = |value: &Value| -> Vec<String> {
            value
                .get("emits")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        for event in emitted(before) {
            if !emitted(after).contains(&event) {
                out.push(Change {
                    severity: Severity::Major,
                    what: format!("command `{name}` no longer emits `{event}`"),
                });
            }
        }
    }

    for name in now.commands.keys() {
        if !was.commands.contains_key(name) {
            out.push(Change {
                severity: Severity::Minor,
                what: format!("command `{name}` is new"),
            });
        }
    }
}

fn compare_events(was: &Surface, now: &Surface, out: &mut Vec<Change>) {
    for (name, before) in &was.events {
        let Some(after) = now.events.get(name) else {
            out.push(Change {
                severity: Severity::Major,
                what: format!("event `{name}` was removed"),
            });
            continue;
        };

        let old_payload = object_of(before, "payload");
        let new_payload = object_of(after, "payload");

        for (field, spec) in &old_payload {
            match new_payload.get(field) {
                None => out.push(Change {
                    severity: Severity::Major,
                    what: format!("event `{name}` lost the payload field `{field}`"),
                }),
                Some(now_spec) if now_spec != spec => out.push(Change {
                    severity: Severity::Major,
                    what: format!(
                        "event `{name}`'s payload field `{field}` changed: {spec} → {now_spec}"
                    ),
                }),
                _ => {}
            }
        }

        for field in new_payload.keys() {
            if !old_payload.contains_key(field) {
                out.push(Change {
                    severity: Severity::Minor,
                    what: format!("event `{name}` gained the payload field `{field}`"),
                });
            }
        }
    }

    for name in now.events.keys() {
        if !was.events.contains_key(name) {
            out.push(Change {
                severity: Severity::Minor,
                what: format!("event `{name}` is new"),
            });
        }
    }
}

/// Named types, field by field.
///
/// This is the half ADR 0006 admitted was on trust: `contract_agreement.rs`
/// checks that the *set* of commands agrees with the code, and nothing checked
/// the shapes. A field dropped from `Project` does not change any command's
/// `returns`, so nothing else here would see it — and the front end reads
/// `undefined` and renders a blank cell.
fn compare_types(was: &Surface, now: &Surface, out: &mut Vec<Change>) {
    for (name, before) in &was.types {
        let Some(after) = now.types.get(name) else {
            out.push(Change {
                severity: Severity::Major,
                what: format!("type `{name}` was removed"),
            });
            continue;
        };

        let (Some(old_fields), Some(new_fields)) = (before.as_object(), after.as_object()) else {
            if before != after {
                out.push(Change {
                    severity: Severity::Major,
                    what: format!("type `{name}` was redefined"),
                });
            }
            continue;
        };

        for (field, spec) in old_fields {
            match new_fields.get(field) {
                None => out.push(Change {
                    severity: Severity::Major,
                    what: format!("type `{name}` lost the field `{field}`"),
                }),
                Some(now_spec) if now_spec != spec => out.push(Change {
                    severity: Severity::Major,
                    what: format!("type `{name}`'s field `{field}` changed: {spec} → {now_spec}"),
                }),
                _ => {}
            }
        }

        for field in new_fields.keys() {
            if !old_fields.contains_key(field) {
                out.push(Change {
                    severity: Severity::Minor,
                    what: format!("type `{name}` gained the field `{field}`"),
                });
            }
        }
    }

    for name in now.types.keys() {
        if !was.types.contains_key(name) {
            out.push(Change {
                severity: Severity::Minor,
                what: format!("type `{name}` is new"),
            });
        }
    }
}

fn classify(was: &Surface, now: &Surface) -> Vec<Change> {
    let mut out = Vec::new();
    compare_commands(was, now, &mut out);
    compare_events(was, now, &mut out);
    compare_types(was, now, &mut out);
    out
}

// -------------------------------------------------------------------- semver

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u64, u64, u64);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

fn parse_version(text: &str) -> Version {
    let mut parts = text.split('.').map(|p| {
        p.parse::<u64>()
            .unwrap_or_else(|_| panic!("`{text}` is not a three-part version"))
    });
    let version = Version(
        parts.next().expect("major"),
        parts.next().expect("minor"),
        parts.next().expect("patch"),
    );
    assert!(parts.next().is_none(), "`{text}` has more than three parts");
    version
}

fn least_acceptable(from: Version, severity: Severity) -> Version {
    match severity {
        Severity::None => from,
        Severity::Minor => Version(from.0, from.1 + 1, 0),
        Severity::Major => Version(from.0 + 1, 0, 0),
    }
}

// --------------------------------------------------------------------- tests

fn lock_document(version: &str, surface: &Surface) -> Value {
    json!({
        "_note": "The last RELEASED call surface, and the version it went out as. \
                  Generated: UPDATE_CONTRACT_LOCK=1 cargo test --test contract_version. \
                  Refresh it at a release and at no other time — see docs/durum.md §6, decision 0008.",
        "contractVersion": version,
        "commands": surface.commands.clone().into_iter().collect::<Map<_, _>>(),
        "events": surface.events.clone().into_iter().collect::<Map<_, _>>(),
        "types": surface.types.clone().into_iter().collect::<Map<_, _>>(),
    })
}

/// The gate, and the refresh.
#[test]
fn the_contract_version_describes_the_change_since_the_last_release() {
    let contract = read_json(&contract_path());
    let now = surface_of(&contract);
    let current = contract["contractVersion"]
        .as_str()
        .expect("the contract declares a contractVersion");

    if std::env::var_os("UPDATE_CONTRACT_LOCK").is_some() {
        let text = serde_json::to_string_pretty(&lock_document(current, &now))
            .expect("the lock serialises");
        std::fs::write(lock_path(), text + "\n").expect("writing the lock");
        eprintln!("contracts/surface.lock.json refreshed at {current}");
        return;
    }

    let lock = read_json(&lock_path());
    let was = surface_of(&lock);
    let released = parse_version(
        lock["contractVersion"]
            .as_str()
            .expect("the lock records the version it was taken at"),
    );

    let changes = classify(&was, &now);
    let severity = changes
        .iter()
        .map(|c| c.severity)
        .max()
        .unwrap_or(Severity::None);
    let required = least_acceptable(released, severity);
    let declared = parse_version(current);

    let mut reasons: Vec<&Change> = changes
        .iter()
        .filter(|c| c.severity == severity && severity != Severity::None)
        .collect();
    reasons.sort_by(|a, b| a.what.cmp(&b.what));

    assert!(
        declared >= required,
        "contractVersion is {declared}, and the surface has moved past what that \
         says.\n\nThe last released contract was {released}. Since then, {} \
         change{} of severity {:?}:\n  {}\n\nSo contractVersion must be at least \
         {required}. Raise it in contracts/ipc.json — the rule is in \
         docs/durum.md §6, decision 0008.",
        reasons.len(),
        if reasons.len() == 1 { "" } else { "s" },
        severity,
        reasons
            .iter()
            .take(12)
            .map(|c| c.what.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    // The other direction: a version raised past what happened is a version
    // that means as little as one raised too late. Both make the number stop
    // being derivable from the diff, which is the whole point of having it.
    assert!(
        declared <= least_acceptable(released, Severity::Major),
        "contractVersion is {declared}, which is further than any single change \
         from the released {released} can justify (the most a change can ask for \
         is {}). If two releases happened, refresh the lock at each one.",
        least_acceptable(released, Severity::Major)
    );
}

/// The lock is the released surface, so it must describe the same tree the
/// contract does — otherwise the comparison above is against a fiction.
#[test]
fn the_lock_is_a_plausible_surface() {
    let lock = read_json(&lock_path());
    let was = surface_of(&lock);

    assert!(
        was.commands.len() > 100,
        "the lock holds {} commands, which is not this contract",
        was.commands.len()
    );
    assert!(
        was.events.len() > 20,
        "the lock holds {} events",
        was.events.len()
    );
    assert!(
        was.types.len() > 20,
        "the lock holds {} types",
        was.types.len()
    );
}

/// Prose is not surface: rewriting a `why` must not demand a version.
#[test]
fn documentation_changes_are_not_contract_changes() {
    let before = json!({
        "commands": { "a": { "kind": "query", "args": {}, "returns": "X", "why": "one" } },
        "events": {},
        "types": {},
    });
    let after = json!({
        "commands": {
            "a": { "kind": "query", "args": {}, "returns": "X", "why": "a much better one",
                   "notes": "added later", "new": true }
        },
        "events": {},
        "types": {},
    });

    assert!(classify(&surface_of(&before), &surface_of(&after)).is_empty());
}

/// Every rule, against the shape of change it is about.
///
/// Written as a table because the value of this file is entirely in these
/// verdicts: a classifier nobody tested is a paragraph with syntax.
#[test]
fn each_kind_of_change_gets_the_severity_the_adr_gives_it() {
    let base = json!({
        "commands": {
            "keep": { "kind": "query", "args": { "name": "string" }, "returns": "Project",
                      "emits": ["a:done"] },
            "gone": { "kind": "mutation", "args": {}, "returns": "void" }
        },
        "events": { "a:done": { "payload": { "project": "string" } } },
        "types": { "Project": { "name": "string", "note": "prose" } },
    });

    let cases: Vec<(&str, Value, Severity)> = vec![
        (
            "a removed command",
            json!({ "commands": { "keep": base["commands"]["keep"] },
                    "events": base["events"], "types": base["types"] }),
            Severity::Major,
        ),
        (
            "a new command",
            json!({ "commands": { "keep": base["commands"]["keep"], "gone": base["commands"]["gone"],
                                  "fresh": { "kind": "query", "args": {}, "returns": "void" } },
                    "events": base["events"], "types": base["types"] }),
            Severity::Minor,
        ),
        (
            "a renamed argument",
            json!({ "commands": { "keep": { "kind": "query", "args": { "project": "string" },
                                            "returns": "Project", "emits": ["a:done"] },
                                  "gone": base["commands"]["gone"] },
                    "events": base["events"], "types": base["types"] }),
            Severity::Major,
        ),
        (
            "a new optional argument",
            json!({ "commands": { "keep": { "kind": "query",
                                            "args": { "name": "string", "deep": "bool?" },
                                            "returns": "Project", "emits": ["a:done"] },
                                  "gone": base["commands"]["gone"] },
                    "events": base["events"], "types": base["types"] }),
            Severity::Minor,
        ),
        (
            "a new required argument",
            json!({ "commands": { "keep": { "kind": "query",
                                            "args": { "name": "string", "deep": "bool" },
                                            "returns": "Project", "emits": ["a:done"] },
                                  "gone": base["commands"]["gone"] },
                    "events": base["events"], "types": base["types"] }),
            Severity::Major,
        ),
        (
            "an event that is no longer emitted",
            json!({ "commands": { "keep": { "kind": "query", "args": { "name": "string" },
                                            "returns": "Project", "emits": [] },
                                  "gone": base["commands"]["gone"] },
                    "events": base["events"], "types": base["types"] }),
            Severity::Major,
        ),
        (
            "a field dropped from a type",
            json!({ "commands": base["commands"], "events": base["events"],
                    "types": { "Project": { "note": "prose" } } }),
            Severity::Major,
        ),
        (
            "a field added to a type",
            json!({ "commands": base["commands"], "events": base["events"],
                    "types": { "Project": { "name": "string", "size": "u64", "note": "prose" } } }),
            Severity::Minor,
        ),
        (
            "a command that becomes deferred",
            json!({ "commands": { "keep": base["commands"]["keep"],
                                  "gone": { "kind": "mutation", "args": {}, "returns": "void",
                                            "status": "deferred" } },
                    "events": base["events"], "types": base["types"] }),
            Severity::Major,
        ),
    ];

    let was = surface_of(&base);
    for (what, after, expected) in cases {
        let severity = classify(&was, &surface_of(&after))
            .iter()
            .map(|c| c.severity)
            .max()
            .unwrap_or(Severity::None);
        assert_eq!(severity, expected, "{what} was classified as {severity:?}");
    }
}

/// The arithmetic, including the case that reads wrong at first glance: a minor
/// bump zeroes the patch, because 1.2.3 + a new command is 1.3.0 and not 1.3.3.
#[test]
fn a_bump_is_the_smallest_version_that_describes_the_change() {
    let from = Version(1, 2, 3);
    assert_eq!(least_acceptable(from, Severity::None), Version(1, 2, 3));
    assert_eq!(least_acceptable(from, Severity::Minor), Version(1, 3, 0));
    assert_eq!(least_acceptable(from, Severity::Major), Version(2, 0, 0));

    // Ordering is by field, not by string: "1.10.0" is above "1.9.0", which a
    // lexicographic comparison gets backwards.
    assert!(parse_version("1.10.0") > parse_version("1.9.0"));
}
