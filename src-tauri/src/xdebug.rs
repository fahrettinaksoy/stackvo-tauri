//! Step debugging, as one switch.
//!
//! `xdebug` was already in `contracts/php-extensions.json` with per-version
//! pecl pins, so it could be compiled in by hand-editing `stackvo.json`. It
//! then still would not connect to anything, because installing the extension
//! is only the first of three things that have to be true:
//!
//! 1. **Compiled in.** `php.extensions` in the manifest, and a rebuild, because
//!    the generator writes `pecl install` into the Dockerfile.
//! 2. **Configured.** Xdebug 3 does nothing without `xdebug.mode=debug`, and it
//!    cannot reach an IDE without knowing which host to dial back to.
//! 3. **Live in the container that is actually running.**
//!
//! The three are reported separately because they come apart in practice, and a
//! single "on" that quietly means one of them would send people to their IDE
//! settings to look for a fault that is not there.
//!
//! ## Why an overlay file
//!
//! Steps 2 and 3 need an `environment:` block on the project's compose service,
//! and the PHP compose generator emits none — only `node.sh` has one. Adding it
//! is not available: the generator's output is under a byte-for-byte contract
//! with the Bash implementation (`generate_with` in `rust` mode refuses to write
//! when the two disagree), so a change on one side alone breaks the very check
//! that makes the port safe.
//!
//! Compose already solves this. The CLI layers three generated files with `-f`;
//! this writes a fourth that the generator neither produces nor knows about:
//!
//! ```yaml
//! services:
//!   shop:
//!     environment:
//!       XDEBUG_MODE: debug
//!       XDEBUG_CONFIG: "client_host=host.docker.internal client_port=9003 ..."
//! ```
//!
//! The overlay is a pure function of the manifests and is re-rendered on every
//! compose invocation rather than kept as state. That is not tidiness: an
//! overlay naming a project that has since been deleted defines a service with
//! no image and no build context, and compose then refuses **every** command,
//! including the `down` that would clear it. Deriving it fresh each time means
//! it cannot be stale, at the cost of reading a few small JSON files.
//!
//! ## The one thing this cannot fix
//!
//! `stackvo up` from the Bash CLI layers three files, not four. It will happily
//! recreate a container without the Xdebug environment, and debugging stops
//! working with nothing on screen to say why. `active` exists to make that
//! visible: it is read from the running container, never inferred.

use crate::error::{Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The catalog name; also what `docker-php-ext-enable` is given.
pub const EXTENSION: &str = "xdebug";

/// Xdebug 3's own default. Xdebug 2 used 9000, which collides with PHP-FPM —
/// the change is the single most common reason a listener never fires.
pub const PORT: u16 = 9003;

/// `debug` alone, not `debug,develop`: every extra mode is overhead on every
/// request, and `develop` changes `var_dump` output, which is a surprise nobody
/// asked for by clicking a debugger switch.
pub const MODE: &str = "debug";

/// Sent as `idekey`, and what a PhpStorm/VS Code listener matches on.
pub const IDE_KEY: &str = "STACKVO";

/// Where the project is mounted, from `generate_common_volumes`:
/// `${host_project_path}:/var/www/html`.
pub const CONTAINER_PATH: &str = "/var/www/html";

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.xdebug.yml")
}

// ------------------------------------------------------------- pure logic

/// Is the extension in this manifest's list?
pub fn listed(extensions: &[String]) -> bool {
    extensions.iter().any(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// The list with the extension added or removed, order otherwise untouched.
///
/// Appended rather than inserted in sorted position: the generator installs
/// pecl extensions in list order, and moving an existing entry to satisfy an
/// alphabet would reorder somebody's build for no reason.
pub fn with_extension(extensions: &[String], enabled: bool) -> Vec<String> {
    let mut out: Vec<String> = extensions
        .iter()
        .filter(|e| !e.eq_ignore_ascii_case(EXTENSION))
        .cloned()
        .collect();

    if enabled {
        out.push(EXTENSION.to_string());
    }
    out
}

/// What Xdebug is being used for.
///
/// Two values, not a set, and that is a finding rather than a simplification.
/// The modes want *opposite* start triggers: stepping wants
/// `start_with_request=default` so a breakpoint fires on the next page load,
/// while profiling wants `trigger` so every request does not write a
/// multi-megabyte cachegrind file. Offering `debug,profile` would have to pick
/// one of those, and either choice silently breaks the other half.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The default: a toggle that has never been touched is a step debugger,
    /// which is what the Xdebug switch has always meant.
    #[default]
    Debug,
    Profile,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Debug => "debug",
            Mode::Profile => "profile",
        }
    }
}

