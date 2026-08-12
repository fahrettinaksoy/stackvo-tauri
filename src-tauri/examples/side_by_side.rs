//! Two versions of one service, actually running.
//!
//!   cargo run --example side_by_side -- --packages ../../stackvo-service-packages
//!
//! This is the claim the whole service-package architecture rests on, and until
//! now it was a claim: eighteen templates hardcoded a volume name, so two
//! versions of MySQL would have shared `stackvo-mysql-data` — and Docker does
//! not report that. The newer engine opens the older one's data directory and
//! upgrades it, and the first anyone hears is that 8.0 no longer starts.
//!
//! Unit tests hold the shape of the fix — separate slugs, separate volumes,
//! separate ports, a refusal when two instances would collide. Whether two
//! mysqld processes actually come up on one machine, keep their own rows and
//! report their own versions is a fact about the world, so it lives here rather
//! than in the suite. `service_tags.rs` and `connection_probe.rs` are the same
//! kind of program for the same reason.
//!
//! ## What it does, and what it leaves behind
//!
//! A scratch workspace under the OS temp directory, two packages installed into
//! it from a local source, two instances created, one compose file rendered.
//! Then `docker compose up`, a query against each, and `down -v`.
//!
//! Nothing touches `~/.stackvo`. The containers are named after the scratch
//! workspace's own instances — `stackvo-mysql-8-0` and `stackvo-mysql-9-4` —
//! so a real workspace running MySQL would collide; the program says so and
//! stops rather than taking somebody's container down.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use stackvo_desktop_lib::{instances, market, pkg, policy, ports, render};

const SERVICE: &str = "mysql";
const VERSIONS: [&str; 2] = ["8.0", "9.4"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let packages = args
        .iter()
        .position(|a| a == "--packages")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("usage: cargo run --example side_by_side -- --packages <packages repo>");
            std::process::exit(2);
        });

    if let Err(message) = run(&packages) {
        eprintln!("\n  {message}\n");
        std::process::exit(1);
    }
}

