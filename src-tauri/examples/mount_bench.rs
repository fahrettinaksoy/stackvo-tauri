//! What a bind mount actually costs, measured against a real Laravel suite.
//!
//!   cargo run --example mount_bench
//!   cargo run --example mount_bench -- --runs 5 --only bind,volume
//!   cargo run --example mount_bench -- --fresh          # drop the composer cache too
//!
//! I-1 in `docs/durum.md`, and the first thing that section asks for is not a
//! feature — it is this number. Bind-mounted source on macOS and Windows is the
//! single most common reason people leave a Docker-based workflow, DDEV ships
//! Mutagen turned on to answer it, and StackVo currently mounts a project with
//! no flag at all (`generator.rs`, `{projects_root}/{name}:/var/www/html`).
//! Whether the answer here is a mount flag or a Mutagen-class subsystem is a
//! decision that costs weeks in one direction and an afternoon in the other,
//! and until this program ran, nothing in the repository could tell them apart.
//!
//! ## What is compared
//!
//! Four ways of putting the same tree in front of the same PHP:
//!
//! - `bind`      — what StackVo writes today, no consistency flag
//! - `cached`    — `:cached`, the host-authoritative relaxation
//! - `delegated` — `:delegated`, the container-authoritative relaxation
//! - `volume`    — a named volume: no host filesystem in the path at all
//!
//! The fourth is not a shipping option — a project the editor cannot open is
//! not a project — it is the **ceiling**. A synchronising layer (Mutagen,
//! docker-sync) works by making the container read a container-native
//! filesystem and reconciling it with the host out of band, so what it can win,
//! at best, is the distance between `bind` and `volume`. If that distance is
//! small there is nothing for a sync layer to buy and the item closes; if it is
//! large, the size of it is the budget for building one. That is the whole
//! argument, and it needs the fourth column to be made.
//!
//! ## What is run
//!
//! A real `laravel/laravel`, created fresh into each mount, then four workloads:
//!
//! - `install` — `composer create-project`: some thirty thousand files written
//! - `stat`    — every file under the tree stat'ed, which is the shape of an
//!   autoloader miss and of every `php artisan` boot
//! - `write`   — two thousand small files created, read back and removed
//! - `test`    — `php artisan test`, the framework's own suite
//!
//! The composer cache is a named volume shared by every mode, so `install`
//! times writing files rather than downloading them — the network is not one of
//! the four things being compared and would swamp all of them.
//!
//! `noop` is reported too: a container that starts and exits. It is the same
//! constant in every row, and printing it means the reader can subtract it
//! instead of trusting that it was subtracted. It has a second job — see
//! [`rounds`] — which is to say when the table should not be read at all.
//!
//! The repeatable workloads run **round-robin across the modes**, not one mode
//! at a time. Measuring `bind` to completion and then `cached` puts each mode in
//! its own slice of time, so anything that drifts across the run is recorded as
//! a property of whichever mount happened to be measured while it happened.
//!
//! ## Why it is a program and not a test
//!
//! It measures the machine it runs on. The number is different on Intel and
//! Apple silicon, different again under VirtioFS and gRPC-FUSE, and different
//! on Windows — so there is no value a suite could assert. What the suite can
//! hold, once a decision follows from this, is that the decision reached the
//! renderer. This only produces the table the decision is made from.
//!
//! ## What it leaves behind
//!
//! Scratch directories under the OS temp directory and one named volume per
//! mode, all removed on the way out. The composer cache volume is kept, because
//! rebuilding it is a download and the second run of this program should not
//! pay for it; `--fresh` removes that too.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// PHP with composer and the extensions Laravel asks for, in one image.
const IMAGE: &str = "composer:2";

/// Shared by every mode on purpose — see the header.
const CACHE_VOLUME: &str = "stackvo-mount-bench-composer";

/// Everything this program creates is named from here, so a stray container or
/// volume from an interrupted run is identifiable and removable by hand.
const PREFIX: &str = "stackvo-mount-bench";

/// Stat every file in the tree. This is what an autoloader miss looks like, and
/// what `php artisan` does before it prints anything.
const STAT_PHP: &str = r#"
$n = 0; $bytes = 0;
$it = new RecursiveIteratorIterator(
    new RecursiveDirectoryIterator("/app", FilesystemIterator::SKIP_DOTS)
);
foreach ($it as $f) { $n++; $bytes += $f->isFile() ? $f->getSize() : 0; }
echo $n, " files\n";
"#;

