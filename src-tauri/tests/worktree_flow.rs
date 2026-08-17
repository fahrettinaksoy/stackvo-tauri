//! N — a branch with an environment of its own, against a real repository.
//!
//! The unit tests in `worktree.rs` cover the derivations, which are pure. What
//! they cannot cover is the thing this feature actually rests on: that
//! `git worktree add` plus a machine-local overlay produces a directory the
//! rest of this app reads as an ordinary project — with the right name, the
//! right hostname, no manifest error, and a compose overlay carrying the
//! branch's own variables.
//!
//! Every assertion in the default run is against real git in a real temporary
//! repository, and Docker is not involved. The database half needs a running
//! engine, which CI does not have, so it sits at the bottom of this file behind
//! `#[ignore]` — a check somebody runs against their own stack rather than one
//! that fails for the absence of a daemon.
//!
//! Skipped rather than failed without git, for the reason `real_checkout.rs`
//! skips without a checkout: a machine with no git is not a machine this
//! feature is broken on.

use stackvo_desktop_lib::{manifest, site, worktree};
use std::path::{Path, PathBuf};

/// A committed manifest, as a repository would carry it: it names the project,
/// not the worktree, and that mismatch is the whole problem this solves.
const COMMITTED: &str = r#"{
  "name": "shop",
  "domain": "shop.loc",
  "runtime": "php",
  "server": "nginx",
  "php": {
    "version": "8.4",
    "extensions": [
      "pdo",
      "pdo_mysql"
    ]
  }
}
"#;

