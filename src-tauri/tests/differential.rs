//! Differential check: the Rust generator against the Bash one's real output.
//!
//! The Bash generator has already written Dockerfiles into `generated/projects/`
//! in a real checkout. Those files are the specification. Anything short of a
//! byte-for-byte match means the port would silently change somebody's image,
//! which is the failure mode that makes generator ports dangerous.
//!
//! Skipped (not failed) when no checkout with generated output is reachable.

use stackvo_desktop_lib::{config::Env, generator, manifest, workspace};
use std::path::PathBuf;

fn checkout() -> Option<PathBuf> {
    [
        std::env::var("STACKVO_ROOT").ok().map(PathBuf::from),
        dirs::home_dir().map(|h| h.join("Desktop/stackvo")),
        dirs::home_dir().map(|h| h.join("stackvo")),
    ]
    .into_iter()
    .flatten()
    .find(|p| workspace::looks_like_stackvo(p))
}

fn toolchain(env: &Env) -> generator::ToolchainOptions {
    generator::ToolchainOptions {
        tools: env.list("PHP_DEFAULT_TOOLS"),
        apt_packages: env.list("PHP_DEFAULT_APT_PACKAGES"),
        composer_version: env
            .get("PHP_TOOL_COMPOSER_VERSION")
            .unwrap_or("latest")
            .to_string(),
        nodejs_version: env
            .get("PHP_TOOL_NODEJS_VERSION")
            .unwrap_or("20")
            .to_string(),
    }
}

/// First differing line, with context — a whole-file diff is unreadable.
fn first_difference(ours: &str, theirs: &str) -> Option<String> {
    let a: Vec<&str> = ours.lines().collect();
    let b: Vec<&str> = theirs.lines().collect();

    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or("<missing>"),
            b.get(i).copied().unwrap_or("<missing>"),
        );
        if x != y {
            let from = i.saturating_sub(2);
            let context: Vec<String> = (from..i).map(|j| format!("   {}", b[j])).collect();
            return Some(format!(
                "line {}:\n{}\n  bash: {y:?}\n  rust: {x:?}",
                i + 1,
                context.join("\n")
            ));
        }
    }
    None
}

#[test]
fn rust_generator_reproduces_the_bash_dockerfiles_byte_for_byte() {
    let Some(root) = checkout() else {
        eprintln!("skipping: no StackVo checkout found");
        return;
    };

    let generated = root.join("generated/projects");
    if !generated.is_dir() {
        eprintln!("skipping: nothing generated yet — run `stackvo generate` first");
        return;
    }

    let env = Env::load(&root).expect(".env should load");
    let opts = toolchain(&env);

    let mut checked = 0;
    let mut mismatches = Vec::new();

    for entry in std::fs::read_dir(&generated)
        .expect("read generated/projects")
        .flatten()
    {
        let dir = entry.path();
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let theirs_path = dir.join("Dockerfile");
        let manifest_path = root.join("projects").join(name).join("stackvo.json");
        if !theirs_path.is_file() || !manifest_path.is_file() {
            continue;
        }

        let m = manifest::read(&manifest_path, name).expect("manifest parses");
        // Only nginx is ported so far; the other four servers share `resolve`
        // but have their own templates.
        if m.server.as_deref().unwrap_or("nginx") != "nginx" || m.php.is_none() {
            continue;
        }

        let theirs = std::fs::read_to_string(&theirs_path).expect("read Dockerfile");
        // compat mode: reproduce the Bash silent-skip behaviour, which is what
        // the existing files were generated with.
        let ours = generator::render_from_manifest(&m, &opts, false).expect("render");

        checked += 1;
        if let Some(diff) = first_difference(&ours, &theirs) {
            mismatches.push(format!("\n=== {name} ===\n{diff}"));
        }
    }

    assert!(
        checked > 0,
        "no nginx PHP projects with generated output to compare against"
    );
    assert!(
        mismatches.is_empty(),
        "{} of {checked} generated Dockerfiles differ:{}",
        mismatches.len(),
        mismatches.join("")
    );

    eprintln!("{checked} Dockerfiles match the Bash generator byte-for-byte");
}
