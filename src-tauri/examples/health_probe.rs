//! Does every healthcheck in the catalogue actually go green?
//!
//!   cargo run --example health_probe -- --packages ../stackvo-service-packages
//!   cargo run --example health_probe -- --packages ../pkgs --only mysql,redis
//!
//! S-11 in `docs/durum.md`. Every one of the 101 packages shipped with an empty
//! `health` block, and the cost of that was measured rather than argued: in Faz
//! 3 `docker compose up --wait` reported two MySQL instances ready and both
//! refused the connection. With no healthcheck declared, `--wait` and
//! `condition: service_healthy` both fall back to "the process exists", which
//! is the thing they were added to stop meaning.
//!
//! The table that fixes it lives in `examples/build_packages.rs` →
//! `health_of`, and a table of commands is a table of claims. `command -v`
//! inside each image says a binary is *present*; only this says the check
//! *passes*, on a container that is really starting, in the time the manifest
//! allows. Those are different questions and the gap between them is where a
//! permanently-unhealthy service lives — which is worse than no healthcheck at
//! all, because `depends_on` waits on it forever.
//!
//! ## Why it is a program and not a test
//!
//! It pulls images and starts containers, so it is a fact about the world in
//! the sense `service_tags.rs` and `side_by_side.rs` are. The suite holds the
//! shape — that a declared test parses, that the renderer emits it, that a
//! fragment may not declare its own — because those are facts about the source.
//!
//! ## What it leaves behind
//!
//! A scratch workspace under the OS temp directory and, per service, a compose
//! project that is torn down with `down -v` before the next one starts. One
//! service at a time on purpose: Elasticsearch, Cassandra and Kafka together
//! ask for more memory than a laptop has, and a service that failed because the
//! machine was out of memory would be reported as a healthcheck that does not
//! work.
//!
//! Containers are named after the scratch instances — `stackvo-mysql-8-0` —
//! so a real workspace running the same service would collide. The program
//! checks first and stops rather than taking somebody's container down.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use stackvo_desktop_lib::{instances, market, pkg, policy, ports, render};

/// How long one service gets to report healthy, in seconds.
///
/// Generous, and deliberately not the manifest's own budget: the first run
/// pulls the image, and a probe that reported Cassandra unhealthy because a
/// 400 MB layer was still downloading would be measuring the network.
const BUDGET: u64 = 420;

#[derive(Debug)]
enum Outcome {
    Healthy(u64),
    Unhealthy(String),
    NeverStarted(String),
    Undeclared,
    Skipped(String),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let Some(packages) = flag("--packages").map(PathBuf::from) else {
        eprintln!(
            "usage: cargo run --example health_probe -- --packages <packages repo> \
             [--only a,b,c]"
        );
        std::process::exit(2);
    };
    let only: Option<Vec<String>> = flag("--only").map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    });

    match run(&packages, only.as_deref()) {
        Ok(failed) if failed == 0 => println!("\nevery declared healthcheck reported healthy"),
        Ok(failed) => {
            eprintln!("\n{failed} service(s) did not report healthy");
            std::process::exit(1);
        }
        Err(message) => {
            eprintln!("\n{message}");
            std::process::exit(1);
        }
    }
}