fn git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "StackVo Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "StackVo Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A workspace with one project in it, and that project a git repository whose
/// committed manifest is already in a branch.
///
/// Returns `(app root, projects dir, parent project dir)`.
fn workspace(label: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
    if !stackvo_desktop_lib::git::available() {
        return None;
    }

    let root =
        std::env::temp_dir().join(format!("stackvo-worktree-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let projects = root.join("projects");
    let parent = projects.join("shop");
    std::fs::create_dir_all(&parent).expect("the project directory");

    // The pointer, so `workspace::projects_root` finds the tree — the same file
    // the app writes when somebody chooses a folder.
    stackvo_desktop_lib::workspace::point_at_projects(&root, &projects).expect("the pointer");

    std::fs::write(parent.join(manifest::FILE), COMMITTED).expect("the manifest");
    std::fs::create_dir_all(parent.join("public")).expect("the document root");
    std::fs::write(parent.join("public/index.php"), "<?php\n").expect("index.php");

    // `-b main` rather than relying on the machine's `init.defaultBranch`,
    // which differs between git versions and between developers.
    assert!(git(&parent, &["init", "-b", "main"]), "git init failed");
    assert!(git(&parent, &["add", "-A"]));
    assert!(git(&parent, &["commit", "-m", "first"]), "commit failed");

    Some((root, projects, parent))
}

/// The steps `worktree_create` performs, minus the Tauri layer: check out the
/// branch, write the overlay, and keep it out of commits.
fn create(parent: &Path, projects: &Path, branch: &str) -> (String, String, PathBuf) {
    let slug = worktree::slug(branch).expect("a slug");
    let name = worktree::project_name("shop", &slug);
    let domain = worktree::domain("shop.loc", &worktree::domain_label("shop", &name));
    let dir = projects.join(&name);

    let args = worktree::add_args(&dir, branch, true);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    assert!(git(parent, &refs), "git worktree add failed: {args:?}");

    manifest::write_local(&dir, &name, &worktree::local_overlay(&name, &domain))
        .expect("the overlay was refused");

    (name, domain, dir)
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

/// The claim the whole feature rests on: after `git worktree add` and one
/// overlay file, the directory is a project this app reads with no complaint,
/// under its own name and at its own hostname.
#[test]
fn a_checked_out_branch_reads_as_a_project_of_its_own() {
    let Some((root, projects, parent)) = workspace("reads") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (name, domain, dir) = create(&parent, &projects, "feature/login");
    assert_eq!(name, "shop-feature-login");
    assert_eq!(domain, "feature-login.shop.loc");

    // The branch's own files came across.
    assert!(dir.join("public/index.php").is_file(), "no checkout");
    // And the committed manifest in it is still the branch's, untouched.
    assert_eq!(
        std::fs::read_to_string(dir.join(manifest::FILE)).unwrap(),
        COMMITTED,
        "the branch's manifest was rewritten"
    );

    let m = manifest::read(&dir.join(manifest::FILE), &name).expect("readable");
    assert!(m.valid, "W-04 or another rule fired: {:?}", m.errors);
    assert_eq!(m.name, name, "the directory is the identity");
    assert_eq!(m.domain.as_deref(), Some(domain.as_str()));
    // Everything else is the branch's, unchanged.
    assert_eq!(m.php.as_ref().unwrap().version, "8.4");
    assert_eq!(m.server.as_deref(), Some("nginx"));

    cleanup(&root);
}

/// Nothing this app wrote may show up as a change on somebody's branch.
///
/// The overlay is untracked *and* excluded, so `git status` is clean — which is
/// the difference between a worktree a colleague can work on and one that has
/// a stray file in every diff they take.
#[test]
fn the_branch_is_left_exactly_as_clean_as_it_was_found() {
    let Some((root, projects, parent)) = workspace("clean") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (_, _, dir) = create(&parent, &projects, "feature/x");

    // Before the exclude: the overlay is there and git can see it.
    assert!(dir.join(manifest::LOCAL_FILE).is_file());
    assert_eq!(
        stackvo_desktop_lib::git::is_ignored(&dir.join(manifest::LOCAL_FILE)),
        Some(false),
        "a fresh repository should not ignore it yet"
    );

    assert!(
        worktree::exclude_local_file(&dir),
        "the exclude line was not written"
    );
    assert_eq!(
        stackvo_desktop_lib::git::is_ignored(&dir.join(manifest::LOCAL_FILE)),
        Some(true),
        "git still does not ignore the overlay"
    );
    assert_eq!(
        worktree::is_dirty(&dir),
        Some(false),
        "the worktree is not clean; something this app wrote is visible to git"
    );

    // Written once. A second call must not append the rule again.
    assert!(!worktree::exclude_local_file(&dir), "written twice");

    // And it reaches the main checkout too, which is the point of using the
    // repository's shared local exclude rather than a per-worktree trick.
    assert_eq!(
        stackvo_desktop_lib::git::is_ignored(&parent.join(manifest::LOCAL_FILE)),
        Some(true)
    );

    cleanup(&root);
}

/// git's own view of the repository, read back through the porcelain parser
/// against real output rather than a fixture.
#[test]
fn git_reports_the_worktree_and_the_branch_it_holds() {
    let Some((root, projects, parent)) = workspace("listing") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (_, _, dir) = create(&parent, &projects, "feature/x");

    assert!(worktree::is_repository(&dir));
    assert!(
        worktree::is_linked_worktree(&dir),
        "the new checkout is not recognised as a linked worktree"
    );
    assert!(
        !worktree::is_linked_worktree(&parent),
        "the main checkout was mistaken for a linked one"
    );
    assert_eq!(worktree::current_branch(&dir).as_deref(), Some("feature/x"));

    let checkouts = worktree::checkouts(&parent);
    assert_eq!(checkouts.len(), 2, "{checkouts:?}");
    assert!(checkouts
        .iter()
        .any(|c| c.branch.as_deref() == Some("feature/x")));

    // The branch is now taken, which is what the plan refuses a second worktree
    // on — git allows one working tree per branch and reports it as an error.
    assert!(worktree::branches(&parent).contains(&"feature/x".to_string()));

    cleanup(&root);
}

/// A branch that has no `stackvo.json` is the case a repository is in before
/// anybody adopts StackVo on it. Creation writes a full manifest derived from
/// the parent's rather than an overlay, and the parent's extra hostnames are
/// deliberately not inherited.
#[test]
fn a_branch_without_a_manifest_is_given_one_derived_from_the_parents() {
    let Some((root, projects, parent)) = workspace("nomanifest") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let slug = worktree::slug("legacy").unwrap();
    let name = worktree::project_name("shop", &slug);
    let domain = worktree::domain("shop.loc", &worktree::domain_label("shop", &name));
    let dir = projects.join(&name);

    // A branch created before the manifest existed.
    assert!(git(&parent, &["checkout", "-b", "legacy"]));
    assert!(git(&parent, &["rm", "--cached", manifest::FILE]));
    std::fs::remove_file(parent.join(manifest::FILE)).unwrap();
    assert!(git(&parent, &["commit", "-m", "before stackvo"]));
    assert!(git(&parent, &["checkout", "main"]));

    let args = worktree::add_args(&dir, "legacy", false);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    assert!(git(&parent, &refs), "git worktree add failed");
    assert!(!dir.join(manifest::FILE).is_file(), "fixture is wrong");

    // What `worktree_create` does in that branch.
    let mut derived = manifest::read_committed(&parent.join(manifest::FILE), &name).unwrap();
    derived.name = name.clone();
    derived.domain = Some(domain.clone());
    derived.aliases.clear();
    manifest::write(&dir.join(manifest::FILE), &derived).expect("the manifest was refused");

    let m = manifest::read(&dir.join(manifest::FILE), &name).unwrap();
    assert!(m.valid, "{:?}", m.errors);
    assert_eq!(m.name, name);
    assert_eq!(m.domain.as_deref(), Some(domain.as_str()));
    assert!(
        m.aliases.is_empty(),
        "the parent's hostnames were inherited"
    );
    assert!(
        !dir.join(manifest::LOCAL_FILE).is_file(),
        "an overlay was written where a manifest was enough"
    );

    cleanup(&root);
}

/// The branch's variables have to reach its container, and they reach it the
/// same way every other per-project variable does — the compose overlay.
///
/// Rendered against the generated projects file, because that is the gate
/// `site::entries` applies: a project with no compose service gets no overlay
/// entry, and a worktree that was not in the compose file would have its
/// variables silently dropped.
#[test]
fn the_worktrees_variables_reach_the_compose_overlay() {
    let Some((root, projects, parent)) = workspace("overlay") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (name, domain, _) = create(&parent, &projects, "feature/x");

    let mut table = worktree::Table::default();
    table
        .insert(worktree::Record {
            name: name.clone(),
            parent: "shop".into(),
            branch: "feature/x".into(),
            domain: domain.clone(),
            path: projects.join(&name).display().to_string(),
            database: None,
            env: std::collections::BTreeMap::from([("APP_ENV".into(), "branch".into())]),
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();
    table.save(&root).unwrap();

    // The compose file the renderer would have written, reduced to the one
    // thing `site::entries` reads out of it: which services exist.
    std::fs::create_dir_all(root.join("generated")).unwrap();
    std::fs::write(
        root.join("generated/docker-compose.projects.yml"),
        format!("name: stackvo\n\nservices:\n\n  shop:\n    image: x\n\n  {name}:\n    image: y\n"),
    )
    .unwrap();

    assert!(site::sync(&root), "no overlay was written");
    let yaml = std::fs::read_to_string(site::overlay_path(&root)).expect("the overlay file");

    assert!(yaml.contains(&format!("  {name}:")), "{yaml}");
    assert!(yaml.contains("APP_ENV: \"branch\""), "{yaml}");
    // The hostname the branch answers on, so a framework's own links do not
    // send everybody back to the parent mid-flow.
    assert!(
        yaml.contains(&format!("APP_URL: \"https://{domain}\"")),
        "{yaml}"
    );
    assert!(
        yaml.contains("STACKVO_WORKTREE_BRANCH: \"feature/x\""),
        "{yaml}"
    );
    // The parent has no variables of its own and must not gain an empty block.
    assert!(!yaml.contains("  shop:\n"), "{yaml}");

    cleanup(&root);
}

/// Removal takes the checkout and git's registration with it, and leaves the
/// repository able to check that branch out again.
#[test]
fn removing_a_worktree_gives_the_branch_back() {
    let Some((root, projects, parent)) = workspace("removal") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (_, _, dir) = create(&parent, &projects, "feature/x");
    assert_eq!(worktree::checkouts(&parent).len(), 2);

    worktree::remove(&parent, &dir, false).expect("removal was refused");
    assert!(!dir.exists(), "the directory is still there");
    assert_eq!(worktree::checkouts(&parent).len(), 1);

    // The branch survives the worktree, which is the default and the reason
    // deleting it is a switch of its own.
    assert!(worktree::branches(&parent).contains(&"feature/x".to_string()));
    assert!(worktree::delete_branch(&parent, "feature/x"));
    assert!(!worktree::branches(&parent).contains(&"feature/x".to_string()));

    cleanup(&root);
}

/// A worktree with uncommitted work is not removed by accident. `force` is the
/// switch the screen puts that behind, and without it git refuses and the
/// refusal carries the hint that names the way out.
#[test]
fn uncommitted_work_stops_a_removal_until_it_is_asked_for() {
    let Some((root, projects, parent)) = workspace("dirty") else {
        eprintln!("skipping: git is not installed");
        return;
    };

    let (name, domain, dir) = create(&parent, &projects, "feature/x");
    std::fs::write(dir.join("public/index.php"), "<?php // edited\n").unwrap();
    assert_eq!(worktree::is_dirty(&dir), Some(true));

    let err = worktree::remove(&parent, &dir, false).expect_err("a dirty worktree was removed");
    assert_eq!(err.hint_key, Some("worktreeIsDirty"), "{err:?}");
    assert!(dir.exists(), "the work was thrown away");

    // A refused removal leaves the worktree as capable as it was. Removal moves
    // the overlay out of git's way first, and without putting it back a failure
    // here would strip the checkout of its name and hostname — a project the
    // app would then report as broken, for a button press that did nothing.
    assert!(
        dir.join(manifest::LOCAL_FILE).is_file(),
        "the overlay was not put back after a refused removal"
    );
    let m = manifest::read(&dir.join(manifest::FILE), &name).unwrap();
    assert!(m.valid, "{:?}", m.errors);
    assert_eq!(m.domain.as_deref(), Some(domain.as_str()));

    worktree::remove(&parent, &dir, true).expect("force was refused");
    assert!(!dir.exists());

    cleanup(&root);
}

// ------------------------------------------------------- against a real engine
//
// `#[ignore]` because it needs a running stack: a workspace at `STACKVO_ROOT`
// (or `~/.stackvo`) with a database instance up. CI has neither, and a test
// that fails for the absence of Docker is a test people learn to ignore for
// real reasons too.
//
//     cargo test --test worktree_flow -- --ignored --nocapture
//
// What it is for: every other test in this file asserts on an argument list or
// a file on disk, and the statements this feature builds are neither. A
// `CREATE DATABASE` with the wrong quoting, or a `mongosh` call that cannot
// authenticate, is right in every unit test and wrong against the server — the
// mistake `mariadb-dump` and the QR encoder both cost this repository once.

/// The workspace this machine is actually running, if there is one.
fn live_workspace() -> Option<PathBuf> {
    let root = std::env::var("STACKVO_ROOT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".stackvo")))?;
    root.join("services/instances.json")
        .is_file()
        .then_some(root)
}

/// Create a database, see it listed, and drop it — through the code the
/// worktree flow uses, on whatever engines are up.
///
/// The name is a probe of its own and nothing else is touched: no existing
/// database is read, and the workspace's own is refused by `drop_database`
/// whatever is asked of it.
#[test]
#[ignore = "needs a running database instance"]
fn a_database_is_created_and_dropped_on_every_engine_that_is_up() {
    let Some(root) = live_workspace() else {
        eprintln!("skipping: no workspace with an instance table");
        return;
    };

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let instances = stackvo_desktop_lib::db::instances(&root)
            .await
            .expect("the instance table");
        let running: Vec<_> = instances.into_iter().filter(|i| i.running).collect();
        assert!(
            !running.is_empty(),
            "no database instance is running; start one and try again"
        );

        for instance in running {
            let probe = "stackvo_worktree_probe";
            eprintln!("— {} ({:?})", instance.id, instance.kind);

            // The listing goes through `run_sql_with`, which is where the Mongo
            // authentication database matters: without it the root credentials
            // are checked against a database that has never heard of them.
            let before = stackvo_desktop_lib::db::databases(&root, &instance.id)
                .await
                .unwrap_or_else(|e| {
                    panic!("{} could not list databases: {}", instance.id, e.message)
                });
            assert!(!before.is_empty(), "{} listed nothing at all", instance.id);
            assert!(
                !before.contains(&probe.to_string()),
                "{probe} already exists on {}; drop it and run again",
                instance.id
            );

            let created =
                stackvo_desktop_lib::db::create_database(&root, &instance.id, probe, None)
                    .await
                    .unwrap_or_else(|e| panic!("{} refused CREATE: {}", instance.id, e.message));

            // MongoDB has no CREATE DATABASE and says so by creating nothing;
            // the other three have to have produced one.
            if instance.kind == stackvo_desktop_lib::db::Kind::Mongo {
                assert!(!created, "Mongo reported a database it cannot create");
                continue;
            }

            assert!(created, "{} reported no database created", instance.id);
            let after = stackvo_desktop_lib::db::databases(&root, &instance.id)
                .await
                .expect("a listing");
            assert!(
                after.contains(&probe.to_string()),
                "{probe} is not on {} after creating it",
                instance.id
            );

            assert!(
                stackvo_desktop_lib::db::drop_database(&root, &instance.id, probe)
                    .await
                    .unwrap_or_else(|e| panic!("{} refused DROP: {}", instance.id, e.message)),
                "{} reported nothing dropped",
                instance.id
            );
            let finally = stackvo_desktop_lib::db::databases(&root, &instance.id)
                .await
                .expect("a listing");
            assert!(
                !finally.contains(&probe.to_string()),
                "{probe} survived the drop on {}",
                instance.id
            );
        }
    });
}

/// The guard that matters more than any of the above: the workspace's own
/// database is not a worktree's to drop, however it is asked for.
#[test]
#[ignore = "needs a running database instance"]
fn the_workspaces_own_database_is_refused() {
    let Some(root) = live_workspace() else {
        eprintln!("skipping: no workspace with an instance table");
        return;
    };

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let instances = stackvo_desktop_lib::db::instances(&root)
            .await
            .expect("instances");
        let mut checked = 0;

        for instance in instances.into_iter().filter(|i| i.running) {
            let connection =
                stackvo_desktop_lib::db::connection(&root, &instance.id).expect("a connection");
            let Some(own) = connection.database else {
                continue;
            };
            let err = stackvo_desktop_lib::db::drop_database(&root, &instance.id, &own)
                .await
                .expect_err("the workspace's own database was dropped");
            assert!(err.message.contains(&own), "{}", err.message);
            checked += 1;
        }

        assert!(checked > 0, "no running instance names a database to guard");
    });
}
