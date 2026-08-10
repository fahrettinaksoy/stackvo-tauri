//! Every host this app can reach is named in `PRIVACY.md`.
//!
//! The readiness review's §4.3 asked for a decision rather than a silence: a
//! tool that collects nothing and a tool that collects something look identical
//! from the outside, so "there is no telemetry" is not a property of a build —
//! it is a claim about one. `PRIVACY.md` makes the claim. This file is what
//! stops it from quietly stopping being true.
//!
//! The failure it exists for is not a developer deciding to add analytics. It
//! is the ordinary one: a feature needs to fetch something, a URL goes into a
//! module, and the document that says "the only thing it contacts is the update
//! endpoint" is now wrong and nothing anywhere disagrees with it. `README.md`
//! had exactly this class of defect for months and it took a measurement to
//! find (`readme_claims.rs`); a privacy statement is a worse place for it,
//! because the reader has no way of checking.
//!
//! ## What is scanned
//!
//! * `src-tauri/src/*.rs`, production regions only — a `https://example.com` in
//!   a test is a fixture, not a destination.
//! * `src/**/*.{js,vue}`, excluding specs — the front end opens links in the
//!   user's browser, which leaves the machine just as surely.
//! * `tauri.conf.json`'s updater endpoints, which are a destination compiled
//!   into the build and appear in no source file at all.
//!
//! ## What is not a destination
//!
//! Four rules, each stated rather than a list of exceptions that grows every
//! time the gate is inconvenient. A blocklist of hosts to ignore is how a gate
//! stops meaning anything: the tenth entry is added without a thought, and the
//! tenth entry is the one that mattered.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The source with every top-level `#[cfg(test)]` item removed.
///
/// **Found by indentation, not by counting braces**, for the reason
/// `readme_claims.rs` records at length: a test in this repository writes
/// deliberately truncated JSON containing an unmatched `{` inside a string
/// literal, and a brace counter never recovers from it. The cheaper invariant
/// is one CI already enforces — `cargo fmt --check` runs on every push and
/// rustfmt closes a top-level item with a `}` in column zero, which nothing
/// inside a literal can imitate because rustfmt indents every line it owns.
///
/// Two copies of this scan now exist, here and in `readme_claims.rs`. That is
/// deliberate: integration tests are separate crates, sharing would mean a
/// `tests/support/` module, and the shared thing would be forty lines whose
/// only consumer is a pair of tests that must not fail together for a reason
/// neither of them is about.
fn production_regions(src: &str) -> String {
    let mut kept = String::with_capacity(src.len());
    let mut from = 0;

    while let Some(offset) = src[from..].find("\n#[cfg(test)]") {
        let start = from + offset + 1;
        kept.push_str(&src[from..start]);

        match src[start..].find("\n}\n") {
            Some(end) => from = start + end + 3,
            // An unterminated test module means the rest of the file is test
            // code — or that the file does not end in a newline, in which case
            // there is nothing after it either.
            None => return kept,
        }
    }

    kept.push_str(&src[from..]);
    kept
}

/// The authority for the host part of every `http://` or `https://` in a file.
///
/// Deliberately permissive about what a host may contain: `{`, `}` and `$`
/// appear in interpolated URLs, and a scanner that stopped at them would report
/// a truncated host as a real one — `traefik.` instead of the template it came
/// from — and the rules below would never see the brace that makes it a
/// placeholder.
fn hosts_in(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();

    for (index, _) in text.match_indices("//") {
        let before = &text[..index];
        if !(before.ends_with("http:") || before.ends_with("https:")) {
            continue;
        }

        let rest = &text[index + 2..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || "._~%:{}$[]-".contains(c)))
            .unwrap_or(rest.len());

        let authority = &rest[..end];
        if authority.is_empty() {
            continue;
        }

        // Ports are not a destination; `127.0.0.1:8025` and `127.0.0.1:1080`
        // are one host.
        let host = authority.split(':').next().unwrap_or(authority);
        if !host.is_empty() {
            found.insert(host.to_string());
        }
    }

    found
}

/// Is this something the process can actually connect to somewhere else?
///
/// * **Interpolated** — the string carries a `{`, `}` or `$`, or ends in a `.`
///   because interpolation was cut off there. The real host is decided at run
///   time by a value from somewhere else in this list.
/// * **Loopback** — `localhost`, `127.x`, `[::1]`. Traffic that never reaches
///   an interface.
/// * **Not a name** — no dot at all, like the `ssh://host/path` in an error
///   message telling the user what a URL should look like.
/// * **This machine's own stack** — `.loc`, `.test`, `.localhost`, the suffixes
///   project domains take, resolved out of `/etc/hosts` on this machine.
/// * **Reserved for documentation** — `example.com` and its siblings, which
///   RFC 2606 guarantees resolve to nothing anybody owns.
fn is_reachable_elsewhere(host: &str) -> bool {
    let interpolated = host.contains(['{', '}', '$']) || host.ends_with('.');
    let loopback = host == "localhost" || host.starts_with("127.") || host.starts_with("[::1]");
    let not_a_name = !host.contains('.');
    let own_stack =
        host.ends_with(".loc") || host.ends_with(".test") || host.ends_with(".localhost");
    let reserved = matches!(host, "example.com" | "example.org" | "example.net")
        || host.ends_with(".example.com");

    !(interpolated || loopback || not_a_name || own_stack || reserved)
}