/// Create, read back and remove two thousand small files. Writes are the half
/// of the problem `:delegated` claims to address, so they are timed apart from
/// the reads that `:cached` claims.
const WRITE_PHP: &str = r#"
$dir = "/app/.mount-bench";
@mkdir($dir, 0777, true);
for ($i = 0; $i < 2000; $i++) { file_put_contents("$dir/f$i", str_repeat("x", 512)); }
clearstatcache();
for ($i = 0; $i < 2000; $i++) { file_get_contents("$dir/f$i"); }
for ($i = 0; $i < 2000; $i++) { unlink("$dir/f$i"); }
rmdir($dir);
echo "2000 files\n";
"#;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Bind,
    Cached,
    Delegated,
    Volume,
}

impl Mode {
    const ALL: [Mode; 4] = [Mode::Bind, Mode::Cached, Mode::Delegated, Mode::Volume];

    fn name(self) -> &'static str {
        match self {
            Mode::Bind => "bind",
            Mode::Cached => "cached",
            Mode::Delegated => "delegated",
            Mode::Volume => "volume",
        }
    }

    /// Does the host filesystem appear anywhere in the path?
    fn is_bind(self) -> bool {
        !matches!(self, Mode::Volume)
    }

    /// The `-v` argument, which is the only thing that differs between rows.
    fn mount(self, scratch: &Path) -> String {
        let host = scratch.join(self.name());
        match self {
            Mode::Bind => format!("{}:/app", host.display()),
            Mode::Cached => format!("{}:/app:cached", host.display()),
            Mode::Delegated => format!("{}:/app:delegated", host.display()),
            Mode::Volume => format!("{PREFIX}-volume:/app"),
        }
    }
}

/// One measured workload. `runs` timings are kept rather than a mean, because
/// the spread is the thing that says whether the difference is real.
struct Phase {
    label: &'static str,
    seconds: Vec<f64>,
}

impl Phase {
    fn median(&self) -> f64 {
        let mut sorted = self.seconds.clone();
        sorted.sort_by(f64::total_cmp);
        if sorted.is_empty() {
            return f64::NAN;
        }
        sorted[sorted.len() / 2]
    }
}

struct Row {
    mode: Mode,
    phases: Vec<Phase>,
    /// The tree really was created — a mode that installed nothing would report
    /// a very fast `stat` for the honest reason that there was nothing to stat.
    files: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let has = |name: &str| args.iter().any(|a| a == name);

    if has("--help") || has("-h") {
        println!(
            "usage: cargo run --example mount_bench -- \
             [--runs N] [--only bind,cached,delegated,volume] [--fresh] [--keep]"
        );
        return;
    }

    let runs: usize = flag("--runs")
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(3);
    let only: Option<Vec<String>> = flag("--only").map(|s| {
        s.split(',')
            .map(|p| p.trim().to_ascii_lowercase())
            .filter(|p| !p.is_empty())
            .collect()
    });
    let modes: Vec<Mode> = Mode::ALL
        .into_iter()
        .filter(|m| {
            only.as_ref()
                .is_none_or(|list| list.iter().any(|w| w == m.name()))
        })
        .collect();

    match run(&modes, runs, has("--fresh"), has("--keep")) {
        Ok(()) => {}
        Err(message) => {
            eprintln!("\n{message}");
            std::process::exit(1);
        }
    }
}

fn run(modes: &[Mode], runs: usize, fresh: bool, keep: bool) -> Result<(), String> {
    if modes.is_empty() {
        return Err("--only matched no mode".into());
    }

    let server = docker_output(&[
        "version",
        "--format",
        "{{.Server.Version}} ({{.Server.Arch}})",
    ])
    .map_err(|_| "Docker is not answering; this program needs a running engine".to_string())?;
    let host = docker_output(&["info", "--format", "{{.OperatingSystem}}"]).unwrap_or_default();
    println!("engine     {server} on {host}");
    println!("image      {IMAGE}");
    println!("runs       {runs} per phase, median reported\n");

    if fresh {
        let _ = docker_quiet(&["volume", "rm", "-f", CACHE_VOLUME]);
    }
    docker_quiet(&["volume", "create", CACHE_VOLUME])
        .map_err(|e| format!("creating the composer cache volume: {e}"))?;

    let scratch = std::env::temp_dir().join(PREFIX);
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;

    let mut rows: Vec<Row> = Vec::new();
    for &mode in modes {
        match install(mode, &scratch) {
            Ok(row) => rows.push(row),
            Err(e) => {
                eprintln!("  {} — {e}", mode.name());
                cleanup(modes, &scratch, keep);
                return Err(format!("{} did not complete", mode.name()));
            }
        }
    }

    if let Err(e) = rounds(&mut rows, &scratch, runs) {
        cleanup(modes, &scratch, keep);
        return Err(e);
    }

    report(&rows);
    cleanup(modes, &scratch, keep);
    Ok(())
}

