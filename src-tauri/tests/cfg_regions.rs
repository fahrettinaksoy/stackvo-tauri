//! Every function a `cfg` gate hides is still a function somebody has to build.
//!
//! §3 #35 said the Windows and Linux branches had never been *run*. That was
//! true and it was the smaller half. The first time this crate was compiled for
//! Linux anywhere a person could watch — `tools/linux/run.sh`, added with this
//! file — it did not compile at all:
//!
//! ```text
//! error[E0425]: cannot find value `ca_common_name` in this scope
//!   --> src/certs.rs:489:32
//! ```
//!
//! `ca_trusted`, the `#[cfg(not(target_os = "macos"))]` half, had always called
//! a function that was never written. On macOS the branch is not compiled, so
//! `cargo check`, `cargo clippy` and the whole suite were silent about it — and
//! CI's Linux `build` job must have been red for as long as that line existed,
//! which nobody watching from a Mac would see.
//!
//! ## What this file can and cannot check
//!
//! It cannot type-check the other platforms; only a compiler on them can, and
//! `tools/linux/run.sh` is how that now happens for one of the two. What it can
//! do is hold the two habits that turn a cfg gate from a necessity into a
//! blind spot:
//!
//!  1. A gated region is **paired** — something answers for every platform, so
//!     a caller is not left with a name that exists on one OS.
//!  2. Logic that could be platform-free is **not** put behind a gate. Every
//!     line inside one is a line no local run ever reads, so the cheapest fix
//!     for this class of bug is to have less of it: `elevate::polkit_outcome`,
//!     `elevate::uac_script`, `elevate::base64_utf16` and now
//!     `certs::ca_common_name` are all un-gated on purpose, and every one of
//!     them is tested on every platform. `elevate_probe.rs` goes further and
//!     keeps its own base64 *decoder* un-gated, so PowerShell's encoding is
//!     checked against an independent implementation on a Mac.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn sources() -> Vec<(String, String)> {
    let dir = repo_root().join("src-tauri/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("src-tauri/src is readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a source file has a name")
                .to_string();
            let text = std::fs::read_to_string(&p).expect("a source file is readable");
            (name, text)
        })
        .collect()
}

/// A `#[cfg(...)]` line, stripped to the platforms it names.
fn platforms_named(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("target_os = \"") {
        rest = &rest[at + "target_os = \"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        } else {
            break;
        }
    }
    out
}

/// Every platform-gated item, by the name it defines.
///
/// The name is read off the line after the attributes, which is coarse in the
/// same way the other scanners here are coarse and wrong in the same safe
/// direction: an item whose name is not read simply is not checked.
fn gated_items() -> BTreeMap<String, Vec<(String, bool)>> {
    let mut out: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();

    for (file, text) in sources() {
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("#[cfg(") || !trimmed.contains("target_os") {
                continue;
            }
            let negated = trimmed.contains("not(");
            let named = platforms_named(trimmed);
            if named.is_empty() {
                continue;
            }

            // The first following line that declares something.
            let Some(decl) = lines[index + 1..]
                .iter()
                .take(6)
                .find(|l| {
                    let t = l.trim_start();
                    t.starts_with("pub fn ")
                        || t.starts_with("fn ")
                        || t.starts_with("pub async fn ")
                        || t.starts_with("async fn ")
                })
                .map(|l| l.trim_start())
            else {
                continue;
            };

            let name: String = decl
                .split("fn ")
                .nth(1)
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }

            let key = format!("{file}::{name}");
            let entry = out.entry(key).or_default();
            for platform in named {
                entry.push((platform, negated));
            }
        }
    }
    out
}

