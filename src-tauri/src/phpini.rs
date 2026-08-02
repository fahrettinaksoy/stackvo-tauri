//! `memory_limit` and the rest of `php.ini`, per project.
//!
//! ## Why this is a feature and not a form
//!
//! `docs/*/configuration/project.md` documents `.stackvo/php.ini` as a project
//! config file, and the old web UI's `DockerService.js:388` listed it as one.
//! Neither was true: `php.ini` appears **nowhere** in `core/cli`. No generator
//! mounts it, no Dockerfile copies it. Dropping the file in did exactly nothing,
//! and the previous attempt at a settings form was cut on that evidence — it
//! would have written carefully validated values into a file no process reads.
//!
//! So the mount has to come first. Every competitor exposes `memory_limit` and
//! `upload_max_filesize`; a PHP environment that cannot raise an upload limit is
//! not finished.
//!
//! ## Where it goes, and why there
//!
//! Every PHP image the generator builds on — `php:X-fpm`, `php:X-apache`,
//! `php:X-cli` for Swoole, and FrankenPHP, which is itself built on the official
//! image — reads `/usr/local/etc/php/conf.d/*.ini` after the main `php.ini`.
//! The file lands there as `zz-stackvo.ini`: `zz` sorts after the
//! `docker-php-ext-*.ini` files the build writes, and last-parsed wins, so a
//! value set here is the value PHP ends up with.
//!
//! Mounted read-only. The container has no business writing to the user's
//! source tree, and a `:ro` mount says so in the one place anybody would look.
//!
//! ## Why an overlay, again
//!
//! Same reason as [`crate::xdebug`]: the compose generator's output is under a
//! byte-for-byte contract with the Bash implementation, so adding a volume to it
//! from this side alone would break the check that makes the port safe. Compose
//! layers files instead, so this writes a fifth `-f` that the generator neither
//! produces nor knows about. It is re-derived on every compose invocation and
//! never stored, because an overlay naming a deleted project declares a service
//! with no image and no build context, and compose then refuses *every* command
//! against the whole stack — including the `down` that would clear it.
//!
//! Its own file rather than a section of the Xdebug overlay: the two are
//! independent (one adds `volumes`, the other `environment`), and sharing a
//! document means a fault in one can take the other down with it.
//!
//! ## The three states, kept apart
//!
//! Like Xdebug, "on" is not one boolean:
//!
//! 1. **On disk** — the file exists under `projects/<name>/.stackvo/`.
//! 2. **Mounted** — the running container actually has it. Read from the
//!    container, never inferred: `stackvo up` from the Bash CLI layers three
//!    files, not five, and will recreate the container without this mount.
//! 3. **Applied** — PHP reads its ini once, at process start. Editing a value
//!    in a bind-mounted file changes nothing until the container restarts.
//!
//! Reporting them separately is what stops "I set memory_limit and nothing
//! happened" turning into an afternoon in the wrong config file.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the file lands inside the container.
///
/// `zz-` so it is parsed after `docker-php-ext-*.ini`; last one wins.
pub const CONTAINER_PATH: &str = "/usr/local/etc/php/conf.d/zz-stackvo.ini";

/// The project-local config directory, matching `CONST_STACKVO_CONFIG_DIR`.
pub const CONFIG_DIR: &str = ".stackvo";

pub const FILE_NAME: &str = "php.ini";

/// The directives the form offers.
///
/// A fixed list, not "whatever is in the file". These four are what every
/// competitor exposes and what people actually come here to change; anything
/// else is still editable by hand and is preserved untouched — see `unmanaged`.
pub const MANAGED: [&str; 4] = [
    "memory_limit",
    "upload_max_filesize",
    "post_max_size",
    "max_execution_time",
];

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.phpini.yml")
}

pub fn ini_path(root: &Path, name: &str) -> PathBuf {
    crate::workspace::projects_root(root)
        .unwrap_or_default()
        .join(name)
        .join(CONFIG_DIR)
        .join(FILE_NAME)
}

// -------------------------------------------------------------- pure logic