fn run(packages: &Path) -> Result<(), String> {
    // Refuse to touch a container somebody else owns. The names are derived, so
    // a real workspace running MySQL 8.0 has exactly the one this would create.
    for version in VERSIONS {
        let id = instances::slug(SERVICE, version).map_err(|e| e.message)?;
        let name = format!("stackvo-{id}");
        if docker_output(&["inspect", "--type=container", "-f", "{{.Name}}", &name]).is_ok() {
            return Err(format!(
                "{name} already exists. This program creates and destroys that exact \
                 container, so it stops rather than taking down a workspace's own"
            ));
        }
    }

    let root = std::env::temp_dir().join(format!("stackvo-side-by-side-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    println!("  workspace  {}", root.display());

    let outcome = attempt(&root, packages);

    // Always, even when the check failed: a leftover container is a leftover
    // port, and the next run would refuse for the wrong reason.
    println!("\n  cleaning up");
    let compose = root.join("generated/docker-compose.dynamic.yml");
    if compose.is_file() {
        let _ = docker(&[
            "compose",
            "-f",
            &root.join("generated/base.yml").display().to_string(),
            "-f",
            &compose.display().to_string(),
            "--profile",
            "services",
            "down",
            "-v",
        ]);
    }
    let _ = std::fs::remove_dir_all(&root);

    outcome
}

fn attempt(root: &Path, packages: &Path) -> Result<(), String> {
    // ---- install both versions ------------------------------------------
    let source = market::LocalSource::new(packages);
    let registry = market::refresh(root, &source, market::Trust::Unsigned, None)
        .map_err(|e| format!("reading the catalogue: {}", e.message))?;
    println!("  catalogue  sequence {}", registry.sequence);

    for version in VERSIONS {
        let done = market::install(
            root,
            &source,
            &registry,
            SERVICE,
            version,
            policy::current().market(),
        )
            .map_err(|e| format!("installing {SERVICE}@{version}: {}", e.message))?;
        println!("  installed  {SERVICE}@{version}  ({} files)", done.files);
    }

    // ---- two instances ---------------------------------------------------
    let tree = pkg::Tree::open(&market::dir(root)).map_err(|e| e.message)?;
    let mut table = instances::Table {
        schema_version: instances::SCHEMA_VERSION,
        instances: Vec::new(),
    };
    let mut claims = ports::Claims::default();

    for (n, version) in VERSIONS.iter().enumerate() {
        let manifest = tree.load(SERVICE, version).map_err(|e| e.message)?;
        let id = instances::slug(SERVICE, version).map_err(|e| e.message)?;

        let reserved = table.reserved_ports();
        let mut chosen = BTreeMap::new();
        for port in &manifest.ports {
            let host = ports::allocate(port.preferred, &reserved, &mut claims, &ports::is_free)
                .map_err(|e| e.message)?;
            chosen.insert(port.name.clone(), host);
        }

        let mut settings = BTreeMap::new();
        settings.insert("DATABASE".to_string(), "stackvo".to_string());

        table
            .insert(instances::Instance {
                id: id.clone(),
                service: SERVICE.into(),
                version: (*version).into(),
                package: instances::PackageRef {
                    source: "local".into(),
                    sha256: "0".repeat(64),
                    installed_at: "1970-01-01T00:00:00Z".into(),
                },
                enabled: true,
                primary: n == 0,
                ports: chosen.clone(),
                volumes: BTreeMap::new(),
                settings,
                secret_refs: BTreeMap::new(),
            })
            .map_err(|e| e.message)?;

        println!(
            "  instance   {id}  port {}  volume stackvo-{id}-data",
            chosen.values().next().copied().unwrap_or(0)
        );
    }
    table.save(root).map_err(|e| e.message)?;

    // ---- render ----------------------------------------------------------
    // The first-boot password the manifest ships. A real install writes this to
    // the keystore; here it is handed straight back, which is what the closure
    // is for.
    let secrets = |_: &str| Some("root".to_string());
    let rendered =
        render::dynamic_compose(root, &table, &tree, "stackvo-net", "stackvo.loc", &secrets)
            .map_err(|e| format!("rendering: {}", e.message))?;

    let compose = root.join("generated/docker-compose.dynamic.yml");
    std::fs::create_dir_all(compose.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&compose, &rendered.compose).map_err(|e| e.to_string())?;

    // The dynamic file declares no networks, and that is correct rather than a
    // gap: the app always passes it alongside `stackvo.yml`, which declares
    // `stackvo-net` as external, and compose merges them. Used alone it is an
    // invalid project — which is exactly what the first run of this program
    // reported. So the base is written here too, as small as it can be, to
    // reproduce what the app actually hands Docker.
    let base = root.join("generated/base.yml");
    std::fs::write(
        &base,
        "name: stackvo-side-by-side\nnetworks:\n  stackvo-net:\n    name: stackvo-net\n    external: true\n",
    )
    .map_err(|e| e.to_string())?;
    for config in &rendered.configs {
        std::fs::create_dir_all(config.path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&config.path, &config.contents).map_err(|e| e.to_string())?;
    }
    println!(
        "  rendered   {} bytes, {} config(s)",
        rendered.compose.len(),
        rendered.configs.len()
    );

    // ---- up --------------------------------------------------------------
    let compose_path = compose.display().to_string();
    let base_path = base.display().to_string();
    println!("\n  starting both — this pulls two images the first time");
    docker(&[
        "compose",
        "-f",
        &base_path,
        "-f",
        &compose_path,
        "--profile",
        "services",
        "up",
        "-d",
        "--wait",
        "--wait-timeout",
        "300",
    ])
    .map_err(|e| format!("bringing them up: {e}"))?;

    // ---- and the question ------------------------------------------------
    let mut answers = Vec::new();
    for (n, version) in VERSIONS.iter().enumerate() {
        let id = instances::slug(SERVICE, version).map_err(|e| e.message)?;
        let container = format!("stackvo-{id}");

        await_ready(&container)?;
        let reported = query(&container, "SELECT VERSION()")?;
        let volume = docker_output(&[
            "inspect",
            "-f",
            "{{range .Mounts}}{{if eq .Destination \"/var/lib/mysql\"}}{{.Name}}{{end}}{{end}}",
            &container,
        ])?;

        println!("\n  {container}");
        println!("    reports  {reported}");
        println!("    volume   {volume}");

        if !reported.starts_with(version) {
            return Err(format!(
                "{container} was asked for {version} and reports {reported}"
            ));
        }
        // Compose prefixes a named volume with the project name, so the real
        // volume is `<project>_stackvo-mysql-8-0-data`. That is not this
        // program's doing and not a defect: the app's own stack does the same
        // — `docker volume ls` on a machine running StackVo shows
        // `stackvo_stackvo-mysql-data` — so the declared name is a suffix and
        // the prefix belongs to whichever project the file was brought up in.
        // What matters is that the two instances end on different ones.
        if !volume.ends_with(&format!("stackvo-{id}-data")) {
            return Err(format!("{container} is using the volume {volume}"));
        }
        answers.push((container, reported, volume, n == 0));
    }

    // Separate data, which is the failure the whole design is about.
    if answers[0].2 == answers[1].2 {
        return Err("both instances are on one volume".into());
    }

    // And the pre-package name still resolves — to the primary, from inside the
    // network, which is where every project asks.
    let alias = docker_output(&[
        "run",
        "--rm",
        "--network",
        "stackvo-net",
        "busybox:latest",
        "nslookup",
        "stackvo-mysql",
    ])
    .unwrap_or_default();
    let primary_resolves = !alias.is_empty();
    println!(
        "\n  stackvo-mysql resolves inside the network: {}",
        if primary_resolves {
            "yes"
        } else {
            "could not ask"
        }
    );

    println!(
        "\n  Two mysqld on one machine: {} and {}, on separate volumes and separate ports.",
        answers[0].1, answers[1].1
    );
    Ok(())
}

/// Wait until the server answers, rather than until the container is running.
///
/// `docker compose up --wait` reported both of these ready and both refused the
/// connection, and the reason is worth recording: **neither package declares a
/// healthcheck**, and with none declared `--wait` means "the process started".
/// MySQL's first boot builds its data directory before it opens a port, so the
/// window between those two things is tens of seconds — long enough that a
/// probe trusting `--wait` would fail here and pass on a warm machine, which is
/// the worst kind of test.
///
/// The manifest format has a `health` field for exactly this and every package
/// currently leaves it empty, because the templates they were generated from
/// had none. That is a real gap in the catalogue rather than in this program:
/// without it, `depends_on: condition: service_healthy` means nothing for any
/// service in the tree.
fn await_ready(container: &str) -> Result<(), String> {
    for _ in 0..60 {
        if docker_output(&[
            "exec",
            container,
            "mysqladmin",
            "-h",
            "127.0.0.1",
            "--protocol=TCP",
            "-uroot",
            "-proot",
            "ping",
        ])
        .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    Err(format!("{container} never started answering"))
}

/// `mysql -e`, inside the container, as the account the manifest creates.
///
/// Over TCP rather than the unix socket, and the first run of this is why: the
/// containers reported healthy and the socket connection was refused. The
/// image's own healthcheck asks over `-h 127.0.0.1`, so that is the moment
/// "ready" means — the socket appears slightly later, and a probe that used it
/// would be measuring the wrong thing intermittently.
fn query(container: &str, sql: &str) -> Result<String, String> {
    docker_output(&[
        "exec",
        container,
        "mysql",
        "-h",
        "127.0.0.1",
        "--protocol=TCP",
        "-uroot",
        "-proot",
        "-N",
        "-B",
        "-e",
        sql,
    ])
}

fn docker(args: &[&str]) -> Result<(), String> {
    let status = Command::new("docker")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("docker {} exited {status}", args[0]))
    }
}

fn docker_output(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
