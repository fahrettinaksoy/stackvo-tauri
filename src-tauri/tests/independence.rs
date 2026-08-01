//! The app must work with no StackVo checkout anywhere.
//!
//! Everything it needs — templates, `.env.example`, the directory layout — is
//! compiled into the binary, so pointing it at an empty folder has to produce
//! a workspace that generates. This was verified by hand while the skeleton
//! was being embedded; it is a test now because the failure mode is silent.
//! A missing template does not crash, it just renders a shorter file, and the
//! only way to notice is to check that the output is actually complete.

use stackvo_desktop_lib::{commands, skeleton};

/// Installs into a fresh temp directory and renders. No `STACKVO_ROOT`, no
/// sibling checkout, nothing on disk but what `install` put there.
#[test]
fn an_empty_folder_becomes_a_working_workspace() {
    let dir = std::env::temp_dir().join(format!(
        "stackvo-independence-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    assert_eq!(
        skeleton::fitness(&dir),
        skeleton::Fitness::Installable,
        "an empty folder should be installable"
    );
    skeleton::install(&dir).expect("install");

    // The directories the generator writes into, and the settings file it
    // reads. If any of these is missing the app is still depending on a
    // checkout it no longer ships with.
    for required in ["projects", "generated", "logs", "core/templates/services"] {
        assert!(
            dir.join(required).exists(),
            "{required} missing from a freshly installed workspace"
        );
    }

    // And no settings file. The whole point of the embedded defaults is that
    // there is nothing to copy into a new workspace; a `.env` here would mean
    // something was being shipped again.
    assert!(
        !dir.join(".env").exists(),
        "a fresh workspace should carry no overrides"
    );

    let (files, skipped) = commands::render_generated(&dir).expect("render");
    assert!(
        skipped.is_empty(),
        "a fresh workspace should have nothing to skip, got {skipped:?}"
    );
    assert!(
        files.len() >= 10,
        "expected a full render, got {} file(s)",
        files.len()
    );

    // The service assembly is the piece that reaches for templates one by one,
    // so a gap in what was compiled in shows up here as a short file rather
    // than an error.
    let compose = files
        .iter()
        .find(|f| f.path.ends_with("docker-compose.dynamic.yml"))
        .expect("docker-compose.dynamic.yml should be rendered");
    for service in ["mysql", "redis"] {
        assert!(
            compose.content.contains(&format!("\n  {service}:\n")),
            "{service} missing from the assembled compose file"
        );
    }

    // Mailpit is off in a fresh workspace on purpose. The Mail page offers to
    // turn it on, and a workspace that shipped with it already running would
    // never reach that offer — the feature would look like it did nothing
    // because there was nothing left for it to do.
    assert!(
        !compose.content.contains("\n  mailpit:\n"),
        "mailpit should not be running before anyone asked for it"
    );

    // The retired web UI must not come back, whatever the settings say.
    assert!(
        !compose.content.contains("stackvo-ui"),
        "the retired containerised UI was emitted"
    );

    // Stack-shaping defaults are no longer written into a fresh `.env`, which
    // only works if they reach the renderer from the binary instead. A
    // workspace whose routes lost their domain, or whose services lost their
    // network, would still render and still start — and be unreachable. That
    // is the failure this pins down.
    assert!(
        compose.content.contains("stackvo.loc"),
        "the embedded domain suffix did not reach the routing labels"
    );
    assert!(
        compose.content.contains("stackvo-net"),
        "the embedded network name did not reach the service definitions"
    );

    std::fs::remove_dir_all(&dir).ok();
}
