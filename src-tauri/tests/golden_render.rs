//! The generator's output, frozen, with nothing on the machine required.
//!
//! ## What this replaces
//!
//! `real_checkout.rs` carried a byte-for-byte comparison against files the
//! retired Bash generator had written into a checkout on disk. It was the only
//! coverage the service configs and `docker-compose.dynamic.yml` had, and it
//! was worth exactly as much as the checkout it found — which, on a machine
//! where nobody had cloned StackVo, was nothing: `checkout()` returned `None`
//! and all seventeen tests in that file returned `ok` without asserting
//! anything. A guard that silently stops guarding is worse than one that was
//! never written, because the count still says seventeen.
//!
//! ## What it is, honestly
//!
//! These files were produced by this renderer and reviewed once. They are not
//! an independent oracle — the Bash generator that was one is gone, and the
//! fixtures it did leave behind (`tests/fixtures/probe-*`, `traefik/`,
//! `docker-compose.projects.yml`) still anchor the Dockerfiles, the routers and
//! the projects compose file in `fixtures_differential.rs`. What is frozen here
//! is the *rest*: twenty-five assembled service blocks, the awk filter that
//! trims them, the harvested volumes section and six config renders. Their
//! value is that a change to any of it shows up as a reviewable diff instead of
//! reaching a running container.
//!
//! ## Regenerating
//!
//! ```sh
//! STACKVO_GOLDEN_UPDATE=1 cargo test --test golden_render
//! ```
//!
//! Then read `git diff`. That diff is the whole point: it is the change to
//! every container the app produces, stated in full, before it ships.

use stackvo_desktop_lib::{config::Env, skeleton, template};
use std::path::{Path, PathBuf};

/// A workspace that does not exist.
///
/// Every template resolves workspace-first and falls back to the copy compiled
/// into the binary, so a root with nothing at it renders purely from the
/// embedded skeleton — which is what a packaged app does on a fresh machine,
/// and what makes this test independent of anything on disk.
const NO_WORKSPACE: &str = "/stackvo-golden-render-no-such-workspace";

/// The five configs `render_generated` writes into `generated/configs/`, as
/// (template, output) pairs.
const CONFIGS: [(&str, &str); 6] = [
    ("services/redis/redis.conf.tpl", "redis.conf"),
    ("services/mysql/my.cnf.tpl", "mysql.cnf"),
    ("services/mongo/mongo.conf.tpl", "mongo.conf"),
    ("services/postgres/postgres.conf.tpl", "postgres.conf"),
    (
        "services/elasticsearch/elasticsearch.yml.tpl",
        "elasticsearch.yml",
    ),
    ("services/valkey/valkey.conf.tpl", "valkey.conf"),
];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden")
}

fn updating() -> bool {
    std::env::var_os("STACKVO_GOLDEN_UPDATE").is_some()
}

/// Compare against the frozen file, or rewrite it when regenerating.
fn check(name: &str, rendered: &str) {
    let path = golden_dir().join(name);

    if updating() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("creating the golden directory");
        }
        std::fs::write(&path, rendered).expect("writing the golden file");
        eprintln!("updated {name} ({} bytes)", rendered.len());
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{name} has no golden file ({e}). Create it with STACKVO_GOLDEN_UPDATE=1 and read the diff."
        )
    });

    if rendered == expected {
        return;
    }

    // A whole-file dump buries the one line that matters, and these files run
    // to hundreds of lines.
    panic!(
        "{name} differs from the golden file{}\n\n\
         If the change is intended: STACKVO_GOLDEN_UPDATE=1 cargo test --test golden_render",
        first_difference(rendered, &expected)
    );
}