/// Every `key = value` in an ini, in file order.
///
/// Section headers (`[PHP]`) are ignored rather than tracked: a file under
/// `conf.d` is parsed into the running configuration whatever section it
/// claims, and pretending otherwise would mean rejecting a file PHP accepts.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // A trailing `; note` is a comment in ini, not part of the value.
        let value = match value.find(" ;") {
            Some(i) => &value[..i],
            None => value,
        };
        out.push((key.to_string(), value.trim().to_string()));
    }

    out
}

/// Apply a patch to ini text, preserving order, comments and unknown keys.
///
/// `None` removes the directive outright. That is not the same as setting it to
/// PHP's default: this file is an *override* layer, and leaving
/// `memory_limit = 128M` behind because it happens to match the default would
/// keep overriding a `php.ini` the user might later change underneath it.
///
/// Modelled on [`crate::env_writer::patch_text`] deliberately — same problem,
/// same shape, and two different answers to "edit one line of a config file"
/// is a maintenance cost with nothing to show for it.
pub fn patch_text(original: &str, patch: &BTreeMap<String, Option<String>>) -> String {
    if patch.is_empty() {
        return original.to_string();
    }

    let mut remaining = patch.clone();
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let mut out: Vec<String> = Vec::new();

    for raw in original.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();

        if trimmed.is_empty()
            || trimmed.starts_with(';')
            || trimmed.starts_with('#')
            || trimmed.starts_with('[')
        {
            out.push(line.to_string());
            continue;
        }

        let Some((key_part, value_part)) = line.split_once('=') else {
            out.push(line.to_string());
            continue;
        };

        let key = key_part.trim();
        let Some(new_value) = remaining.remove(key) else {
            out.push(line.to_string());
            continue;
        };

        // Removal drops the line entirely; leaving a commented-out corpse
        // behind accumulates one per edit for as long as the file lives.
        let Some(new_value) = new_value else { continue };

        let indent: String = key_part.chars().take_while(|c| c.is_whitespace()).collect();
        let comment = value_part
            .find(" ;")
            .map(|i| value_part[i..].to_string())
            .unwrap_or_default();

        out.push(format!("{indent}{key} = {new_value}{comment}"));
    }

    // Whatever is left is a directive the file did not have. Removals among
    // them are already satisfied — there was nothing to remove.
    let added: Vec<(&String, &String)> = remaining
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| (k, v)))
        .collect();

    if !added.is_empty() {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
            out.push(String::new());
        }
        for (key, value) in added {
            out.push(format!("{key} = {value}"));
        }
    }

    out.join(newline)
}

/// The header a file this app creates starts with.
///
/// Says what reads the file and what it overrides, because the alternative is
/// somebody finding an unexplained ini in their repository a year from now.
pub const HEADER: &str = "; PHP overrides for this project, written by StackVo Desktop.\n\
                          ;\n\
                          ; Mounted read-only at /usr/local/etc/php/conf.d/zz-stackvo.ini, which PHP\n\
                          ; parses after its own php.ini and after the extension inis the build\n\
                          ; writes — so what is set here is what PHP ends up with.\n\
                          ;\n\
                          ; Safe to edit by hand and safe to commit. Directives this app does not\n\
                          ; manage are left alone; PHP restarts on container restart, not on save.\n";

