//! Differential check against frozen Bash output, one fixture per server.
//!
//! The user's checkout only contains nginx projects, so `differential.rs` can
//! only ever verify nginx. These fixtures were produced by running the real
//! Bash generator in a throwaway sandbox — a copy of `core/` and `.env` with
//! synthetic projects — so all five web servers and the Node runtime have
//! reference output, and the check needs neither Bash nor a checkout to run.
//!
//! Regenerate with `tools/make-fixtures.sh` if the Bash generator changes; the
//! diff then shows exactly what changed about the produced images.

use stackvo_desktop_lib::{generator, manifest};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The toolchain values the fixtures were generated with, frozen alongside them
/// so a change to the user's .env cannot silently invalidate the comparison.
fn toolchain() -> generator::ToolchainOptions {
    let text = std::fs::read_to_string(fixtures().join("toolchain.env")).expect("toolchain.env");
    let mut vars = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            vars.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    let list = |key: &str| -> Vec<String> {
        vars.get(key)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };

    generator::ToolchainOptions {
        tools: list("PHP_DEFAULT_TOOLS"),
        apt_packages: list("PHP_DEFAULT_APT_PACKAGES"),
        composer_version: vars
            .get("PHP_TOOL_COMPOSER_VERSION")
            .cloned()
            .unwrap_or("latest".into()),
        nodejs_version: vars
            .get("PHP_TOOL_NODEJS_VERSION")
            .cloned()
            .unwrap_or("20".into()),
    }
}

/// First differing line with context. A whole-file diff is unreadable and
/// tends to hide the one line that actually matters.
fn first_difference(ours: &str, theirs: &str) -> Option<String> {
    let a: Vec<&str> = ours.lines().collect();
    let b: Vec<&str> = theirs.lines().collect();

    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or("<missing>");
        let y = b.get(i).copied().unwrap_or("<missing>");
        if x != y {
            let from = i.saturating_sub(3);
            let context: Vec<String> = (from..i)
                .filter_map(|j| b.get(j).map(|l| format!("     {l}")))
                .collect();
            return Some(format!(
                "line {}:\n{}\n  bash: {y:?}\n  rust: {x:?}",
                i + 1,
                context.join("\n")
            ));
        }
    }
    None
}

fn check(fixture: &str) -> Result<(), String> {
    let dir = fixtures().join(fixture);
    let m = manifest::read(&dir.join("stackvo.json"), fixture)
        .map_err(|e| format!("{fixture}: manifest did not parse: {}", e.message))?;

    // compat mode: the fixtures were produced by the Bash generator, which
    // skips incompatible extensions silently.
    let ours = generator::render_from_manifest(&m, &toolchain(), false)
        .map_err(|e| format!("{fixture}: render failed: {e}"))?;
    let theirs = std::fs::read_to_string(dir.join("Dockerfile"))
        .map_err(|e| format!("{fixture}: no reference Dockerfile: {e}"))?;

    match first_difference(&ours, &theirs) {
        None => Ok(()),
        Some(diff) => Err(format!("\n=== {fixture} ===\n{diff}")),
    }
}

