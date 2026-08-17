//! What `projects_list` actually costs, split into its two halves.
//!
//! §3 #27 has stood at half done with the note "the hidden-window slowdown was
//! closed; no cache". A cache is the kind of thing that is obviously worth
//! adding until somebody measures the thing it would avoid — and the field this
//! one would have to cache is `running`, which is the single field on the row
//! that must never be stale. So this measures first.
//!
//! ```sh
//! cargo run --release --example list_bench
//! ```
//!
//! Two numbers, because they scale differently and only one of them can be
//! made faster by remembering anything:
//!
//! * **the engine** — one `stackvo_containers()` call, a fixed cost whatever
//!   the workspace holds;
//! * **the tree** — one directory scan plus a manifest read and a hosts lookup
//!   per project, which grows with the number of projects.
//!
//! Point it at a workspace with as many projects as you like:
//!
//! ```sh
//! STACKVO_ROOT=/tmp/fifty cargo run --release --example list_bench
//! ```

use stackvo_desktop_lib::{commands, engine, workspace};
use std::time::Instant;

/// Enough runs for the median to mean something, few enough to stay quick.
const RUNS: usize = 20;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let Ok(root) = workspace::resolve().require_root() else {
        println!("no workspace — nothing to measure");
        return;
    };
    println!("workspace {}\n", root.display());

    // Once, unmeasured: the first call pays for the connection to the daemon
    // and for whatever the operating system has not cached yet, and a number
    // that includes it is a number about start-up rather than about this.
    let _ = commands::list_projects(&root).await;

    let mut whole = Vec::new();
    let mut engine_only = Vec::new();
    let mut count = 0;

    for _ in 0..RUNS {
        let started = Instant::now();
        match commands::list_projects(&root).await {
            Ok(projects) => {
                count = projects.len();
                whole.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            Err(e) => {
                println!("list_projects failed — {}", e.message);
                return;
            }
        }

        let started = Instant::now();
        let _ = engine::stackvo_containers().await;
        engine_only.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let report = |name: &str, mut samples: Vec<f64>| {
        samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
        println!(
            "{name:<22} median {:6.1} ms   min {:6.1}   max {:6.1}",
            samples[samples.len() / 2],
            samples[0],
            samples[samples.len() - 1]
        );
    };

    println!("{count} project(s), {RUNS} runs\n");
    report("projects_list", whole.clone());
    report("  of which engine", engine_only.clone());

    let median = |mut s: Vec<f64>| {
        s.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
        s[s.len() / 2]
    };
    let tree = (median(whole) - median(engine_only)).max(0.0);
    println!("  the tree, by difference   {tree:6.1} ms");
    if count > 0 {
        println!("  per project               {:6.2} ms", tree / count as f64);
    }
}