/// Is this a plausible directive name?
///
/// Narrow on purpose. The value ends up in a YAML-adjacent file and an ini
/// parser, and a key carrying `=`, a newline or a bracket corrupts one of them.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Sizes PHP accepts: a number, optionally with a K/M/G shorthand, or `-1`.
fn valid_size(value: &str) -> bool {
    let value = value.trim();
    if value == "-1" {
        return true;
    }
    let digits = value.trim_end_matches(['K', 'M', 'G', 'k', 'm', 'g']);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn valid_seconds(value: &str) -> bool {
    let value = value.trim();
    value == "-1" || (!value.is_empty() && value.chars().all(|c| c.is_ascii_digit()))
}

/// Reject a value before it reaches the file.
///
/// PHP does not fail loudly on a malformed directive — it warns to a log nobody
/// is watching and carries on with the previous value, which presents as "the
/// setting did nothing". Catching it here is the only place it is visible.
pub fn validate(patch: &BTreeMap<String, Option<String>>) -> Result<()> {
    for (key, value) in patch {
        if !valid_key(key) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("`{key}` is not a valid php.ini directive name"),
            )
            .with_hint("Directive names are letters, digits, underscores and dots."));
        }

        let Some(value) = value else { continue };

        if value.contains('\n') || value.contains('\r') {
            return Err(Error::new(
                Code::InvalidInput,
                format!("the value for `{key}` contains a line break"),
            )
            .with_hint("php.ini is one directive per line."));
        }

        let ok = match key.as_str() {
            "memory_limit" | "upload_max_filesize" | "post_max_size" => valid_size(value),
            "max_execution_time" => valid_seconds(value),
            // An unmanaged directive the user typed themselves. Format-checked
            // above, not second-guessed — this file is not a whitelist.
            _ => true,
        };

        if !ok {
            return Err(Error::new(
                Code::InvalidInput,
                format!("`{value}` is not a valid value for {key}"),
            )
            .with_hint(
                "Sizes are a number with an optional K, M or G — 256M, 1G, 512. \
                 Times are whole seconds. -1 means unlimited.",
            ));
        }
    }

    Ok(())
}

/// `post_max_size` smaller than `upload_max_filesize`, which silently caps it.
///
/// Not an error — it is a legal configuration, and somebody may want it. But it
/// is almost always a mistake, and its symptom is an upload that fails at a
/// limit the user has already raised and can see is raised. Reported as a
/// warning so the form can say so next to the field.
pub fn size_warning(values: &BTreeMap<String, String>) -> Option<String> {
    let bytes = |key: &str| values.get(key).and_then(|v| as_bytes(v));
    let post = bytes("post_max_size")?;
    let upload = bytes("upload_max_filesize")?;

    if post > 0 && post < upload {
        return Some(format!(
            "post_max_size ({}) is smaller than upload_max_filesize ({}); uploads are capped by the smaller of the two.",
            values.get("post_max_size")?,
            values.get("upload_max_filesize")?
        ));
    }
    None
}

/// A PHP size shorthand as bytes. `-1` (unlimited) becomes 0, which compares as
/// "no limit worth warning about".
fn as_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    if value == "-1" {
        return Some(0);
    }
    let (digits, scale) = match value.chars().last()? {
        'K' | 'k' => (&value[..value.len() - 1], 1024),
        'M' | 'm' => (&value[..value.len() - 1], 1024 * 1024),
        'G' | 'g' => (&value[..value.len() - 1], 1024 * 1024 * 1024),
        _ => (value, 1),
    };
    digits.parse::<u64>().ok().map(|n| n * scale)
}

/// One project's worth of overlay input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The compose service name, which the generator sets to the project name.
    pub service: String,
    /// The host path of the ini, as compose will read it.
    pub host_path: String,
}