#[test]
fn nginx_matches() {
    check("probe-nginx").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn apache_matches() {
    check("probe-apache").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn caddy_matches() {
    check("probe-caddy").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn frankenphp_matches() {
    check("probe-frankenphp").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn swoole_matches() {
    check("probe-swoole").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn node_matches() {
    check("probe-node").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn node_dockerignore_matches() {
    let theirs = std::fs::read_to_string(fixtures().join("probe-node/.dockerignore"))
        .expect("reference .dockerignore");
    assert_eq!(generator::NODE_DOCKERIGNORE, theirs);
}

#[test]
fn compose_projects_matches() {
    use stackvo_desktop_lib::generator::{compose_projects_from, render_compose_projects};

    let dir = fixtures();
    let mut manifests = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("fixtures").flatten() {
        let path = entry.path();
        // Only project fixtures carry a manifest; traefik/ does not.
        if !path.is_dir() || !path.join("stackvo.json").is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();
        let m = manifest::read(&path.join("stackvo.json"), &name).expect("manifest");
        manifests.push((name, m));
    }

    let projects = compose_projects_from(&manifests);
    // The fixture was written with the sandbox path replaced by this
    // placeholder, so rendering with it as the host root makes the two
    // comparable without depending on where the sandbox happened to live. The
    // project tree was `<root>/projects` when Bash wrote it, which is what the
    // second argument says.
    let ours = render_compose_projects(&projects, "__ROOT__", "__ROOT__/projects");
    let theirs = std::fs::read_to_string(dir.join("docker-compose.projects.yml"))
        .expect("reference compose file");

    // One line differs on purpose, and the fixture stays as Bash wrote it so
    // that the difference is visible here rather than edited into the
    // reference.
    //
    // Bash gave a Node project the build context `../projects/<name>`, relative
    // to `generated/` and therefore only correct while the project tree was a
    // fixed distance from the compose file. It is a directory the user chooses
    // now, possibly on another volume, and no relative path reaches that. The
    // absolute form names the same directory in the single-root layout this
    // fixture describes — which is what makes rewriting the expectation honest
    // rather than convenient.
    let theirs = theirs.replace("context: ../projects/", "context: __ROOT__/projects/");

    if let Some(diff) = first_difference(&ours, &theirs) {
        panic!("\n=== docker-compose.projects.yml ===\n{diff}");
    }
}

// ---------------------------------------------------------------- traefik

/// Two fixtures per file: SSL on and SSL off. The off variant is not a corner
/// case — it changes both files and exposes the inconsistency in C-20.
fn traefik_opts(
    ssl: bool,
    services: Vec<(&'static str, bool)>,
) -> generator::TraefikOptions<'static> {
    generator::TraefikOptions {
        tld_suffix: "stackvo.loc",
        network: "stackvo-net",
        ssl_enabled: ssl,
        redirect_to_https: true,
        services: services
            .into_iter()
            .map(|(id, on)| (id, on, None))
            .collect(),
    }
}

fn traefik_fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures().join("traefik").join(name))
        .unwrap_or_else(|e| panic!("reference {name}: {e}"))
}

#[test]
fn traefik_config_matches_with_ssl() {
    let opts = traefik_opts(true, vec![]);
    let ours = generator::render_traefik_config(&opts);
    if let Some(d) = first_difference(&ours, &traefik_fixture("traefik-ssl.yml")) {
        panic!("\n=== traefik.yml (ssl) ===\n{d}");
    }
}

#[test]
fn traefik_config_matches_without_ssl() {
    let opts = traefik_opts(false, vec![]);
    let ours = generator::render_traefik_config(&opts);
    if let Some(d) = first_difference(&ours, &traefik_fixture("traefik-nossl.yml")) {
        panic!("\n=== traefik.yml (no ssl) ===\n{d}");
    }
}

#[test]
fn traefik_routes_match_with_ssl() {
    // These fixtures were frozen from the Bash generator; since the takeover
    // they document the Rust renderer's own contract, updated when the
    // routed-service set changes (mailhog -> mailpit, Sprint 19) — and now
    // that the bare suffix has a router of its own. Bash never wrote one, so
    // `https://<suffix>/` resolved, presented a valid certificate and returned
    // 404 with nothing on the machine explaining why.
    let opts = traefik_opts(
        true,
        vec![
            ("rabbitmq", true),
            ("mailpit", true),
            ("kibana", true),
            ("grafana", true),
        ],
    );
    let ours = generator::render_traefik_routes(&opts);
    if let Some(d) = first_difference(&ours, &traefik_fixture("routes-ssl.yml")) {
        panic!("\n=== routes.yml (ssl) ===\n{d}");
    }
}

#[test]
fn traefik_routes_match_with_a_subset_and_no_ssl() {
    let opts = traefik_opts(
        false,
        vec![
            ("rabbitmq", true),
            ("mailpit", true),
            ("kibana", false),
            ("grafana", false),
        ],
    );
    let ours = generator::render_traefik_routes(&opts);
    if let Some(d) = first_difference(&ours, &traefik_fixture("routes-nossl.yml")) {
        panic!("\n=== routes.yml (no ssl) ===\n{d}");
    }
}

#[test]
fn disabled_ssl_is_reported_as_a_broken_routing_setup() {
    // C-20: routers target `websecure`, which only exists when SSL is on.
    let on = traefik_opts(true, vec![]);
    assert!(generator::traefik_routing_warning(&on).is_none());

    let off = traefik_opts(false, vec![]);
    let warning = generator::traefik_routing_warning(&off).expect("warning expected");
    assert!(warning.contains("websecure"));

    // And the output really is inconsistent, not just flagged.
    assert!(!generator::render_traefik_config(&off).contains("websecure"));
    assert!(generator::render_traefik_routes(&off).contains("websecure"));
}
