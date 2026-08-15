//! What a migrated workspace gets, against what it has today.
//!
//! This is the check `docs/servis-market-mimarisi.md` §7 calls the handover's
//! only real safety, and it is worth saying exactly what it can and cannot be.
//!
//! It **cannot** be a byte comparison. The whole point of the new pipeline is
//! that names change: `stackvo-mysql` becomes `stackvo-mysql-8-0`, and a
//! service key becomes an instance id. A test demanding identical output would
//! be a test demanding the feature not work.
//!
//! So it compares the things that are supposed to be *unchanged*, field by
//! field: the image somebody's container runs, the port their tooling connects
//! to, the volume their rows are in, the environment the engine boots with.
//! Those four are what a migration must not touch, and each of them has a
//! specific way of going wrong — a re-pulled image, a moved port, an adopted
//! volume that was not adopted, a password that came from the wrong place.
//!
//! ## The fixture, and what it does not cover
//!
//! Four packages, copied from `stackvo-service-packages` and pinned here:
//! mysql (a config file, a named volume, a secret), redis (a config file and no
//! settings at all), phpmyadmin (a Traefik router and no volume), and kafka (a
//! companion container). Between them they reach every branch of the renderer.
//!
//! They are **not** the whole catalogue. Twenty-one services are not exercised
//! here, and saying so is the point of this paragraph: the packages repository
//! runs `validate.mjs` and `compose-check.mjs` over all hundred and one, and
//! this file is about the *shape of the migration*, not about coverage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use stackvo_desktop_lib::{
    commands, config::Env, handover, instances, pkg, ports, render, secrets, template, workspace,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/packages")
}

