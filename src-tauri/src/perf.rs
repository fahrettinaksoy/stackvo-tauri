//! Taking the heavy directories off the host filesystem (I-1).
//!
//! ## The measurement this starts from
//!
//! `examples/mount_bench.rs` compared four ways of putting the same tree in
//! front of the same PHP, and the table it produced is the whole argument:
//! `:cached` and `:delegated` are **inert** — they were removed from Docker
//! Desktop's implementation years ago and remain accepted syntax — and the
//! distance between a bind mount and a named volume is **2–3× on metadata and
//! on writes**. So there is something real to win, and the size of it is the
//! budget for winning it.
//!
//! ## What was refused, and why
//!
//! **Bundling Mutagen**, which is what DDEV does. It is a second binary to
//! package, sign and update for three platforms, a daemon to supervise, and a
//! failure mode where somebody's source code is *in the middle of a sync* when
//! something goes wrong. This repository already made that call once, against
//! dnsmasq, for the same reasons and in the same words.
//!
//! **Writing a bidirectional sync in this app.** Two-way file synchronisation
//! is a hard problem with a long tail — deletes, renames, permissions, symlinks,
//! conflicting writes, and a reconciliation policy for every one of them. A
//! half-correct implementation does not fail loudly; it loses a file somebody
//! wrote. Mutagen is several years of work by people who do only that, and a
//! worse copy of it living inside a GUI is not a feature, it is a liability.
//!
//! ## What is done instead, and why it is enough for most of the win
//!
//! The expensive directories in a PHP or Node project are not the ones people
//! edit. A Laravel install is a few hundred files of somebody's own code and
//! **twenty-five to thirty thousand** in `vendor/`; a Node project's ratio is
//! worse. Those directories are *written by tooling inside the container* and
//! read by it on every request — which is exactly the traffic a bind mount is
//! slow at — and they are not what an editor is for.
//!
//! So they go in named volumes, per project, by name. No synchronisation is
//! needed because nothing on the host writes them. What is lost is that an IDE
//! cannot see `vendor/` for autocomplete, and that is why [`export`] exists: one
//! explicit copy out to the host, stated as the snapshot it is, whenever the
//! index wants refreshing.
//!
//! ## The two cliffs, and what is done about each
//!
//! **A fresh named volume is empty.** Docker seeds a new volume from the image
//! only if the image has content at that path, and no PHP image ships a
//! `vendor/`. So switching this on without thinking would swap a working
//! `vendor/` for an empty directory and the site would 500 on the next request.
//! [`seed`] copies the host's own directory in before the container is
//! recreated, and enabling refuses to be silent about a directory it cannot
//! find.
//!
//! **The data then lives only in the volume.** Turning the setting off leaves a
//! volume nobody references, and deleting the volume throws away a `vendor/`
//! nobody has a copy of. So turning off offers the export first, and removal is
//! its own explicit act.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the setting lives, beside `php.ini` and `xdebug.json`.
pub const FILE_NAME: &str = "perf.json";

/// Where a project's tree is mounted in every generated service.
const PROJECT_ROOT: &str = "/var/www/html";

/// The prefix every volume this module creates carries.
///
/// Named rather than left to compose, which would prefix them with the project
/// directory's name and produce something nobody can find with `docker volume
/// ls` when they are looking for what is eating their disk.
pub const VOLUME_PREFIX: &str = "stackvo-cache-";

/// One project's setting.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Project-relative directories that live in a named volume.
    #[serde(default)]
    pub volumes: Vec<String>,
}

pub fn config_path(root: &Path, name: &str) -> PathBuf {
    crate::workspace::projects_root(root)
        .unwrap_or_default()
        .join(name)
        .join(crate::phpini::CONFIG_DIR)
        .join(FILE_NAME)
}

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.perf.yml")
}