fn first_difference(ours: &str, theirs: &str) -> String {
    let a: Vec<&str> = ours.lines().collect();
    let b: Vec<&str> = theirs.lines().collect();

    for i in 0..a.len().max(b.len()) {
        let (ours, theirs) = (a.get(i), b.get(i));
        if ours != theirs {
            return format!(
                " at line {}:\n  rendered: {:?}\n  golden:   {:?}",
                i + 1,
                ours.unwrap_or(&"<end of file>"),
                theirs.unwrap_or(&"<end of file>")
            );
        }
    }

    format!(
        " — same lines, different bytes ({} vs {})",
        ours.len(),
        theirs.len()
    )
}

/// The frozen settings, merged over the embedded defaults exactly as a real
/// workspace merges its `.env`.
fn variables() -> std::collections::BTreeMap<String, String> {
    let text = std::fs::read_to_string(golden_dir().join("overrides.env")).expect("overrides.env");
    let env = Env::parse(&text);

    // The path passed here is what `variables` would fall back to for
    // STACKVO_ROOT and HOST_STACKVO_ROOT. The fixture pins both, so this value
    // must not survive into the output — `the_render_carries_nothing_from_this_machine`
    // is what proves it does not.
    template::variables(&env, Path::new("/this-path-must-not-appear"))
}

#[test]
fn the_service_configs_render_as_frozen() {
    let vars = variables();

    for (tpl, out) in CONFIGS {
        let text =
            skeleton::read_template(Path::new(NO_WORKSPACE), &format!("core/templates/{tpl}"))
                .unwrap_or_else(|| panic!("{tpl} is not compiled into the binary"));

        check(&format!("configs/{out}"), &template::render(&text, &vars));
    }
}

#[test]
fn the_assembled_services_file_renders_as_frozen() {
    let vars = variables();
    let rendered = template::render_dynamic_compose(Path::new(NO_WORKSPACE), &vars);
    check("docker-compose.dynamic.yml", &rendered);
}

/// The frozen files must not carry anything true only of the machine that
/// produced them.
///
/// This is the failure mode a golden test invites: a uid, a home directory or a
/// checkout path bakes itself into the fixture, and every other machine — and
/// CI — fails on a difference that is not a regression. The renderer fills all
/// four of those from the process when nothing else has, so pinning them in the
/// fixture is only half the job; this is the half that checks.
#[test]
fn the_render_carries_nothing_from_this_machine() {
    let vars = variables();

    assert_eq!(
        vars.get("HOST_STACKVO_ROOT").map(String::as_str),
        Some("/stackvo")
    );
    assert_eq!(vars.get("HOST_UID").map(String::as_str), Some("1000"));
    assert_eq!(vars.get("HOST_GID").map(String::as_str), Some("1000"));

    let rendered = template::render_dynamic_compose(Path::new(NO_WORKSPACE), &vars);
    assert!(
        !rendered.contains("/this-path-must-not-appear"),
        "the fallback root reached the output — the fixture is not pinning it"
    );

    if let Some(home) = dirs::home_dir().and_then(|h| h.to_str().map(str::to_string)) {
        assert!(
            !rendered.contains(&home),
            "this machine's home directory is in the render"
        );
    }
}

/// The render describes the whole catalogue, not a subset.
///
/// `render_dynamic_compose` warns and continues when a template will not
/// resolve, which is right for one missing file and wrong as a silent floor: a
/// service that stopped resolving would simply leave the output, be regenerated
/// out of the golden file on the next update, and read as an intended change.
///
/// Asserted against the render rather than the frozen file on purpose. Reading
/// the file would make this test depend on another test having written it —
/// they run in parallel, and it failed exactly that way the first time.
/// `the_assembled_services_file_renders_as_frozen` ties the two together.
#[test]
fn every_dynamic_service_is_in_the_render() {
    let text = template::render_dynamic_compose(Path::new(NO_WORKSPACE), &variables());

    for (flag, path) in template::DYNAMIC_SERVICES {
        // `services/mysql/docker-compose.mysql.tpl` → `mysql`
        let service = path
            .split('/')
            .nth(1)
            .expect("a template path names its service");
        assert!(
            text.contains(&format!("stackvo-{service}")),
            "{flag} ({service}) is enabled in the fixture but absent from the render"
        );
    }
}
