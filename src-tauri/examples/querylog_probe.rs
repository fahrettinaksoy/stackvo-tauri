//! Does the query log actually record what it claims, on the real databases?
//!
//! F-1. `querylog.rs` has unit tests, and every one of them parses a **fixture**
//! — a string somebody typed to look like what MySQL, Postgres or Mongo would
//! write. That proves the parser reads what its author believed the format to
//! be, and proves nothing about what a `mysql:9` container actually emits, or
//! whether `SET GLOBAL log_output='TABLE'` still takes effect on that version,
//! or whether the enable statement works at all on a database that has moved on
//! two major versions since the note in the module header was written.
//!
//! So this runs the real thing against whatever databases are up:
//!
//! ```sh
//! cargo run --example querylog_probe
//! ```
//!
//! For each one it switches recording on, asks a question **it can recognise
//! coming back** — including the N+1 shape the feature exists to find — reads
//! the session, and switches recording off again. It restores the previous
//! state: a database that was already recording is left recording.

use stackvo_desktop_lib::{db, querylog, workspace};
use std::path::Path;

/// A statement that is unmistakably ours coming back out of the log.
const MARKER: &str = "stackvo_querylog_probe";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let Ok(root) = workspace::resolve().require_root() else {
        println!("no workspace — nothing to measure");
        return;
    };
    println!("workspace {}\n", root.display());

    let mut failures = 0;
    let mut measured = 0;

    for service in ["mysql", "mariadb", "postgres", "mongo"] {
        let Some(kind) = db::Kind::from_service(service) else {
            continue;
        };
        if !querylog::supports(kind) {
            println!("{service:<9} not supported by this feature");
            continue;
        }

        // Is it even up? `read` on a stopped container fails with the engine's
        // error, which is a different thing from the feature being broken.
        let before = match querylog::read(&root, service).await {
            Ok(session) => session,
            Err(e) => {
                println!("{service:<9} skipped — {}", e.message);
                continue;
            }
        };
        measured += 1;
        println!("{service:<9} up, recording={}", before.recording);

        match exercise(&root, service).await {
            Ok(report) => {
                let ok = report.entries > 0
                    && report.found_marker
                    && report.repeat_seen
                    && report.cleared;
                if !ok {
                    failures += 1;
                }
                println!(
                    "  {} entries={} marker={} repeats={} (n+1 of {} seen as {}) cleared={}",
                    if ok { "ok  " } else { "FAIL" },
                    report.entries,
                    report.found_marker,
                    report.repeats,
                    report.expected_repeat,
                    report.repeat_seen,
                    report.cleared,
                );
                for line in &report.detail {
                    println!("      {line}");
                }
            }
            Err(e) => {
                failures += 1;
                println!("  FAIL {}", e.message);
            }
        }

        // The probe's own database goes with it — a measurement that leaves a
        // database behind is one nobody can run twice and trust.
        if service == "mongo" {
            let _ = db::run_sql(
                &root,
                service,
                &format!("db.getSiblingDB('{MARKER}').dropDatabase()"),
            )
            .await;
        }

        // Put it back the way it was found.
        let restore = if before.recording {
            querylog::enable(&root, service).await
        } else {
            querylog::disable(&root, service).await
        };
        if let Err(e) = restore {
            println!(
                "  WARNING: could not restore recording state — {}",
                e.message
            );
        }
    }

    println!();
    if measured == 0 {
        println!("no database was running — nothing was measured.");
    } else if failures == 0 {
        println!("{measured} database(s) recorded and reported what was asked of them.");
    } else {
        println!("{failures} of {measured} did not. The lines above are the evidence.");
    }
}

struct Report {
    entries: usize,
    repeats: usize,
    found_marker: bool,
    /// The number of times the probe asked the same question.
    expected_repeat: usize,
    /// Whether the repeat detector saw that group.
    repeat_seen: bool,
    /// Whether "start again from here" actually started again.
    ///
    /// Measured rather than trusted because on Postgres it is the one operation
    /// that cannot delete anything: the log belongs to the server, so clearing
    /// is a watermark written into it and honoured on the way back out. A
    /// watermark that was never written and a `clear` that returns `Ok(())`
    /// having done nothing look identical from the caller's side — which is
    /// what the Postgres branch used to be.
    cleared: bool,
    detail: Vec<String>,
}

/// Switch on, ask, read back.
async fn exercise(root: &Path, service: &str) -> stackvo_desktop_lib::error::Result<Report> {
    querylog::enable(root, service).await?;
    querylog::clear(root, service).await?;

    // Mongo profiles per database, so there has to be one. A freshly started
    // container has only the server's own — which is exactly the case that used
    // to leave the switch reporting itself as off.
    if service == "mongo" {
        db::run_sql(
            root,
            service,
            &format!("db.getSiblingDB('{MARKER}').seed.insertOne({{ probe: 1 }})"),
        )
        .await?;
        // One read before the queries, on purpose, and it is the window this
        // feature has rather than a trick to pass a test: Mongo profiles per
        // database, so a database that did not exist when recording started is
        // switched on by the next read. On screen that read is the pane's own
        // refresh; here it has to be asked for.
        let _ = querylog::read(root, service).await?;
    }

    // The N+1 shape: the same statement with a different literal each time. If
    // `shape_of` is doing its job these collapse into one group of five.
    let repeat = 5;
    for n in 0..repeat {
        let _ = db::run_sql(root, service, &probe_sql(service, n)).await?;
    }
    // Postgres writes to the container's log stream, and the line is not there
    // the instant the statement returns.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

    let session = querylog::read(root, service).await?;
    let found_marker = session
        .entries
        .iter()
        .any(|entry| entry.sql.contains(MARKER));
    // Matched on the **example**, not the shape: `shape_of` replaces the
    // literal with `?`, which is the whole point of it — so a probe looking for
    // its own marker in the shape is a probe that can never pass. (It did not,
    // twice, and the feature was right both times.)
    let group = session
        .repeats
        .iter()
        .find(|group| group.example.contains(MARKER));

    // And now the other button. Recording stays on, so anything still carrying
    // the marker afterwards is a session that was never actually cleared.
    querylog::clear(root, service).await?;
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let after = querylog::read(root, service).await?;
    let cleared = !after.entries.iter().any(|entry| entry.sql.contains(MARKER));

    Ok(Report {
        cleared,
        entries: session.entries.len(),
        repeats: session.repeats.len(),
        found_marker,
        expected_repeat: repeat,
        repeat_seen: group.is_some_and(|g| g.count >= repeat),
        detail: session
            .entries
            .iter()
            .take(8)
            .map(|e| {
                format!(
                    "entry  {:>12.3}  shape={:?}  sql={:?}",
                    e.at, e.shape, e.sql
                )
            })
            .chain(
                session
                    .repeats
                    .iter()
                    .map(|g| format!("group  x{}  {:?}", g.count, g.shape)),
            )
            .collect(),
    })
}

/// The same question in each dialect, carrying the marker.
fn probe_sql(service: &str, n: usize) -> String {
    match service {
        "mongo" => format!("db.getSiblingDB('{MARKER}').seed.find({{ probe: {n} }}).toArray()"),
        "postgres" => format!("SELECT {n} AS n, '{MARKER}' AS marker"),
        _ => format!("SELECT {n} AS n, '{MARKER}' AS marker"),
    }
}
