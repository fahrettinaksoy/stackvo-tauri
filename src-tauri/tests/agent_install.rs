//! The installer against a real filesystem, in a home directory of its own.
//!
//! `agents.rs`'s unit tests settle the merge, which is where the interesting
//! mistakes are — but they are all string in, string out. What they cannot
//! reach is the half that only exists once a path is involved: that the file
//! lands where `config_path` says it does, that the backup is written *before*
//! the new contents and holds the old ones, that a directory which does not
//! exist yet is created, and that removing puts the file back the way it was.
//!
//! ## Why one test function
//!
//! `HOME` and `PATH` are process-global, and Rust runs a file's tests on
//! several threads. Two tests each pointing `HOME` somewhere else would pass
//! individually and fail together, which is the worst kind of test. One
//! function, one home, executed in order.
//!
//! `PATH` is set for the same reason the sibling lookup is not relied on: the
//! integration binary lives in `target/debug/deps`, and what it is beside is
//! not what the app is beside. Putting a stand-in on `PATH` exercises the third
//! branch of [`agents::binary`] honestly rather than shaping the search to suit
//! a test.

use stackvo_desktop_lib::agents;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("stackvo-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

#[test]
fn the_whole_round_trip_against_a_real_home_directory() {
    let home = scratch("home");
    let bin = scratch("bin");

    // A file called `stackvo-mcp` that is never run — `binary()` reports what
    // it found, and running it is the client's job, not ours.
    let name = if cfg!(windows) {
        "stackvo-mcp.exe"
    } else {
        "stackvo-mcp"
    };
    let server = bin.join(name);
    std::fs::write(&server, "#!/bin/sh\n").unwrap();

    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    std::env::set_var("PATH", &bin);

    let (found, source) = agents::binary().expect("the stand-in on PATH is found");
    assert_eq!(found, server);
    assert_eq!(source, agents::Source::Path);

    // ---- a client with a file that already has a server in it -------------

    let cursor = agents::config_path("cursor").expect("cursor has a path");
    assert!(
        cursor.starts_with(&home),
        "the test is not using its own home"
    );

    let before = r#"{
  "mcpServers": {
    "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] }
  }
}
"#;
    std::fs::create_dir_all(cursor.parent().unwrap()).unwrap();
    std::fs::write(&cursor, before).unwrap();

    let written = agents::install("cursor", false, Some("/srv/stack")).expect("install");
    assert_eq!(written, cursor.display().to_string());

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cursor).unwrap()).unwrap();
    assert_eq!(
        after["mcpServers"]["stackvo"]["command"],
        server.display().to_string()
    );
    assert_eq!(
        after["mcpServers"]["stackvo"]["env"]["STACKVO_ROOT"],
        "/srv/stack"
    );
    // The other server, untouched.
    assert_eq!(after["mcpServers"]["github"]["command"], "npx");

    // The backup holds what was there, byte for byte — a backup written after
    // the new contents, or written from the parsed document, would be a copy of
    // the thing it was supposed to be protecting against.
    let backup = agents::backup_path(&cursor);
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), before);

    // ---- status agrees with the disk --------------------------------------

    let status = agents::status(Some("/srv/stack"));
    let row = status
        .clients
        .iter()
        .find(|c| c.id == "cursor")
        .expect("cursor is in the status");
    assert!(row.exists && row.present && row.parseable);
    assert!(
        row.current,
        "the registration points at the binary we found"
    );
    assert_eq!(
        row.command.as_deref(),
        Some(server.display().to_string()).as_deref()
    );

    // ---- a client whose directory does not exist yet ----------------------

    let gemini = agents::config_path("gemini-cli").unwrap();
    assert!(!gemini.exists());
    agents::install("gemini-cli", true, None).expect("install into a fresh directory");

    let fresh: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&gemini).unwrap()).unwrap();
    assert_eq!(
        fresh["mcpServers"]["stackvo"]["args"],
        serde_json::json!(["--allow-writes"])
    );
    // No workspace was passed, so no `env` is invented for one.
    assert!(fresh["mcpServers"]["stackvo"].get("env").is_none());
    // And nothing was backed up, because there was nothing to lose.
    assert!(!agents::backup_path(&gemini).exists());

    // ---- removing leaves the file the way it was --------------------------

    agents::uninstall("cursor").expect("uninstall");
    let restored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cursor).unwrap()).unwrap();
    assert!(restored["mcpServers"].get("stackvo").is_none());
    assert_eq!(restored["mcpServers"]["github"]["command"], "npx");

    // ---- a file this must not touch ---------------------------------------

    let windsurf = agents::config_path("windsurf").unwrap();
    let comments = "{\n  // mine\n  \"mcpServers\": {}\n}\n";
    std::fs::create_dir_all(windsurf.parent().unwrap()).unwrap();
    std::fs::write(&windsurf, comments).unwrap();

    assert!(agents::install("windsurf", false, None).is_err());
    assert_eq!(
        std::fs::read_to_string(&windsurf).unwrap(),
        comments,
        "a file that could not be parsed was written to anyway"
    );
    assert!(!agents::backup_path(&windsurf).exists());

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&bin);
}
