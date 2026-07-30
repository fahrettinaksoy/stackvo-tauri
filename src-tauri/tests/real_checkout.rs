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

/// The parser, against a certificate mkcert actually produced.
///
/// `certs::parse_pem` is unit-tested against a synthetic PEM, which proves it
/// reads X.509 — not that it reads *mkcert's* X.509. The SAN list is the whole
/// output of this feature, so it is worth checking against the real thing on
/// any machine that has one.
#[test]
fn the_real_wildcard_certificate_parses() {
    use stackvo_desktop_lib::certs;

    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    let path = certs::cert_path(&root);
    let Ok(pem) = std::fs::read(&path) else {
        eprintln!("skipping: no certificate at {}", path.display());
        return;
    };

    let facts = certs::parse_pem(&pem).expect("mkcert's own output should parse");

    let suffix = Env::load(&root)
        .ok()
        .and_then(|e| e.get("DEFAULT_TLD_SUFFIX").map(str::to_string))
        .unwrap_or_else(|| certs::FALLBACK_SUFFIX.to_string());

    assert!(
        facts.sans.iter().any(|s| s == &format!("*.{suffix}")),
        "the wildcard for {suffix} should be in the SAN list, got {:?}",
        facts.sans
    );
    assert!(
        facts.not_after.is_some(),
        "a certificate always has a not_after"
    );

    // Not an assertion about the developer's machine — an expired certificate
    // is a legitimate state, and reporting it is the point.
    eprintln!(
        "certificate covers {} name(s), {} day(s) remaining",
        facts.sans.len(),
        facts
            .days_remaining
            .map(|d| d.to_string())
            .unwrap_or_else(|| "expired, 0".into())
    );
}

