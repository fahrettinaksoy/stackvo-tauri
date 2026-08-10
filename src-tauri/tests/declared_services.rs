//! A project declaring what it needs, from the `.env` it was cloned with to
//! the `.env` the workspace ends up with.
//!
//! The unit tests either side of this settle the pieces: `detect` decides what
//! counts as evidence, `manifest` decides what a declaration may say, `preset`
//! decides what a plan changes. What none of them covers is the join — that the
//! id `detect` produces is the id `manifest` writes, is the id `preset` looks
//! up in the catalog, is the key `env_writer` sets. Four modules agreeing about
//! a string is exactly the kind of thing that is true until somebody
//! lower-cases in one of them.
//!
//! The two Tauri commands are the thin part above this and are not reachable
//! from a test — they resolve `State<AppState>`. What they add over what runs
//! here is a workspace lookup and a lock.

use stackvo_desktop_lib::{config::Env, detect, manifest, preset};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("stackvo-declared-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

#[test]
fn a_declaration_travels_from_the_projects_env_to_the_workspaces() {
    let root = scratch("root");
    let project = root.join("projects").join("shop");
    std::fs::create_dir_all(&project).unwrap();

    // A workspace where nothing is on — the state a fresh install is in, and
    // the state a colleague's clone meets.
    std::fs::write(
        root.join(".env"),
        "SERVICE_MYSQL_ENABLE=false\nSERVICE_REDIS_ENABLE=false\n",
    )
    .unwrap();

    // The project as it arrives: a Laravel-shaped `.env.example`, which is the
    // file a clone actually has.
    std::fs::write(
        project.join(".env.example"),
        "\
APP_NAME=Shop
DB_CONNECTION=mysql
DB_HOST=stackvo-mysql
CACHE_STORE=redis
REDIS_HOST=stackvo-redis
MAIL_MAILER=log
",
    )
    .unwrap();

    // ---- 1. what the project's own file implies ---------------------------

    let hints = detect::services_of(&project);
    let suggested: Vec<String> = hints.iter().map(|h| h.service.clone()).collect();
    assert_eq!(suggested, ["mysql", "redis"]);
    // The key, not the value. This file is where credentials live.
    assert!(hints
        .iter()
        .all(|h| h.key.ends_with("CONNECTION") || h.key.ends_with("STORE")));

    // ---- 2. written into the manifest, which is what gets committed --------

    let mut m = manifest::normalize_spec(
        &serde_json::json!({
            "name": "shop",
            "domain": "shop.loc",
            "runtime": "php",
            "php": { "version": "8.4", "extensions": ["mbstring"] }
        }),
        "shop",
    );
    m.services = suggested.clone();

    let path = project.join("stackvo.json");
    manifest::write(&path, &m).expect("writing the manifest");

    // Read back off disk rather than trusted in memory: the write path
    // round-trips through `to_json`, and the layout rule W-01 reserves the end
    // of the document, so a new key in the wrong place fails here.
    let back = manifest::read(&path, "shop").expect("reading it back");
    assert!(back.valid, "{:?}", back.errors);
    assert_eq!(back.services, ["mysql", "redis"]);

    // ---- 3. planned against the workspace ---------------------------------

    let plan = preset::plan_declared(&root, &back.services).expect("planning");
    assert_eq!(plan.changes.len(), 2);
    assert!(plan.needs_regenerate);
    assert!(plan.rejected.is_empty());

    // ---- 4. applied, and the workspace agrees -----------------------------

    preset::apply_declared(&root, &back.services).expect("applying");

    let env = Env::load(&root).expect("loading the workspace .env");
    assert!(env.service_enabled("mysql"));
    assert!(env.service_enabled("redis"));

    // Applying twice is not an error and writes nothing the second time — the
    // button is one somebody will click again after a regenerate.
    let again = preset::plan_declared(&root, &back.services).expect("re-planning");
    assert!(again.changes.is_empty());
    assert_eq!(again.unchanged, 2);

    let _ = std::fs::remove_dir_all(&root);
}

/// A declaration never turns anything off, and never touches a service it does
/// not name. The failure this rules out is a shared machine: opening project A
/// must not stop project B's database.
#[test]
fn nothing_a_project_did_not_ask_for_changes() {
    let root = scratch("untouched");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(".env"),
        "SERVICE_MYSQL_ENABLE=false\nSERVICE_ELASTICSEARCH_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n",
    )
    .unwrap();

    preset::apply_declared(&root, &["mysql".to_string()]).expect("applying");

    let env = Env::load(&root).unwrap();
    assert!(env.service_enabled("mysql"));
    // Still on, though this project never mentioned it.
    assert!(env.service_enabled("elasticsearch"));
    // And the version is the workspace's business, not the project's.
    assert_eq!(env.get("SERVICE_MYSQL_VERSION"), Some("8.0"));

    let _ = std::fs::remove_dir_all(&root);
}