/// Render the overlay, or None when no project has a file.
///
/// None rather than an empty document: compose rejects a `services` map with no
/// entries, so "nothing to mount" has to mean "no file at all".
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from projects/*/.stackvo/php.ini before every compose command,\n\
         # so edits here are lost. Edit the project's php.ini instead.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and will\n\
         # recreate these containers without the mount below.\n\
         services:\n",
    );

    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in sorted {
        out.push_str(&format!("  {}:\n", entry.service));
        out.push_str("    volumes:\n");
        // Quoted because a Windows host path contains a colon, which is the
        // separator compose splits this on.
        out.push_str(&format!(
            "      - \"{}:{CONTAINER_PATH}:ro\"\n",
            entry.host_path
        ));
    }

    Some(out)
}

// ------------------------------------------------------------------- I/O

/// Every PHP project with a `php.ini` **and** a compose service to mount it on.
///
/// The second half is not belt-and-braces — see [`crate::xdebug`]: naming a
/// service the generator did not emit declares one with neither an image nor a
/// build context, and compose then refuses every command against the stack.
fn entries(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();

    let generated =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = crate::xdebug::generated_services(&generated);

    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(dirs) = std::fs::read_dir(&projects) else {
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
        if !services.iter().any(|s| s == name) {
            continue;
        }

        let ini = path.join(CONFIG_DIR).join(FILE_NAME);
        if !ini.is_file() {
            continue;
        }

        // Node has no PHP to configure, and its compose service is generated
        // from a different template with no conf.d to speak of.
        let manifest_file = path.join("stackvo.json");
        let Ok(manifest) = crate::manifest::read(&manifest_file, name) else {
            continue;
        };
        if manifest.runtime != "php" {
            continue;
        }

        out.push(Entry {
            service: name.to_string(),
            host_path: ini.display().to_string(),
        });
    }

    out
}

/// Re-render the overlay from what is on disk, and report whether it now exists.
pub fn sync(root: &Path) -> bool {
    let path = overlay_path(root);

    match overlay_yaml(&entries(root)) {
        Some(yaml) => {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
            // A write failure must not take compose down with it. The honest
            // degradation is "the overrides are not mounted", which `mounted`
            // then reports, rather than "no container can be started".
            match crate::atomic::write(&path, &yaml) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not write the php.ini overlay");
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpIniStatus {
    /// False for node projects; there is no conf.d to write into.
    pub supported: bool,
    /// Does the file exist on disk?
    pub exists: bool,
    /// Host path, whether or not it exists yet.
    pub path: String,
    pub container_path: String,
    /// Does the *running* container carry the mount? None when nothing is
    /// running. Read from the container, never inferred — the Bash CLI's `up`
    /// layers three compose files and will recreate it without this one.
    pub mounted: Option<bool>,
    pub running: bool,
    /// The managed directives that are set, by name.
    pub values: BTreeMap<String, String>,
    /// Directives in the file this app does not manage, preserved verbatim.
    pub unmanaged: BTreeMap<String, String>,
    /// True when the file is on disk but the running container has no mount —
    /// the project needs bringing up again, not just restarting.
    pub needs_recreate: bool,
    /// A legal but near-certainly-unintended combination, if there is one.
    pub warning: Option<String>,
    /// What PHP in the running container actually has, right now. None when
    /// nothing is running.
    ///
    /// Read rather than assumed, because assuming was wrong. Measured on this
    /// stack: `php -i` reports **`Loaded Configuration File => (none)`** — the
    /// official images ship no `php.ini` at all, so conf.d is not one layer
    /// among several, it is the *only* one. And the defaults are not the ones
    /// the manual lists: `max_execution_time` is 0 under FPM, not 30. A form
    /// whose placeholder says 30 is telling the user something untrue about
    /// their own container.
    ///
    /// It also closes the loop: after a save and a restart, this is where you
    /// see that the override actually landed.
    pub effective: Option<BTreeMap<String, String>>,
    pub overlay_path: String,
}

/// Ask PHP inside the container what it currently has.
///
/// One `docker exec` for all four, and best-effort throughout: a container that
/// is starting, has no `php` on its PATH, or is simply not running yields None,
/// which the form renders as "no measurement" rather than as a default it made
/// up.
async fn effective(container: &str) -> Option<BTreeMap<String, String>> {
    let script = format!(
        "foreach ([{}] as $k) echo $k, '=', ini_get($k), \"\\n\";",
        MANAGED
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",")
    );

    let output = tokio::process::Command::new("docker")
        .args(["exec", container, "php", "-r", &script])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let map: BTreeMap<String, String> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();

    (!map.is_empty()).then_some(map)
}

fn split_values(
    pairs: Vec<(String, String)>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut managed = BTreeMap::new();
    let mut unmanaged = BTreeMap::new();

    for (key, value) in pairs {
        if MANAGED.contains(&key.as_str()) {
            managed.insert(key, value);
        } else {
            unmanaged.insert(key, value);
        }
    }

    (managed, unmanaged)
}

/// What is true for one project, across all three layers.
pub async fn status(root: &Path, name: &str) -> Result<PhpIniStatus> {
    let manifest_file = crate::workspace::require_projects_root(root)?
        .join(name)
        .join("stackvo.json");
    if !manifest_file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&manifest_file, name)?;
    let supported = manifest.runtime == "php";

    let path = ini_path(root, name);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let exists = path.is_file();
    let (values, unmanaged) = split_values(parse(&text));

    // Best-effort: with the engine down there is no container to describe, and
    // that is not an error worth failing the whole query over.
    let details = crate::engine::inspect(name).await.ok();
    let running = details.as_ref().is_some_and(|d| d.running);
    let mounted = details
        .as_ref()
        .map(|d| d.mounts.iter().any(|m| m.destination == CONTAINER_PATH));

    let effective = if running {
        effective(&crate::engine::container_name(name)).await
    } else {
        None
    };

    Ok(PhpIniStatus {
        supported,
        exists,
        path: path.display().to_string(),
        container_path: CONTAINER_PATH.to_string(),
        mounted,
        running,
        warning: size_warning(&values),
        needs_recreate: exists && mounted == Some(false),
        values,
        unmanaged,
        effective,
        overlay_path: overlay_path(root).display().to_string(),
    })
}

/// Write directives, creating or removing the file as needed.
///
/// Removing the last directive removes the file, which then removes the mount
/// on the next compose invocation. An empty ini left behind would keep a mount
/// alive that does nothing but make the container differ from a fresh one.
pub async fn set(
    root: &Path,
    name: &str,
    patch: &BTreeMap<String, Option<String>>,
) -> Result<PhpIniStatus> {
    let manifest_file = crate::workspace::require_projects_root(root)?
        .join(name)
        .join("stackvo.json");
    if !manifest_file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&manifest_file, name)?;
    if manifest.runtime != "php" {
        return Err(Error::new(
            Code::Unsupported,
            format!(
                "{name} is a {} project; php.ini applies to PHP only",
                manifest.runtime
            ),
        ));
    }

    validate(patch)?;

    let path = ini_path(root, name);
    let original = std::fs::read_to_string(&path).unwrap_or_default();
    // A file this app creates gets the header explaining what reads it; one the
    // user already had is patched as it stands.
    let base = if original.is_empty() {
        HEADER.to_string()
    } else {
        original.clone()
    };

    let updated = patch_text(&base, patch);

    if parse(&updated).is_empty() {
        // Nothing left but comments. Remove the file so the mount goes with it.
        let _ = std::fs::remove_file(&path);
    } else if updated != original {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        crate::atomic::write(&path, &updated)?;
    }

    // So the next compose invocation is not the first time the mount appears —
    // and so `needs_recreate` in the reply is answered against a current file.
    sync(root);

    status(root, name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(pairs: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
            .collect()
    }

    #[test]
    fn directives_are_read_with_comments_and_sections_ignored() {
        let text = "; a note\n[PHP]\nmemory_limit = 512M\nupload_max_filesize=64M ; why\n\n#hash\n";
        assert_eq!(
            parse(text),
            vec![
                ("memory_limit".to_string(), "512M".to_string()),
                ("upload_max_filesize".to_string(), "64M".to_string()),
            ]
        );
    }

    /// The file is the user's. An edit that reformats it, drops their comments
    /// or reorders their directives is a diff they did not ask for.
    #[test]
    fn patching_preserves_comments_order_and_unknown_directives() {
        let original = "; mine\nmemory_limit = 128M ; deliberate\nzend.assertions = 1\n";
        let updated = patch_text(original, &patch(&[("memory_limit", Some("512M"))]));

        assert_eq!(
            updated,
            "; mine\nmemory_limit = 512M ; deliberate\nzend.assertions = 1\n"
        );
    }

    #[test]
    fn a_new_directive_is_appended() {
        let updated = patch_text(
            "memory_limit = 128M\n",
            &patch(&[("post_max_size", Some("64M"))]),
        );
        assert!(updated.contains("memory_limit = 128M"));
        assert!(updated.trim_end().ends_with("post_max_size = 64M"));
    }

    /// Removal drops the line. Commenting it out instead leaves one corpse per
    /// edit for as long as the file lives.
    #[test]
    fn removing_a_directive_deletes_its_line() {
        let updated = patch_text(
            "; note\nmemory_limit = 512M\npost_max_size = 64M\n",
            &patch(&[("memory_limit", None)]),
        );
        assert!(!updated.contains("memory_limit"));
        assert!(updated.contains("; note"));
        assert!(updated.contains("post_max_size = 64M"));
    }

    #[test]
    fn removing_a_directive_that_is_not_there_changes_nothing() {
        let original = "post_max_size = 64M\n";
        assert_eq!(
            patch_text(original, &patch(&[("memory_limit", None)])),
            original
        );
    }

    /// PHP does not fail loudly on a malformed directive — it warns to a log
    /// nobody is watching and keeps the old value, which presents as "the
    /// setting did nothing". This is the only place it is visible.
    #[test]
    fn malformed_values_are_refused() {
        assert!(validate(&patch(&[("memory_limit", Some("512 MB"))])).is_err());
        assert!(validate(&patch(&[("memory_limit", Some("lots"))])).is_err());
        assert!(validate(&patch(&[("max_execution_time", Some("30s"))])).is_err());
        assert!(validate(&patch(&[("memory_limit", Some("512M\nevil = 1"))])).is_err());
        assert!(validate(&patch(&[("memory limit", Some("512M"))])).is_err());

        assert!(validate(&patch(&[("memory_limit", Some("512M"))])).is_ok());
        assert!(validate(&patch(&[("memory_limit", Some("-1"))])).is_ok());
        assert!(validate(&patch(&[("upload_max_filesize", Some("1G"))])).is_ok());
        assert!(validate(&patch(&[("max_execution_time", Some("300"))])).is_ok());
        // An unmanaged directive is format-checked, not second-guessed.
        assert!(validate(&patch(&[("zend.assertions", Some("1"))])).is_ok());
    }

    /// The classic: the upload limit is raised, the post limit is not, and the
    /// upload still fails at a number the user can see is no longer set.
    #[test]
    fn a_post_limit_below_the_upload_limit_is_flagged() {
        let values: BTreeMap<String, String> =
            [("upload_max_filesize", "64M"), ("post_max_size", "8M")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

        assert!(size_warning(&values).is_some());

        let fine: BTreeMap<String, String> =
            [("upload_max_filesize", "64M"), ("post_max_size", "128M")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        assert!(size_warning(&fine).is_none());
    }

    #[test]
    fn sizes_compare_across_units() {
        assert_eq!(as_bytes("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(as_bytes("1024M"), Some(1024 * 1024 * 1024));
        assert_eq!(as_bytes("512"), Some(512));
        // Unlimited is not a number to warn about.
        assert_eq!(as_bytes("-1"), Some(0));
    }

    /// Compose rejects a `services:` map with no entries, so "nothing to
    /// mount" has to mean "no file", not "an empty document".
    #[test]
    fn an_empty_overlay_is_no_overlay() {
        assert!(overlay_yaml(&[]).is_none());
    }

    #[test]
    fn the_overlay_mounts_read_only_at_the_scan_directory() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".to_string(),
            host_path: "/w/projects/shop/.stackvo/php.ini".to_string(),
        }])
        .unwrap();

        assert!(yaml.contains("  shop:\n    volumes:\n"));
        assert!(yaml.contains(
            "\"/w/projects/shop/.stackvo/php.ini:/usr/local/etc/php/conf.d/zz-stackvo.ini:ro\""
        ));
    }

    /// `zz` is load order, not decoration: PHP parses conf.d alphabetically and
    /// the build writes `docker-php-ext-*.ini` in there. A name sorting before
    /// those would be overridden by the very files it is meant to override.
    #[test]
    fn the_mount_name_sorts_after_the_extension_inis() {
        let ours = CONTAINER_PATH.rsplit('/').next().unwrap();
        assert!(ours > "docker-php-ext-zzzz.ini", "{ours}");
    }
}
