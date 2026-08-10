//! Every act the audit module claims to record is actually wired to it.
//!
//! `audit.rs` opens with a list: writes to `/etc/hosts`, certificate trust,
//! deleting a project, writing `.env`, restoring a database, loading an image
//! bundle. A list in a doc comment is a promise nothing keeps — and the failure
//! is silent in the worst way, because an audit trail that is missing entries
//! looks exactly like one for a machine where nothing happened.
//!
//! So the promise is read out of the comment and checked against the code.
//! Adding a bullet without a call site fails; so does removing a call site and
//! leaving the bullet.
//!
//! ## Why the source and not a runtime test
//!
//! Every one of these commands needs Tauri `State`, an `AppHandle`, a workspace
//! on disk and — for two of them — a password prompt. There is no way to drive
//! them from a unit test, which is the same reason `commands.rs` has the
//! coverage it has. What *can* be settled mechanically is whether the call
//! exists, and that is the half that goes wrong: the audit call is the line
//! somebody deletes while refactoring an error path, not the one they get
//! subtly wrong.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The `action` strings the module doc promises, paired with what to look for.
///
/// The action name is the audit column somebody filters on, so it is also the
/// natural thing to search the command surface for: `audit::record*` is always
/// called with a literal.
const PROMISED: &[(&str, &str)] = &[
    ("a write to `/etc/hosts`", "hosts_apply"),
    ("a change to certificate trust", "cert_apply"),
    ("deleting a project", "project_delete"),
    ("writing `.env`", "env_set"),
    ("restoring a database", "db_restore"),
    ("loading an image bundle", "release_load"),
    ("importing a site from another tool", "project_import"),
    ("moving a credential into the OS keystore", "secret_move"),
    ("taking it back out", "secret_restore"),
    (
        "registering the MCP server with an assistant",
        "agent_install",
    ),
    ("unregistering it", "agent_remove"),
];

#[test]
fn every_promised_act_has_a_call_site() {
    let commands = read("src/commands.rs");

    let missing: Vec<&str> = PROMISED
        .iter()
        .filter(|(_, action)| {
            // The literal as it appears in a `record` / `record_with` call.
            !commands.contains(&format!("\"{action}\""))
        })
        .map(|(prose, _)| *prose)
        .collect();

    assert!(
        missing.is_empty(),
        "audit.rs promises to record these and commands.rs never does:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_module_comment_and_this_list_say_the_same_thing() {
    let audit = read("src/audit.rs");

    for (prose, action) in PROMISED {
        assert!(
            audit.contains(prose),
            "this test expects audit.rs to promise `{prose}` ({action}); the \
             comment no longer says it, so either the promise was withdrawn \
             without updating this list, or it was reworded and the list is now \
             checking nothing"
        );
    }
}

/// The property that makes it an audit trail rather than a second log.
#[test]
fn the_trail_is_not_swept_up_by_the_log_rotation() {
    let logging = read("src/logging.rs");
    let audit = read("src/audit.rs");

    assert!(
        audit.contains("audit.jsonl"),
        "the trail still has its own file name"
    );
    assert!(
        !logging.contains("audit.jsonl"),
        "logging.rs must not know about the trail: the rotation keeps 7 files \
         and deletes the rest, which is the one thing this file exists not to do"
    );
}

/// `.env` values must never reach the trail.
#[test]
fn the_env_entry_records_keys_and_not_values() {
    let commands = read("src/commands.rs");

    // The argument list, not "up to the first `)`" — the first version of this
    // cut inside `keys()` and reported the production code as wrong when it was
    // the test that could not read it.
    let call = commands
        .split_once("\"env_set\",")
        .expect("env_set is audited")
        .1;
    let subject: String = call.lines().take(8).collect::<Vec<_>>().join("\n");

    assert!(
        subject.contains("patch.keys()"),
        "the subject of an env_set entry must come from the keys; \
         `.env` is where the passwords are and a trail carrying them is one \
         nobody can hand to anybody. Got: {subject}"
    );
    assert!(
        !subject.contains("values()"),
        "the values must not be in the trail: {subject}"
    );
}
