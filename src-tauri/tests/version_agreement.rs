//! One release, one number.
//!
//! The version is written in three files that nothing joins up:
//!
//!   * `src-tauri/Cargo.toml` — what the binary reports, and what the crash
//!     report in `crash.rs` stamps on itself;
//!   * `src-tauri/tauri.conf.json` — what the bundle carries, and therefore what
//!     the updater compares against `latest.json` to decide there is one;
//!   * `package.json` — what the release workflow and the changelog read.
//!
//! They agree today. Nothing was keeping them that way, and the failure is
//! quiet in the worst direction: bump `Cargo.toml` alone and the app announces a
//! version whose bundle still claims the old one, so every installed copy either
//! updates in a loop or never notices the release at all. Neither shows up in a
//! build, a lint or any other test — the three files are simply read by three
//! different tools that never meet.
//!
//! Six lines of intent, in the file that already gets run.

/// Where the number is written, and how to dig it out.
///
/// `Cargo.toml` is not parsed — this crate has no TOML reader and adding one for
/// a single field would be the larger change. `CARGO_PKG_VERSION` is the same
/// value, resolved by Cargo itself at compile time, which is strictly better
/// evidence than re-parsing the file it came from.
fn cargo() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn json_version(path: &std::path::Path) -> String {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()));

    value
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{} has no string `version`", path.display()))
        .to_string()
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("a repository root above src-tauri")
        .to_path_buf()
}

#[test]
fn the_three_declared_versions_are_the_same() {
    let root = repo_root();
    let cargo = cargo();
    let bundle = json_version(&root.join("src-tauri/tauri.conf.json"));
    let package = json_version(&root.join("package.json"));

    assert_eq!(
        cargo, bundle,
        "Cargo.toml says {cargo} and tauri.conf.json says {bundle}. The updater \
         compares the bundle's number against latest.json, so a release built \
         from this tree would be invisible to installed copies — or offered to \
         them for ever."
    );
    assert_eq!(
        cargo, package,
        "Cargo.toml says {cargo} and package.json says {package}. The release \
         workflow reads package.json to tag and name the artefacts."
    );
}

/// The updater does a semantic-version comparison, so a number it cannot parse
/// is a number it will never treat as newer. Cheap to assert, and it catches the
/// `0.2` and `v0.2.0` a hand edit produces.
#[test]
fn the_version_is_a_plain_semantic_version() {
    let version = cargo();
    let parts: Vec<&str> = version.split('.').collect();

    assert_eq!(
        parts.len(),
        3,
        "{version} is not major.minor.patch; the updater cannot order it"
    );
    for part in parts {
        // The pre-release suffix on the patch component (`0.2.0-beta.1`) is
        // legitimate, so only the leading digits have to be there.
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        assert!(
            !digits.is_empty(),
            "{version} has a component that does not start with a digit"
        );
    }
}
