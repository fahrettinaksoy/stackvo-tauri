//! Does the workbench actually run code in the projects on this machine?
//!
//! F-5. `repl.rs` has unit tests and every one of them is about an **argv** — a
//! Vec<String> compared against another Vec<String>. That proves the command
//! this app builds is the one its author meant to build, and proves nothing
//! about whether `php artisan tinker --execute` exists in that container, or
//! whether `timeout` is on its PATH, or whether the output comes back on the
//! stream the pane reads.
//!
//! ```sh
//! cargo run --example repl_probe          # every runner, every project
//! cargo run --example repl_probe -- --slow # and the 30-second limit
//! ```
//!
//! For each project it asks which runners are offered, then for each one runs
//! three snippets: one that prints something this probe can recognise, one that
//! fails on purpose, and — with `--slow` — one that never finishes. What it is
//! checking is not "did something happen" but the three things the pane shows:
//! the output, the exit code, and whether the limit was in force.

use stackvo_desktop_lib::{applog, engine, repl, workspace};
use std::path::Path;

/// Printed by the snippet, looked for in the output. Deliberately not a word
/// that could appear in a framework's own start-up chatter.
const MARKER: &str = "stackvo_repl_probe_4711";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let slow = std::env::args().any(|arg| arg == "--slow");

    let Ok(root) = workspace::resolve().require_root() else {
        println!("no workspace — nothing to measure");
        return;
    };
    println!("workspace {}\n", root.display());

    let Ok(projects) = applog::projects(&root) else {
        println!("no projects directory — nothing to measure");
        return;
    };

    let mut measured = 0;
    let mut failures = 0;

    for project in projects {
        let runners = match repl::for_project(&root, &project) {
            Ok(list) => list,
            Err(e) => {
                println!("{project:<28} skipped — {}", e.message);
                continue;
            }
        };
        if runners.is_empty() {
            println!("{project:<28} no runner — nothing in it this can load");
            continue;
        }

        // A stopped project is not a broken feature, and the two must not
        // print the same line.
        let running = engine::inspect(&project)
            .await
            .map(|details| details.running)
            .unwrap_or(false);
        if !running {
            println!(
                "{project:<28} skipped — not running ({} runner(s) offered)",
                runners.len()
            );
            continue;
        }

        println!("{project}");
        for runner in &runners {
            measured += 1;
            match exercise(&root, &project, runner, slow).await {
                Ok(Outcome::Measured(report)) => {
                    if !report.ok() {
                        failures += 1;
                    }
                    println!("  {report}");
                }
                Ok(Outcome::Skipped(why)) => {
                    measured -= 1;
                    println!("  --   {:<10} skipped — {why}", runner.id);
                }
                Err(e) => {
                    failures += 1;
                    println!("  FAIL {:<10} {}", runner.id, e.message);
                }
            }
        }
    }

    println!();
    if measured == 0 {
        println!("nothing was running that this could measure.");
    } else if failures == 0 {
        println!("{measured} runner(s) ran what they were given and reported it.");
    } else {
        println!("{failures} of {measured} did not. The lines above are the evidence.");
    }
    if !slow {
        println!("the 30-second limit was not exercised — pass --slow for that.");
    }
}

/// A runner that was measured, or one there was nothing to measure.
enum Outcome {
    Measured(Report),
    /// The reason, in the words the next reader needs. A booted runner in a
    /// project whose dependencies were never installed is not a broken feature
    /// — it is a project that cannot boot, and reporting the two the same way
    /// is how a probe becomes one nobody runs.
    Skipped(String),
}

struct Report {
    id: String,
    /// The marker came back on stdout.
    printed: bool,
    /// A snippet that throws is reported as a failure rather than as success
    /// with text in it — the measurement that decided `Run::exit_code` is what
    /// the pane reads, because a PHP fatal is written to **stdout**.
    failed_loudly: bool,
    /// The in-container limit was in force.
    limited: bool,
    /// `None` when `--slow` was not passed.
    timed_out: Option<bool>,
    ms: u64,
}