/// A root holding the pinned packages, so `pkg::Tree::open` finds them where it
/// expects: `<root>/packages/…`.
fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("stackvo-equiv-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("packages")).unwrap();
    copy_tree(&fixture(), &root.join("packages"));

    // Twice, and the second one is where a real workspace keeps them:
    // `market::dir(root)/packages`. The tests above hand `pkg::Tree::open(root)`
    // in themselves, so `<root>/packages` is all they ever needed — but a test
    // that calls a *command* gets the tree the command builds, and that one
    // looks under `market/`. Without this the manifest simply came back `None`
    // and every field derived from it degraded to a default, which is a test
    // passing on a page that had quietly lost half its data.
    let market = root.join("market").join("packages");
    std::fs::create_dir_all(&market).unwrap();
    copy_tree(&fixture(), &market);
    root
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The `.env` of a workspace that has been running for a while: four services
/// on, one of them with a port somebody changed by hand, and a password.
const ENV: &str = "\
SERVICE_MYSQL_ENABLE=true
SERVICE_MYSQL_VERSION=8.0
SERVICE_MYSQL_ROOT_PASSWORD=hunter2
SERVICE_MYSQL_DATABASE=shop
HOST_PORT_MYSQL=3399
SERVICE_REDIS_ENABLE=true
SERVICE_REDIS_VERSION=7.0
SERVICE_PHPMYADMIN_ENABLE=true
SERVICE_PHPMYADMIN_VERSION=5.2
SERVICE_PHPMYADMIN_URL=phpmyadmin
SERVICE_KAFKA_ENABLE=true
SERVICE_KAFKA_VERSION=7.5.0
";

fn free(_: u16) -> bool {
    true
}

/// Everything a service block says, read back out of a rendered compose file.
///
/// A parser rather than a `contains` per assertion, because the failure this is
/// looking for is a value that moved — and `contains` cannot tell "the port is
/// 3399" from "3399 appears somewhere in the file".
#[derive(Debug, Default, PartialEq, Eq)]
struct Block {
    image: String,
    container: String,
    ports: Vec<String>,
    volumes: Vec<String>,
    environment: BTreeMap<String, String>,
}

fn blocks(compose: &str) -> BTreeMap<String, Block> {
    let mut out: BTreeMap<String, Block> = BTreeMap::new();
    let mut current = String::new();
    let mut section = String::new();

    for raw in compose.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();

        if indent == 2 && line.ends_with(':') && !line.contains(' ') {
            current = line.trim_end_matches(':').to_string();
            section.clear();
            out.entry(current.clone()).or_default();
            continue;
        }
        if indent == 0 {
            current.clear();
            continue;
        }
        if current.is_empty() {
            continue;
        }
        let block = out.get_mut(&current).unwrap();

        if indent == 4 {
            section = line.split(':').next().unwrap_or("").to_string();
            if let Some(rest) = line.strip_prefix("image:") {
                block.image = unquote(rest);
            }
            if let Some(rest) = line.strip_prefix("container_name:") {
                block.container = unquote(rest);
            }
            continue;
        }

        match section.as_str() {
            "ports" if line.starts_with("- ") => block.ports.push(unquote(&line[2..])),
            "volumes" if line.starts_with("- ") => block.volumes.push(unquote(&line[2..])),
            "environment" => {
                if let Some((key, value)) = line.split_once(':') {
                    block
                        .environment
                        .insert(key.trim().to_string(), unquote(value));
                } else if let Some(rest) = line.strip_prefix("- ") {
                    if let Some((key, value)) = rest.split_once('=') {
                        block
                            .environment
                            .insert(key.trim().to_string(), unquote(value));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn unquote(text: &str) -> String {
    text.trim().trim_matches('"').trim_matches('\'').to_string()
}

/// What the app rendered from `.env` and the templates compiled into the
/// binary — read from a frozen file, because that renderer no longer exists.
///
/// ADR 0016 removed it. This test is the proof that the migration keeps every
/// image, port and volume, and it made that proof by rendering *both* sides;
/// with one side gone the choice was to delete the proof or to keep its output.
/// The output is kept. `tests/fixtures/golden/handover-before.yml` is what
/// `render_dynamic_compose` produced from `ENV` below on the last commit that
/// still had it, and every assertion in this file still compares against it.
///
/// A frozen side cannot drift, which is the honest limitation: this no longer
/// notices if `ENV` changes and the fixture does not. So `ENV` and the fixture
/// are a pair, and the file says so at the top.
fn today(_root: &Path, _env: &Env) -> BTreeMap<String, Block> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden/handover-before.yml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    blocks(&text)
}

/// What it renders after the handover, from the instance table and the packages.
fn after(root: &Path, env: &Env) -> (BTreeMap<String, Block>, handover::Plan) {
    let tree = pkg::Tree::open(root).expect("the pinned packages");
    let plan = handover::plan(root, env, &tree, &free, "2026-08-11T09:00:00Z");
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    handover::apply(root, &plan).expect("writing the table");

    let table = instances::Table::load(root).unwrap();
    // The keystore, as the app will hold it after migration: the value that was
    // in `.env`, under the reference the plan recorded. Built with the same
    // helper rather than written out, because that helper appends a digest of
    // the workspace path and a literal here would only match one machine.
    let entry = secrets::reference_for("mysql-8-0/ROOT_PASSWORD", root);
    let secrets = move |reference: &str| (reference == entry).then(|| "hunter2".to_string());
    let rendered =
        render::dynamic_compose(root, &table, &tree, "stackvo-net", "stackvo.loc", &secrets)
            .expect("rendering the migrated table");

    (blocks(&rendered.compose), plan)
}

/// The image is the same. A migration that re-pulls is a migration that can
/// change a database engine under a running datadir.
#[test]
fn every_service_runs_the_image_it_ran_before() {
    let root = workspace("images");
    let env = Env::parse(ENV);
    let before = today(&root, &env);
    let (now, _) = after(&root, &env);

    for (service, instance) in [
        ("mysql", "mysql-8-0"),
        ("redis", "redis-7-0"),
        ("phpmyadmin", "phpmyadmin-5-2"),
        ("kafka", "kafka-7-5-0"),
    ] {
        assert_eq!(
            before[service].image, now[instance].image,
            "{service} would run a different image after the handover"
        );
    }
}

/// The port is the same. It is the number in somebody's TablePlus and in their
/// project's `.env`, and `HOST_PORT_MYSQL=3399` is the case that proves the
/// manifest's `legacyKey` is doing its job — the old key family had no default
/// anywhere, so a migration that did not read it would silently hand back 3306.
#[test]
fn every_published_port_survives_the_handover() {
    let root = workspace("ports");
    let env = Env::parse(ENV);
    let before = today(&root, &env);
    let (now, _) = after(&root, &env);

    for (service, instance) in [
        ("mysql", "mysql-8-0"),
        ("redis", "redis-7-0"),
        ("phpmyadmin", "phpmyadmin-5-2"),
        ("kafka", "kafka-7-5-0"),
    ] {
        assert_eq!(
            before[service].ports, now[instance].ports,
            "{service}'s published ports moved"
        );
    }
    assert!(
        now["mysql-8-0"]
            .ports
            .iter()
            .any(|p| p.starts_with("3399:")),
        "the hand-set HOST_PORT_MYSQL was not carried across: {:?}",
        now["mysql-8-0"].ports
    );
}

/// The volume is the same one. This is the failure that is not recoverable:
/// a new name is an empty database, and the old one is still on disk with
/// nothing pointing at it.
#[test]
fn every_named_volume_is_adopted_rather_than_recreated() {
    let root = workspace("volumes");
    let env = Env::parse(ENV);
    let before = today(&root, &env);
    let (now, _) = after(&root, &env);

    for (service, instance) in [("mysql", "mysql-8-0"), ("redis", "redis-7-0")] {
        let named = |b: &Block| -> Vec<String> {
            b.volumes
                .iter()
                .filter(|v| v.starts_with("stackvo-"))
                .map(|v| v.split(':').next().unwrap_or_default().to_string())
                .collect()
        };
        assert_eq!(
            named(&before[service]),
            named(&now[instance]),
            "{service} would start against a different volume"
        );
    }
}

/// The environment is the same, including the password — which now comes from
/// the keystore rather than from `.env`, and must arrive at the same value.
#[test]
fn the_environment_a_container_boots_with_is_unchanged() {
    let root = workspace("environment");
    let env = Env::parse(ENV);
    let before = today(&root, &env);
    let (now, _) = after(&root, &env);

    for (service, instance) in [
        ("mysql", "mysql-8-0"),
        ("redis", "redis-7-0"),
        ("phpmyadmin", "phpmyadmin-5-2"),
    ] {
        assert_eq!(
            before[service].environment, now[instance].environment,
            "{service} would boot with a different environment"
        );
    }
    assert_eq!(
        now["mysql-8-0"]
            .environment
            .get("MYSQL_ROOT_PASSWORD")
            .map(String::as_str),
        Some("hunter2"),
        "the password came through the keystore as something else"
    );
}

/// Every project's `DB_HOST=stackvo-mysql` still resolves.
///
/// The container name necessarily changed; the alias is what keeps the promise.
#[test]
fn the_pre_package_names_still_resolve() {
    let root = workspace("aliases");
    let env = Env::parse(ENV);
    let before = today(&root, &env);
    let (_, _) = after(&root, &env);

    let table = instances::Table::load(&root).unwrap();
    for service in ["mysql", "redis", "phpmyadmin", "kafka"] {
        let old = &before[service].container;
        let instance = table
            .primary_of(service)
            .unwrap_or_else(|| panic!("{service} has no primary instance"));
        assert!(
            instance.aliases().contains(old),
            "{old} stops resolving: {service} answers to {:?}",
            instance.aliases()
        );
    }
}

/// Kafka's Zookeeper comes across as a container of its own, named against the
/// instance — so a second Kafka would get a second coordinator rather than
/// sharing one.
#[test]
fn a_companion_is_named_against_its_instance() {
    let root = workspace("companion");
    let env = Env::parse(ENV);
    let (now, _) = after(&root, &env);

    let zookeeper = now
        .get("kafka-7-5-0-zookeeper")
        .expect("the companion has a block of its own");
    assert_eq!(zookeeper.container, "stackvo-kafka-7-5-0-zookeeper");
    assert!(zookeeper.image.contains("cp-zookeeper"));
}

/// The handover says what it did, and the `latest`-shaped part of it is the one
/// a user has to be told about.
#[test]
fn the_plan_reports_what_it_changed() {
    let root = workspace("notes");
    let env = Env::parse(ENV);
    let (_, plan) = after(&root, &env);

    assert!(
        plan.notes.iter().any(|n| matches!(
            n,
            handover::Note::AdoptedVolume { volume, .. } if volume == "stackvo-mysql-data"
        )),
        "{:?}",
        plan.notes
    );
    // Nothing moved on a machine where nothing was listening.
    assert!(
        !plan
            .notes
            .iter()
            .any(|n| matches!(n, handover::Note::PortMoved { .. })),
        "{:?}",
        plan.notes
    );
}

/// And when the machine has taken the port, the instance moves rather than
/// failing — with the move recorded.
#[test]
fn a_port_the_machine_has_since_taken_moves_and_is_reported() {
    let root = workspace("moved");
    let env = Env::parse(ENV);
    let tree = pkg::Tree::open(&root).unwrap();

    let busy = |p: u16| p != 3399;
    let plan = handover::plan(&root, &env, &tree, &busy, "2026-08-11T09:00:00Z");

    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert!(plan
        .notes
        .iter()
        .any(|n| matches!(n, handover::Note::PortMoved { from: 3399, .. })));
    let mysql = plan
        .instances
        .iter()
        .find(|i| i.service == "mysql")
        .unwrap();
    assert_ne!(mysql.ports["main"], 3399);
    assert!(ports::is_free(mysql.ports["main"]) || mysql.ports["main"] == 3306);
}

// ------------------------------------------------------------ the refusal

/// What a workspace that has not migrated gets now.
///
/// This module used to assert the opposite property, and the change is the
/// whole of ADR 0016. `render_generated` had two sources — `.env` and the
/// templates compiled into the binary when there was no instance table, the
/// table and the package tree when there was — and the test that mattered was
/// that the *old* path stayed byte for byte identical, because every install in
/// existence was on it.
///
/// The old path is gone. So the property that matters is the opposite one: a
/// workspace still keeping its services in `.env` must be refused **with a name
/// on it**, not rendered into an empty stack. `MigrationGate` is what a person
/// meets before this; reaching here means the gate was bypassed, and a silent
/// empty render would be the worst possible answer to that.
mod the_refusal {
    use super::*;

    #[test]
    fn an_unmigrated_workspace_is_refused_rather_than_rendered_empty() {
        let root = workspace("unmigrated");
        std::fs::create_dir_all(root.join("projects")).unwrap();
        workspace::point_at_projects(&root, &root.join("projects")).unwrap();
        std::fs::write(root.join(".env"), ENV).unwrap();
        assert!(!instances::path(&root).exists());

        // `GenFile` has no `Debug`, so the Ok side cannot be unwrapped into a
        // panic message — matched instead, which says the same thing.
        let err = match commands::render_generated(&root) {
            Err(e) => e,
            Ok(_) => panic!("a workspace with no table must not render a stack"),
        };

        // The message has to name the state, because the only repair is a
        // migration and a generic failure sends people to the wrong place.
        assert!(
            err.message.contains(".env") && err.message.contains("instances.json"),
            "the refusal does not say what is wrong: {}",
            err.message
        );
        assert!(err.hint.is_some(), "and it has to say what to do about it");
    }

    /// Once the table exists, the services half comes from it: instance-named
    /// blocks, and configs one directory deeper.
    #[test]
    fn a_migrated_workspace_renders_from_the_table() {
        let root = workspace("migrated");
        std::fs::create_dir_all(root.join("projects")).unwrap();
        // `render_generated` asks where the projects are, and the pointer is
        // the only thing that answers.
        workspace::point_at_projects(&root, &root.join("projects")).unwrap();
        std::fs::write(root.join(".env"), ENV).unwrap();

        // The packages have to be where the market puts them for the render to
        // find them, which is a level below where the fixture was copied.
        let market = root.join("market/packages");
        std::fs::create_dir_all(&market).unwrap();
        copy_tree(&root.join("packages"), &market);

        let env = Env::parse(ENV);
        let tree = pkg::Tree::open(&root.join("market")).unwrap();
        let plan = handover::plan(&root, &env, &tree, &free, "2026-08-11T09:00:00Z");
        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
        handover::apply(&root, &plan).unwrap();

        let (files, _) = commands::render_generated(&root).expect("the new path");
        let dynamic = files
            .iter()
            .find(|f| f.label == "docker-compose.dynamic.yml")
            .unwrap();

        // Instance keys, not service keys.
        assert!(
            dynamic.content.contains("  mysql-8-0:"),
            "{}",
            dynamic.content
        );
        assert!(!dynamic.content.contains("\n  mysql:\n"));
        // And the alias that keeps every project's DB_HOST working.
        assert!(dynamic.content.contains("\"stackvo-mysql\""));

        // Configs are per instance now, because two versions each mount their
        // own and a shared path would let the last render decide for both.
        let configs: Vec<&str> = files
            .iter()
            .filter(|f| f.label.starts_with("configs/"))
            .map(|f| f.label.as_str())
            .collect();
        assert!(
            configs.iter().any(|c| c.contains("mysql-8-0/")),
            "{configs:?}"
        );
    }

    /// A table that cannot render is an error naming what is wrong, not a
    /// silent fall back to `.env` — which would build a stack from state the
    /// user has already replaced.
    #[test]
    fn a_table_naming_a_package_that_is_gone_fails_rather_than_falling_back() {
        let root = workspace("gone");
        std::fs::create_dir_all(root.join("projects")).unwrap();
        // `render_generated` asks where the projects are, and the pointer is
        // the only thing that answers.
        workspace::point_at_projects(&root, &root.join("projects")).unwrap();
        std::fs::write(root.join(".env"), ENV).unwrap();

        let market = root.join("market/packages");
        std::fs::create_dir_all(&market).unwrap();
        copy_tree(&root.join("packages"), &market);

        let env = Env::parse(ENV);
        let tree = pkg::Tree::open(&root.join("market")).unwrap();
        let plan = handover::plan(&root, &env, &tree, &free, "2026-08-11T09:00:00Z");
        handover::apply(&root, &plan).unwrap();

        // The package goes away under the table's feet — an uninstall that
        // should have been refused, or a directory somebody deleted.
        std::fs::remove_dir_all(market.join("databases/mysql")).unwrap();

        let err = match commands::render_generated(&root) {
            Ok(_) => panic!("a table naming a missing package rendered anyway"),
            Err(e) => e,
        };
        assert!(err.message.contains("mysql@8.0"), "{}", err.message);
    }
}

/// The Services page survives the handover.
///
/// It did not. `list_services` walked the compiled-in catalogue and built every
/// container name as `stackvo-<id>`, so a migrated workspace got twenty-five
/// rows, all of them reported stopped — the containers are `stackvo-mysql-8-0`
/// now — and the detail sheet behind those rows, with the connection string,
/// the dumps and the **logs**, was reachable for nothing that was actually
/// running.
///
/// Checked against the instance-shaped rows rather than against a screen,
/// because the failure was in what the command returned.
#[test]
fn the_services_page_lists_instances_once_the_table_exists() {
    let root = workspace("services-after");
    let env = Env::parse(ENV);
    let (_, _) = after(&root, &env);

    let rows = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(stackvo_desktop_lib::commands::list_services(&root))
        .expect("the services list");

    let ids: Vec<&str> = rows.iter().map(|s| s.id.as_str()).collect();

    // Instance ids, not service ids: the detail sheet keys off this, and two
    // versions of one service are two rows that must not collapse into one.
    assert!(ids.contains(&"mysql-8-0"), "{ids:?}");
    assert!(ids.contains(&"redis-7-0"), "{ids:?}");
    assert!(
        !ids.contains(&"mysql"),
        "the pre-package id is still here: {ids:?}"
    );

    // And the compiled-in catalogue is not being listed alongside them. Before
    // this, every one of the twenty-five was a row.
    assert_eq!(ids.len(), rows.len());
    assert!(
        rows.len() < 10,
        "the whole compiled catalogue came back: {ids:?}"
    );

    let mysql = rows.iter().find(|s| s.id == "mysql-8-0").unwrap();
    // The name the detail sheet asks the engine for.
    assert_eq!(mysql.container_name, "stackvo-mysql-8-0");
    assert_eq!(mysql.version.as_deref(), Some("8.0"));
    assert!(mysql.enabled);
    // A secret setting is masked here as it is everywhere else: the value lives
    // in the keystore and the table holds a reference (ADR 0010).
    let root_password = mysql
        .credentials
        .iter()
        .find(|c| c.key == "ROOT_PASSWORD")
        .expect("the manifest declares it");
    assert!(root_password.secret);
    assert_ne!(root_password.value, "hunter2");
}

/// An unmigrated workspace is untouched — the same rule the renderer follows.
#[test]
fn a_workspace_with_no_table_still_lists_the_env_catalogue() {
    let root = workspace("services-before");

    let rows = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(stackvo_desktop_lib::commands::list_services(&root))
        .expect("the services list");

    assert!(
        rows.len() > 20,
        "the compiled catalogue should still be the source"
    );
    assert!(rows.iter().any(|s| s.id == "mysql"));
}
