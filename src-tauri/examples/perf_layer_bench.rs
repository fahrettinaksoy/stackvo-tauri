//! What does moving `vendor/` off the host actually buy? (I-1)
//!
//! `mount_bench` answered the general question — a bind mount costs 2–3× a
//! named volume on metadata and writes — and that number is what justified
//! building anything at all. This one answers the question the *feature* has to
//! answer, which is narrower and is the one a person would ask:
//!
//! > I have a Laravel project. If its `vendor/` moves into a volume and the rest
//! > of my code stays where my editor can see it, how much faster is the thing I
//! > actually wait for?
//!
//! ```sh
//! cargo run --example perf_layer_bench
//! cargo run --example perf_layer_bench -- --runs 5
//! ```
//!
//! ## What is compared
//!
//! The same tree, twice, in front of the same PHP:
//!
//! - `bind`  — everything bind-mounted, which is what StackVo writes today
//! - `layer` — the source bind-mounted, `vendor/` in a named volume
//! - `both`  — and `storage/framework` in one as well, which is the second
//!   thing `perf::suggestions` offers a Laravel project
//!
//! and three workloads, chosen because they are what a request and a command
//! actually do:
//!
//! - `boot`   — `require` every autoloadable class: the shape of a framework
//!   boot, and the reason a cold page load on macOS feels the way it does
//! - `stat`   — walk and stat the whole tree, which is what an autoloader miss
//!   and every `artisan` invocation do before anything is printed
//! - `write`  — two thousand small files created, read back and removed, in the
//!   directory a framework writes its compiled views and cache into
//!
//! ## What it leaves behind
//!
//! Nothing. A scratch directory under the OS temp directory and two named
//! volumes, all removed on the way out — including when a workload fails.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const IMAGE: &str = "composer:2";
const PREFIX: &str = "stackvo-perf-bench";

/// How many files the fake `vendor/` holds.
///
/// A real Laravel install is twenty-five to thirty thousand. Eight is enough to
/// separate the two mounts by more than the noise and keeps a run to a minute
/// rather than ten — the shape of the answer is what is being measured, and the
/// shape does not change with the count.
const VENDOR_FILES: usize = 8_000;

/// Require every file in `vendor/`, which is what a framework boot is.
const BOOT_PHP: &str = r#"
$n = 0;
foreach (glob("/app/vendor/pkg*/src/*.php") as $file) { require_once $file; $n++; }
echo $n, " required\n";
"#;

const STAT_PHP: &str = r#"
$n = 0;
$it = new RecursiveIteratorIterator(
    new RecursiveDirectoryIterator("/app", FilesystemIterator::SKIP_DOTS)
);
foreach ($it as $f) { $n++; }
echo $n, " files\n";
"#;

/// The compiled-view and cache traffic a framework produces on every request.
const WRITE_PHP: &str = r#"
$dir = "/app/storage/framework/views";
@mkdir($dir, 0777, true);
for ($i = 0; $i < 2000; $i++) { file_put_contents("$dir/v$i.php", "<?php // $i"); }
clearstatcache();
for ($i = 0; $i < 2000; $i++) { file_get_contents("$dir/v$i.php"); }
for ($i = 0; $i < 2000; $i++) { unlink("$dir/v$i.php"); }
echo "2000 files\n";
"#;

/// One workload's timings, one column per mount arrangement.
struct Row {
    name: &'static str,
    bind: Vec<Duration>,
    layer: Vec<Duration>,
    both: Vec<Duration>,
}