/// One mode, from an empty mount to a tree with a Laravel in it.
///
/// Timed, and the one workload that cannot be repeated: a second
/// `create-project` into a populated tree is a different operation, and
/// emptying the tree between runs on a bind mount would put the host's `rm -rf`
/// inside the mount's measurement.
fn install(mode: Mode, scratch: &Path) -> Result<Row, String> {
    use std::io::Write as _;

    // Prepare the mount. A bind wants an empty directory that exists; a volume
    // wants to not exist yet, because a leftover from an interrupted run would
    // arrive already populated and `install` would measure nothing.
    if mode.is_bind() {
        let host = scratch.join(mode.name());
        std::fs::create_dir_all(&host).map_err(|e| e.to_string())?;
    } else {
        let _ = docker_quiet(&["volume", "rm", "-f", &format!("{PREFIX}-volume")]);
        docker_quiet(&["volume", "create", &format!("{PREFIX}-volume")])
            .map_err(|e| format!("creating the volume: {e}"))?;
    }

    print!("  {:<10} install ", mode.name());
    let _ = std::io::stdout().flush();

    let started = Instant::now();
    docker_run(
        mode,
        scratch,
        &[
            "composer",
            "create-project",
            "--prefer-dist",
            "--no-interaction",
            "--no-progress",
            "--quiet",
            "laravel/laravel",
            ".",
        ],
    )
    .map_err(|e| format!("install: {e}"))?;
    let elapsed = started.elapsed().as_secs_f64();
    println!("{elapsed:>8.2}s");

    let files = docker_run(mode, scratch, &["php", "-r", STAT_PHP])?
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    if files < 1000 {
        return Err(format!(
            "only {files} files under the mount after install — the tree is not there, \
             so the timings would be measuring an empty directory"
        ));
    }

    Ok(Row {
        mode,
        phases: vec![
            Phase {
                label: "noop",
                seconds: Vec::new(),
            },
            Phase {
                label: "install",
                seconds: vec![elapsed],
            },
            Phase {
                label: "stat",
                seconds: Vec::new(),
            },
            Phase {
                label: "write",
                seconds: Vec::new(),
            },
            Phase {
                label: "test",
                seconds: Vec::new(),
            },
        ],
        files,
    })
}

/// The repeatable workloads, round-robin across the modes.
///
/// The order is the correction. Measuring one mode to completion and then the
/// next puts every mode in a different slice of time, so anything that drifts
/// over the run — a laptop warming up, another process arriving, Docker
/// Desktop's own memory pressure — is indistinguishable from a property of the
/// mount. That is not hypothetical: three attempts at this table in a row were
/// thrown out, and the giveaway in the last of them was a `noop` that climbed
/// monotonically in exactly the order the modes were measured in.
///
/// Interleaved, drift still exists but it lands on every mode roughly equally,
/// which is all a comparison needs. `install` is not in here, because it cannot
/// be repeated — so it is the one number that keeps the old weakness, and the
/// only one that should be read with that in mind.
fn rounds(rows: &mut [Row], scratch: &Path, runs: usize) -> Result<(), String> {
    use std::io::Write as _;

    for round in 0..runs {
        print!("  round {:<2}   ", round + 1);
        let _ = std::io::stdout().flush();

        for row in rows.iter_mut() {
            let mode = row.mode;
            for phase in row.phases.iter_mut() {
                let command: Vec<&str> = match phase.label {
                    "noop" => vec!["php", "-r", ";"],
                    "stat" => vec!["php", "-r", STAT_PHP],
                    "write" => vec!["php", "-r", WRITE_PHP],
                    "test" => vec!["php", "artisan", "test"],
                    _ => continue, // install, already done and not repeatable
                };
                let started = Instant::now();
                docker_run(mode, scratch, &command)
                    .map_err(|e| format!("{} {}: {e}", mode.name(), phase.label))?;
                phase.seconds.push(started.elapsed().as_secs_f64());
            }
            print!(" {}", mode.name());
            let _ = std::io::stdout().flush();
        }
        println!();
    }
    println!();
    Ok(())
}