/// The per-project mode, stored beside the other `.stackvo/` settings.
///
/// A file rather than a manifest key: the schema is `additionalProperties:
/// false`, and this belongs with `php.ini` and `devserver.json` as something a
/// teammate's clone can carry. On/off stays where it was — `xdebug` in
/// `php.extensions` — because that is what actually compiles the extension in.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ModeConfig {
    #[serde(default)]
    pub mode: Mode,
}

pub const MODE_FILE: &str = "xdebug.json";

pub fn mode_path(root: &Path, name: &str) -> PathBuf {
    crate::workspace::projects_root(root)
        .unwrap_or_default()
        .join(name)
        .join(crate::phpini::CONFIG_DIR)
        .join(MODE_FILE)
}

pub fn read_mode(root: &Path, name: &str) -> Mode {
    std::fs::read_to_string(mode_path(root, name))
        .ok()
        .and_then(|text| serde_json::from_str::<ModeConfig>(&text).ok())
        .map(|c| c.mode)
        .unwrap_or_default()
}

/// Where the generated ini is mounted inside the container.
///
/// `zzz-` rather than `zz-`: PHP parses `conf.d` alphabetically and last wins,
/// and `zz-stackvo.ini` is the *user's* `php.ini` from [`crate::phpini`]. A
/// name sorting before it would let a hand-written `xdebug.output_dir` send
/// profiles somewhere this app cannot read them, which presents as profiling
/// that produced nothing.
pub const INI_CONTAINER_PATH: &str = "/usr/local/etc/php/conf.d/zzz-stackvo-xdebug.ini";

pub fn ini_dir(root: &Path) -> PathBuf {
    root.join("generated").join("xdebug")
}

/// The ini the overlay mounts for a profiling project.
///
/// Measured, not assumed: `XDEBUG_CONFIG` *does* carry `output_dir` and
/// `start_with_request` — both were confirmed to take effect from the
/// environment — but it silently ignores `use_compression`, and Xdebug 3.4
/// compresses by default. One gzipped file is the difference between a profile
/// view and a parse error, so the settings go in an ini where all four land.
pub fn profile_ini() -> String {
    format!(
        "; Generated by StackVo Desktop — do not edit.\n\
         ;\n\
         ; Re-rendered before every compose command. Turn profiling off in the\n\
         ; app to remove it.\n\
         xdebug.output_dir={}\n\
         ; Xdebug 3.4 gzips by default, and XDEBUG_CONFIG cannot turn that off.\n\
         xdebug.use_compression=0\n\
         ; Otherwise every single request writes a multi-megabyte file.\n\
         xdebug.start_with_request=trigger\n",
        crate::profile::CONTAINER_DIR
    )
}

/// One project's worth of overlay input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The compose service name, which the generator sets to the project name.
    pub service: String,
    /// Used as `PHP_IDE_CONFIG=serverName=…`, which is how PhpStorm picks the
    /// right path mapping without the user selecting it on every session.
    pub server_name: Option<String>,
    pub mode: Mode,
    /// Host path of the generated ini, when profiling.
    pub ini_path: Option<String>,
}

/// `XDEBUG_CONFIG`, as Xdebug parses it: space-separated `key=value`.
pub fn xdebug_config() -> String {
    format!("client_host=host.docker.internal client_port={PORT} idekey={IDE_KEY} log_level=0")
}

