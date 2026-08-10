//! The snapshot registry against a real filesystem.
//!
//! The registry *is* the directory — there is no index file — so everything
//! interesting about it is a question about `read_dir`, file stems, extensions
//! and modification times, and none of that is exercised by the pure tests in
//! `snapshot.rs`. This file builds a workspace with files in it and asks.
//!
//! What is deliberately **not** here: taking a dump. `db_snapshot_take` resolves
//! a name to a path and then calls `db::dump`, which is unchanged and was
//! already the path `db_dump` used. Standing a database up to prove that a
//! function this change did not touch still works would be testing the wrong
//! thing — and doing it against the databases on the machine that runs the
//! suite would be worse.

use stackvo_desktop_lib::snapshot;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("stackvo-snapshots-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

/// Write a file where a snapshot of `service` would be.
///
/// No mtime is forced. Setting one needs `filetime`, and a dependency added so
/// a test can pretend a file is old is a dependency in the shipped lockfile for
/// ever — [`snapshot::expired`] breaks ties on the name precisely so that the
/// order does not depend on the clock.
fn place(root: &std::path::Path, service: &str, file: &str) {
    let dir = snapshot::dir(root, service);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), b"-- dump\n").unwrap();
}

#[test]
fn the_directory_listing_is_the_registry() {
    let root = scratch("listing");

    place(&root, "mysql", "before-migration.sql");
    place(&root, "mysql", "auto-2026-08-01T00-00-00.sql");
    place(&root, "postgres", "nightly.sql");

    // Not a snapshot: the wrong extension for this engine. Offering to restore
    // one would run a text file through `mysql` as if it were SQL.
    place(&root, "mysql", "notes.txt");
    // Nor is a directory somebody made by hand.
    std::fs::create_dir_all(snapshot::dir(&root, "mysql").join("old.sql")).unwrap();

    let found = snapshot::list(&root);
    let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"before-migration"), "{names:?}");
    assert!(names.contains(&"auto-2026-08-01T00-00-00"), "{names:?}");
    assert!(names.contains(&"nightly"), "{names:?}");
    assert!(!names.contains(&"notes"), "{names:?}");
    assert!(!names.contains(&"old"), "{names:?}");

    // The prefix decides what retention may touch, read off the filename.
    let auto = found.iter().find(|s| s.name.starts_with("auto-")).unwrap();
    assert!(auto.automatic);
    assert!(
        !found
            .iter()
            .find(|s| s.name == "before-migration")
            .unwrap()
            .automatic
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A workspace where nothing has been taken yet is no snapshots, not an error.
#[test]
fn a_workspace_with_no_backups_directory_has_no_snapshots() {
    let root = scratch("empty");
    assert!(snapshot::list(&root).is_empty());
    assert!(snapshot::last_automatic(&root, "mysql").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// `last_automatic` is what the schedule compares against, and it must ignore
/// the snapshot somebody took by hand five minutes ago — otherwise taking a
/// manual copy silently postpones the automatic one.
#[test]
fn the_schedule_reads_only_its_own_snapshots() {
    let root = scratch("last");

    place(&root, "mysql", "auto-old.sql");
    // A whole second, because a modification time has second resolution on the
    // filesystems this runs on and the point here is that the two files are
    // distinguishable by age. One sleep in one test, rather than a dependency
    // that can forge a timestamp.
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    place(&root, "mysql", "just-now.sql");

    let last = snapshot::last_automatic(&root, "mysql").expect("an automatic one exists");
    let newest = std::fs::metadata(snapshot::dir(&root, "mysql").join("just-now.sql"))
        .and_then(|m| m.modified())
        .unwrap();

    assert!(
        last < newest,
        "the schedule read the hand-named snapshot; taking a manual copy would \
         silently postpone the automatic one"
    );

    // And the listing opens on what somebody just took. Asserted here rather
    // than in the test above, where every file shares a second and "newest" is
    // not a question the filesystem can answer.
    let listed = snapshot::list(&root);
    assert_eq!(listed.first().map(|s| s.name.as_str()), Some("just-now"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn removing_is_idempotent_and_confined_to_the_directory() {
    let root = scratch("remove");
    place(&root, "mysql", "gone.sql");

    assert!(snapshot::remove(&root, "mysql", "gone").is_ok());
    assert!(!snapshot::dir(&root, "mysql").join("gone.sql").exists());
    // A second click is not an error.
    assert!(snapshot::remove(&root, "mysql", "gone").is_ok());

    // A file outside the snapshot directory, which a traversal would reach.
    let outside = root.join("keep-me.sql");
    std::fs::write(&outside, b"x").unwrap();
    assert!(snapshot::remove(&root, "mysql", "../keep-me").is_err());
    assert!(outside.exists(), "a traversing name deleted a file outside");

    let _ = std::fs::remove_dir_all(&root);
}

/// The retention rule, end to end over files rather than over structs: take
/// four automatic snapshots and one named, keep two, and see what is left.
#[test]
fn retention_leaves_the_named_snapshot_alone() {
    let root = scratch("retention");

    // Written in the same second and distinguished by name — which is exactly
    // the case `expired`'s tie-break exists for, and the one an hourly schedule
    // catching up after a sleep produces.
    for index in 0..4 {
        place(&root, "mysql", &format!("auto-{index}.sql"));
    }
    place(&root, "mysql", "before-migration.sql");

    for name in snapshot::expired(&snapshot::list(&root), 2) {
        snapshot::remove(&root, "mysql", &name).expect("removing an expired snapshot");
    }

    let left: Vec<String> = snapshot::list(&root).into_iter().map(|s| s.name).collect();

    assert_eq!(left.len(), 3, "{left:?}");
    assert!(left.contains(&"before-migration".to_string()), "{left:?}");
    assert!(left.contains(&"auto-2".to_string()), "{left:?}");
    assert!(left.contains(&"auto-3".to_string()), "{left:?}");
    assert!(!left.contains(&"auto-0".to_string()), "{left:?}");

    let _ = std::fs::remove_dir_all(&root);
}