fn run(packages: &Path, only: Option<&[String]>) -> Result<usize, String> {
    docker_output(&["version", "--format", "{{.Server.Version}}"])
        .map_err(|_| "Docker is not answering; this program needs a running engine".to_string())?;

    let root = std::env::temp_dir().join("stackvo-health-probe");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    let source = market::LocalSource::new(packages);
    let registry = market::refresh(&root, &source, market::Trust::Unsigned, None)
        .map_err(|e| format!("reading the catalogue: {}", e.message))?;
    println!(
        "catalogue  sequence {}, {} package(s)\n",
        registry.sequence,
        registry.packages.len()
    );

    let wanted: Vec<String> = registry
        .packages
        .iter()
        .map(|p| p.service.clone())
        .filter(|s| only.is_none_or(|list| list.iter().any(|w| w == s)))
        .collect();

    let mut results: Vec<(String, String, Outcome)> = Vec::new();

    for service in &wanted {
        let Some(row) = registry.recommended(service) else {
            results.push((
                service.clone(),
                "-".into(),
                Outcome::Skipped("no recommended version".into()),
            ));
            continue;
        };
        let version = row.version.clone();
        print!("{service}@{version} ... ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        let outcome = probe(&root, &source, &registry, service, &version);
        match &outcome {
            Ok(Outcome::Healthy(secs)) => println!("healthy in {secs}s"),
            Ok(Outcome::Undeclared) => println!("declares no healthcheck"),
            Ok(Outcome::Unhealthy(why)) => println!("UNHEALTHY — {why}"),
            Ok(Outcome::NeverStarted(why)) => println!("DID NOT START — {why}"),
            Ok(Outcome::Skipped(why)) => println!("skipped — {why}"),
            Err(e) => println!("ERROR — {e}"),
        }
        results.push((
            service.clone(),
            version,
            outcome.unwrap_or_else(|e| Outcome::NeverStarted(e)),
        ));
    }

    // ---- the report ------------------------------------------------------
    println!("\n{:-<72}", "");
    let mut failed = 0;
    for (service, version, outcome) in &results {
        let (mark, detail) = match outcome {
            Outcome::Healthy(s) => ("ok", format!("healthy in {s}s")),
            Outcome::Undeclared => ("--", "no healthcheck declared".into()),
            Outcome::Skipped(w) => ("--", format!("skipped: {w}")),
            Outcome::Unhealthy(w) => {
                failed += 1;
                ("FAIL", format!("never healthy: {w}"))
            }
            Outcome::NeverStarted(w) => {
                failed += 1;
                ("FAIL", format!("container did not run: {w}"))
            }
        };
        println!("  {mark:<4} {service:<16} {version:<12} {detail}");
    }
    println!("{:-<72}", "");

    let _ = std::fs::remove_dir_all(&root);
    Ok(failed)
}

/// One service, from install to a health status.
fn probe(
    root: &Path,
    source: &market::LocalSource,
    registry: &market::Registry,
    service: &str,
    version: &str,
) -> Result<Outcome, String> {
    // Everything this service says it cannot work without. Kibana against no
    // Elasticsearch answers 503 on `/api/status` forever, and reporting that as
    // a broken healthcheck would be reporting a correct one.
    let mut plan: Vec<(String, String)> = Vec::new();
    let manifest_path = registry
        .version(service, version)
        .ok_or_else(|| format!("{service}@{version} is not in the index"))?;
    let _ = manifest_path;

    market::install(
        root,
        source,
        registry,
        service,
        version,
        policy::current().market(),
    )
    .map_err(|e| format!("installing: {}", e.message))?;
    let tree = pkg::Tree::open(&market::dir(root)).map_err(|e| e.message)?;
    let manifest = tree.load(service, version).map_err(|e| e.message)?;

    if manifest.health.is_none() && manifest.companions.iter().all(|c| c.health.is_none()) {
        return Ok(Outcome::Undeclared);
    }

    for dependency in &manifest.depends_on {
        if !dependency.required {
            continue;
        }
        let Some(provider) = provider_of(registry, dependency) else {
            return Ok(Outcome::Skipped(format!(
                "needs a {:?} and the catalogue offers none",
                dependency.capability
            )));
        };
        let Some(row) = registry.recommended(&provider) else {
            continue;
        };
        market::install(
            root,
            source,
            registry,
            &provider,
            &row.version,
            policy::current().market(),
        )
        .map_err(|e| format!("installing the dependency {provider}: {}", e.message))?;
        plan.push((provider, row.version.clone()));
    }
    plan.push((service.to_string(), version.to_string()));

    let tree = pkg::Tree::open(&market::dir(root)).map_err(|e| e.message)?;

    // ---- the table -------------------------------------------------------
    let mut table = instances::Table {
        schema_version: instances::SCHEMA_VERSION,
        instances: Vec::new(),
    };
    let mut claims = ports::Claims::default();
    for (svc, ver) in &plan {
        let manifest = tree.load(svc, ver).map_err(|e| e.message)?;
        let id = instances::slug(svc, ver).map_err(|e| e.message)?;
        let container = format!("stackvo-{id}");
        if docker_output(&["inspect", "--type=container", "-f", "{{.Name}}", &container]).is_ok() {
            return Ok(Outcome::Skipped(format!(
                "{container} already exists on this machine"
            )));
        }

        let reserved = table.reserved_ports();
        let mut chosen = BTreeMap::new();
        for port in &manifest.ports {
            let host = ports::allocate(port.preferred, &reserved, &mut claims, &ports::is_free)
                .map_err(|e| e.message)?;
            chosen.insert(port.name.clone(), host);
        }

        table
            .insert(instances::Instance {
                id,
                service: svc.clone(),
                version: ver.clone(),
                package: instances::PackageRef {
                    source: "local".into(),
                    sha256: "0".repeat(64),
                    installed_at: "1970-01-01T00:00:00Z".into(),
                },
                enabled: true,
                // Every instance is primary, and it has to be: a dependency's
                // default setting names the old alias — `stackvo-mysql`, not
                // `stackvo-mysql-8-0` — and that alias only exists on the
                // primary. One instance per service here, so there is nothing
                // for it to collide with.
                primary: true,
                ports: chosen,
                volumes: BTreeMap::new(),
                settings: BTreeMap::new(),
                secret_refs: BTreeMap::new(),
            })
            .map_err(|e| e.message)?;
    }

    // ---- render ----------------------------------------------------------
    // No keystore: an unresolved reference falls through to the manifest's own
    // first-boot default, which is what a fresh install would have written.
    let secrets = |_: &str| None;
    let rendered =
        render::dynamic_compose(root, &table, &tree, "stackvo-net", "stackvo.loc", &secrets)
            .map_err(|e| format!("rendering: {}", e.message))?;

    let compose = root.join("generated/docker-compose.dynamic.yml");
    std::fs::create_dir_all(compose.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&compose, &rendered.compose).map_err(|e| e.to_string())?;
    for config in &rendered.configs {
        std::fs::create_dir_all(config.path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&config.path, &config.contents).map_err(|e| e.to_string())?;
    }

    // The dynamic file names an external network it does not declare, exactly
    // as the app's does — the app always passes `stackvo.yml` beside it.
    let base = root.join("generated/base.yml");
    std::fs::write(
        &base,
        "name: stackvo-health-probe\nnetworks:\n  stackvo-net:\n    name: stackvo-health-probe-net\n",
    )
    .map_err(|e| e.to_string())?;

    let base_path = base.display().to_string();
    let compose_path = compose.display().to_string();
    let project = ["-f", &base_path, "-f", &compose_path];

    let teardown = || {
        let mut argv = vec!["compose"];
        argv.extend_from_slice(&project);
        argv.extend_from_slice(&["--profile", "services", "down", "-v", "--remove-orphans"]);
        let _ = docker_quiet(&argv);
    };

    // Not `--wait`: that is the thing under test. `up -d` returns as soon as
    // the containers are created and this asks the engine itself.
    let mut argv = vec!["compose"];
    argv.extend_from_slice(&project);
    argv.extend_from_slice(&["--profile", "services", "up", "-d"]);
    if let Err(e) = docker_quiet(&argv) {
        teardown();
        return Ok(Outcome::NeverStarted(e));
    }

    // ---- and the question ------------------------------------------------
    let id = instances::slug(service, version).map_err(|e| e.message)?;
    let mut targets = vec![format!("stackvo-{id}")];
    for companion in &manifest.companions {
        if companion.health.is_some() {
            targets.push(format!("stackvo-{id}-{}", companion.name));
        }
    }

    let started = std::time::Instant::now();
    let mut outcome = Outcome::Healthy(0);
    for container in &targets {
        match await_health(container, BUDGET) {
            Ok(secs) => outcome = Outcome::Healthy(secs.max(started.elapsed().as_secs())),
            Err(why) => {
                outcome = Outcome::Unhealthy(format!("{container}: {why}"));
                break;
            }
        }
    }

    teardown();
    Ok(outcome)
}

/// Which installed service satisfies a declared dependency.
///
/// The manifest may name one — Kibana wants Elasticsearch specifically — and
/// otherwise any package claiming the capability will do, which is the point of
/// stating dependencies by capability: phpMyAdmin is satisfied by MariaDB.
fn provider_of(registry: &market::Registry, dependency: &pkg::Dependency) -> Option<String> {
    if let Some(named) = &dependency.service {
        if registry.package(named).is_some() {
            return Some(named.clone());
        }
    }
    registry
        .packages
        .iter()
        .find(|p| p.capabilities.iter().any(|c| c == &dependency.capability))
        .map(|p| p.service.clone())
}

/// Poll the engine's own view until it settles, or the budget runs out.
///
/// `starting` is not a failure and must not be treated as one: a `startPeriod`
/// exists precisely so a database building its data directory is starting
/// rather than unhealthy, and a probe that gave up during it would push every
/// manifest towards a shorter grace period than the software needs.
fn await_health(container: &str, budget: u64) -> Result<u64, String> {
    let started = std::time::Instant::now();
    let mut last = String::from("starting");
    loop {
        let status = docker_output(&[
            "inspect",
            "-f",
            "{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}",
            container,
        ])
        .unwrap_or_else(|_| "gone".into());
        let status = status.trim().to_string();

        match status.as_str() {
            "healthy" => return Ok(started.elapsed().as_secs()),
            "none" => {
                return Err(
                    "the engine sees no healthcheck on this container — the manifest declared \
                     one and it did not reach the compose file"
                        .into(),
                )
            }
            "gone" => return Err(format!("the container is not there ({})", tail(container))),
            other => last = other.to_string(),
        }

        if started.elapsed().as_secs() >= budget {
            return Err(format!("still {last} after {budget}s; {}", why(container)));
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// What the engine recorded for the last failing probe, which is the only thing
/// that separates "the command is wrong" from "the service is not up yet".
fn why(container: &str) -> String {
    let log = docker_output(&[
        "inspect",
        "-f",
        "{{range .State.Health.Log}}{{.ExitCode}}: {{.Output}}{{end}}",
        container,
    ])
    .unwrap_or_default();
    let line = log.lines().last().unwrap_or("").trim();
    if line.is_empty() {
        tail(container)
    } else {
        format!("last probe said {line:?}")
    }
}

fn tail(container: &str) -> String {
    docker_output(&["logs", "--tail", "3", container])
        .unwrap_or_default()
        .lines()
        .last()
        .unwrap_or("no output")
        .trim()
        .to_string()
}

fn docker_quiet(args: &[&str]) -> Result<(), String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("running docker: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr)
        .lines()
        .last()
        .unwrap_or("docker failed")
        .trim()
        .to_string())
}

fn docker_output(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("running docker: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