fn docker_run(mode: Mode, scratch: &Path, command: &[&str]) -> Result<String, String> {
    let mount = mode.mount(scratch);
    let mut args: Vec<&str> = vec![
        "run",
        "--rm",
        "-v",
        &mount,
        "-v",
        // Named, so no mode pays for the download.
        concat!("stackvo-mount-bench-composer", ":/tmp/composer-cache"),
        "-e",
        "COMPOSER_CACHE_DIR=/tmp/composer-cache",
        "-e",
        "COMPOSER_ALLOW_SUPERUSER=1",
        "-w",
        "/app",
        IMAGE,
    ];
    args.extend_from_slice(command);
    docker_output(&args)
}

fn report(rows: &[Row]) {
    println!("{:-<78}", "");
    print!("  {:<10}", "mode");
    if let Some(first) = rows.first() {
        for phase in &first.phases {
            print!("{:>12}", phase.label);
        }
    }
    println!("{:>10}", "files");
    println!("{:-<78}", "");

    for row in rows {
        print!("  {:<10}", row.mode.name());
        for phase in &row.phases {
            print!("{:>11.2}s", phase.median());
        }
        println!("{:>10}", row.files);
    }
    println!("{:-<78}", "");

    // Was the machine quiet? `noop` is the same container doing the same
    // nothing in every row, so any spread across it is the machine and not the
    // mount. This is not a refinement — the first nine-run attempt at this
    // table was taken while a compiler was running, and it reported `:cached`
    // three times slower at writes than a plain bind, which is not a thing a
    // no-op flag can do. The tell was right there in a `noop` that had tripled,
    // and a reader who trusted the ratios would have concluded the opposite of
    // what a quiet machine says. So the program says it rather than printing a
    // number that reads as a result.
    let noops: Vec<f64> = rows
        .iter()
        .filter_map(|r| r.phases.iter().find(|p| p.label == "noop"))
        .map(Phase::median)
        .collect();
    if let (Some(low), Some(high)) = (
        noops.iter().copied().reduce(f64::min),
        noops.iter().copied().reduce(f64::max),
    ) {
        if high / low > 1.5 {
            println!(
                "  !! `noop` ranges {low:.2}s to {high:.2}s across modes. That is the same\n     \
                 container doing the same nothing each time, so the machine was busy with\n     \
                 something else and the rows below are not comparable. Run it again on an\n     \
                 idle machine before reading anything into them.\n"
            );
        }
    }

    // The comparison the section is actually asking for. Everything is read
    // against `volume`, because that is the ceiling a sync layer aims at, and a
    // ratio near 1.00 means there is nothing there to win.
    let Some(base) = rows.iter().find(|r| r.mode == Mode::Volume) else {
        println!("\n  no `volume` row, so there is no ceiling to read the others against");
        return;
    };
    println!("\n  against `volume` (1.00 = nothing a sync layer could win)\n");
    print!("  {:<10}", "mode");
    for phase in &base.phases {
        print!("{:>12}", phase.label);
    }
    println!();
    for row in rows {
        print!("  {:<10}", row.mode.name());
        for (phase, baseline) in row.phases.iter().zip(&base.phases) {
            let ratio = phase.median() / baseline.median();
            print!("{ratio:>11.2}x");
        }
        println!();
    }
    println!();
}

fn cleanup(modes: &[Mode], scratch: &Path, keep: bool) {
    if keep {
        println!(
            "  --keep: left {} and the volumes in place",
            scratch.display()
        );
        return;
    }
    if modes.iter().any(|m| !m.is_bind()) {
        let _ = docker_quiet(&["volume", "rm", "-f", &format!("{PREFIX}-volume")]);
    }
    // A bind mount created files as root inside the container, so the host user
    // may not be able to remove them. Ask the container to do it, then take the
    // now-empty directories.
    for mode in modes.iter().filter(|m| m.is_bind()) {
        let host = scratch.join(mode.name());
        if host.exists() {
            let _ = docker_quiet(&[
                "run",
                "--rm",
                "-v",
                &format!("{}:/app", host.display()),
                IMAGE,
                "sh",
                "-c",
                "rm -rf /app/* /app/.[!.]* 2>/dev/null || true",
            ]);
        }
    }
    let _ = std::fs::remove_dir_all(scratch);
}

fn docker_quiet(args: &[&str]) -> Result<(), String> {
    docker_output(args).map(|_| ())
}

fn docker_output(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .output()
        .map_err(|e| format!("running docker: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr)
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("docker failed")
            .trim()
            .to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