pub fn read(root: &Path, name: &str) -> Config {
    std::fs::read_to_string(config_path(root, name))
        .ok()
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .map(|mut config| {
            // A file edited by hand can hold anything; what this module acts on
            // is only ever a path it would have written itself.
            config.volumes.retain(|path| checked_path(path).is_ok());
            config.volumes.sort();
            config.volumes.dedup();
            config
        })
        .unwrap_or_default()
}

pub fn write(root: &Path, name: &str, config: &Config) -> Result<()> {
    let path = config_path(root, name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    let text = serde_json::to_string_pretty(config)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the setting: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))
}

/// Is this a directory this module is willing to take over?
///
/// The value becomes three things at once: a path inside somebody's project, a
/// path inside a container, and part of a Docker volume name. Each of those has
/// its own way of being escaped, so the answer is one narrow rule rather than
/// three checks — relative, no `..`, no leading slash, ordinary characters.
///
/// `..` is the one that matters: `vendor/../../..` mounts a volume over the
/// user's home directory, and this value comes from a JSON file in a project
/// that may have arrived by `git clone`.
pub fn checked_path(path: &str) -> Result<&str> {
    let plain = !path.is_empty()
        && path.len() <= 128
        && !path.starts_with('/')
        && !path.starts_with('.')
        && !path.contains("..")
        && !path.ends_with('/')
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'));

    if !plain {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{path}\" is not a directory inside the project"),
        )
        .with_hint(crate::hints::PERF_PATH_IS_RELATIVE));
    }
    Ok(path)
}

/// The volume that holds one directory of one project.
pub fn volume_name(project: &str, path: &str) -> String {
    // Docker allows `[a-zA-Z0-9][a-zA-Z0-9_.-]*`, so the separator inside a
    // path has to become something legal — and something that cannot collide
    // with a project called `shop-vendor`.
    format!("{VOLUME_PREFIX}{project}--{}", path.replace('/', "-"))
}

/// The directories worth offering for a project, given what it is.
///
/// Offered, never applied on its own: this changes where a running application
/// reads its dependencies from, and a tool that did that to somebody's project
/// because it guessed the framework would be a tool people stop trusting.
///
/// The list is short on purpose. `vendor` and `node_modules` are where the file
/// count is; `storage/framework` and `bootstrap/cache` are small but are written
/// on **every request**, which is the other half of what a bind mount is slow
/// at.
pub fn suggestions(runtime: &str, project_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let has = |p: &str| project_dir.join(p).is_dir();

    if runtime.eq_ignore_ascii_case("php") {
        out.push("vendor".into());
        // Laravel's, and only when this actually is one — the directories are
        // framework-specific and a plain PHP project has neither.
        if has("storage/framework") || project_dir.join("artisan").is_file() {
            out.push("storage/framework".into());
            out.push("bootstrap/cache".into());
        }
    }
    if has("node_modules") || project_dir.join("package.json").is_file() {
        out.push("node_modules".into());
    }

    out.sort();
    out.dedup();
    out
}

// ------------------------------------------------------------------ overlay

/// One project's worth of overlay input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub service: String,
    pub volumes: Vec<String>,
}

