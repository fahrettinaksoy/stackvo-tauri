//! One guard on the render, with nothing on the machine required.
//!
//! ## What is left here, and what used to be
//!
//! This file was a golden suite. It froze twenty-five assembled service blocks,
//! the awk filter that trimmed them, the harvested volumes section and six
//! config renders — and it froze them because `real_checkout.rs` had been
//! comparing against a checkout that, on a machine where nobody had cloned
//! StackVo, was simply absent: seventeen tests returned `ok` without asserting
//! anything.
//!
//! **ADR 0016 deleted what those goldens froze.** `skeleton/core/templates/
//! services/` left the binary with the `.env` render branch, so
//! `docker-compose.dynamic.yml` is no longer assembled from templates at all —
//! it comes from the instance table and the package tree, whose own hashes
//! `pkg::verify` checks on every read. A golden file of an output nothing
//! produces is not a weakened guard; it is a fixture that can only ever be
//! deleted or lied to.
//!
//! What survives is the one assertion that was never about the frozen bytes:
//! the render must carry nothing from the machine that ran it. That failure
//! mode outlived the templates, because every remaining template still
//! interpolates the same four process-derived variables.
//!
//! The regeneration machinery went with the fixtures. It was three functions —
//! `updating`, `check`, `first_difference` — that nothing called after the
//! goldens were removed, and which `cargo clippy -- -D warnings` had been
//! reporting as dead ever since. If a golden is wanted again, it should be
//! written for what the renderer produces *now* rather than restored from the
//! shape of an output that is gone.
//!
//! `tests/fixtures/golden/overrides.env` stays: [`variables`] reads it, and it
//! is what makes this test's input a fixed workspace rather than this one.
//! `handover-before.yml` beside it belongs to `handover_equivalence.rs`.

use stackvo_desktop_lib::{config::Env, skeleton, template};
use std::path::{Path, PathBuf};

/// A workspace that does not exist.
///
/// Every template resolves workspace-first and falls back to the copy compiled
/// into the binary, so a root with nothing at it renders purely from the
/// embedded skeleton — which is what a packaged app does on a fresh machine,
/// and what makes this test independent of anything on disk.
const NO_WORKSPACE: &str = "/stackvo-golden-render-no-such-workspace";

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden")
}

/// The frozen settings, merged over the embedded defaults exactly as a real
/// workspace merges its `.env`.
fn variables() -> std::collections::BTreeMap<String, String> {
    let text = std::fs::read_to_string(golden_dir().join("overrides.env")).expect("overrides.env");
    let env = Env::parse(&text);

    // The path passed here is what `variables` would fall back to for
    // STACKVO_ROOT and HOST_STACKVO_ROOT. The fixture pins both, so this value
    // must not survive into the output — `the_render_carries_nothing_from_this_machine`
    // is what proves it does not.
    template::variables(&env, Path::new("/this-path-must-not-appear"))
}

/// The frozen files must not carry anything true only of the machine that
/// produced them.
///
/// This is the failure mode a golden test invites: a uid, a home directory or a
/// checkout path bakes itself into the fixture, and every other machine — and
/// CI — fails on a difference that is not a regression. The renderer fills all
/// four of those from the process when nothing else has, so pinning them in the
/// fixture is only half the job; this is the half that checks.
#[test]
fn the_render_carries_nothing_from_this_machine() {
    let vars = variables();

    assert_eq!(
        vars.get("HOST_STACKVO_ROOT").map(String::as_str),
        Some("/stackvo")
    );
    assert_eq!(vars.get("HOST_UID").map(String::as_str), Some("1000"));
    assert_eq!(vars.get("HOST_GID").map(String::as_str), Some("1000"));

    // Asserted against a config render rather than the assembled services file:
    // that file is no longer produced (ADR 0016), and these three variables are
    // the ones every remaining template still interpolates.
    let text = skeleton::read_template(Path::new(NO_WORKSPACE), "core/compose/base.yml")
        .expect("base.yml is compiled in");
    let rendered = template::render(&text, &vars);
    assert!(
        !rendered.contains("/this-path-must-not-appear"),
        "the fallback root reached the output — the fixture is not pinning it"
    );

    if let Some(home) = dirs::home_dir().and_then(|h| h.to_str().map(str::to_string)) {
        assert!(
            !rendered.contains(&home),
            "this machine's home directory is in the render"
        );
    }
}