/// The reload has to work against the directory the generator really writes,
/// not only against a fixture shaped like it.
///
/// Reissuing a certificate replaces a file Traefik does not watch: the proxy
/// watches `generated/traefik/dynamic`, and reads a `certFile` only while
/// parsing what it finds there. On the checkout this was written against,
/// Traefik had been up two days serving a certificate a day older than the one
/// on disk. Rewriting the watched files with their own bytes is what closes
/// that gap, so this asserts both halves: that it finds something to rewrite,
/// and that the bytes survive — `generated/` is under a byte-for-byte contract
/// with the Bash generator, and a reload that reformatted a file would break it.
#[test]
fn the_traefik_reload_touches_real_config_without_altering_it() {
    use stackvo_desktop_lib::certs;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let dir = root.join("generated").join("traefik").join("dynamic");
    if !dir.is_dir() {
        eprintln!("{} has not been generated, skipping", dir.display());
        return;
    }

    let before: Vec<(PathBuf, String)> = std::fs::read_dir(&dir)
        .expect("reading the dynamic directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|text| (p, text)))
        .collect();

    assert!(
        certs::reload_proxy(&root),
        "a generated dynamic directory always holds at least routes.yml"
    );

    for (path, text) in &before {
        assert_eq!(
            &std::fs::read_to_string(path).expect("re-reading after the reload"),
            text,
            "{} changed, and nothing under generated/ may",
            path.display()
        );
    }

    eprintln!("reloaded {} dynamic config file(s)", before.len());
}

/// Discovery has to find what real projects actually write, which is not what a
/// fixture would contain: Laravel channels nest into subdirectories, roll over
/// daily, and sit alongside a separate tree of nginx, php-fpm and supervisord
/// files that the stack mounts from `logs/projects/<name>`.
#[test]
fn real_projects_expose_the_logs_they_actually_write() {
    use stackvo_desktop_lib::applog;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let mut total = 0;
    for (name, _) in manifests(&root) {
        let files = match applog::candidates(&root, &name) {
            Ok(files) => files,
            Err(e) => panic!("{name}: {e:?}"),
        };
        total += files.len();

        for file in &files {
            // Every id round-trips: what discovery hands the UI is exactly what
            // the UI can hand back and have opened.
            let path = applog::resolve(&root, &name, &file.id)
                .unwrap_or_else(|e| panic!("{name}: {} did not resolve: {e:?}", file.id));
            assert!(path.is_file(), "{}", path.display());

            // And it opens. A listed file that cannot be read is a menu entry
            // that produces an empty pane.
            applog::tail(&path, 4096)
                .unwrap_or_else(|e| panic!("{name}: {} did not read: {e:?}", file.id));
        }

        if !files.is_empty() {
            eprintln!("{name}: {} file(s), e.g. {}", files.len(), files[0].label);
        }
    }

    // Not an assertion that any particular project logs — a fresh checkout may
    // not have run anything yet.
    eprintln!("{total} log file(s) discovered across the checkout");
}

/// Quick commands against the projects that actually exist here.
///
/// The claim worth checking on real data is that the offer matches the files:
/// a project with `artisan` gets `tinker`, one without must not, and no project
/// is ever offered something it cannot run. A fixture can only restate the
/// filter; eleven real checkouts exercise it.
#[test]
fn quick_commands_match_what_each_real_project_has() {
    use stackvo_desktop_lib::{detect, quickcmd};

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let mut offered_any = 0usize;
    for (name, _) in manifests(&root) {
        let dir = root.join("projects").join(&name);
        let print = detect::fingerprint(&dir);
        let commands = quickcmd::for_project(&root, &name).expect("commands");

        for command in &commands {
            // Every offer resolves back to the catalog — the id the UI is given
            // is exactly the id `quick_command_run` will accept.
            let spec = quickcmd::resolve(&command.id).expect("offered id must resolve");
            assert_eq!(spec.display, command.display);
            // And the marker it claims really is there.
            assert!(
                dir.join(&command.because).exists(),
                "{name}: offered {} on the strength of {}, which is absent",
                command.id,
                command.because
            );
        }

        let has_tinker = commands.iter().any(|c| c.id == "tinker");
        assert_eq!(
            has_tinker, print.artisan,
            "{name}: tinker offered={has_tinker}, artisan present={}",
            print.artisan
        );

        if !commands.is_empty() {
            offered_any += 1;
            eprintln!("{name}: {} command(s)", commands.len());
        }
    }

    eprintln!("{offered_any} project(s) have at least one command");
}

/// The dump catcher, against a real running project.
///
/// The whole design rests on Symfony's own collector rendering the dumps, so
/// what is worth checking on a real machine is exactly that: start it, send a
/// dump, and confirm readable text comes back. A fixture cannot check it,
/// because the rendering is the part this app does not own.
#[tokio::test]
async fn symfony_renders_a_real_dump_through_its_own_collector() {
    use stackvo_desktop_lib::dumps;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    // Any running PHP project that has the collector installed.
    let Some(name) = manifests(&root)
        .into_iter()
        .map(|(name, _)| name)
        .find(|name| {
            root.join("projects")
                .join(name)
                .join(dumps::BINARY)
                .is_file()
        })
    else {
        eprintln!("skipping: no project has {} installed", dumps::BINARY);
        return;
    };

    let argv = match dumps::prepare(&root, &name).await {
        Ok(argv) => argv,
        Err(e) => {
            eprintln!("skipping: {name} cannot run the collector ({e:?})");
            return;
        }
    };

    let container = stackvo_desktop_lib::engine::container_name(&name);
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let sink = collected.clone();
    let mut server = tokio::process::Command::new("docker")
        .args(&argv)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn the collector");

    let stdout = server.stdout.take().expect("piped");
    let reader = tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !dumps::is_banner(&line) {
                sink.lock().unwrap().push(line);
            }
        }
    });

    // Let it bind before dumping at it.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let dumped = tokio::process::Command::new("docker")
        .args(["exec", "-w", "/var/www/html"])
        .args(["-e", "VAR_DUMPER_FORMAT=server"])
        .args([
            "-e",
            &format!("VAR_DUMPER_SERVER={}", dumps::server_address()),
        ])
        .arg(&container)
        .args([
            "php",
            "-r",
            "require 'vendor/autoload.php'; dump(['stackvo' => ['caught' => true]]);",
        ])
        .output()
        .await;

    if dumped.map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("skipping: could not dump into {name}");
        return;
    }

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    // Through the product's own cleanup, which is the half that was broken:
    // killing the `docker exec` client leaves the PHP process holding the port,
    // and the next run fails with "Address already in use".
    dumps::stop(&container).await;
    let _ = server.kill().await;
    let _ = reader.await;

    let lines = collected.lock().unwrap().clone();
    let text = lines.join("\n");

    assert!(
        text.contains("\"stackvo\""),
        "the dump never arrived; collector said:\n{text}"
    );
    // Rendered by Symfony, not decoded here — the point of the whole design.
    assert!(text.contains("array:1 ["), "not rendered output:\n{text}");
    assert!(
        !text.contains("Symfony Var Dumper Server"),
        "the banner leaked into the pane"
    );

    eprintln!("dumps: {} line(s) caught from {name}", lines.len());
}