/// Render the overlay, or `None` when no project wants one.
///
/// `None` rather than an empty document, for the reason `xdebug::overlay_yaml`
/// states: compose rejects a file whose `services` map is empty, so "nothing to
/// add" has to mean "no file".
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    let entries: Vec<&Entry> = entries.iter().filter(|e| !e.volumes.is_empty()).collect();
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from projects/*/.stackvo/perf.json before every compose\n\
         # command, so edits here are lost. Change it in the app instead.\n\
         #\n\
         # These directories live in named volumes rather than on the host\n\
         # filesystem: a bind mount costs 2-3x on metadata and writes, and this\n\
         # is where a project's file count is. The host copy is left where it\n\
         # is and is no longer what the container reads.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and\n\
         # will recreate these containers reading the host directory again.\n\
         services:\n",
    );

    let mut sorted = entries.clone();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in &sorted {
        out.push_str(&format!("  {}:\n", entry.service));
        out.push_str("    volumes:\n");
        for path in &entry.volumes {
            out.push_str(&format!(
                "      - \"{}:{PROJECT_ROOT}/{path}\"\n",
                volume_name(&entry.service, path)
            ));
        }
    }

    // Declared as external: false with an explicit name, so the volume is this
    // one whatever directory compose was invoked from. Compose derives its
    // default names from the project directory, which changes when a workspace
    // is moved and would silently hand back an empty volume.
    out.push_str("volumes:\n");
    for entry in &sorted {
        for path in &entry.volumes {
            let name = volume_name(&entry.service, path);
            out.push_str(&format!("  {name}:\n    name: {name}\n"));
        }
    }

    Some(out)
}

/// Every project with a setting **and** a compose service to hang it on.
///
/// The second half is the same guard `phpini` and `xdebug` carry, for the same
/// reason: naming a service the generator did not emit declares one with
/// neither an image nor a build context, and compose then refuses every command
/// against the whole stack.
fn entries(root: &Path) -> Vec<Entry> {
    let Some(projects) = crate::workspace::projects_root(root) else {
        return Vec::new();
    };
    let Ok(dir) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };

    let compose =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = crate::xdebug::generated_services(&compose);

    let mut out = Vec::new();
    for item in dir.flatten() {
        let path = item.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !services.iter().any(|s| s == name) {
            continue;
        }
        let config = read(root, name);
        if config.volumes.is_empty() {
            continue;
        }
        out.push(Entry {
            service: name.to_string(),
            volumes: config.volumes,
        });
    }

    out.sort_by(|a, b| a.service.cmp(&b.service));
    out
}

/// Re-render the overlay, and report whether it now exists.
pub fn sync(root: &Path) -> bool {
    let path = overlay_path(root);
    match overlay_yaml(&entries(root)) {
        Some(yaml) => {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
            crate::atomic::write(&path, &yaml).is_ok()
        }
        None => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

// -------------------------------------------------------------- moving data

/// What one directory of one project currently is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub path: String,
    /// Whether the setting says this directory lives in a volume.
    pub enabled: bool,
    pub volume: String,
    /// Whether that volume exists on the engine.
    pub exists: bool,
    /// Bytes the volume holds, when the engine can say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Whether the host still has a copy — what an editor would be indexing.
    pub on_host: bool,
    /// Files in the host copy, capped: the point of the number is the order of
    /// magnitude, and walking a real `node_modules` to the end costs seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_files: Option<usize>,
}

/// Copy the host's own directory into the volume, before anything reads it.
///
/// The cliff this exists for: a fresh named volume is **empty** — Docker seeds
/// one from the image only where the image has content at that path, and no PHP
/// image ships a `vendor/`. Recreating the container without this step swaps a
/// working directory for an empty one, and the site 500s on the next request
/// with nothing on screen connecting the two.
///
/// A helper container rather than `docker cp` into the running one: the volume
/// has to be mounted somewhere to be written, the project's own container is
/// the wrong place to do it (it is about to be recreated, and it may not even
/// be up), and `alpine` with a `cp` is the smallest thing that can hold a
/// mount. `-a` so ownership and modes survive — a `vendor/` whose bin files
/// lost their executable bit is a subtler failure than an empty one.
pub async fn seed(root: &Path, project: &str, path: &str) -> Result<()> {
    let path = checked_path(path)?;
    let source = crate::workspace::project_dir(root, project)?.join(path);
    if !source.is_dir() {
        return Err(
            Error::new(Code::NotFound, format!("{project} has no {path} to copy"))
                .with_hint(crate::hints::PERF_NOTHING_TO_SEED),
        );
    }

    let volume = volume_name(project, path);
    let mount = crate::paths::to_docker_mount(&source.display().to_string());
    let args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "-v".to_string(),
        format!("{mount}:/from:ro"),
        "-v".to_string(),
        format!("{volume}:/to"),
        HELPER_IMAGE.to_string(),
        "sh".to_string(),
        "-c".to_string(),
        // The trailing `/.` copies the *contents*, not the directory itself —
        // without it the volume ends up holding `/to/vendor/…` and every path
        // inside the container is one level too deep.
        "cp -a /from/. /to/ 2>&1".to_string(),
    ];

    let out = tokio::process::Command::new("docker")
        .args(&args)
        .output()
        .await
        .map_err(|e| Error::io("running docker", e))?;

    if !out.status.success() {
        return Err(Error::new(
            Code::IoError,
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        )
        .with_hint(crate::hints::PERF_SEED_FAILED));
    }
    Ok(())
}

/// Copy the volume back out to the host, so an editor can index it.
///
/// This is the price of the feature, paid explicitly rather than pretended
/// away: the container reads its dependencies from a volume the IDE cannot see,
/// so autocomplete needs a copy on the host. It is a **snapshot** and the screen
/// says so — the container keeps writing to the volume, and this copy does not
/// follow.
///
/// The host directory is replaced, not merged. A half-updated `vendor/` is
/// worse for an index than an old one, because nothing about it says which half
/// is which.
pub async fn export(root: &Path, project: &str, path: &str) -> Result<u64> {
    let path = checked_path(path)?;
    let target = crate::workspace::project_dir(root, project)?.join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }

    let volume = volume_name(project, path);
    let mount = crate::paths::to_docker_mount(&target.display().to_string());
    let out = tokio::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{volume}:/from:ro"),
            "-v",
            &format!("{mount}:/to"),
            HELPER_IMAGE,
            "sh",
            "-c",
            // Emptied first, for the reason above. `/to/.` so the mount point
            // itself survives — removing it would break the bind.
            "rm -rf /to/..?* /to/.[!.]* /to/* 2>/dev/null; cp -a /from/. /to/ && du -s /to | cut -f1",
        ])
        .output()
        .await
        .map_err(|e| Error::io("running docker", e))?;

    if !out.status.success() {
        return Err(Error::new(
            Code::IoError,
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024)
}

/// Throw the volume away.
///
/// Its own act, and never part of turning the setting off: what is in there is
/// a `vendor/` that may be the only copy, and a checkbox that deletes thirty
/// thousand files as a side effect is a checkbox nobody should trust.
pub async fn drop_volume(project: &str, path: &str) -> Result<()> {
    let volume = volume_name(project, checked_path(path)?);
    let out = tokio::process::Command::new("docker")
        .args(["volume", "rm", "-f", &volume])
        .output()
        .await
        .map_err(|e| Error::io("running docker", e))?;

    if !out.status.success() {
        return Err(Error::new(
            Code::IoError,
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// Which volumes exist, and how big they are.
///
/// One `docker system df -v` rather than a call per volume: a project with four
/// layers would otherwise pay four round trips to draw one panel, and the
/// engine computes all of it in the same walk anyway.
async fn volume_sizes() -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    let Ok(result) = tokio::process::Command::new("docker")
        .args([
            "system",
            "df",
            "-v",
            "--format",
            "{{range .Volumes}}{{.Name}}\t{{.Size}}\n{{end}}",
        ])
        .output()
        .await
    else {
        return out;
    };

    for line in String::from_utf8_lossy(&result.stdout).lines() {
        let Some((name, size)) = line.split_once('\t') else {
            continue;
        };
        if !name.starts_with(VOLUME_PREFIX) {
            continue;
        }
        out.insert(name.to_string(), parse_size(size.trim()));
    }
    out
}

/// `1.234GB`, `56.7MB`, `0B` — what the engine prints, as bytes.
///
/// Parsed rather than asked for in bytes because the CLI has no such format,
/// and an unparseable value becomes zero rather than a panic: this is a number
/// beside a directory name, not something a decision hangs on.
fn parse_size(text: &str) -> u64 {
    let digits: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = digits.parse().unwrap_or(0.0);
    let unit: String = text[digits.len()..].trim().to_ascii_lowercase();

    let scale = match unit.as_str() {
        "b" => 1.0,
        "kb" => 1_000.0,
        "mb" => 1_000_000.0,
        "gb" => 1_000_000_000.0,
        "tb" => 1_000_000_000_000.0,
        _ => 1.0,
    };
    (value * scale) as u64
}

/// What this project's layers are, for a screen that has to explain them.
pub async fn status(root: &Path, project: &str) -> Result<Vec<Layer>> {
    let dir = crate::workspace::project_dir(root, project)?;
    let runtime = crate::manifest::read(&dir.join("stackvo.json"), project)
        .map(|m| m.runtime.clone())
        .unwrap_or_default();

    let config = read(root, project);
    let mut paths = config.volumes.clone();
    for suggested in suggestions(&runtime, &dir) {
        if !paths.contains(&suggested) {
            paths.push(suggested);
        }
    }
    paths.sort();

    let sizes = volume_sizes().await;

    Ok(paths
        .into_iter()
        .map(|path| {
            let volume = volume_name(project, &path);
            let host = dir.join(&path);
            let on_host = host.is_dir();
            Layer {
                enabled: config.volumes.contains(&path),
                exists: sizes.contains_key(&volume),
                bytes: sizes.get(&volume).copied(),
                host_files: on_host.then(|| count_files(&host, FILE_COUNT_CAP)),
                on_host,
                volume,
                path,
            }
        })
        .collect())
}

/// The image the copy runs in.
///
/// `alpine` because it is small, is already on most machines, and the whole job
/// is one `cp` — nothing here needs a language runtime.
const HELPER_IMAGE: &str = "alpine:3";

/// Where counting files stops. See [`count_files`].
const FILE_COUNT_CAP: usize = 20_000;

/// Count files under a directory, giving up at `cap`.
///
/// Capped because this is called to draw a screen and `node_modules` is
/// routinely a hundred thousand files on a filesystem that is slow at exactly
/// this — the number is there to say "this is the big one", and it says that
/// just as well at 20,000.
pub fn count_files(dir: &Path, cap: usize) -> usize {
    fn walk(dir: &Path, cap: usize, seen: &mut usize) {
        if *seen >= cap {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if *seen >= cap {
                return;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(&entry.path(), cap, seen),
                // A symlink is counted and not followed: `node_modules` is full
                // of them and a loop would not return.
                Ok(_) => *seen += 1,
                Err(_) => {}
            }
        }
    }

    let mut seen = 0;
    walk(dir, cap, &mut seen);
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_could_escape_the_project_is_refused() {
        for good in [
            "vendor",
            "node_modules",
            "storage/framework",
            "bootstrap/cache",
        ] {
            assert!(checked_path(good).is_ok(), "{good}");
        }
        for hostile in [
            "/etc",
            "../../../etc",
            "vendor/../../..",
            "./vendor",
            "vendor/",
            "",
            "vendor;rm -rf /",
            "vendor$(id)",
            "ven dor",
        ] {
            assert!(checked_path(hostile).is_err(), "{hostile} was accepted");
        }
    }

    /// The name is a Docker identifier, and a path separator is not one.
    #[test]
    fn a_volume_name_is_legal_and_cannot_collide() {
        assert_eq!(
            volume_name("shop", "storage/framework"),
            "stackvo-cache-shop--storage-framework"
        );
        // The double dash is what keeps a project called `shop-vendor` from
        // producing the same name as `shop` plus `vendor`.
        assert_ne!(
            volume_name("shop-vendor", ""),
            volume_name("shop", "vendor")
        );
        for name in [volume_name("shop", "vendor"), volume_name("a_b.c", "x/y")] {
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
                "{name} is not a legal volume name"
            );
        }
    }

    /// The overlay has to declare the volumes as well as mount them, with the
    /// name pinned — compose derives a default name from the directory it was
    /// invoked in, which changes when a workspace moves and would hand back an
    /// empty volume with no error.
    #[test]
    fn the_overlay_mounts_and_declares_every_volume() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            volumes: vec!["vendor".into(), "storage/framework".into()],
        }])
        .expect("an overlay");

        assert!(yaml.contains("  shop:\n    volumes:\n"), "{yaml}");
        assert!(yaml.contains("\"stackvo-cache-shop--vendor:/var/www/html/vendor\""));
        assert!(yaml
            .contains("\"stackvo-cache-shop--storage-framework:/var/www/html/storage/framework\""));
        assert!(yaml.contains(
            "volumes:\n  stackvo-cache-shop--vendor:\n    name: stackvo-cache-shop--vendor\n"
        ));
    }

    /// Compose refuses a file whose `services` map is empty, so "nothing to
    /// add" has to mean "no file" — the same rule the other two overlays follow.
    #[test]
    fn nothing_to_add_is_no_file_at_all() {
        assert!(overlay_yaml(&[]).is_none());
        assert!(overlay_yaml(&[Entry {
            service: "shop".into(),
            volumes: Vec::new(),
        }])
        .is_none());
    }

    /// A hand-edited file is read defensively: what this module acts on is only
    /// ever a path it would have written itself.
    #[test]
    fn a_hostile_setting_on_disk_is_dropped_rather_than_obeyed() {
        let dir = std::env::temp_dir().join(format!("stackvo-perf-{}", std::process::id()));
        let projects = dir.join("projects");
        let config_dir = projects.join("shop").join(".stackvo");
        std::fs::create_dir_all(&config_dir).unwrap();
        // The project tree is wherever `projects.path` says, and there is
        // deliberately no default — see `workspace::projects_root`.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("projects.path"), projects.display().to_string()).unwrap();
        std::fs::write(
            config_dir.join(FILE_NAME),
            r#"{"volumes":["vendor","../../../etc","/etc/passwd","vendor"]}"#,
        )
        .unwrap();

        let config = read(&dir, "shop");
        assert_eq!(
            config.volumes,
            vec!["vendor".to_string()],
            "only the safe one, once"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Suggestions describe the project in front of them rather than a
    /// framework somebody assumed.
    #[test]
    fn suggestions_follow_what_the_project_actually_has() {
        let dir = std::env::temp_dir().join(format!("stackvo-perf-sug-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Plain PHP: vendor and nothing framework-shaped.
        assert_eq!(suggestions("php", &dir), vec!["vendor".to_string()]);

        std::fs::write(dir.join("artisan"), "#!/usr/bin/env php\n").unwrap();
        let laravel = suggestions("php", &dir);
        assert!(laravel.contains(&"storage/framework".to_string()));
        assert!(laravel.contains(&"bootstrap/cache".to_string()));

        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert!(suggestions("php", &dir).contains(&"node_modules".to_string()));
        // A Node project has no vendor to offer.
        assert!(!suggestions("node", &dir).contains(&"vendor".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The engine prints sizes for people, and this reads them back.
    #[test]
    fn the_engines_own_size_strings_are_read_rather_than_guessed() {
        assert_eq!(parse_size("0B"), 0);
        assert_eq!(parse_size("512B"), 512);
        assert_eq!(parse_size("56.7MB"), 56_700_000);
        assert_eq!(parse_size("1.234GB"), 1_234_000_000);
        // Anything unexpected is a number beside a directory name, not
        // something a decision hangs on.
        assert_eq!(parse_size(""), 0);
        assert_eq!(parse_size("N/A"), 0);
        assert_eq!(parse_size("12 whats"), 12);
    }

    #[test]
    fn counting_files_stops_at_the_cap_rather_than_walking_a_node_modules() {
        let dir = std::env::temp_dir().join(format!("stackvo-perf-count-{}", std::process::id()));
        let deep = dir.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        for n in 0..20 {
            std::fs::write(deep.join(format!("f{n}")), "x").unwrap();
        }

        assert_eq!(count_files(&dir, 100), 20);
        assert_eq!(count_files(&dir, 5), 5, "the cap is a stop, not a filter");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
