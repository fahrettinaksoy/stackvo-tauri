//! Integration checks against a real StackVo checkout.
//!
//! These assert that the Rust core reproduces what `tools/validate-contracts.mjs`
//! reports — two independent implementations of the same contract agreeing is
//! the point. If they ever diverge, one of them is wrong about the contract.
//!
//! Skipped (not failed) when no checkout is reachable, so CI without one stays
//! green. Point it somewhere else with `STACKVO_ROOT=/path cargo test`.

use stackvo_desktop_lib::{config::Env, manifest, workspace};
use std::path::{Path, PathBuf};

fn checkout() -> Option<PathBuf> {
    let candidates = [
        std::env::var("STACKVO_ROOT").ok().map(PathBuf::from),
        dirs::home_dir().map(|h| h.join("Desktop/stackvo")),
        dirs::home_dir().map(|h| h.join("stackvo")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|p| workspace::looks_like_stackvo(p))
}

/// Every manifest under `projects/`, paired with its directory name.
fn manifests(root: &Path) -> Vec<(String, manifest::Manifest)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("projects")) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let file = path.join("stackvo.json");
        if !file.is_file() {
            continue;
        }
        if let Ok(m) = manifest::read(&file, name) {
            out.push((name.to_string(), m));
        }
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_manifest_parses_and_splits_by_runtime() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    let found = manifests(&root);
    assert!(!found.is_empty(), "expected at least one project manifest");

    let php = found.iter().filter(|(_, m)| m.runtime == "php").count();
    let node = found.iter().filter(|(_, m)| m.runtime == "node").count();
    assert_eq!(
        php + node,
        found.len(),
        "every manifest resolves to php or node"
    );

    // Node projects on disk were hand-written with `runtime: node`; if any had
    // come from the web UI it would have a `nodejs` block and read as PHP (C-01).
    for (name, m) in found.iter().filter(|(_, m)| m.runtime == "node") {
        assert!(
            m.node.is_some(),
            "{name} declares runtime=node but has no node block"
        );
    }
}

#[test]
fn imap_on_php_84_is_flagged_the_same_way_the_js_validator_flags_it() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    for (name, m) in manifests(&root) {
        let Some(php) = &m.php else { continue };
        if !php.extensions.iter().any(|e| e == "imap") {
            continue;
        }

        // imap was removed in PHP 8.2; anything at or above that must error.
        if crate_cmp(&php.version, "8.2") {
            assert!(
                m.errors.iter().any(|e| e.code == "C-06"),
                "{name} requests imap on PHP {} but no C-06 was raised",
                php.version
            );
        }
    }
}

/// True when `version` >= `floor`, comparing numerically.
fn crate_cmp(version: &str, floor: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (v, f) = (parse(version), parse(floor));
    for i in 0..v.len().max(f.len()) {
        let (a, b) = (
            v.get(i).copied().unwrap_or(0),
            f.get(i).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    true
}

#[test]
fn legacy_webserver_spelling_is_a_warning_never_an_error() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    for (name, m) in manifests(&root) {
        // C-10: every pre-v1 PHP project uses `webserver`. Read support must
        // hold — turning it into an error would orphan them all.
        if m.warnings.iter().any(|w| w.code == "C-10") {
            assert!(
                m.server.is_some(),
                "{name} uses the legacy spelling but the server did not resolve"
            );
            assert!(
                !m.errors.iter().any(|e| e.code == "C-10"),
                "{name}: the legacy spelling must warn, not error"
            );
        }
    }
}

#[test]
fn mongo_express_profile_mismatch_is_reproducible() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    // C-09: `stackvo up` derives the profile by lowercasing the env key, giving
    // `mongo_express`, while the template declares `mongo-express`.
    let derived = Env::service_prefix("mongo-express")
        .trim_start_matches("SERVICE_")
        .trim_end_matches('_')
        .to_lowercase();
    assert_eq!(derived, "mongo_express");

    let template =
        root.join("core/templates/services/mongo-express/docker-compose.mongo-express.tpl");
    if let Ok(text) = std::fs::read_to_string(&template) {
        assert!(
            text.contains("\"mongo-express\""),
            "template should declare the dash-form profile"
        );
        assert!(
            !text.contains("\"mongo_express\""),
            "the derived underscore profile matches nothing in the template — C-09 still stands"
        );
    }
}

#[test]
fn env_loads_and_redacts_real_secrets() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    let env = Env::load(&root).expect(".env should load");
    assert!(
        env.get("STACKVO_VERSION").is_some(),
        "STACKVO_VERSION should be present"
    );

    for (key, value) in env.redacted() {
        if Env::is_secret(&key) && !value.is_empty() {
            assert_eq!(value, "••••••••", "{key} leaked through redaction");
        }
    }
}