/// Every reachable host in the shipped surface, and where it was found.
fn shipped_hosts() -> BTreeMap<String, BTreeSet<String>> {
    let root = repo_root();
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut record = |host: &str, origin: &str| {
        if is_reachable_elsewhere(host) {
            found
                .entry(host.to_string())
                .or_default()
                .insert(origin.to_string());
        }
    };

    // The Rust side, production regions only.
    let rust = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&rust)
        .expect("src/ is readable")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        for host in hosts_in(&production_regions(&read(&path))) {
            record(&host, &name);
        }
    }

    // The front end. A link the app opens leaves the machine as surely as a
    // request it makes; the browser is not a privacy boundary.
    let mut stack = vec![root.join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let interesting = name.ends_with(".vue") || name.ends_with(".js");
            if !interesting || name.ends_with(".spec.js") {
                continue;
            }
            for host in hosts_in(&read(&path)) {
                record(&host, &name);
            }
        }
    }

    // The updater endpoint: a destination that exists in no source file.
    let conf: serde_json::Value = serde_json::from_str(&read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"),
    ))
    .expect("tauri.conf.json is valid JSON");
    if let Some(endpoints) = conf["plugins"]["updater"]["endpoints"].as_array() {
        for endpoint in endpoints.iter().filter_map(|e| e.as_str()) {
            for host in hosts_in(endpoint) {
                record(&host, "tauri.conf.json");
            }
        }
    }

    found
}

/// The claim, and the surface it is made about.
#[test]
fn every_host_the_app_can_reach_is_named_in_the_privacy_statement() {
    let privacy = read(&repo_root().join("PRIVACY.md"));
    let hosts = shipped_hosts();

    let undocumented: Vec<String> = hosts
        .iter()
        .filter(|(host, _)| !privacy.contains(host.as_str()))
        .map(|(host, origins)| {
            let mut from: Vec<&str> = origins.iter().map(String::as_str).collect();
            from.sort_unstable();
            format!("{host} (from {})", from.join(", "))
        })
        .collect();

    assert!(
        undocumented.is_empty(),
        "these hosts are reachable from the shipped code and PRIVACY.md does \
         not mention them.\n\nAdd each one to the table it belongs in — the \
         app's own initiative, something the user asked for, or a Docker \
         build — or remove it from the code:\n  {}",
        undocumented.join("\n  ")
    );
}

/// A scanner that finds nothing passes everything.
///
/// The count is deliberately loose. Pinning it exactly would turn every added
/// link into a failure of this file, which is the noise `readme_claims.rs`
/// removed from itself when a hardcoded `143` started reporting each new
/// command as a scanner fault.
#[test]
fn the_scanner_finds_a_realistic_number_of_hosts() {
    let hosts = shipped_hosts();

    assert!(
        hosts.len() >= 8,
        "only {} reachable hosts found — the scan has stopped matching, and a \
         scan that finds nothing agrees with any document: {:?}",
        hosts.len(),
        hosts.keys().collect::<Vec<_>>()
    );

    // The two the statement is largely about. If either stops being found, the
    // scanner is broken in a way the loose count above would not notice.
    for expected in ["raw.githubusercontent.com", "github.com"] {
        assert!(
            hosts.contains_key(expected),
            "{expected} was not found by the scan, and it is in the tree"
        );
    }
}

/// The rules, against the strings that produced them.
///
/// Each of these was a real reading from this repository, and each would have
/// been reported as a destination by an obvious implementation.
#[test]
fn the_not_a_destination_rules_hold() {
    for placeholder in [
        "stackvo-{service}", // generator.rs, a compose service address
        "{}",                // tunnel.rs, formatted at call time
        "traefik.",          // useStackShape.js, `https://traefik.${suffix}`
        "localhost",         // the dev server
        "127.0.0.1",         // the mail catcher
        "host",              // git.rs, in "use ssh://host/path"
        "stackvo.loc",       // this machine's own stack
        "example.com",       // RFC 2606
    ] {
        assert!(
            !is_reachable_elsewhere(placeholder),
            "{placeholder} was treated as a destination"
        );
    }

    for real in [
        "raw.githubusercontent.com",
        "github.com",
        "deb.nodesource.com",
        "www.youtube.com",
    ] {
        assert!(
            is_reachable_elsewhere(real),
            "{real} was treated as a placeholder — the rules have swallowed a \
             real destination, which is the failure this gate cannot report"
        );
    }
}

/// Ports are not destinations, and an interpolated port must not hide the host.
#[test]
fn a_host_is_found_without_its_port() {
    let found = hosts_in(r#"format!("http://127.0.0.1:{port}")"#);
    assert_eq!(found, BTreeSet::from(["127.0.0.1".to_string()]));

    let found = hosts_in("https://raw.githubusercontent.com/stackvo/x/main/latest.json");
    assert_eq!(
        found,
        BTreeSet::from(["raw.githubusercontent.com".to_string()])
    );
}

/// Test fixtures are not destinations — the property the region scan exists for.
#[test]
fn a_url_inside_a_test_module_is_not_a_destination() {
    let src = "fn real() { let _ = \"https://kept.example-real.com/x\"; }\n\
               #[cfg(test)]\n\
               mod tests {\n    const URL: &str = \"https://fixture-only.invalid-host.com\";\n}\n";

    let hosts = hosts_in(&production_regions(src));
    assert!(hosts.contains("kept.example-real.com"));
    assert!(
        !hosts.contains("fixture-only.invalid-host.com"),
        "a host from a #[cfg(test)] module reached the scan"
    );
}
