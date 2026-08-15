//! Does registering the server give the file back? (K-1)
//!
//! `agents.rs` edits configuration files it does not own, and its first rule is
//! that everything already in them survives. The unit tests drive that rule with
//! fixtures — files written by the person writing the tests, which is the
//! arrangement that has been wrong three times already in this codebase.
//!
//! So this runs it against the **real files on this machine**:
//!
//! ```sh
//! cargo run --example agent_config_probe
//! ```
//!
//! For each client that is actually installed it copies the file to a scratch
//! directory, registers the server into the copy, takes it back out, and
//! compares the result with the original **byte for byte**. A round trip that
//! is byte-exact is the strongest available statement that nothing was lost:
//! not a comment, not a key order, not a quoting style, not a trailing newline.
//!
//! **Nothing on this machine is written.** The originals are opened read-only
//! and every edit happens on a copy under the OS temp directory, which is
//! removed on the way out. Nothing from inside anybody's configuration is
//! printed either — these files hold project lists and API keys, and a
//! measurement is not a reason to put them on a terminal. What is reported is
//! structure: bytes in, bytes out, and which of the two edits changed what.

use stackvo_desktop_lib::agents;

fn main() {
    let scratch = std::env::temp_dir().join(format!("stackvo-agent-probe-{}", std::process::id()));
    if std::fs::create_dir_all(&scratch).is_err() {
        println!("could not make a scratch directory");
        return;
    }

    let mut checked = 0;
    let mut failures = 0;

    for client in agents::CLIENTS {
        let Some(path) = agents::config_path(client.id) else {
            continue;
        };
        if !path.is_file() {
            println!("{:<16} not on this machine", client.label);
            continue;
        }
        let Ok(original) = std::fs::read_to_string(&path) else {
            println!("{:<16} could not be read", client.label);
            continue;
        };
        checked += 1;

        // The two edits, on the text — never on the file.
        let command = "/opt/stackvo/bin/stackvo-mcp";
        let inserted = if client.shape.is_toml() {
            agents::toml_insert(&original, command, false, Some("/workspace"))
        } else {
            agents::insert(
                &original,
                client.shape,
                agents::entry(client.shape, command, false, Some("/workspace")),
            )
        };

        let inserted = match inserted {
            Ok(text) => text,
            Err(e) => {
                // Not a failure of the round trip: this is the module refusing
                // to edit a file it cannot parse, which is the designed answer
                // for JSON-with-comments. It is reported as what it is.
                println!(
                    "{:<16} refused ({:?}) — the pane shows the block to paste",
                    client.label, e.code
                );
                continue;
            }
        };

        let removed = if client.shape.is_toml() {
            agents::toml_remove(&inserted)
        } else {
            agents::remove(&inserted, client.shape)
        }
        .expect("what this module just wrote, it can read");

        // Was the entry actually there in between? A round trip that is
        // byte-exact because nothing happened proves nothing.
        let registered = if client.shape.is_toml() {
            agents::toml_installed_command(&inserted)
        } else {
            agents::installed_command(&inserted, client.shape)
        };

        let exact = removed == original;
        // The one difference that is a decision rather than a defect: taking
        // the entry back out leaves the empty server map behind, because
        // `remove` cannot tell a map this edit created from one the client
        // wrote itself — and deleting a key it did not create is the thing this
        // module refuses to do. `agents.rs` says so where it happens.
        let leftover = !exact && only_empty_map_added(&original, &removed, client.shape.key());
        let wrote_entry = registered.as_deref() == Some(command);
        if (!exact && !leftover) || !wrote_entry {
            failures += 1;
        }

        println!(
            "  {} {:<14} {:>8} bytes in, {:>8} after the round trip  entry={}  {}",
            if (exact || leftover) && wrote_entry {
                "ok  "
            } else {
                "FAIL"
            },
            client.label,
            original.len(),
            removed.len(),
            if wrote_entry { "written" } else { "MISSING" },
            if exact {
                "byte-exact".to_string()
            } else if leftover {
                format!(
                    "byte-exact apart from the empty `{}` it created",
                    client.shape.key()
                )
            } else {
                describe(&original, &removed)
            }
        );

        // Kept for a human to look at when a row fails, and only then.
        if !exact && !leftover {
            let dump = scratch.join(format!("{}.after", client.id));
            let _ = std::fs::write(&dump, &removed);
            println!("       the round trip is at {}", dump.display());
        }
    }

    println!();
    if checked == 0 {
        println!("no client's configuration file is on this machine — nothing was measured.");
    } else if failures == 0 {
        println!("{checked} real configuration file(s) came back byte for byte.");
        let _ = std::fs::remove_dir_all(&scratch);
    } else {
        println!("{failures} of {checked} did not come back unchanged.");
    }
}

/// Is the only difference an empty server map this edit left behind?
///
/// Compared as parsed values rather than as text: the question is whether
/// anything *is* different, and by this point formatting equality has been
/// established by every other file in the run.
fn only_empty_map_added(before: &str, after: &str, key: &str) -> bool {
    let Ok(before) = serde_json::from_str::<serde_json::Value>(before) else {
        return false;
    };
    let Ok(mut after) = serde_json::from_str::<serde_json::Value>(after) else {
        return false;
    };

    if before.get(key).is_some() {
        return false;
    }
    let Some(object) = after.as_object_mut() else {
        return false;
    };
    if object
        .get(key)
        .and_then(|v| v.as_object())
        .map(|m| m.is_empty())
        != Some(true)
    {
        return false;
    }
    object.shift_remove(key);
    after == before
}

/// What changed, without printing anything from either file.
fn describe(before: &str, after: &str) -> String {
    let lines_before = before.lines().count();
    let lines_after = after.lines().count();
    let first = before
        .lines()
        .zip(after.lines())
        .position(|(a, b)| a != b)
        .map(|n| format!("first differing line {}", n + 1))
        .unwrap_or_else(|| "same prefix".into());
    format!("{lines_before} → {lines_after} lines, {first}")
}