/// Render the overlay, or None when no project wants it.
///
/// None rather than an empty document: compose rejects a file whose `services`
/// map is empty, so "nothing to add" has to mean "no file", not "an empty one".
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from projects/*/stackvo.json before every compose command,\n\
         # so edits here are lost. Turn Xdebug off in the app to remove it.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and will\n\
         # recreate these containers without the settings below.\n\
         services:\n",
    );

    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in sorted {
        out.push_str(&format!("  {}:\n", entry.service));
        out.push_str("    environment:\n");
        // The env var, not the ini: `XDEBUG_MODE` takes precedence over
        // `xdebug.mode`, so setting both and letting them disagree would be a
        // bug waiting for somebody to hand-edit one of them.
        out.push_str(&format!("      XDEBUG_MODE: \"{}\"\n", entry.mode.as_str()));
        out.push_str(&format!("      XDEBUG_CONFIG: \"{}\"\n", xdebug_config()));
        if let Some(name) = &entry.server_name {
            out.push_str(&format!("      PHP_IDE_CONFIG: \"serverName={name}\"\n"));
        }
        // Docker Desktop resolves host.docker.internal on its own; on Linux
        // nothing does, and without this the container dials a name that does
        // not exist. Declaring it where it already resolves is harmless.
        out.push_str("    extra_hosts:\n");
        out.push_str("      - \"host.docker.internal:host-gateway\"\n");

        // Profiling needs three settings the environment cannot carry — see
        // `profile_ini`. Read-only: the container has no business rewriting the
        // configuration this app generated for it.
        if let Some(ini) = &entry.ini_path {
            out.push_str("    volumes:\n");
            out.push_str(&format!("      - \"{ini}:{INI_CONTAINER_PATH}:ro\"\n"));
        }
    }

    Some(out)
}

/// The service names declared in the generated projects compose file.
///
/// The overlay may only name services that already exist, and a manifest on
/// disk is not proof that one does. In a real checkout 21 directories under
/// `projects/` produced 10 compose services — the rest have no `stackvo.json`,
/// or have one the generator rejected. Naming any of them declares a service
/// with neither an image nor a build context, at which point compose refuses
/// every command against the whole stack, not just that project.
///
/// Parsed by indentation rather than with a YAML crate: this file is generated
/// by a Bash `cat <<EOF`, its shape is fixed, and the alternative is a
/// dependency plus a schema for a document we already know the layout of. The
/// section tracking matters — `networks:` has two-space keys too, and
/// `stackvo-net` is not a service.
pub fn generated_services(compose_yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_services = false;

    for raw in compose_yaml.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        // A top-level key ends the services block and starts something else.
        if !line.starts_with(' ') {
            in_services = line.starts_with("services:");
            continue;
        }
        if !in_services {
            continue;
        }

        // Exactly two spaces of indent is a service name; deeper is its body.
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        if rest.starts_with(' ') || rest.starts_with('-') {
            continue;
        }
        if let Some(name) = rest.strip_suffix(':') {
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }

    out
}

/// Does this container's environment carry the settings the overlay defines?
///
/// Both are required: `XDEBUG_MODE` without `XDEBUG_CONFIG` produces a debugger
/// that is on and dials localhost — inside the container, which is itself.
pub fn env_is_active(env: &[String]) -> bool {
    let has = |key: &str| {
        env.iter()
            .any(|line| line.starts_with(&format!("{key}=")) && !line.ends_with('='))
    };
    has("XDEBUG_MODE") && has("XDEBUG_CONFIG")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XdebugStatus {
    /// False for node projects; there is nothing to switch on.
    pub supported: bool,
    /// What the manifest asks for.
    pub enabled: bool,
    /// What the running container carries. None when nothing is running.
    pub active: Option<bool>,
    pub running: bool,
    /// The extension is compiled in, so the manifest can be ahead of the image.
    pub needs_rebuild: bool,
    pub port: u16,
    pub mode: String,
    pub ide_key: String,
    pub server_name: Option<String>,
    /// The host half of the IDE's path mapping.
    pub host_path: Option<String>,
    pub container_path: String,
    pub php_version: Option<String>,
    /// Which Xdebug the catalog pins for this PHP version.
    pub pecl_version: Option<String>,
    pub overlay_path: String,
}

/// Has the generator caught up, and has the build?
///
/// Three states that look identical from the manifest and need different
/// fixes, which is why this is not one boolean:
///
/// * the manifest asks for Xdebug but the Dockerfile has no `pecl install`
///   line yet — regenerate;
/// * the Dockerfile has it but the container predates that Dockerfile — the
///   image was never rebuilt, so the extension is not in it;
/// * both are current — nothing to do.
///
/// Taken as a pure function of two timestamps so it can be tested without a
/// daemon. `container_created` is None when nothing is running, in which case
/// the Dockerfile is all there is to go on.
pub fn needs_rebuild(
    enabled: bool,
    dockerfile_has_extension: bool,
    dockerfile_mtime: Option<i64>,
    container_created: Option<i64>,
) -> bool {
    if !enabled {
        return false;
    }
    if !dockerfile_has_extension {
        return true;
    }

    match (dockerfile_mtime, container_created) {
        // A container built before the Dockerfile that installs the extension
        // cannot contain it, however confidently the environment is set.
        (Some(dockerfile), Some(container)) => container < dockerfile,
        _ => false,
    }
}

/// Docker's RFC 3339 timestamps, as Unix seconds.
fn epoch(rfc3339: &str) -> Option<i64> {
    time::OffsetDateTime::parse(rfc3339, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
}

fn mtime(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

// ------------------------------------------------------------------- I/O

fn projects_dir(root: &Path) -> PathBuf {
    crate::workspace::projects_root(root).unwrap_or_default()
}

fn manifest_path(root: &Path, name: &str) -> PathBuf {
    projects_dir(root).join(name).join("stackvo.json")
}

/// The Dockerfile compose actually builds from.
///
/// Not `projects/<name>/Dockerfile`. The build context in the generated compose
/// file is `./projects/<name>` *relative to that file*, which sits in
/// `generated/` — so the image is built from `generated/projects/<name>`. Some
/// project directories do hold a hand-written `Dockerfile` of the user's own,
/// with different contents and an older date; reading that one answers a
/// question nobody asked, and reading it when it is absent reports "never
/// built" forever.
fn dockerfile_path(root: &Path, name: &str) -> PathBuf {
    root.join("generated")
        .join("projects")
        .join(name)
        .join("Dockerfile")
}

/// Every PHP project that asks for Xdebug **and** exists as a compose service.
///
/// The second half is not belt-and-braces. A manifest on disk is not proof the
/// generator emitted a service for it, and an overlay naming one it did not
/// breaks every compose command against the entire stack.
fn entries(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();

    let generated =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = generated_services(&generated);

    let Ok(dirs) = std::fs::read_dir(projects_dir(root)) else {
        return out;
    };

    for dir in dirs.flatten() {
        let path = dir.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Not generated yet, or the generator rejected it. Either way there is
        // nothing for the overlay to merge onto.
        if !services.iter().any(|s| s == name) {
            continue;
        }
        let file = path.join("stackvo.json");
        if !file.is_file() {
            continue;
        }
        let Ok(manifest) = crate::manifest::read(&file, name) else {
            continue;
        };
        // Node projects have no PHP to debug, and the compose service the
        // overlay would name is generated from a different template.
        if manifest.runtime != "php" {
            continue;
        }
        let Some(php) = &manifest.php else { continue };
        if !listed(&php.extensions) {
            continue;
        }

        let mode = read_mode(root, name);

        // Written here rather than by the toggle, for the reason the overlay
        // itself is derived: an ini left behind by a project that has since
        // stopped profiling would keep sending its output somewhere, and the
        // symptom is profiles appearing that nobody asked for.
        let ini_path = if mode == Mode::Profile {
            let path = ini_dir(root).join(format!("{name}.ini"));
            match std::fs::create_dir_all(ini_dir(root))
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    crate::atomic::write(&path, &profile_ini()).map_err(|e| e.to_string())
                }) {
                Ok(()) => Some(path.display().to_string()),
                Err(e) => {
                    tracing::warn!(project = name, error = %e, "could not write the Xdebug profile ini");
                    None
                }
            }
        } else {
            let _ = std::fs::remove_file(ini_dir(root).join(format!("{name}.ini")));
            None
        };

        out.push(Entry {
            service: name.to_string(),
            server_name: manifest.domain.clone(),
            mode,
            ini_path,
        });
    }

    out
}

/// Re-render the overlay from the manifests, and report whether it now exists.
///
/// Called before every compose invocation rather than only when the toggle
/// changes. An overlay naming a project that has since been deleted declares a
/// service with neither an image nor a build context, and compose then refuses
/// **every** command — including the `down` that would have cleared it. Derived
/// state cannot go stale; stored state can, and the failure is total.
pub fn sync(root: &Path) -> bool {
    let path = overlay_path(root);

    match overlay_yaml(&entries(root)) {
        Some(yaml) => {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
            // A write failure must not take compose down with it: the honest
            // degradation is "Xdebug is not applied", which `active` then
            // reports, rather than "no container can be started".
            match crate::atomic::write(&path, &yaml) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not write the Xdebug overlay");
                    let _ = std::fs::remove_file(&path);
                    false
                }
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

/// What is true for one project, across all three layers.
pub async fn status(root: &Path, name: &str) -> Result<XdebugStatus> {
    let file = manifest_path(root, name);
    if !file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&file, name)?;

    let supported = manifest.runtime == "php";
    let php = manifest.php.as_ref();
    let enabled = supported && php.is_some_and(|p| listed(&p.extensions));

    let dockerfile = dockerfile_path(root, name);
    let dockerfile_text = std::fs::read_to_string(&dockerfile).unwrap_or_default();
    // The generator emits `pecl install xdebug-<version>`, so the extension
    // name alone is enough and survives a version bump.
    let in_dockerfile = dockerfile_text.contains(&format!("pecl install {EXTENSION}"));

    // Inspecting is best-effort: with the engine down there is no container to
    // describe, and that is not an error worth failing the whole query over.
    let details = crate::engine::inspect(name).await.ok();
    let running = details.as_ref().is_some_and(|d| d.running);
    let active = details.as_ref().map(|d| env_is_active(&d.env));

    let host_path = details.as_ref().and_then(|d| {
        d.mounts
            .iter()
            .find(|m| m.destination == CONTAINER_PATH)
            .and_then(|m| m.source.clone())
    });

    Ok(XdebugStatus {
        supported,
        enabled,
        active,
        running,
        needs_rebuild: needs_rebuild(
            enabled,
            in_dockerfile,
            mtime(&dockerfile),
            details
                .as_ref()
                .and_then(|d| d.created.as_deref())
                .and_then(epoch),
        ),
        port: PORT,
        mode: MODE.to_string(),
        ide_key: IDE_KEY.to_string(),
        server_name: manifest.domain.clone(),
        // Falls back to the path on disk so the IDE mapping can be shown before
        // anything has ever been built.
        host_path: host_path.or_else(|| Some(projects_dir(root).join(name).display().to_string())),
        container_path: CONTAINER_PATH.to_string(),
        php_version: php.map(|p| p.version.clone()),
        pecl_version: php.and_then(|p| pinned_version(&p.version)),
        overlay_path: overlay_path(root).display().to_string(),
    })
}

/// Which Xdebug the catalog pins for a PHP version.
fn pinned_version(php_version: &str) -> Option<String> {
    let matrix = &crate::contracts::php_extensions().extensions;
    let spec = matrix.get(EXTENSION)?;
    spec.pecl_versions
        .get(php_version)
        .or_else(|| spec.pecl_versions.get("default"))
        .cloned()
}

/// Turn it on or off: the manifest, then the overlay.
pub async fn set(root: &Path, name: &str, enabled: bool) -> Result<XdebugStatus> {
    let file = manifest_path(root, name);
    if !file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }

    let mut manifest = crate::manifest::read(&file, name)?;

    if manifest.runtime != "php" {
        return Err(Error::new(
            crate::error::Code::Unsupported,
            format!(
                "{name} is a {} project; Xdebug is PHP-only",
                manifest.runtime
            ),
        ));
    }

    let php = manifest.php.as_mut().ok_or_else(|| {
        Error::new(
            crate::error::Code::InvalidManifest,
            format!("{name} declares runtime php but has no php block"),
        )
    })?;

    // Only write when something actually changes: every manifest write wakes
    // the file watcher, which flags the project as needing a regenerate.
    // Toggling to the state it is already in would raise that flag for nothing.
    if listed(&php.extensions) != enabled {
        php.extensions = with_extension(&php.extensions, enabled);
        crate::manifest::write(&file, &manifest)?;
    }

    sync(root);
    status(root, name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn listing_is_case_insensitive() {
        assert!(listed(&s(&["gd", "Xdebug"])));
        assert!(!listed(&s(&["gd", "xdebug_client"])));
        assert!(!listed(&[]));
    }

    #[test]
    fn enabling_appends_and_disabling_removes() {
        let base = s(&["pdo", "pdo_mysql", "gd"]);

        let on = with_extension(&base, true);
        assert_eq!(on, s(&["pdo", "pdo_mysql", "gd", "xdebug"]));

        let off = with_extension(&on, false);
        assert_eq!(off, base, "turning it off restores the original list");
    }

    /// The generator installs pecl extensions in list order. Re-sorting to
    /// insert one alphabetically would reorder somebody else's build.
    #[test]
    fn the_existing_order_is_preserved() {
        let base = s(&["zip", "gd", "pdo"]);
        assert_eq!(
            with_extension(&base, true),
            s(&["zip", "gd", "pdo", "xdebug"])
        );
    }

    #[test]
    fn enabling_twice_does_not_duplicate() {
        let once = with_extension(&s(&["gd"]), true);
        let twice = with_extension(&once, true);
        assert_eq!(once, twice);
        assert_eq!(twice.iter().filter(|e| *e == "xdebug").count(), 1);
    }

    /// Compose rejects a file whose `services` map is empty, so the last
    /// project turning Xdebug off has to remove the file, not empty it.
    #[test]
    fn no_projects_means_no_file_at_all() {
        assert!(overlay_yaml(&[]).is_none());
    }

    #[test]
    fn the_overlay_carries_mode_config_and_a_host_gateway() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            server_name: Some("shop.loc".into()),
            mode: Mode::Debug,
            ini_path: None,
        }])
        .expect("one project renders a file");

        assert!(yaml.contains("  shop:\n"));
        assert!(yaml.contains("XDEBUG_MODE: \"debug\""));
        assert!(yaml.contains("client_port=9003"));
        assert!(yaml.contains("client_host=host.docker.internal"));
        assert!(yaml.contains("PHP_IDE_CONFIG: \"serverName=shop.loc\""));
        // Without this the container dials a name that does not resolve on
        // Linux, and the failure looks like a firewall problem.
        assert!(yaml.contains("host.docker.internal:host-gateway"));
    }

    /// Profiling needs an ini mount; stepping does not. Measured against a real
    /// container: `XDEBUG_CONFIG` carries `output_dir` and `start_with_request`
    /// but silently ignores `use_compression`, and Xdebug 3.4 gzips by default
    /// — one compressed file is the difference between a profile view and a
    /// parse error.
    #[test]
    fn only_the_profile_mode_mounts_an_ini() {
        let stepping = overlay_yaml(&[Entry {
            service: "shop".into(),
            server_name: None,
            mode: Mode::Debug,
            ini_path: None,
        }])
        .unwrap();
        assert!(stepping.contains("XDEBUG_MODE: \"debug\""));
        assert!(!stepping.contains("volumes:"), "{stepping}");

        let profiling = overlay_yaml(&[Entry {
            service: "shop".into(),
            server_name: None,
            mode: Mode::Profile,
            ini_path: Some("/w/generated/xdebug/shop.ini".into()),
        }])
        .unwrap();
        assert!(profiling.contains("XDEBUG_MODE: \"profile\""));
        assert!(
            profiling.contains("\"/w/generated/xdebug/shop.ini:/usr/local/etc/php/conf.d/zzz-stackvo-xdebug.ini:ro\""),
            "{profiling}"
        );
    }

    /// `zz-stackvo.ini` is the *user's* php.ini. A name sorting before it would
    /// let a hand-written `xdebug.output_dir` send profiles somewhere this app
    /// cannot read, which presents as profiling that produced nothing.
    #[test]
    fn the_generated_ini_is_parsed_after_the_users_own() {
        let ours = INI_CONTAINER_PATH.rsplit('/').next().unwrap();
        let theirs = crate::phpini::CONTAINER_PATH.rsplit('/').next().unwrap();
        assert!(ours > theirs, "{ours} does not sort after {theirs}");
    }

    /// The three settings the environment could not carry, and the reason each
    /// is there. `use_compression` in particular was confirmed to be ignored by
    /// `XDEBUG_CONFIG` on a running container.
    #[test]
    fn the_profile_ini_sets_what_the_environment_cannot() {
        let ini = profile_ini();
        assert!(ini.contains("xdebug.output_dir=/var/log/xdebug"), "{ini}");
        assert!(ini.contains("xdebug.use_compression=0"), "{ini}");
        assert!(ini.contains("xdebug.start_with_request=trigger"), "{ini}");
        // The mode is NOT here: XDEBUG_MODE takes precedence over xdebug.mode,
        // so setting both invites the two to disagree.
        assert!(!ini.contains("xdebug.mode"), "{ini}");
    }

    /// The output directory has to be under the tree the generated compose
    /// already mounts, or the files exist only inside the container.
    #[test]
    fn profiles_land_where_the_host_can_read_them() {
        assert!(crate::profile::CONTAINER_DIR.starts_with("/var/log/"));
    }

    /// Xdebug 2 listened on 9000, which is also PHP-FPM's port. Pinning 9003
    /// is the single most common fix for "the IDE never catches anything".
    #[test]
    fn the_port_is_xdebug_3s_not_php_fpms() {
        assert_eq!(PORT, 9003);
        assert!(!xdebug_config().contains("9000"));
    }

    #[test]
    fn projects_are_rendered_in_a_stable_order() {
        let entries = vec![
            Entry {
                service: "zebra".into(),
                server_name: None,
                mode: Mode::Debug,
                ini_path: None,
            },
            Entry {
                service: "apple".into(),
                server_name: None,
                mode: Mode::Debug,
                ini_path: None,
            },
        ];
        let yaml = overlay_yaml(&entries).unwrap();
        assert!(
            yaml.find("  apple:").unwrap() < yaml.find("  zebra:").unwrap(),
            "an unstable order rewrites the file on every compose command"
        );
    }

    #[test]
    fn a_project_without_a_domain_still_renders() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            server_name: None,
            mode: Mode::Debug,
            ini_path: None,
        }])
        .unwrap();
        assert!(yaml.contains("XDEBUG_MODE"));
        assert!(!yaml.contains("PHP_IDE_CONFIG"));
    }

    /// `XDEBUG_MODE` on its own leaves a debugger dialling localhost — which,
    /// inside the container, is the container.
    #[test]
    fn both_variables_are_required_to_call_it_active() {
        assert!(env_is_active(&s(&[
            "PATH=/usr/bin",
            "XDEBUG_MODE=debug",
            "XDEBUG_CONFIG=client_host=host.docker.internal client_port=9003",
        ])));

        assert!(!env_is_active(&s(&["XDEBUG_MODE=debug"])));
        assert!(!env_is_active(&s(&["XDEBUG_CONFIG=client_port=9003"])));
        assert!(!env_is_active(&s(&["XDEBUG_MODE=", "XDEBUG_CONFIG="])));
        assert!(!env_is_active(&[]));
    }

    /// Shaped like the real generated file, down to the `networks:` block that
    /// also uses two-space keys. `stackvo-net` is not a service, and an overlay
    /// that named it would break every compose command.
    #[test]
    fn only_real_services_are_read_out_of_the_generated_file() {
        const GENERATED: &str = "\
name: stackvo

services:
  api.oxoeashop:
    build:
      context: ./projects/api.oxoeashop
    volumes:
      - /host:/var/www/html
  vue-builder:
    image: node:22

networks:
  stackvo-net:
    external: true
";

        assert_eq!(
            generated_services(GENERATED),
            s(&["api.oxoeashop", "vue-builder"])
        );
    }

    #[test]
    fn a_file_with_no_services_block_yields_nothing() {
        assert!(generated_services("networks:\n  stackvo-net:\n").is_empty());
        assert!(generated_services("").is_empty());
    }

    /// The state that would otherwise read as "done": the environment is set,
    /// so the container looks configured, but it was built before the
    /// Dockerfile that installs the extension — so PHP has no Xdebug in it.
    /// The overlay applies on a restart; the extension does not.
    #[test]
    fn a_container_older_than_its_dockerfile_still_needs_a_rebuild() {
        assert!(needs_rebuild(true, true, Some(2_000), Some(1_000)));
        assert!(!needs_rebuild(true, true, Some(1_000), Some(2_000)));
    }

    #[test]
    fn a_dockerfile_without_the_extension_always_needs_a_rebuild() {
        assert!(needs_rebuild(true, false, None, None));
        assert!(needs_rebuild(true, false, Some(1), Some(9_999)));
    }

    #[test]
    fn nothing_is_needed_when_it_is_switched_off() {
        assert!(!needs_rebuild(false, false, Some(2_000), Some(1_000)));
    }

    /// With no container there is nothing to compare against, and claiming a
    /// rebuild is needed would put a permanent warning on every project that
    /// has simply never been started.
    #[test]
    fn an_unbuilt_project_is_judged_on_the_dockerfile_alone() {
        assert!(!needs_rebuild(true, true, Some(2_000), None));
        assert!(needs_rebuild(true, false, Some(2_000), None));
    }

    /// The compose build context is `./projects/<name>` written *inside*
    /// `generated/docker-compose.projects.yml`, so it resolves under
    /// `generated/`. This read the project directory instead, where a user's
    /// own unrelated Dockerfile may sit — and usually nothing does, which made
    /// `needsRebuild` true forever no matter how often the image was rebuilt.
    #[test]
    fn the_dockerfile_read_is_the_one_compose_builds() {
        let root = Path::new("/w");
        assert_eq!(
            dockerfile_path(root, "shop"),
            Path::new("/w/generated/projects/shop/Dockerfile")
        );
        // The bind-mount source is the other one, and stays that way — but
        // it is the chosen tree now, so with nothing chosen there is no path
        // to give. `/w` has no pointer file and never will; that empty answer
        // is the honest one.
        assert_eq!(projects_dir(root), Path::new(""));
    }

    #[test]
    fn dockers_timestamps_parse() {
        assert_eq!(epoch("1970-01-01T00:00:01Z"), Some(1));
        // Docker prints nanoseconds; the parser must not choke on them.
        assert!(epoch("2026-07-29T08:12:33.123456789Z").is_some());
        assert_eq!(epoch("not a date"), None);
    }

    /// A variable whose name merely starts the same must not count.
    #[test]
    fn a_similarly_named_variable_is_not_a_match() {
        assert!(!env_is_active(&s(&[
            "XDEBUG_MODE_OVERRIDE=debug",
            "XDEBUG_CONFIGURED=yes",
        ])));
    }
}
