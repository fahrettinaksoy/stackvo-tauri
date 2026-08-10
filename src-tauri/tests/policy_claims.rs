//! The policy layer says several things about itself. This checks them.
//!
//! Most of the claims below are the kind that go quietly wrong:
//!
//!   * the platform paths are written in four places — the module comment, the
//!     ADR, the contract and the code — and a change to one of them is not a
//!     compile error;
//!   * the rewrite is applied by *filename*, so a generated file with images in
//!     it that nobody added to `policy::rewrites` is silently left pointing at
//!     Docker Hub on a network that cannot reach it;
//!   * `Code::Forbidden` is a contract value, and the enum and the contract are
//!     two hand-written files that agree only by habit.
//!
//! The fourth is the one this layer would be worst for getting wrong: the
//! documentation must keep saying it is **not** a security boundary. That
//! sentence is the difference between a feature and a false guarantee, and it
//! is exactly the kind of caveat a later edit tidies away.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// `src-tauri/..` — the ADR and the contract live above this crate.
fn read_up(relative: &str) -> String {
    let path = repo_root().join("..").join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The paths, as the prose everywhere writes them.
///
/// Windows is the odd one because the code builds it from `%ProgramData%` with
/// `join`, so there is no literal to compare; the two components are checked
/// against the function separately below.
const PATHS: [&str; 3] = [
    "/Library/Managed Preferences/com.stackvo.desktop.json",
    "policy.json",
    "/etc/stackvo/policy.json",
];

/// `policy.rs`, minus its module comment.
///
/// The first version of the test below searched the whole file, and a mutation
/// that changed the Linux path *in the code* passed — because the same string
/// was still sitting in the documentation table twenty lines above the parser.
/// A gate that a deliberate break walks through is worse than no gate, so the
/// prose half and the code half are now read separately.
///
/// The test module goes too, for the same reason: `Policy::parse(text,
/// Path::new("/etc/stackvo/policy.json"))` is a fixture, and a gate satisfied
/// by its own test data is satisfied by nothing.
fn policy_code() -> String {
    let source = read("src/policy.rs");
    let start = source
        .find("use std::collections")
        .expect("the module comment ends where the imports begin");
    let end = source.find("#[cfg(test)]").unwrap_or(source.len());
    assert!(start < end, "the source is not in the shape this expects");
    source[start..end].to_string()
}

/// One file's path is a support answer. Four copies are four chances for the
/// answer to be wrong.
#[test]
fn the_module_the_adr_and_the_contract_name_the_same_paths() {
    let sources = [
        ("policy.rs", read("src/policy.rs")),
        ("durum.md §6 · 0009", read_up("docs/durum.md")),
        ("contracts/ipc.json", read_up("contracts/ipc.json")),
    ];

    for path in PATHS {
        for (name, text) in &sources {
            assert!(
                text.contains(path),
                "{name} does not name `{path}`; three copies of a filesystem \
                 path that disagree is three wrong answers to give a user"
            );
        }
    }

    for (name, text) in &sources {
        assert!(
            text.contains("STACKVO_POLICY_FILE"),
            "{name} does not name the override variable, which is the only way \
             anybody can test their own policy file"
        );
    }
}

/// And the code actually looks where the documentation says it does.
#[test]
fn the_code_reads_the_paths_the_documentation_advertises() {
    let code = policy_code();

    for path in [
        "/Library/Managed Preferences/com.stackvo.desktop.json",
        "/etc/stackvo/policy.json",
    ] {
        assert!(
            code.contains(path),
            "the documentation tells administrators to write `{path}` and \
             nothing in the parser looks there"
        );
    }

    // Assembled from components, so checked as components.
    for piece in ["ProgramData", "\"StackVo\"", "\"policy.json\""] {
        assert!(
            code.contains(piece),
            "the Windows path is built from `%ProgramData%\\StackVo\\policy.json` \
             and `{piece}` is not in the code that builds it"
        );
    }

    assert!(
        code.contains("OVERRIDE_VAR"),
        "`path()` must honour the override, or every test of a real policy file \
         has to be run as root"
    );
}

/// The caveat that must survive every future edit.
///
/// This layer reads a file the user can usually write and honours an
/// environment variable the user can set. Both are fine and both are stated. A
/// tidy-up that removes the statement leaves a feature that reads like a
/// guarantee, which is the one outcome worse than not shipping it.
#[test]
fn the_documentation_keeps_saying_it_is_not_a_security_boundary() {
    let places = [
        ("policy.rs", read("src/policy.rs")),
        ("durum.md §6 · 0009", read_up("docs/durum.md")),
        ("contracts/ipc.json", read_up("contracts/ipc.json")),
        (
            "PolicyNotice.vue",
            read_up("src/components/settings/PolicyNotice.vue"),
        ),
        ("en.js", read_up("src/i18n/locales/en.js")),
    ];

    for (name, text) in places {
        let lowered = text.to_lowercase();
        assert!(
            lowered.contains("not a security boundary") || lowered.contains("notasecurityboundary"),
            "{name} no longer says the policy layer is not a security boundary"
        );
    }
}

/// `{name}/Dockerfile` → `x/Dockerfile`.
///
/// The interpolated part is a project name and can be anything; the extension
/// is what `policy::rewrites` decides on, and that is what survives.
fn interpolations_removed(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut depth = 0usize;
    for c in label.chars() {
        match c {
            '{' => {
                if depth == 0 {
                    out.push('x');
                }
                depth += 1;
            }
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Every generated file that can carry an image reference is one the rewrite
/// is allowed near.
///
/// `policy::rewrites` narrows by filename on purpose — `image:` is a key a
/// service's own config could plausibly grow one day. The cost of narrowing is
/// this: a generated file added later, with images in it, silently keeps
/// pointing at Docker Hub. That failure has no symptom on a machine that can
/// reach Docker Hub, which is every machine the change would be written on.
#[test]
fn every_generated_file_with_images_in_it_is_rewritten() {
    let commands = read("src/commands.rs");

    // The labels `render_generated` pushes, read out of the source rather than
    // listed here — a list would be the same omission one level up.
    //
    // Both spellings. The per-project files are `format!("{name}/Dockerfile")`
    // and the shared ones are plain literals; a scanner that read only the
    // literals would skip every Dockerfile in the workspace and pass, which is
    // the failure this test exists to catch happening to the test itself.
    let labels: BTreeSet<String> = commands
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("label: ")?;
            let inner = rest
                .strip_prefix("format!(\"")
                .or_else(|| rest.strip_prefix('"'))?;
            let (literal, _) = inner.split_once('"')?;
            (!literal.is_empty()).then(|| interpolations_removed(literal))
        })
        .collect();

    assert!(
        labels.iter().any(|l| l.ends_with("/Dockerfile")),
        "the scanner did not find the per-project Dockerfile, so it is not \
         reading `render_generated` — it found: {labels:?}"
    );
    assert!(
        labels.len() >= 6,
        "the scanner found {} labels, which is too few to be reading \
         `render_generated` correctly",
        labels.len()
    );

    // Compose files and Dockerfiles. A `.yml` under `generated/` is a compose
    // file in every case this repo has; the two service configs that are not
    // (`elasticsearch.yml`, `traefik.yml`) are exactly why the narrowing
    // exists, so they are named rather than pattern-matched away.
    const NOT_COMPOSE: [&str; 3] = [
        "configs/elasticsearch.yml",
        "traefik/traefik.yml",
        "traefik/dynamic/routes.yml",
    ];

    for label in &labels {
        let carries_images = label.ends_with("Dockerfile")
            || (label.ends_with(".yml") && !NOT_COMPOSE.contains(&label.as_str()));
        if !carries_images {
            continue;
        }
        assert!(
            stackvo_desktop_lib::policy::rewrites(label),
            "`{label}` can carry an image reference and `policy::rewrites` says \
             no, so a private-registry install would leave it pointing at Docker \
             Hub — on a network that by definition cannot reach it"
        );
    }
}

/// A new `Code` is a new contract value whether or not anybody wrote it down.
///
/// `contract_agreement.rs` holds commands and events to this standard and the
/// error codes were never checked, which is how `FORBIDDEN` could have shipped
/// as a string only Rust knew. The front end switches on these.
#[test]
fn every_error_code_is_in_the_contract_and_the_reverse() {
    let error_rs = read("src/error.rs");
    let contract: serde_json::Value =
        serde_json::from_str(&read_up("contracts/ipc.json")).expect("the contract is JSON");

    // `Code::Forbidden => "FORBIDDEN",` — the match arm is the definition, and
    // reading it rather than the enum keeps a variant with no string out.
    let in_rust: BTreeSet<String> = error_rs
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Code::")?;
            let (_, after) = rest.split_once("=> \"")?;
            let (code, _) = after.split_once('"')?;
            Some(code.to_string())
        })
        .collect();

    let in_contract: BTreeSet<String> = contract["errors"]["codes"]
        .as_object()
        .expect("errors.codes is an object")
        .keys()
        .cloned()
        .collect();

    assert!(
        in_rust.len() >= 12,
        "the scanner read {} codes out of error.rs, which cannot be right",
        in_rust.len()
    );

    let undeclared: Vec<&String> = in_rust.difference(&in_contract).collect();
    assert!(
        undeclared.is_empty(),
        "these codes cross the IPC boundary and the contract does not mention \
         them, so nothing on the other side can be written against them: {undeclared:?}"
    );

    let unraised: Vec<&String> = in_contract.difference(&in_rust).collect();
    assert!(
        unraised.is_empty(),
        "the contract declares these and nothing raises them; a code the front \
         end handles and never sees is dead branch: {unraised:?}"
    );
}

/// The write path has to consult the policy, or `locked` means nothing.
///
/// This is one line in `env_writer::apply` and it is the whole enforcement.
/// Deleting it leaves a Settings pane that greys a field out, a `policy_status`
/// that reports it as locked, and a back end that writes it anyway.
#[test]
fn the_env_writer_refuses_locked_keys() {
    let writer = read("src/env_writer.rs");

    assert!(
        writer.contains("check_unlocked(patch, crate::policy::current())"),
        "env_writer::apply no longer consults the policy, so `locked` is a \
         label on a field and nothing more"
    );
    assert!(
        writer.contains("Code::Forbidden"),
        "the refusal must carry FORBIDDEN — PERMISSION_DENIED means the OS said \
         no and can be answered by elevating, which this never can"
    );
}
