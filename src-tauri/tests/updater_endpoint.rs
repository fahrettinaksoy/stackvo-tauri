//! The updater points at something that will exist.
//!
//! §3 #2 was never an engineering problem. `tauri.conf.json` carried an
//! endpoint, the endpoint answered 404, and the row said so — but the reason it
//! answered 404 was a **decision**: nobody had said where `latest.json` would
//! be published or who would hold the signing key. §5 held it, and #21 (release
//! channels, staged rollout, rollback) sat behind it, and so did the key
//! ceremony half of §2 C. One answer, three rows.
//!
//! The answer is GitHub Releases on this repository. Which makes the endpoint
//! derivable rather than typed, and that is what this file checks — because the
//! old one was typed, and it was wrong in two independent ways at once:
//!
//! * **wrong owner.** It named `stackvo/stackvo-tauri`; the remote is
//!   `fahrettinaksoy/stackvo-tauri`. An updater pointed at a repository nobody
//!   owns cannot be fixed by publishing a release.
//! * **wrong mechanism.** It read `latest.json` off the `main` branch through
//!   `raw.githubusercontent.com`, and nothing writes that file to `main`.
//!   `tauri-action` writes it *into the release*, which is where
//!   `releases/latest/download/` serves it from.
//!
//! Either alone is a silent failure: the updater asks, gets a 404, and says
//! nothing — `dialog: false` means this app decides what to show, and what it
//! showed was nothing at all. Two of them meant the row could have been "fixed"
//! once and still been broken.
//!
//! ## What is deliberately not checked
//!
//! That the URL *answers*. This test does no network access — a gate that
//! failed when GitHub was slow, or when the first release genuinely had not
//! happened yet, would be a gate people learn to ignore. What can be checked
//! without a network is that the URL is the one this repository's own release
//! workflow produces, and that is the failure mode that actually occurred.

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

fn endpoints() -> Vec<String> {
    let conf: serde_json::Value =
        serde_json::from_str(&read("src-tauri/tauri.conf.json")).expect("tauri.conf.json parses");
    conf["plugins"]["updater"]["endpoints"]
        .as_array()
        .expect("the updater declares its endpoints")
        .iter()
        .filter_map(|e| e.as_str().map(String::from))
        .collect()
}

/// `owner/repo`, from git's own config.
///
/// Read out of `.git/config` rather than from a constant, for the reason the
/// old endpoint demonstrates: a constant is a second copy of a fact, and the
/// copy is the one that goes stale. Returns `None` in a checkout with no
/// origin — a source tarball, or CI cloning some other way — and the test says
/// what it could not check rather than failing on it.
fn origin_slug() -> Option<String> {
    let config = std::fs::read_to_string(repo_root().join(".git/config")).ok()?;
    let url = config
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("url = "))?;
    let path = url
        .trim_end_matches(".git")
        .rsplit_once("github.com")
        .map(|(_, rest)| rest.trim_start_matches([':', '/']))?
        .to_string();
    (path.matches('/').count() == 1).then_some(path)
}

/// The endpoint is the release asset, on the repository this checkout came
/// from.
#[test]
fn the_updater_asks_the_release_that_this_repository_publishes() {
    let endpoints = endpoints();
    assert_eq!(
        endpoints.len(),
        1,
        "the updater declares {} endpoints. More than one is a fallback chain, \
         and a fallback for a signed manifest is a second place a release has \
         to be published to — say so here before adding it.",
        endpoints.len()
    );
    let endpoint = &endpoints[0];

    let Some(slug) = origin_slug() else {
        eprintln!("no github origin in .git/config — the owner half is unchecked here");
        return;
    };

    let expected = format!("https://github.com/{slug}/releases/latest/download/latest.json");
    assert_eq!(
        endpoint, &expected,
        "the updater endpoint is not the one this repository's release \
         workflow produces.\n  declared: {endpoint}\n  expected: {expected}\n\n\
         `tauri-action` runs with `includeUpdaterJson: true`, which writes \
         latest.json into the release; `releases/latest/download/` is where \
         that file is served from. An endpoint anywhere else answers 404, and \
         with `dialog: false` the app has no way to tell anybody it did."
    );
}

/// The workflow still produces the file the endpoint asks for.
///
/// The endpoint and the flag that writes the file are in two different files
/// and neither mentions the other. Dropping `includeUpdaterJson` would leave a
/// correct-looking URL pointing at nothing.
#[test]
fn the_release_workflow_still_writes_the_file_the_endpoint_asks_for() {
    let workflow = read(".github/workflows/release.yml");
    assert!(
        workflow.contains("includeUpdaterJson: true"),
        "release.yml no longer sets `includeUpdaterJson: true`, so no \
         latest.json is written into the release — and the updater endpoint in \
         tauri.conf.json asks for exactly that file."
    );
    assert!(
        workflow.contains("TAURI_SIGNING_PRIVATE_KEY"),
        "release.yml no longer passes the signing key. An unsigned latest.json \
         is refused by the updater against the `pubkey` compiled into \
         tauri.conf.json, which is the whole point of that key."
    );
}

/// The public half in the config, and the private half named as a secret.
///
/// The key ceremony is what §5 was actually holding: the private key lives at
/// `~/.tauri/stackvo.key` on one machine, and until it is a repository secret
/// no release can be signed — which is why #22's ARM rows were stuck behind a
/// question that had nothing to do with ARM.
#[test]
fn the_updater_carries_a_public_key_and_the_workflow_names_its_private_half() {
    let conf: serde_json::Value =
        serde_json::from_str(&read("src-tauri/tauri.conf.json")).expect("tauri.conf.json parses");
    let pubkey = conf["plugins"]["updater"]["pubkey"]
        .as_str()
        .expect("the updater carries a pubkey");
    assert!(
        !pubkey.is_empty(),
        "the updater's `pubkey` is empty — every update would be accepted \
         unverified, which is worse than having no updater"
    );

    let preflight = read(".github/workflows/release.yml");
    assert!(
        preflight.contains("secrets.TAURI_SIGNING_PRIVATE_KEY"),
        "the private half is no longer read from a repository secret"
    );
}
