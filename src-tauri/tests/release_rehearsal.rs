//! A rehearsal publishes nothing, and stays that way.
//!
//! §3 #22 — the two ARM rows — could not be verified because no tag had ever
//! been pushed, and no tag could be pushed because the signing preflight fails
//! without a secret that is somebody's decision to add. The `rehearsal` input
//! cut that knot: it builds all six targets, runs the suite on each, drops the
//! bundles on the run page, and publishes nothing.
//!
//! "Publishes nothing" is the whole value of it, and it is the part that erodes.
//! A rehearsal that quietly opened a draft release would be worse than no
//! rehearsal — it is *run from a branch*, so the release would be named after
//! whatever branch somebody was on. That already happened once: before the
//! input existed, a manual run passed `tagName: ${{ github.ref_name }}` and
//! drafted a release called after a feature branch.
//!
//! ## Why this is a text test and not a run
//!
//! GitHub Actions cannot be run here — that is the whole of what is left in
//! #22, and this file does not pretend otherwise. What it holds is the claim
//! that survives without a runner: **every step that could publish is gated.**
//! A new `gh release create` or a second upload action added without a gate
//! fails here, on the machine of whoever added it, rather than on the first
//! rehearsal after it.
//!
//! `updater_endpoint.rs` reads this file the same way and for the same reason.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn workflow() -> String {
    let path = repo_root().join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// One step: the lines from its `- name:`/`- uses:` to the next step's.
///
/// Coarse on purpose. The question is which `if:` governs which action, and a
/// block that ran a couple of lines long would only make this test *more*
/// willing to call a step gated — so the sloppiness is checked against, below,
/// by asserting the gated steps are the ones actually expected.
fn steps(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let starts_step = trimmed.starts_with("- ") && indent >= 6;
        if starts_step && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Actions and commands that put something where the public can reach it.
///
/// Named as needles rather than as a list of step names, because a step can be
/// renamed and the thing it runs cannot.
const PUBLISHES: [&str; 4] = [
    "actions/attest-build-provenance",
    "softprops/action-gh-release",
    "gh release create",
    "gh release upload",
];

/// Nothing that publishes runs in a rehearsal.
#[test]
fn every_publishing_step_is_gated_on_this_not_being_a_rehearsal() {
    let text = workflow();
    let mut ungated = Vec::new();
    let mut gated = 0usize;

    for step in steps(&text) {
        let publishes = PUBLISHES.iter().any(|needle| step.contains(needle));
        if !publishes {
            continue;
        }
        if step.contains("!inputs.rehearsal") {
            gated += 1;
        } else {
            let label = step
                .lines()
                .find(|l| l.contains("name:") || l.contains("uses:"))
                .unwrap_or("<unnamed step>")
                .trim()
                .to_string();
            ungated.push(label);
        }
    }

    assert!(
        gated > 0,
        "no publishing step is gated on `!inputs.rehearsal` — either the \
         needles in PUBLISHES no longer match anything in release.yml, in \
         which case this test is checking nothing, or the gating is gone"
    );
    assert!(
        ungated.is_empty(),
        "{} step(s) can publish during a rehearsal:\n{}\n\nA rehearsal runs \
         from a BRANCH. Anything it publishes is named after that branch, \
         which is how a draft release called after a feature branch got \
         created before the `rehearsal` input existed.",
        ungated.len(),
        ungated
            .iter()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `tauri-action` is gated by its arguments rather than by an `if:`.
///
/// It is the step that *builds*, so it has to run in a rehearsal — an empty
/// `tagName` is what makes it build without touching a release. The expression
/// is the fragile part and the comment above it in the workflow says why:
/// written the obvious way round, `${{ inputs.rehearsal && '' || github.ref_name }}`,
/// the empty string is falsy, `||` fires, and the rehearsal publishes under the
/// branch name — the exact bug the input was added to prevent, reintroduced by
/// a tidier-looking line.
#[test]
fn the_builder_is_told_not_to_publish_by_an_expression_with_no_falsy_trap() {
    let text = workflow();
    assert!(
        text.contains("tauri-apps/tauri-action"),
        "the builder step is gone, and this test is about how it is configured"
    );

    for field in ["tagName", "releaseName"] {
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{field}:")))
            .unwrap_or_else(|| panic!("release.yml no longer sets `{field}`"));

        assert!(
            line.contains("!inputs.rehearsal &&"),
            "`{field}` is `{}`. It must be written `!inputs.rehearsal && <value> || ''` \
             — the other order has a falsy left operand on the publishing path, \
             so `''` makes `||` fire and the rehearsal publishes under the \
             branch name.",
            line.trim()
        );
        assert!(
            line.trim_end().ends_with("|| '' }}"),
            "`{field}` does not fall through to an empty string, which is what \
             makes tauri-action build without touching a release",
        );
    }
}

/// A rehearsal is worth running only if it leaves the bundles behind.
///
/// The two ARM rows are the reason the input exists: a `.deb` and an `.msi`
/// that exist are the only proof that the runner label resolved and the
/// bundler ran there. Without the upload the run is green and produces nothing
/// to look at, which answers a different question than the one #22 asks.
#[test]
fn a_rehearsal_keeps_what_it_built() {
    let text = workflow();
    let keeps = steps(&text).into_iter().find(|step| {
        step.contains("actions/upload-artifact") && step.contains("${{ inputs.rehearsal }}")
    });
    assert!(
        keeps.is_some(),
        "no artifact upload runs in a rehearsal. Then a rehearsal proves the \
         six targets COMPILED and nothing about whether they bundled — and the \
         bundler is where an ARM runner actually differs."
    );
}

/// Six targets, and the two ARM rows on native ARM runners.
///
/// §7 says six. Cross-compiling the ARM rows on x86 runners would be a
/// different claim wearing the same number: the bundler runs native tools, and
/// "it cross-compiled" is not "it produced a package on that architecture".
#[test]
fn the_matrix_is_six_targets_with_the_arm_rows_on_arm_runners() {
    let text = workflow();
    let targets: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("target: "))
        .collect();

    assert_eq!(
        targets.len(),
        6,
        "release.yml builds {} target(s); docs/durum.md §3 #22 and §7 both say \
         six:\n{targets:#?}",
        targets.len()
    );

    for (target, runner) in [
        ("aarch64-unknown-linux-gnu", "ubuntu-24.04-arm"),
        ("aarch64-pc-windows-msvc", "windows-11-arm"),
    ] {
        assert!(
            targets.contains(&target),
            "`{target}` is no longer in the matrix — that row is half of what \
             #22 is about"
        );
        assert!(
            text.contains(runner),
            "`{target}` is built without `{runner}`. A cross-compiled ARM \
             bundle is a different claim: the bundler runs native tools, and \
             #22 asks whether a package can be produced on that architecture."
        );
    }
}