/// The dev-server overlay, merged by real Docker.
///
/// What is checked here is that Compose does to this overlay what the feature
/// assumes: the `command` is *replaced* rather than appended to, and the two
/// volumes both survive with the anonymous one intact. Both are Compose's
/// merge semantics rather than this code's, which is exactly why asserting
/// them against a hand-written expectation would prove nothing.
///
/// The runtime half was verified once by hand and is not repeated here because
/// it needs an image build: without the anonymous volume, `require()` inside
/// the container fails with MODULE_NOT_FOUND because the bind hides the
/// image's `node_modules`; with it, the module loads, and an edit on the host
/// reaches the container with no rebuild.
#[tokio::test]
async fn docker_merges_the_dev_server_overlay_as_an_override() {
    use stackvo_desktop_lib::devserver;

    let dir = std::env::temp_dir().join("stackvo-devserver-real");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let base = dir.join("base.yml");
    std::fs::write(
        &base,
        "name: devprobe\nservices:\n  site:\n    image: node:22-alpine\n    command: [\"node\", \"server.js\"]\n",
    )
    .unwrap();

    let overlay = dir.join("overlay.yml");
    std::fs::write(
        &overlay,
        devserver::overlay_yaml(&[devserver::Entry {
            service: "site".into(),
            host_path: dir.display().to_string(),
            command: "npm run dev".into(),
        }])
        .unwrap(),
    )
    .unwrap();

    let output = tokio::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(&base)
        .arg("-f")
        .arg(&overlay)
        .args(["config", "--format", "json"])
        .output()
        .await;

    let Ok(output) = output else {
        eprintln!("skipping: docker unavailable");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    };
    if !output.status.success() {
        eprintln!(
            "skipping: compose refused the merge ({})",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let config: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let site = &config["services"]["site"];

    // Replaced, not appended. An appended command would run the production
    // entrypoint and the dev server one after the other.
    assert_eq!(
        site["command"],
        serde_json::json!(["sh", "-c", "npm run dev"]),
        "{site}"
    );
    assert_eq!(site["environment"]["NODE_ENV"], "development");

    let volumes = site["volumes"].as_array().expect("volumes");
    assert_eq!(volumes.len(), 2, "{volumes:?}");
    assert!(volumes
        .iter()
        .any(|v| v["type"] == "bind" && v["target"] == "/app"));
    // The anonymous volume is what stops the bind hiding the image's install.
    assert!(volumes
        .iter()
        .any(|v| v["type"] == "volume" && v["target"] == "/app/node_modules"));

    eprintln!(
        "devserver overlay: command overridden, {} volume(s)",
        volumes.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The compose reader, through real Docker.
///
/// The design bet is that `docker compose config` does the parsing, so the
/// thing worth testing on a real machine is that bet: anchors, shorthand port
/// strings, a label list and a relative bind all reach this code already
/// normalised. A hand-written fixture of the resolved JSON — which the unit
/// tests use — cannot check that, because it *is* the normalisation.
///
/// Skipped when Docker is unreachable, like every other test in this file.
#[tokio::test]
async fn docker_resolves_a_real_compose_file_into_a_migration() {
    use stackvo_desktop_lib::migrate;

    let dir = std::env::temp_dir().join("stackvo-migrate-real");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("docker-compose.yml");

    // Deliberately written in the shorthands a hand parser gets wrong: a YAML
    // anchor, a merge key, `"8080:80"` as a string, labels as a list, and a
    // relative bind.
    std::fs::write(
        &file,
        r#"x-common: &common
  restart: unless-stopped
services:
  app:
    <<: *common
    build: .
    working_dir: /var/www/html
    volumes:
      - ./:/var/www/html
    labels:
      - "traefik.http.routers.shop.rule=Host(`shop.test`)"
  web:
    image: nginx:1.25-alpine
    ports: ["8080:80"]
    volumes:
      - ./public:/var/www/html/public
  db:
    image: mysql:8.0
  cache:
    image: redis:7.2-alpine
  weird:
    image: ghcr.io/acme/thing:2
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("Dockerfile"),
        "FROM php:8.3-fpm\nRUN docker-php-ext-install -j$(nproc) pdo_mysql gd\n",
    )
    .unwrap();

    let m = match migrate::read(&file).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("skipping: docker compose unavailable ({e:?})");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    };

    // The anchor resolved, so `app` is still the build service.
    assert_eq!(m.app_service.as_deref(), Some("app"));
    // The label list became a map and the rule parsed.
    assert_eq!(m.domain.as_deref(), Some("shop.test"));
    // nginx is the server, not a service — StackVo runs it inside the project
    // container, so importing it as a sidecar would give the project two.
    assert_eq!(m.server.as_deref(), Some("nginx"));
    // The relative bind became an absolute path and still resolved back to a
    // document root relative to working_dir.
    assert_eq!(m.document_root.as_deref(), Some("public"));
    // The Dockerfile filled in what compose could not.
    assert_eq!(m.php_version.as_deref(), Some("8.3"));
    assert_eq!(m.extensions, ["pdo_mysql", "gd"]);

    let ids: Vec<&str> = m.services.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["mysql", "redis"]);
    assert_eq!(m.unmapped, ["weird (ghcr.io/acme/thing:2)"]);

    eprintln!(
        "migrate: {} service(s), {} unmapped, php {}",
        m.services.len(),
        m.unmapped.len(),
        m.php_version.as_deref().unwrap_or("?")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A preset exported from a real `.env`, checked against that file's real
/// secrets.
///
/// The unit test proves the rule against a fixture whose passwords it wrote
/// itself, which mostly proves the fixture. This one takes every key the
/// contract calls a secret, reads its actual value off this machine, and
/// asserts none of them appears anywhere in the serialised preset. If the
/// format ever grows somewhere to put one, this is what notices.
#[test]
fn no_real_secret_survives_a_preset_export() {
    use stackvo_desktop_lib::preset;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let env = Env::load(&root).expect(".env should load");
    let exported = preset::export_current(&root, Some("real".into())).expect("export");
    let value = serde_json::to_value(&exported).expect("serialise");
    let text = serde_json::to_string(&exported).expect("serialise");

    // Every string that ends up *in* the document — object keys and leaf
    // values, at any depth.
    //
    // Compared exactly rather than by substring, because the first version of
    // this test failed on real data for the wrong reason:
    // `SERVICE_GRAFANA_ADMIN_PASSWORD=admin`, and "admin" is a substring of the
    // service ids `phpmyadmin`, `pgadmin` and `phpcacheadmin`, which a preset
    // legitimately contains. Loosening the check by raising a length threshold
    // would have been the wrong repair — it would let a real five-character
    // secret through to keep a coincidence quiet. Exact comparison is what the
    // claim actually is: no secret is a value in this file.
    fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    out.push(k.clone());
                    strings(v, out);
                }
            }
            _ => {}
        }
    }
    let mut present = Vec::new();
    strings(&value, &mut present);

    let mut checked = 0usize;
    for (key, masked) in env.redacted() {
        if !Env::is_secret(&key) || masked.is_empty() {
            continue;
        }
        // `redacted()` masks the value, so ask for the real one to compare.
        let Some(real) = env.get(&key).map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        checked += 1;

        assert!(
            !present.iter().any(|s| s == real),
            "the value of {key} is present in the exported preset"
        );
        // Belt and braces for a secret long enough that a coincidental
        // substring is not credible — this catches one embedded in a URL or a
        // connection string, which exact comparison alone would miss.
        if real.len() >= 12 {
            assert!(
                !text.contains(real),
                "the value of {key} appears inside the exported preset"
            );
        }
    }

    // The preset also has to be worth something: an export that produced
    // nothing would pass the leak check trivially.
    assert!(
        !exported.services.is_empty(),
        "the export found no services at all"
    );

    eprintln!(
        "preset: {} service(s), {} setting(s), {checked} real secret(s) checked absent",
        exported.services.len(),
        exported.settings.len()
    );
}