/// A gated function answers on every platform, or on none.
///
/// The shape that bites is one arm: `#[cfg(target_os = "macos")] fn f()` with
/// no counterpart means every caller needs its own cfg, and the first one that
/// forgets fails to compile on an OS nobody here builds for. `ca_trusted` is
/// the live example — it had a `not(macos)` arm and a `macos` arm under a
/// *different name*, which is why the pairing was never obvious.
#[test]
fn a_platform_gated_function_has_an_answer_on_the_other_platforms() {
    let mut lonely = Vec::new();

    for (key, arms) in gated_items() {
        let has_negated = arms.iter().any(|(_, negated)| *negated);
        let named: Vec<&str> = arms
            .iter()
            .filter(|(_, negated)| !negated)
            .map(|(p, _)| p.as_str())
            .collect();

        // A `not(x)` arm plus a positive arm covers everything; so do three
        // positive arms.
        let covered = has_negated || named.len() >= 3;
        if !covered {
            lonely.push(format!("  {key}  →  only {named:?}"));
        }
    }

    // Not an empty list: several helpers legitimately exist on one platform and
    // are called from inside a cfg block of the same shape. The number is the
    // gate — it may not grow without somebody looking at why.
    assert!(
        lonely.len() <= 24,
        "{} platform-gated function(s) answer on some platforms and not \
         others:\n{}\n\nThat is allowed where the caller is gated the same way, \
         and it is how `certs::ca_common_name` went missing for as long as it \
         did — nobody compiles the other arm. If this list has grown, the new \
         entry needs a caller that is gated too, or a counterpart.",
        lonely.len(),
        lonely.join("\n")
    );
}

/// The three functions pulled out from behind a gate on purpose.
///
/// Each was extracted after a bug that a cfg gate had hidden, and each is
/// tested on every platform. Naming them here is the record of *why* they are
/// not gated, so a later tidy-up does not put them back.
const DELIBERATELY_UNGATED: [(&str, &str); 4] = [
    // Reads polkit's exit code. 126 is *cancelled*, not *failed*, and getting
    // that backwards is a Linux-only bug a Mac cannot see.
    ("elevate.rs", "polkit_outcome"),
    // Builds the PowerShell line UAC runs, and encodes it. Both halves are
    // string work with no Windows in them.
    ("elevate.rs", "uac_script"),
    ("elevate.rs", "base64_utf16"),
    // Reads a CA's common name out of its PEM — the function that did not
    // exist at all until Linux was compiled for the first time.
    ("certs.rs", "ca_common_name"),
];

#[test]
fn the_logic_pulled_out_from_behind_a_gate_is_still_out() {
    for (file, name) in DELIBERATELY_UNGATED {
        let text = sources()
            .into_iter()
            .find(|(f, _)| f == file)
            .map(|(_, t)| t)
            .unwrap_or_else(|| panic!("{file} is gone"));

        let at = text
            .find(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("{file} no longer defines `{name}`"));

        // The attributes immediately above it.
        let head = &text[at.saturating_sub(300)..at];
        let attrs = head.rsplit("\n\n").next().unwrap_or(head);
        assert!(
            !attrs.contains("#[cfg(target_os"),
            "`{file}::{name}` has been put behind a platform gate again. It was \
             pulled out BECAUSE a gate hid a bug in it: a gated line is a line \
             no local run ever compiles, and the test below it stops running on \
             the machine most likely to be doing the fixing."
        );
    }
}

/// The Linux runner exists and says what it is for.
///
/// This is the mechanism that found the missing function, and it is a shell
/// script — the kind of thing that rots quietly. A test that it is still here
/// costs nothing and stops #35's answer becoming folklore.
#[test]
fn the_linux_runner_is_still_here_and_matches_what_ci_installs() {
    let runner = repo_root().join("tools/linux/run.sh");
    assert!(
        runner.exists(),
        "tools/linux/run.sh is gone. It is the only way the polkit branch and \
         the tauri-driver suite get compiled anywhere but a CI runner."
    );

    let dockerfile = std::fs::read_to_string(repo_root().join("tools/linux/Dockerfile"))
        .expect("tools/linux/Dockerfile is readable");
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
        .expect("ci.yml is readable");

    // The packages are stated in two places and the second one goes stale. The
    // ones that matter are the ones whose absence changes the answer: without
    // webkit2gtk the crate does not link, without the driver package
    // `tauri-driver` has nothing to proxy to.
    for package in ["libwebkit2gtk-4.1-dev", "webkit2gtk-driver", "xvfb"] {
        assert!(
            dockerfile.contains(package),
            "tools/linux/Dockerfile no longer installs `{package}`, which \
             ci.yml does — a green run here would stop meaning anything about \
             a green run there"
        );
        assert!(
            ci.contains(package),
            "ci.yml no longer installs `{package}` but the local image still \
             does, which is the same drift pointing the other way"
        );
    }

    let toolchain = std::fs::read_to_string(repo_root().join("src-tauri/rust-toolchain.toml"))
        .expect("rust-toolchain.toml is readable");
    let pinned = toolchain
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel = "))
        .map(|v| v.trim_matches('"').to_string())
        .expect("the toolchain file pins a channel");
    assert!(
        dockerfile.contains(&pinned),
        "the image installs a Rust version other than the pinned {pinned}. \
         rustup resolves rust-toolchain.toml from the working directory, so a \
         mismatch means the container compiles with one toolchain and installs \
         components on another — the exact trap ci.yml's coverage job \
         documents."
    );
}