impl Report {
    fn ok(&self) -> bool {
        self.printed && self.failed_loudly && self.limited && self.timed_out != Some(false)
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:<10} printed={} failed={} limited={} timeout={} ({} ms)",
            if self.ok() { "ok  " } else { "FAIL" },
            self.id,
            self.printed,
            self.failed_loudly,
            self.limited,
            match self.timed_out {
                Some(hit) => hit.to_string(),
                None => "skipped".to_string(),
            },
            self.ms
        )
    }
}

async fn exercise(
    root: &Path,
    project: &str,
    runner: &repl::Runner,
    slow: bool,
) -> stackvo_desktop_lib::error::Result<Outcome> {
    let flavour = repl::Flavour::from_id(&runner.id).expect("a runner the app just offered");

    let printed = repl::run(root, project, &runner.id, print_snippet(flavour)).await?;
    if let Some(why) = cannot_boot(&printed) {
        return Ok(Outcome::Skipped(why));
    }
    let failed = repl::run(root, project, &runner.id, fail_snippet(flavour)).await?;

    let timed_out = if slow {
        Some(
            repl::run(root, project, &runner.id, forever_snippet(flavour))
                .await?
                .timed_out,
        )
    } else {
        None
    };

    Ok(Outcome::Measured(Report {
        id: runner.id.clone(),
        printed: printed.stdout.contains(MARKER),
        // Not "stderr is non-empty": that is exactly the check that would pass
        // on Node and fail on PHP, whose fatals go to stdout.
        failed_loudly: failed.exit_code.is_some_and(|code| code != 0),
        limited: printed.limited,
        timed_out,
        ms: printed.ms,
    }))
}

/// Is this a project that cannot boot, rather than a runner that does not work?
///
/// Each of these is the interpreter saying it never reached the snippet.
/// Matched on the loader's own words rather than on the exit code, which is the
/// same for a snippet that threw — and this exists because the case is the
/// ordinary one on a fresh clone: dependencies are not in a repository.
fn cannot_boot(run: &repl::Run) -> Option<String> {
    let text = format!("{}{}", run.stdout, run.stderr).to_ascii_lowercase();
    for (marker, why) in [
        (
            "vendor/autoload.php",
            "composer dependencies are not installed",
        ),
        ("cannot find module", "node dependencies are not installed"),
        (
            "modulenotfounderror",
            "python dependencies are not installed",
        ),
        (
            "bundler::gemnotfound",
            "ruby dependencies are not installed",
        ),
        ("could not find gem", "ruby dependencies are not installed"),
    ] {
        if text.contains(marker) {
            return Some(why.to_string());
        }
    }
    None
}

/// The same "print this" in each language.
fn print_snippet(flavour: repl::Flavour) -> &'static str {
    match flavour {
        // Laravel's `--execute` does not echo the value of the last expression
        // the way the interactive REPL does — measured — so every one of these
        // prints explicitly.
        repl::Flavour::Laravel | repl::Flavour::WordPress | repl::Flavour::Php => {
            concat!("echo \"", "stackvo_repl_probe_4711", "\";")
        }
        repl::Flavour::Django => concat!("print(\"", "stackvo_repl_probe_4711", "\")"),
        repl::Flavour::Rails => concat!("puts \"", "stackvo_repl_probe_4711", "\""),
        repl::Flavour::Node => concat!("console.log(\"", "stackvo_repl_probe_4711", "\")"),
    }
}

fn fail_snippet(flavour: repl::Flavour) -> &'static str {
    match flavour {
        repl::Flavour::Laravel | repl::Flavour::WordPress | repl::Flavour::Php => {
            "throw new RuntimeException(\"probe\");"
        }
        repl::Flavour::Django => "raise RuntimeError(\"probe\")",
        repl::Flavour::Rails => "raise \"probe\"",
        repl::Flavour::Node => "throw new Error(\"probe\")",
    }
}

/// Longer than the limit, and doing nothing while it waits.
fn forever_snippet(flavour: repl::Flavour) -> &'static str {
    match flavour {
        repl::Flavour::Laravel | repl::Flavour::WordPress | repl::Flavour::Php => "sleep(120);",
        repl::Flavour::Django => "import time; time.sleep(120)",
        repl::Flavour::Rails => "sleep 120",
        repl::Flavour::Node => {
            "Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 120000)"
        }
    }
}
