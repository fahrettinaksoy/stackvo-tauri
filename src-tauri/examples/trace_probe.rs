//! Does the flame graph come out of a **real** Xdebug trace? (F-3)
//!
//! `trace.rs` is tested against a fixture — lines somebody typed to look like
//! what `xdebug.trace_format=1` produces. That is the same arrangement that let
//! the DNS responder ship a malformed reply and the Mongo query log ship five
//! hundred characters of driver envelope: a parser measured against its
//! author's belief about a format.
//!
//! So this asks the real extension, in a container that is already running:
//!
//! ```sh
//! cargo run --example trace_probe          # first running project
//! cargo run --example trace_probe -- shop  # a named one
//! ```
//!
//! **It changes nothing.** The settings go on one `php` invocation as `-d`
//! flags — the same four the app writes into its ini — so the container's
//! configuration, its running processes and the project's mode file are all
//! left exactly as they were. The trace it writes is deleted on the way out.
//!
//! The PHP it runs is built around the one case that separates a flame graph
//! from a call tree: `slow()` called from two different parents. Cachegrind
//! sums those into one edge and cannot say which caller was expensive; the
//! whole point of F-3 is that the trace can.

use stackvo_desktop_lib::{profile, trace, workspace};
use std::process::Command;

/// The program under measurement, and it is written to be recognisable coming
/// back: two parents, one shared callee, and a sleep long enough that a
/// microsecond-resolution timer cannot mistake which is which.
const PHP: &str = "\
function slow($us){ usleep($us); } \
function under_a(){ slow(60000); } \
function under_b(){ slow(10000); } \
function main(){ under_a(); under_b(); } \
main();";

fn main() {
    let Ok(root) = workspace::resolve().require_root() else {
        println!("no workspace — nothing to measure");
        return;
    };

    let wanted = std::env::args().nth(1);
    let Some(project) = wanted.or_else(|| first_running_project(&root)) else {
        println!("no running project with Xdebug — nothing to measure");
        return;
    };
    let container = format!("stackvo-{project}");
    println!("project   {project}\ncontainer {container}");

    // The same settings `xdebug::profile_ini` writes, as flags, so what is
    // measured is this app's configuration and not a convenient one.
    //
    // `XDEBUG_MODE` rather than `-d xdebug.mode`, and finding out why cost a
    // measurement: the environment variable **wins** over the ini setting, the
    // running container already has one (`debug`), and Xdebug quietly ignored
    // the flag. `xdebug::overlay_yaml` says exactly this in a comment and sets
    // the variable for the same reason.
    let args = [
        "exec",
        "-e",
        "XDEBUG_MODE=trace",
        &container,
        "php",
        "-d",
        "xdebug.start_with_request=yes",
        "-d",
        "xdebug.trace_format=1",
        "-d",
        "xdebug.use_compression=0",
        "-d",
        "xdebug.output_dir=/var/log/xdebug",
        "-d",
        "xdebug.trace_output_name=trace.probe",
        "-r",
        PHP,
    ];

    match Command::new("docker").args(args).output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            println!(
                "the container refused: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return;
        }
        Err(e) => {
            println!("docker is not reachable: {e}");
            return;
        }
    }

    let traces = match trace::list(&root, &project) {
        Ok(list) => list,
        Err(e) => {
            println!("could not list traces: {}", e.message);
            return;
        }
    };
    let Some(file) = traces.first() else {
        println!(
            "FAIL nothing was written to {}",
            profile::host_dir(&root, &project).display()
        );
        return;
    };
    println!("trace     {} ({} bytes)\n", file.id, file.bytes);

    let flame = match trace::read(&root, &project, &file.id) {
        Ok(flame) => flame,
        Err(e) => {
            println!("FAIL the trace could not be read: {}", e.message);
            return;
        }
    };

    println!(
        "records={} stacks={} total={}µs pruned={} truncated={}",
        flame.records, flame.stacks, flame.total, flame.pruned, flame.truncated
    );
    print_frames(&flame.frames, 0, flame.total);

    // The assertion the whole feature exists for.
    let a = find(&flame.frames, &["{main}", "main", "under_a", "slow"]);
    let b = find(&flame.frames, &["{main}", "main", "under_b", "slow"]);
    println!();
    match (a, b) {
        (Some(a), Some(b)) => {
            let ratio = a as f64 / b.max(1) as f64;
            let ok = a > b && (3.0..12.0).contains(&ratio);
            println!(
                "{} slow() under under_a = {a}µs, under under_b = {b}µs (asked for 60ms and 10ms)",
                if ok { "ok  " } else { "FAIL" }
            );
            if ok {
                println!(
                    "     two callers, two widths — which is the thing cachegrind cannot say."
                );
            }
        }
        _ => println!("FAIL slow() was not found under both parents — the paths were merged"),
    }

    // Leave nothing behind.
    match trace::delete(&root, &project, &file.id) {
        Ok(()) => println!("\nthe trace was removed."),
        Err(e) => println!("\nWARNING: {} is still there — {}", file.id, e.message),
    }
}

/// The value of one exact path, or `None` when it is not in the tree.
fn find(frames: &[profile::Frame], path: &[&str]) -> Option<u64> {
    let (head, rest) = path.split_first()?;
    let frame = frames.iter().find(|f| f.name == *head)?;
    if rest.is_empty() {
        return Some(frame.value);
    }
    find(&frame.children, rest)
}

fn print_frames(frames: &[profile::Frame], depth: usize, total: u64) {
    for frame in frames.iter().take(6) {
        let share = if total > 0 {
            frame.value as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        println!(
            "  {:indent$}{} {:>9}µs {:>5.1}%{}",
            "",
            frame.name,
            frame.value,
            share,
            if frame.recursive { "  (recursive)" } else { "" },
            indent = depth * 2
        );
        print_frames(&frame.children, depth + 1, total);
    }
}

/// The first project whose container is up, so the common case needs no
/// argument.
fn first_running_project(root: &std::path::Path) -> Option<String> {
    let dir = workspace::projects_root(root)?;
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();

    let running = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .output()
        .ok()?;
    let running = String::from_utf8_lossy(&running.stdout).to_string();

    names.into_iter().find(|name| {
        running
            .lines()
            .any(|line| line == format!("stackvo-{name}"))
    })
}