/// The whole import flow, through real files, against the real `.env`.
///
/// Save → hand-edit → plan, which is what a teammate actually does. Read-only
/// on the checkout: it plans but never applies, because applying would rewrite
/// the user's `.env` from a test.
#[test]
fn a_saved_preset_plans_exactly_the_change_that_was_made_to_it() {
    use stackvo_desktop_lib::preset;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let dir = std::env::temp_dir().join("stackvo-preset-roundtrip");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("team.stackvo-preset.json");

    preset::save(&root, &file, Some("team".into())).expect("save");

    // Straight back out again: the file this stack wrote must describe this
    // stack, or every diff a colleague sees is noise.
    let plan = preset::plan_file(&root, &file).expect("plan");
    assert!(
        plan.changes.is_empty(),
        "a preset of this stack proposes changes to it: {:?}",
        plan.changes
    );
    assert!(plan.rejected.is_empty(), "{:?}", plan.rejected);
    assert!(plan.unchanged > 0, "the plan checked nothing at all");

    // Now edit it the way a teammate's would differ, and confirm the diff is
    // exactly that and nothing else.
    let text = std::fs::read_to_string(&file).unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    let flipped = {
        let services = parsed["services"].as_object_mut().unwrap();
        let (id, entry) = services.iter_mut().next().unwrap();
        let was = entry["enabled"].as_bool().unwrap();
        entry["enabled"] = serde_json::Value::Bool(!was);
        (id.clone(), !was)
    };
    std::fs::write(&file, serde_json::to_string_pretty(&parsed).unwrap()).unwrap();

    let plan = preset::plan_file(&root, &file).expect("plan");
    assert_eq!(plan.changes.len(), 1, "{:?}", plan.changes);
    assert_eq!(plan.changes[0].subject, flipped.0);
    assert_eq!(plan.changes[0].to, flipped.1.to_string());
    assert!(plan.needs_regenerate);

    eprintln!(
        "preset round trip: {} → {} = {}",
        plan.changes[0].key,
        plan.changes[0].from.as_deref().unwrap_or("(absent)"),
        plan.changes[0].to
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The php.ini overlay against the real workspace.
///
/// The claim under test is the one that sank the previous attempt at this
/// feature: an overlay may only name a service the generator actually emitted.
/// Naming one it did not declares a service with neither an image nor a build
/// context, and compose then refuses **every** command against the whole stack
/// — not just that project. Observed while building this: the checkout's
/// `docker-compose.projects.yml` can legitimately be `services: {}` between a
/// regenerate and a build, in which case the correct number of entries is zero,
/// not "one per manifest".
#[test]
fn the_php_ini_overlay_only_names_real_compose_services() {
    use stackvo_desktop_lib::{phpini, xdebug};

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let generated = std::fs::read_to_string(root.join("generated/docker-compose.projects.yml"))
        .unwrap_or_default();
    let services = xdebug::generated_services(&generated);

    // Whatever the overlay would render must be a subset of those. Rendered
    // through the same path the compose invocation uses, so this is the real
    // answer and not a re-derivation of it.
    phpini::sync(&root);
    let overlay = std::fs::read_to_string(phpini::overlay_path(&root)).unwrap_or_default();

    for line in overlay.lines() {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        if rest.starts_with(' ') || rest.starts_with('-') || rest.starts_with('#') {
            continue;
        }
        let Some(name) = rest.strip_suffix(':') else {
            continue;
        };
        assert!(
            services.iter().any(|s| s == name),
            "the overlay names `{name}`, which the generator did not emit — \
             compose would refuse every command against the whole stack"
        );
    }

    eprintln!(
        "php.ini overlay: {} generated service(s), overlay {}",
        services.len(),
        if overlay.is_empty() {
            "not rendered"
        } else {
            "rendered"
        }
    );
}

/// The cross-project tail against the real workspace.
///
/// The two claims worth checking on real data are the ones a fixture cannot
/// make: that the fanout's own project list agrees with the one the rest of the
/// app uses, and that it adopts real files at their end — the "live only"
/// promise is exactly the kind that a fixture with two ten-byte files would
/// keep by accident and a 90 MB `laravel.log` would break loudly.
#[test]
fn the_fanout_covers_the_real_checkout_without_replaying_it() {
    use stackvo_desktop_lib::applog;

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    // The same projects the rest of the app sees. Two answers to "what is a
    // project" is how a view starts quietly missing one.
    let mut expected: Vec<String> = manifests(&root).into_iter().map(|(name, _)| name).collect();
    expected.sort();
    assert_eq!(applog::projects(&root).unwrap(), expected);

    let all = applog::candidates_all(&root).unwrap();
    assert!(
        all.iter().all(|f| expected.contains(&f.project)),
        "a file was attributed to a project that is not in the list"
    );

    let mut fanout = applog::Fanout::new(&root);
    let scan = fanout.scan(&[]);
    assert_eq!(scan.projects, expected.len());
    assert!(
        scan.followed <= scan.total,
        "followed {} of {}",
        scan.followed,
        scan.total
    );

    // Nothing already on disk is replayed. This is the claim that matters: the
    // pane is live-only because interleaved history across these files would be
    // an ordering nothing here can justify.
    let replayed = fanout.poll();
    assert!(
        replayed.is_empty(),
        "{} line(s) of history were replayed, e.g. {:?}",
        replayed.len(),
        replayed.first()
    );

    eprintln!(
        "fanout: following {} of {} file(s) across {} project(s)",
        scan.followed, scan.total, scan.projects
    );
}

/// The template renderer, against the bytes Bash actually wrote.
///
/// This is the check that decides whether the Bash generator can be removed for
/// this surface, so it is not a fixture: it renders the real templates with the
/// real `.env` and compares to `generated/`, which Bash produced.
///
/// Both differences it found on the first run were things reading the shell
/// could not have told me: `HOST_STACKVO_ROOT` is computed in `env-loader.sh`
/// and is not in `.env` (so every volume mount came out relative), and
/// `include_module` ends with a bare `echo ""` after its awk pipeline.
#[test]
fn the_rust_renderer_reproduces_bash_byte_for_byte() {
    use stackvo_desktop_lib::{config::Env, template};

    let Some(root) = checkout() else {
        eprintln!("no StackVo checkout found, skipping");
        return;
    };

    let env = Env::load(&root).expect(".env");
    let vars = template::variables(&env, &root);
    let templates = root.join("core/templates");

    // The service config files, each a direct render of one template.
    const CONFIGS: [(&str, &str); 5] = [
        ("redis/redis.conf.tpl", "redis.conf"),
        ("mysql/my.cnf.tpl", "mysql.cnf"),
        ("mongo/mongo.conf.tpl", "mongo.conf"),
        ("postgres/postgres.conf.tpl", "postgres.conf"),
        ("elasticsearch/elasticsearch.yml.tpl", "elasticsearch.yml"),
    ];

    let mut checked = 0usize;
    for (tpl, out) in CONFIGS {
        let template = templates.join("services").join(tpl);
        let generated = root.join("generated/configs").join(out);
        let (Ok(text), Ok(expected)) = (
            std::fs::read_to_string(&template),
            std::fs::read_to_string(&generated),
        ) else {
            continue;
        };
        checked += 1;
        assert_eq!(
            template::render(&text, &vars),
            expected,
            "{out} does not match what Bash wrote"
        );
    }

    // And the assembled services file — twenty templates, the awk filter, the
    // UI heredoc and the harvested volumes section.
    let generated = root.join("generated/docker-compose.dynamic.yml");
    if let Ok(expected) = std::fs::read_to_string(&generated) {
        let rendered = template::render_dynamic_compose(&templates, &vars);
        assert_eq!(
            rendered.len(),
            expected.len(),
            "docker-compose.dynamic.yml differs in length"
        );
        assert_eq!(rendered, expected, "docker-compose.dynamic.yml differs");
        eprintln!(
            "renderer: {checked} config(s) + docker-compose.dynamic.yml ({} bytes) byte-identical",
            expected.len()
        );
    }
}