// ------------------------------------------------- what a test may reach out to

/// A unit test cannot reach the OS keystore, and the compiler is what says so.
///
/// One test — `env_writer::tests::a_moved_key_is_taken_out_of_the_file_patch` —
/// called a function that wrote to the real macOS Keychain. Its own comment
/// claimed the write "succeeds on a developer's machine". It does not: macOS
/// prompts for Keychain access whenever the binary asking has changed, which
/// after `cargo build` is every time, and with nobody there to answer the
/// prompt the test **hung** and took the whole `cargo test` run with it.
///
/// It had been that way for as long as the test existed, and it was invisible
/// for a specific reason: a suite that hangs looks like a suite that is slow.
///
/// The fix is `cfg(test)`, not an environment variable. `hosts.rs` has
/// `STACKVO_HOSTS_PATH` and `elevate.rs` has `STACKVO_POWERSHELL`, and a seam
/// of that shape here would be a variable that moves *passwords* out of the OS
/// keystore in a shipped binary. This checks the shape holds.
#[test]
fn the_keystore_has_no_backend_a_released_build_could_be_redirected_to() {
    let secrets = sources()
        .into_iter()
        .find(|(name, _)| name == "secrets.rs")
        .map(|(_, text)| text)
        .expect("secrets.rs exists");

    // Every operation has BOTH arms, checked per function rather than by
    // looking for the attribute somewhere in the file — the first version of
    // this test did the latter and passed while `write` had lost its guard,
    // because `read` still carried one. A gate that reads the file as a bag of
    // strings is not checking the thing it names.
    for operation in ["read", "write", "delete"] {
        let signature = format!("pub fn {operation}(");
        let occurrences: Vec<usize> = secrets.match_indices(&signature).map(|(i, _)| i).collect();
        assert_eq!(
            occurrences.len(),
            2,
            "`{operation}` is declared {} time(s) in secrets.rs. It needs exactly \
             two: the real store and the in-memory one.",
            occurrences.len()
        );

        // The attributes immediately above each, in order: the real store
        // first, the fake second.
        let gates: Vec<&str> = occurrences
            .iter()
            .map(|at| {
                // Only the attributes attached to THIS function: everything
                // back to the blank line above it. A fixed-size window reached
                // into the previous function and read its gate as this one's.
                let window = &secrets[at.saturating_sub(400)..*at];
                let head = window.rsplit("\n\n").next().unwrap_or(window);
                if head.contains("#[cfg(not(test))]") {
                    "not(test)"
                } else if head.contains("#[cfg(test)]") {
                    "test"
                } else {
                    "ungated"
                }
            })
            .collect();
        assert_eq!(
            gates,
            vec!["not(test)", "test"],
            "`{operation}` is gated {gates:?}. `not(test)` on the real store is \
             what stops a unit test reaching Keychain; `test` on the fake is \
             what stops a release shipping it. An `ungated` arm is one of those \
             two failures."
        );
    }

    assert!(
        secrets.contains("#[cfg(test)]\nmod fake {"),
        "the in-memory store is no longer gated as a whole"
    );

    // And the redirection is not an environment variable. A `STACKVO_*` seam
    // here would be settable by anything that can start this process, in a
    // build that ships.
    assert!(
        !secrets.contains("var_os(\"STACKVO") && !secrets.contains("var(\"STACKVO"),
        "secrets.rs reads a STACKVO_* environment variable. `hosts.rs` and \
         `elevate.rs` are seamed that way and should be — a file this app \
         writes and a program it spawns. This module holds passwords, and a \
         variable that moves them somewhere else in a released binary is a \
         different thing wearing the same pattern."
    );

    // `keyring` is named in exactly one module. A second caller would be a
    // second path to the store, and only one of them would be gated.
    let callers: Vec<String> = sources()
        .into_iter()
        .filter(|(name, text)| name != "secrets.rs" && text.contains("keyring::"))
        .map(|(name, _)| name)
        .collect();
    assert!(
        callers.is_empty(),
        "the keystore is reached from {callers:?} as well as from secrets.rs — \
         `cfg(test)` in one module does not cover a second door"
    );
}