fn main() {
    let runs = std::env::args()
        .position(|a| a == "--runs")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3);

    let scratch = std::env::temp_dir().join(format!("{PREFIX}-{}", std::process::id()));
    let source = scratch.join("app");
    println!(
        "building a {VENDOR_FILES}-file tree in {}",
        source.display()
    );
    if let Err(e) = build_tree(&source) {
        println!("could not build the tree: {e}");
        return;
    }

    // `layer` needs its vendor in a volume; seeding it is exactly what
    // `perf::seed` does, and it is done here the same way.
    let volume = format!("{PREFIX}-vendor");
    let storage_volume = format!("{PREFIX}-storage");
    if !seed_volume(&source.join("vendor"), &volume) {
        println!("could not seed the volume — is Docker running?");
        cleanup(&scratch, &[&volume, &storage_volume]);
        return;
    }

    println!("running {runs} round(s), alternating between the two mounts\n");

    let workloads = [("boot", BOOT_PHP), ("stat", STAT_PHP), ("write", WRITE_PHP)];
    let mut results: Vec<Row> = workloads
        .iter()
        .map(|(name, _)| Row {
            name,
            bind: vec![],
            layer: vec![],
            both: vec![],
        })
        .collect();

    // Round-robin rather than one mount at a time: anything that drifts across
    // the run — a background build, thermal throttling — would otherwise be
    // recorded as a property of whichever mount was being measured while it
    // happened.
    for round in 1..=runs {
        for (index, (name, php)) in workloads.iter().enumerate() {
            let bind = time(&source, &[], php);
            let layer = time(&source, &[(&volume, "/app/vendor")], php);
            let both = time(
                &source,
                &[
                    (&volume, "/app/vendor"),
                    (&storage_volume, "/app/storage/framework"),
                ],
                php,
            );
            println!(
                "  round {round}  {name:<6} bind {:>7.2}s   layer {:>7.2}s   both {:>7.2}s",
                bind.as_secs_f64(),
                layer.as_secs_f64(),
                both.as_secs_f64()
            );
            results[index].bind.push(bind);
            results[index].layer.push(layer);
            results[index].both.push(both);
        }
    }

    println!(
        "\n  {:<8} {:>10} {:>10} {:>10} {:>12} {:>11}",
        "", "bind", "layer", "both", "layer wins", "both wins"
    );
    for Row {
        name,
        bind,
        layer,
        both,
    } in &results
    {
        let b = median(bind).as_secs_f64();
        let l = median(layer).as_secs_f64();
        let t = median(both).as_secs_f64();
        let ratio = |v: f64| if v > 0.0 { b / v } else { 0.0 };
        println!(
            "  {name:<8} {b:>9.2}s {l:>9.2}s {t:>9.2}s {:>11.2}x {:>10.2}x",
            ratio(l),
            ratio(t)
        );
    }

    println!(
        "\nthe tree is {VENDOR_FILES} files in vendor/ and a handful of source files, which is \
         the ratio a real project has."
    );
    cleanup(&scratch, &[&volume, &storage_volume]);
}

/// A source tree shaped like a PHP project: a little of yours, a lot of theirs.
fn build_tree(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join("app"))?;
    std::fs::create_dir_all(root.join("storage/framework/views"))?;
    for n in 0..40 {
        std::fs::write(
            root.join("app").join(format!("Service{n}.php")),
            format!("<?php\nclass Service{n} {{ public function run() {{ return {n}; }} }}\n"),
        )?;
    }

    // 100 packages of 80 files, which is roughly how a vendor tree is shaped —
    // many directories, small files.
    let per_package = 80;
    let packages = VENDOR_FILES / per_package;
    for p in 0..packages {
        let dir = root.join("vendor").join(format!("pkg{p}")).join("src");
        std::fs::create_dir_all(&dir)?;
        for f in 0..per_package {
            std::fs::write(
                dir.join(format!("File{f}.php")),
                format!("<?php\nclass P{p}F{f} {{ const X = {f}; }}\n"),
            )?;
        }
    }
    Ok(())
}

fn seed_volume(from: &Path, volume: &str) -> bool {
    Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/from:ro", from.display()),
            "-v",
            &format!("{volume}:/to"),
            "alpine:3",
            "sh",
            "-c",
            "cp -a /from/. /to/",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// One workload, one mount arrangement.
fn time(source: &Path, volumes: &[(&str, &str)], php: &str) -> Duration {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/app", source.display()),
    ];
    for (volume, target) in volumes {
        args.push("-v".into());
        args.push(format!("{volume}:{target}"));
    }
    args.extend([
        "-w".into(),
        "/app".into(),
        IMAGE.into(),
        "php".into(),
        "-r".into(),
        php.into(),
    ]);

    let started = Instant::now();
    let _ = Command::new("docker").args(&args).output();
    started.elapsed()
}

fn median(values: &[Duration]) -> Duration {
    let mut sorted: Vec<Duration> = values.to_vec();
    sorted.sort();
    sorted.get(sorted.len() / 2).copied().unwrap_or_default()
}

fn cleanup(scratch: &PathBuf, volumes: &[&str]) {
    let _ = std::fs::remove_dir_all(scratch);
    for volume in volumes {
        let _ = Command::new("docker")
            .args(["volume", "rm", "-f", volume])
            .output();
    }
    println!("scratch and volumes removed.");
}
