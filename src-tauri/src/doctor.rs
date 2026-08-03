//! Deeper diagnosis than the boot gate, with named culprits.
//!
//! `preflight` answers "can the app run at all" and blocks the first screen
//! until it can. This module answers the questions that arrive later, one
//! failed `compose up` at a time — and each answer names the thing to act on:
//!
//! - **Ports.** The single most common Docker failure is a host port already
//!   taken, and compose reports it as "address already in use" with no word on
//!   *by what*. The check reads the ports the generated stack will claim, asks
//!   the OS who is listening, and separates "our own container" (fine) from
//!   "someone else's container" (named) from "a host process" (named, with
//!   pid).
//! - **Generated output.** The compose files are derived from `.env` and the
//!   project manifests; edit an input without re-running the generator and the
//!   stack silently runs yesterday's config. Mtime comparison makes the drift
//!   visible and repairable.
//!
//! Hosts repair, engine start and space reclaim have commands of their own
//! (`hosts_apply`, `engine_start`, `docker_prune`); the doctor report carries
//! their state so one screen can offer every repair.

use crate::preflight::State;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

// ------------------------------------------------------------------- ports

/// One host port the generated stack will try to claim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortCheck {
    pub port: u16,
    /// The compose service that publishes it — "traefik", "mysql", …
    pub required_by: String,
    /// `Ok` free or held by the stack itself · `Fail` held by something else ·
    /// `Unknown` the OS listener table could not be read.
    pub state: State,
    /// Who holds it, when it is not ours: a container name when the engine
    /// could tell us, otherwise the process the OS names.
    pub process: Option<String>,
    pub pid: Option<u32>,
    /// True when the listener is the stack's own published port.
    pub ours: bool,
}

/// `(service, host_port)` pairs from one generated compose file.
///
/// A line scanner rather than a YAML parser, deliberately: the input is the
/// output of StackVo's own generator, whose shape is frozen by the
/// byte-for-byte contract — two-space service keys, a `ports:` list of
/// quoted `"host:container"` strings. Parsing only that shape means a file
/// this cannot read is a file the generator did not write.
fn compose_ports(text: &str) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let mut service: Option<String> = None;
    let mut in_ports = false;

    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // A service key: two-space indent, `name:` with nothing after it.
        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            service = Some(trimmed.trim_end_matches(':').to_string());
            in_ports = false;
            continue;
        }

        if indent == 4 && trimmed.starts_with("ports:") {
            in_ports = true;
            continue;
        }

        if in_ports {
            if let Some(item) = trimmed.strip_prefix("- ") {
                if let (Some(svc), Some(port)) = (service.as_ref(), host_port(item)) {
                    out.push((svc.clone(), port));
                }
                continue;
            }
            // Anything that is not a list item ends the ports block.
            in_ports = false;
        }
    }
    out
}

/// The services in the `core` profile, in the order the file declares them.
///
/// Same line scanner and same justification as [`compose_ports`]: the input is
/// this app's own generated file. Both list shapes are read because the
/// template uses the inline one (`profiles: ["core"]`) and nothing stops a
/// workspace that has taken the template over from using the block one.
fn core_services(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut service: Option<String> = None;
    let mut in_profiles = false;

    let is_core = |value: &str| {
        value
            .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
            .split(',')
            .any(|p| p.trim().trim_matches(|c| c == '"' || c == '\'') == "core")
    };

    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        // A trailing comment is not part of the value. The shipped template
        // carries one on this very line.
        let trimmed = line.trim().split(" #").next().unwrap_or("").trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            service = Some(trimmed.trim_end_matches(':').to_string());
            in_profiles = false;
            continue;
        }

        if indent == 4 {
            if let Some(value) = trimmed.strip_prefix("profiles:") {
                if value.trim().is_empty() {
                    in_profiles = true; // block list on the following lines
                } else if is_core(value) {
                    out.extend(service.clone());
                }
                continue;
            }
            in_profiles = false;
        }

        if in_profiles {
            match trimmed.strip_prefix("- ") {
                Some(item) if is_core(item) => {
                    out.extend(service.clone());
                    in_profiles = false;
                }
                Some(_) => {}
                None => in_profiles = false,
            }
        }
    }

    out.dedup();
    out
}

/// The host side of a compose port mapping, if the mapping publishes one.
///
/// Shapes: `"80:80"` · `"127.0.0.1:8080:80"` · `"6379:6379/tcp"` · `"9000"`
/// (container-only, publishes nothing fixed — skipped).
fn host_port(mapping: &str) -> Option<u16> {
    let s = mapping.trim().trim_matches(|c| c == '"' || c == '\'');
    let s = s.split('/').next().unwrap_or(s);
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => parts[0].parse().ok(),
        3 => parts[1].parse().ok(),
        _ => None,
    }
}

/// Every host port the generated stack claims, with the service that claims it.
///
/// Read from the generated compose files rather than from `.env`: the files
/// are what `compose up` will actually execute, and the generator only writes
/// services that are enabled — so the file *is* the enabled set.
pub fn required_ports(root: &Path) -> Vec<(String, u16)> {
    const FILES: [&str; 3] = [
        "generated/stackvo.yml",
        "generated/docker-compose.dynamic.yml",
        "generated/docker-compose.projects.yml",
    ];

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for file in FILES {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        for (service, port) in compose_ports(&text) {
            if seen.insert(port) {
                out.push((service, port));
            }
        }
    }
    out
}

/// A process the OS reports as listening on a TCP port.
#[derive(Debug, Clone)]
pub struct Listener {
    pub process: Option<String>,
    pub pid: Option<u32>,
}

/// `port → listener` for every listening TCP socket, per platform tool.
///
/// One spawn for the whole table rather than one per port. `None` (as opposed
/// to an empty map) means the table could not be read at all, which the caller
/// reports as `Unknown` rather than "free".
pub async fn listeners() -> Option<HashMap<u16, Listener>> {
    #[cfg(target_os = "macos")]
    {
        let out = capture("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"]).await?;
        Some(parse_lsof(&out))
    }
    #[cfg(target_os = "linux")]
    {
        let out = capture("ss", &["-H", "-tlnp"]).await?;
        Some(parse_ss(&out))
    }
    #[cfg(target_os = "windows")]
    {
        let out = capture("netstat", &["-ano", "-p", "TCP"]).await?;
        Some(parse_netstat(&out))
    }
}

async fn capture(program: &str, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()?;
    // lsof exits 1 when it matched nothing; the empty table is still an answer.
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `COMMAND PID USER … NAME` where NAME is `*:80` or `127.0.0.1:8080`.
#[cfg(any(target_os = "macos", test))]
fn parse_lsof(out: &str) -> HashMap<u16, Listener> {
    let mut map = HashMap::new();
    for line in out.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let Some(port) = cols[8].rsplit(':').next().and_then(|p| p.parse().ok()) else {
            continue;
        };
        map.entry(port).or_insert(Listener {
            process: Some(cols[0].to_string()),
            pid: cols[1].parse().ok(),
        });
    }
    map
}

/// `LISTEN 0 4096 0.0.0.0:80 0.0.0.0:* users:(("nginx",pid=123,fd=6))`
///
/// Without root the `users:` column is absent for other users' processes; the
/// port is still reported, just anonymously.
#[cfg(any(target_os = "linux", test))]
fn parse_ss(out: &str) -> HashMap<u16, Listener> {
    let mut map = HashMap::new();
    for line in out.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 || cols[0] != "LISTEN" {
            continue;
        }
        let Some(port) = cols[3].rsplit(':').next().and_then(|p| p.parse().ok()) else {
            continue;
        };
        let process = line
            .split("users:((\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(str::to_string);
        let pid = line
            .split("pid=")
            .nth(1)
            .and_then(|s| s.split(&[',', ')']).next())
            .and_then(|s| s.parse().ok());
        map.entry(port).or_insert(Listener { process, pid });
    }
    map
}

/// `  TCP    0.0.0.0:80    0.0.0.0:0    LISTENING    4712`
///
/// Names are resolved separately (`tasklist` per unique pid) by the caller —
/// netstat itself only reports pids.
#[cfg(any(target_os = "windows", test))]
fn parse_netstat(out: &str) -> HashMap<u16, Listener> {
    let mut map = HashMap::new();
    for line in out.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || cols[0] != "TCP" || cols[3] != "LISTENING" {
            continue;
        }
        let Some(port) = cols[1].rsplit(':').next().and_then(|p| p.parse().ok()) else {
            continue;
        };
        map.entry(port).or_insert(Listener {
            process: None,
            pid: cols[4].parse().ok(),
        });
    }
    map
}

/// Resolve a Windows pid to an image name. One spawn per unique pid, and only
/// for ports that are actually in conflict — never for the whole table.
#[cfg(target_os = "windows")]
pub async fn process_name(pid: u32) -> Option<String> {
    let out = capture(
        "tasklist",
        &["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"],
    )
    .await?;
    out.trim()
        .strip_prefix('"')?
        .split('"')
        .next()
        .map(str::to_string)
}

/// The docker backend answers for every published port, so its name alone
/// says "a container" without saying whose. Those get upgraded to a container
/// name when the engine can be asked.
fn is_docker_backend(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["docker", "vpnkit", "orbstack", "colima", "qemu", "krunkit"]
        .iter()
        .any(|d| lower.contains(d))
}

/// Verdict for every required port.
///
/// `owners` is `host port → container name` for *running* containers, when
/// the engine could be asked; the stack's own containers make a port `Ok`,
/// anyone else's makes it a named conflict.
pub fn check_ports(
    required: Vec<(String, u16)>,
    table: Option<&HashMap<u16, Listener>>,
    owners: Option<&HashMap<u16, String>>,
) -> Vec<PortCheck> {
    required
        .into_iter()
        .map(|(service, port)| {
            let Some(table) = table else {
                return PortCheck {
                    port,
                    required_by: service,
                    state: State::Unknown,
                    process: None,
                    pid: None,
                    ours: false,
                };
            };

            let Some(listener) = table.get(&port) else {
                // Nothing listening: free for the stack to claim.
                return PortCheck {
                    port,
                    required_by: service,
                    state: State::Ok,
                    process: None,
                    pid: None,
                    ours: false,
                };
            };

            if let Some(container) = owners.and_then(|o| o.get(&port)) {
                let ours = container.starts_with(crate::engine::CONTAINER_PREFIX);
                return PortCheck {
                    port,
                    required_by: service,
                    state: if ours { State::Ok } else { State::Fail },
                    process: Some(container.clone()),
                    pid: None,
                    ours,
                };
            }

            // A docker backend holding a port the engine does not account for
            // usually means the engine could not be asked; stay honest about
            // what is known rather than blaming "com.docker.backend".
            let vague = listener.process.as_deref().is_some_and(is_docker_backend);
            PortCheck {
                port,
                required_by: service,
                state: if vague { State::Warn } else { State::Fail },
                process: listener.process.clone(),
                pid: listener.pid,
                ours: false,
            }
        })
        .collect()
}

// --------------------------------------------------------------- generated

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedStatus {
    /// `Ok` fresh · `Warn` an input is newer than the output · `Fail` never
    /// generated · `Unknown` no workspace.
    pub state: State,
    /// The file that makes it stale or missing — the thing to show.
    pub detail: Option<String>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Has one project's manifest been edited since anything was generated from it?
///
/// The per-project half of `generated_status`, and the honest version of a
/// question the UI used to answer from events: it accumulated every
/// `manifest:changed` the watcher emitted into a "needs regenerating" badge.
/// The watcher cannot tell whose write it saw, so the app writing a manifest
/// during `project_create` — and regenerating from it half a second later —
/// lit the badge on a project that had just been generated. Answered from the
/// two timestamps instead, it is true when it is true, whoever wrote the file.
///
/// Compared against the OLDEST output, so a compose file that was rewritten
/// cannot mask a Dockerfile that was not. A project with no output at all has
/// never been generated, which counts as stale.
///
/// Outputs that are absent are skipped rather than counted as missing, because
/// only one of the two is common to every project: the compose entry. A PHP
/// project's Dockerfile is rendered into `generated/projects/<name>/`, while
/// every snapshot runtime builds from its own source directory and has no file
/// there at all (C-19). Filtering rather than encoding that rule keeps the
/// generator's layout in one place — the generator.
pub fn project_generated_is_stale(root: &Path, name: &str) -> bool {
    let Some(manifest) = crate::workspace::projects_root(root)
        .map(|p| p.join(name).join("stackvo.json"))
        .and_then(|p| mtime(&p))
    else {
        // No manifest is not a staleness problem; it is not a project.
        return false;
    };

    let outputs = [
        root.join("generated/docker-compose.projects.yml"),
        root.join("generated/projects")
            .join(name)
            .join("Dockerfile"),
    ];

    let Some(oldest_output) = outputs.iter().filter_map(|p| mtime(p)).min() else {
        return true;
    };

    manifest > oldest_output
}

/// Is `generated/` older than the inputs it was derived from?
///
/// Inputs: `.env` and every project's `stackvo.json`. Outputs: the compose
/// files the generator writes. The comparison is oldest-output against
/// newest-input, so one regenerated file cannot mask another stale one.
pub fn generated_status(root: &Path) -> GeneratedStatus {
    let core = root.join("generated/stackvo.yml");
    if mtime(&core).is_none() {
        return GeneratedStatus {
            state: State::Fail,
            detail: Some("generated/stackvo.yml".into()),
        };
    }

    let outputs = [
        "generated/stackvo.yml",
        "generated/docker-compose.dynamic.yml",
        "generated/docker-compose.projects.yml",
    ];
    let Some(oldest_output) = outputs.iter().filter_map(|f| mtime(&root.join(f))).min() else {
        return GeneratedStatus {
            state: State::Fail,
            detail: Some("generated/stackvo.yml".into()),
        };
    };

    let mut newest_input: Option<(SystemTime, String)> = None;
    let mut consider = |path: &Path, label: String| {
        if let Some(t) = mtime(path) {
            if newest_input.as_ref().is_none_or(|(n, _)| t > *n) {
                newest_input = Some((t, label));
            }
        }
    };

    consider(&root.join(".env"), ".env".into());
    if let Some(entries) =
        crate::workspace::projects_root(root).and_then(|p| std::fs::read_dir(p).ok())
    {
        for entry in entries.flatten() {
            let manifest = entry.path().join("stackvo.json");
            if manifest.is_file() {
                consider(
                    &manifest,
                    format!(
                        "projects/{}/stackvo.json",
                        entry.file_name().to_string_lossy()
                    ),
                );
            }
        }
    }

    match newest_input {
        Some((t, label)) if t > oldest_output => GeneratedStatus {
            state: State::Warn,
            detail: Some(label),
        },
        _ => GeneratedStatus {
            state: State::Ok,
            detail: None,
        },
    }
}

// ------------------------------------------------------------------ report

/// An extension selection that cannot build where it is being asked to.
///
/// Found by running `tools/validate-contracts.mjs` and reading its output
/// instead of the summary line. Three of that tool's four errors on this
/// checkout are `C-06`, all one root cause — `imap`, removed in PHP 8.2, in
/// two project manifests *and* in the stack's default extension set — and every
/// one of them is local and fixable. The desktop app reported none of them,
/// because the doctor knew about ports, disk and hosts and nothing about
/// extensions.
///
/// It matters more than a lint: the Bash generator **skips a bad extension
/// silently**. Nothing fails, nothing is logged, and the container comes up
/// without it — so the symptom is a fatal `Call to undefined function
/// imap_open()` at runtime, with nothing anywhere connecting it to a build that
/// reported success.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionProblem {
    /// The project name, or `.env` for the stack's default selection.
    pub subject: String,
    /// The PHP version the extension was measured against.
    pub php_version: String,
    pub extension: String,
    pub detail: String,
    /// True for the default set, which is the worse case: it means a project
    /// created right now, with nothing customised, cannot build.
    pub is_default_set: bool,
}

/// Extensions that cannot build on the PHP version they are paired with.
///
/// Reuses [`crate::manifest::normalize`] for projects rather than restating the
/// rule — one implementation of "removed in", used by the validator, the
/// manifest editor and this.
pub fn extension_problems(root: &Path) -> Vec<ExtensionProblem> {
    use std::cmp::Ordering;

    let mut out = Vec::new();
    let matrix = &crate::contracts::php_extensions().extensions;

    // --- the default selection -------------------------------------------
    //
    // The default PHP version is itself contested — `DEFAULT_PHP_VERSION` and
    // `SUPPORTED_LANGUAGES_PHP_DEFAULT` disagree, which is CONFLICTS.md C-12.
    // Both are checked rather than one being picked, because an extension that
    // fails on either is a problem whichever key turns out to win.
    if let Ok(env) = crate::config::Env::load(root) {
        let mut versions: Vec<String> = ["SUPPORTED_LANGUAGES_PHP_DEFAULT", "DEFAULT_PHP_VERSION"]
            .iter()
            .filter_map(|key| env.get(key).map(str::to_string))
            .collect();
        versions.sort();
        versions.dedup();

        for name in env.list("SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT") {
            let Some(spec) = matrix.get(&name) else {
                continue;
            };
            for version in &versions {
                if let Some(removed) = &spec.removed_in {
                    if crate::contracts::cmp_php_version(version, removed) != Ordering::Less {
                        out.push(ExtensionProblem {
                            subject: ".env".into(),
                            php_version: version.clone(),
                            extension: name.clone(),
                            detail: format!("removed in PHP {removed}"),
                            is_default_set: true,
                        });
                    }
                }
                if let Some(min) = &spec.min_php {
                    if crate::contracts::cmp_php_version(version, min) == Ordering::Less {
                        out.push(ExtensionProblem {
                            subject: ".env".into(),
                            php_version: version.clone(),
                            extension: name.clone(),
                            detail: format!("needs PHP {min} or newer"),
                            is_default_set: true,
                        });
                    }
                }
            }
        }
    }

    // --- each project -----------------------------------------------------
    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return out;
    };

    for dir in dirs.flatten() {
        let path = dir.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !path.is_dir() || name.starts_with('.') {
            continue;
        }
        let file = path.join("stackvo.json");
        if !file.is_file() {
            continue;
        }
        let Ok(manifest) = crate::manifest::read(&file, name) else {
            continue;
        };
        let version = manifest
            .php
            .as_ref()
            .map(|p| p.version.clone())
            .unwrap_or_default();

        for finding in &manifest.errors {
            if finding.code != "C-06" && finding.code != "C-07" {
                continue;
            }
            // `php.extensions[imap]` → `imap`.
            let extension = finding
                .path
                .rsplit_once('[')
                .and_then(|(_, rest)| rest.strip_suffix(']'))
                .unwrap_or(&finding.path)
                .to_string();

            out.push(ExtensionProblem {
                subject: name.to_string(),
                php_version: version.clone(),
                extension,
                detail: finding.message.clone(),
                is_default_set: false,
            });
        }
    }

    // The default set first: it is the one that breaks a project nobody has
    // touched yet.
    out.sort_by(|a, b| {
        b.is_default_set
            .cmp(&a.is_default_set)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.extension.cmp(&b.extension))
    });
    out
}

/// Take one extension out of one selection.
///
/// **This changes nothing about what runs**, and that is the point worth
/// stating before anyone presses the button. The generator already drops the
/// extension — silently, which is the bug — so it is already absent from every
/// built container. Removing it from the manifest does not remove a capability;
/// it stops the manifest claiming one the container never had.
///
/// The catalog leaves no alternative to offer: `imap` is `install: core` with
/// `removedIn: 8.2` and no PECL package, so on PHP 8.2 or newer there is
/// nothing StackVo could install instead. If the project genuinely needs it,
/// the answer is an older PHP version, which is a decision for the person who
/// owns the project and not a button.
///
/// Writes through the same paths the rest of the app uses — `manifest::write`
/// for a project, `env_writer` for `.env`, which backs the file up and
/// serialises against the other writers.
pub fn drop_extension(root: &Path, subject: &str, extension: &str) -> crate::error::Result<()> {
    use crate::error::{Code, Error};

    if subject == ".env" {
        let env = crate::config::Env::load(root)?;
        const KEY: &str = "SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT";

        let kept: Vec<String> = env
            .list(KEY)
            .into_iter()
            .filter(|name| !name.eq_ignore_ascii_case(extension))
            .collect();

        // The contract's own parsing rules: comma-separated, no spaces after
        // the comma. A list written any other way is read back wrong.
        let patch = std::collections::BTreeMap::from([(KEY.to_string(), kept.join(","))]);
        return crate::env_writer::apply(root, &patch).map(|_| ());
    }

    let dir = crate::workspace::project_dir(root, subject)?;
    let file = dir.join("stackvo.json");
    let mut manifest = crate::manifest::read(&file, subject)?;

    let Some(php) = manifest.php.as_mut() else {
        return Err(Error::new(
            Code::Unsupported,
            format!("{subject} has no PHP block"),
        ));
    };

    let before = php.extensions.len();
    php.extensions
        .retain(|name| !name.eq_ignore_ascii_case(extension));
    if php.extensions.len() == before {
        // Already gone — someone else got there, or the panel is stale. Not an
        // error: the state the caller wanted is the state on disk.
        return Ok(());
    }

    crate::manifest::write(&file, &manifest)
}

/// A container the stack does not work without.
///
/// Traefik is the whole of this set today: every project and service domain is
/// routed through it, and with it down nothing answers by name — which is the
/// single most common way a working install looks broken.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreContainer {
    /// The compose service name, as the generated file declares it.
    pub service: String,
    /// What the engine calls it — `stackvo-traefik`.
    pub container: String,
    pub state: State,
    /// Whether a container object exists at all.
    ///
    /// Separate from `running` because the two need different sentences and the
    /// settings pane conflated them: `compose down` removes containers, so
    /// "stopped" and "never created" both showed as one red chip, and somebody
    /// went looking for a stopped container that was not there.
    pub exists: bool,
    pub running: bool,
    /// The image, when there is a container to read one from.
    pub image: Option<String>,
}

/// Which core containers exist, and which are up.
///
/// Read from `generated/stackvo.yml` rather than from a name written here: the
/// generated file is what `compose up` executes, so it is also the honest
/// answer to "what does core mean in this workspace".
pub async fn core_containers(root: Option<&Path>, engine_up: bool) -> Vec<CoreContainer> {
    let Some(root) = root else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(root.join("generated/stackvo.yml")) else {
        // Nothing generated yet. The `generated` row already says so, and a
        // second row repeating it as "traefik is missing" would send someone
        // after the wrong repair.
        return Vec::new();
    };

    // With the engine down there is no answer to "is it running", and reporting
    // one as "no" sends the user to start a stack when the problem is Docker.
    let containers = if engine_up {
        crate::engine::stackvo_containers().await.ok()
    } else {
        None
    };

    core_services(&text)
        .into_iter()
        .map(|service| {
            let found = containers.as_ref().map(|c| c.get(&service));
            let (state, exists, running, image) = match found {
                None => (State::Unknown, false, false, None),
                Some(None) => (State::Fail, false, false, None),
                Some(Some(info)) => (
                    if info.running { State::Ok } else { State::Fail },
                    true,
                    info.running,
                    info.image.clone(),
                ),
            };
            CoreContainer {
                container: crate::engine::container_name(&service),
                service,
                state,
                exists,
                running,
                image,
            }
        })
        .collect()
}

/// The full report: the boot gate's rows plus everything above.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doctor {
    pub preflight: crate::preflight::Preflight,
    /// The containers the stack is addressed through — Traefik today.
    ///
    /// New because the gate does not cover it and nothing else did either: the
    /// requirements screen answers "can this run", the ports section answers
    /// "is 80 free", and between the two there was no row for "the proxy every
    /// domain resolves through is not running".
    pub core: Vec<CoreContainer>,
    pub ports: Vec<PortCheck>,
    /// Project domains with no hosts entry. Repaired through the reviewed
    /// `hosts_plan` / `hosts_apply` flow, never blindly.
    pub hosts_missing: Vec<String>,
    pub generated: GeneratedStatus,
    /// Unused image/volume counts and bytes; `None` with the engine down.
    pub space: Option<crate::engine::SystemResources>,
    /// Extensions that will be dropped from a build without anyone being told.
    pub extensions: Vec<ExtensionProblem>,
}

/// Assemble the whole report. Shared by the IPC command and the MCP tool.
///
/// The root is optional on purpose: with no workspace the doctor still
/// reports the gate rows, and everything root-derived reads as empty or
/// unknown rather than erroring the whole screen away.
pub async fn run(root: Option<&Path>) -> Doctor {
    let preflight = crate::preflight::run().await;
    let engine_up = preflight
        .requirements
        .iter()
        .any(|r| r.id == "engine" && r.state == State::Ok);

    let (required, generated) = match root {
        Some(root) => (required_ports(root), generated_status(root)),
        None => (
            Vec::new(),
            GeneratedStatus {
                state: State::Unknown,
                detail: None,
            },
        ),
    };

    let table = listeners().await;
    let owners = if engine_up {
        crate::engine::port_owners().await.ok()
    } else {
        None
    };
    let ports = check_ports(required, table.as_ref(), owners.as_ref());

    // The same list the dashboard offers to fix. It used to be computed here
    // from projects alone, so the panel opened when something is wrong knew
    // about fewer broken domains than the page that is working fine.
    let hosts_missing = match root {
        Some(root) => crate::commands::missing_hosts(root).await,
        None => Vec::new(),
    };

    let space = if engine_up {
        crate::engine::system_resources().await.ok()
    } else {
        None
    };

    Doctor {
        core: core_containers(root, engine_up).await,
        preflight,
        ports,
        hosts_missing,
        generated,
        space,
        // Reads files only, so it survives the engine being down — which is
        // exactly when somebody is reading this page.
        extensions: root.map(extension_problems).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace with the failure this check exists for: `imap` in the
    /// default selection, on a PHP version where it no longer exists.
    fn workspace_with_imap(dir: &Path) {
        // The project tree is chosen, never defaulted; a test root says where
        // its own is, like a real one does.
        crate::workspace::point_at_projects(dir, &dir.join("projects")).unwrap();
        let shop = dir.join("projects").join("shop");
        std::fs::create_dir_all(&shop).unwrap();
        std::fs::write(
            dir.join(".env"),
            "DEFAULT_PHP_VERSION=8.2\n\
             SUPPORTED_LANGUAGES_PHP_DEFAULT=8.4\n\
             SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT=mbstring,imap,gd\n",
        )
        .unwrap();
        std::fs::write(
            shop.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["mbstring","imap"]}}"#,
        )
        .unwrap();
    }

    /// The gap this closes: the validator reported three `C-06` errors on the
    /// real checkout and the desktop app showed none of them, because the
    /// doctor knew about ports, disk and hosts and nothing about extensions.
    #[test]
    fn an_extension_that_cannot_build_is_reported_for_the_stack_and_the_project() {
        let dir = std::env::temp_dir().join("stackvo-doctor-ext-test");
        let _ = std::fs::remove_dir_all(&dir);
        workspace_with_imap(&dir);

        let found = extension_problems(&dir);

        // Every one of them is imap, which is the point: one root cause,
        // reported everywhere it bites.
        assert!(found.iter().all(|p| p.extension == "imap"), "{found:?}");

        // The default set comes first — it is the case that breaks a project
        // nobody has touched yet.
        assert!(found[0].is_default_set);
        assert_eq!(found[0].subject, ".env");

        // Both candidate default versions are checked, because the two keys
        // disagree (C-12) and an extension failing on either is a problem
        // whichever one turns out to win.
        let versions: Vec<&str> = found
            .iter()
            .filter(|p| p.is_default_set)
            .map(|p| p.php_version.as_str())
            .collect();
        assert!(versions.contains(&"8.2"), "{versions:?}");
        assert!(versions.contains(&"8.4"), "{versions:?}");

        // And the project's own manifest.
        let project = found.iter().find(|p| p.subject == "shop").expect("project");
        assert!(!project.is_default_set);
        assert!(
            project.detail.contains("skips it silently"),
            "{}",
            project.detail
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The repair, end to end: the finding must disappear because the files
    /// changed, not because the check stopped looking.
    #[test]
    fn dropping_the_extension_clears_the_finding_from_both_places() {
        let dir = std::env::temp_dir().join("stackvo-doctor-ext-fix-test");
        let _ = std::fs::remove_dir_all(&dir);
        workspace_with_imap(&dir);

        assert!(!extension_problems(&dir).is_empty());

        drop_extension(&dir, ".env", "imap").expect("drop from the default set");
        drop_extension(&dir, "shop", "imap").expect("drop from the manifest");

        assert!(
            extension_problems(&dir).is_empty(),
            "{:?}",
            extension_problems(&dir)
        );

        // The rest of each selection survives — a repair that emptied the list
        // would also clear the finding.
        let env = crate::config::Env::load(&dir).unwrap();
        let kept = env.list("SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT");
        assert_eq!(kept, ["mbstring", "gd"], "the other defaults were lost");

        let manifest = crate::manifest::read(
            &dir.join("projects").join("shop").join("stackvo.json"),
            "shop",
        )
        .unwrap();
        assert_eq!(
            manifest.php.as_ref().unwrap().extensions,
            ["mbstring"],
            "the other extensions were lost"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Removing something already gone is the state the caller asked for, not
    /// an error — the panel can be a moment stale.
    #[test]
    fn dropping_twice_is_not_an_error() {
        let dir = std::env::temp_dir().join("stackvo-doctor-ext-twice-test");
        let _ = std::fs::remove_dir_all(&dir);
        workspace_with_imap(&dir);

        drop_extension(&dir, "shop", "imap").unwrap();
        drop_extension(&dir, "shop", "imap").expect("the second call is a no-op");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A selection that is fine must produce nothing, or the panel cries wolf
    /// and stops being read.
    #[test]
    fn a_valid_selection_reports_nothing() {
        let dir = std::env::temp_dir().join("stackvo-doctor-ext-ok-test");
        let _ = std::fs::remove_dir_all(&dir);
        let shop = dir.join("projects").join("shop");
        std::fs::create_dir_all(&shop).unwrap();
        std::fs::write(
            dir.join(".env"),
            "DEFAULT_PHP_VERSION=8.4\n\
             SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT=mbstring,gd\n",
        )
        .unwrap();
        std::fs::write(
            shop.join("stackvo.json"),
            r#"{"name":"shop","domain":"shop.loc","runtime":"php",
                "php":{"version":"8.4","extensions":["mbstring","gd"]}}"#,
        )
        .unwrap();

        assert!(extension_problems(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    const COMPOSE: &str = r#"
services:

  traefik:
    image: "traefik:v3.1"
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - "/var/run/docker.sock:/var/run/docker.sock:ro"

  mysql:
    image: "mysql:8.0"
    ports:
      - "3306:3306"

  pinned:
    ports:
      - "127.0.0.1:8081:80"
      - "9000"
      - "6379:6379/tcp"
"#;

    /// The shipped template's exact line, trailing comment and all — that
    /// comment is what a naive `contains("core")` check would still pass on
    /// while a `starts_with` on the value would fail.
    const PROFILED: &str = r#"
services:

  traefik:
    profiles: ["core"] # Core service - starts automatically in minimal setup
    image: "traefik:latest"

  mysql:
    profiles: ["services"]
    image: "mysql:8.0"

  blockstyle:
    profiles:
      - core
    image: "whatever"

  mongo-express:
    profiles: ["services", "mongo-express"]
    image: "mongo-express"
"#;

    #[test]
    fn the_core_profile_is_read_from_the_generated_file() {
        assert_eq!(
            core_services(PROFILED),
            vec!["traefik".to_string(), "blockstyle".to_string()],
            "the core set is what the file says, not a name written in Rust"
        );
    }

    /// The failure this guards: a substring match. `core` appears inside
    /// `mongo-express`'s comment-free profile list nowhere, but "scorecard" or
    /// a service literally named in a comment would sail through a
    /// `contains("core")`, and the doctor would then report a container that
    /// does not belong to the core profile as missing.
    #[test]
    fn a_profile_that_merely_contains_the_word_is_not_the_core_profile() {
        const NEAR: &str = r#"
services:

  scorecard:
    profiles: ["scorecard"]
    image: "x"

  commented:
    profiles: ["services"] # not core, whatever this comment says about core
    image: "y"
"#;
        assert!(core_services(NEAR).is_empty());
    }

    /// A workspace that has never generated has no compose file, and the
    /// `generated` row already says so. A second row claiming Traefik is
    /// missing would send someone to start a stack that cannot be started yet.
    #[tokio::test]
    async fn nothing_generated_reports_no_core_containers() {
        let dir = std::env::temp_dir().join(format!("stackvo-doctor-core-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        assert!(core_containers(Some(&dir), false).await.is_empty());
        assert!(core_containers(None, true).await.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// With the engine down there is no answer to "is it running", and `Fail`
    /// would send the user to start a stack when the problem is Docker.
    #[tokio::test]
    async fn the_engine_being_down_makes_the_answer_unknown_not_failed() {
        let dir =
            std::env::temp_dir().join(format!("stackvo-doctor-core-down-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(dir.join("generated/stackvo.yml"), PROFILED).unwrap();

        let found = core_containers(Some(&dir), false).await;
        assert_eq!(found.len(), 2);
        for c in &found {
            assert_eq!(c.state, State::Unknown, "{} was not unknown", c.service);
            assert!(!c.exists && !c.running);
        }
        assert_eq!(found[0].container, "stackvo-traefik");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn compose_ports_reads_the_generator_shape() {
        let ports = compose_ports(COMPOSE);
        assert_eq!(
            ports,
            vec![
                ("traefik".to_string(), 80),
                ("traefik".to_string(), 443),
                ("mysql".to_string(), 3306),
                ("pinned".to_string(), 8081),
                ("pinned".to_string(), 6379),
            ]
        );
    }

    #[test]
    fn host_port_reads_every_published_shape_and_skips_container_only() {
        assert_eq!(host_port("\"80:80\""), Some(80));
        assert_eq!(host_port("\"127.0.0.1:8080:80\""), Some(8080));
        assert_eq!(host_port("\"6379:6379/tcp\""), Some(6379));
        assert_eq!(host_port("\"9000\""), None);
    }

    #[test]
    fn lsof_table_maps_port_to_process() {
        let out = "COMMAND     PID USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\n\
                   com.docke  1234 user   99u  IPv6 0x0      0t0  TCP *:80 (LISTEN)\n\
                   nginx      4321 user   12u  IPv4 0x0      0t0  TCP 127.0.0.1:8081 (LISTEN)\n";
        let map = parse_lsof(out);
        assert_eq!(map[&80].process.as_deref(), Some("com.docke"));
        assert_eq!(map[&80].pid, Some(1234));
        assert_eq!(map[&8081].process.as_deref(), Some("nginx"));
    }

    #[test]
    fn ss_table_reads_port_name_and_pid_and_survives_missing_users() {
        let out = "LISTEN 0 4096 0.0.0.0:80 0.0.0.0:* users:((\"docker-proxy\",pid=123,fd=4))\n\
                   LISTEN 0 511  0.0.0.0:5432 0.0.0.0:*\n";
        let map = parse_ss(out);
        assert_eq!(map[&80].process.as_deref(), Some("docker-proxy"));
        assert_eq!(map[&80].pid, Some(123));
        assert!(map[&5432].process.is_none());
    }

    #[test]
    fn netstat_table_reads_listening_rows_only() {
        let out = "  TCP    0.0.0.0:80     0.0.0.0:0    LISTENING    4712\n\
                   TCP    127.0.0.1:9000 0.0.0.0:0    TIME_WAIT    0\n";
        let map = parse_netstat(out);
        assert_eq!(map[&80].pid, Some(4712));
        assert!(!map.contains_key(&9000));
    }

    #[test]
    fn a_free_port_is_ok_and_an_unreadable_table_is_unknown() {
        let required = vec![("traefik".to_string(), 80)];
        let free = check_ports(required.clone(), Some(&HashMap::new()), None);
        assert_eq!(free[0].state, State::Ok);

        let unknown = check_ports(required, None, None);
        assert_eq!(unknown[0].state, State::Unknown);
    }

    #[test]
    fn our_container_is_ok_and_a_foreign_one_is_a_named_conflict() {
        let mut table = HashMap::new();
        table.insert(
            80,
            Listener {
                process: Some("com.docker.backend".into()),
                pid: Some(1),
            },
        );
        let mut owners = HashMap::new();
        owners.insert(80, "stackvo-traefik".to_string());

        let ours = check_ports(
            vec![("traefik".to_string(), 80)],
            Some(&table),
            Some(&owners),
        );
        assert_eq!(ours[0].state, State::Ok);
        assert!(ours[0].ours);

        owners.insert(80, "someone-elses-nginx".to_string());
        let theirs = check_ports(
            vec![("traefik".to_string(), 80)],
            Some(&table),
            Some(&owners),
        );
        assert_eq!(theirs[0].state, State::Fail);
        assert_eq!(theirs[0].process.as_deref(), Some("someone-elses-nginx"));
    }

    #[test]
    fn a_host_process_is_a_named_conflict_and_a_bare_backend_is_a_warning() {
        let mut table = HashMap::new();
        table.insert(
            80,
            Listener {
                process: Some("nginx".into()),
                pid: Some(4321),
            },
        );
        let named = check_ports(vec![("traefik".to_string(), 80)], Some(&table), None);
        assert_eq!(named[0].state, State::Fail);
        assert_eq!(named[0].process.as_deref(), Some("nginx"));
        assert_eq!(named[0].pid, Some(4321));

        // The docker backend without an engine answer: a container holds it,
        // but whose is unknown — that is a warning, not a verdict.
        table.insert(
            80,
            Listener {
                process: Some("com.docker.backend".into()),
                pid: Some(1),
            },
        );
        let vague = check_ports(vec![("traefik".to_string(), 80)], Some(&table), None);
        assert_eq!(vague[0].state, State::Warn);
    }

    /// The badge that appeared on every newly created project.
    ///
    /// It was an accumulator of watcher events, and the watcher cannot tell
    /// whose write it saw — so the app writing the manifest during
    /// `project_create` lit it, and the regenerate that followed did not put
    /// it out. Measured from timestamps it is simply true or false.
    #[test]
    fn a_project_is_stale_only_when_its_manifest_outlives_the_output() {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-project-stale-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects/app")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        // A directory with no manifest is not a project, so not stale either.
        assert!(!project_generated_is_stale(&dir, "app"));

        // A manifest and nothing generated from it: stale.
        std::fs::write(dir.join("projects/app/stackvo.json"), "{}").unwrap();
        assert!(project_generated_is_stale(&dir, "app"));

        // Generated after it — the state every create ends in, and the one
        // that used to show the badge anyway.
        std::fs::create_dir_all(dir.join("generated/projects/app")).unwrap();
        std::fs::write(dir.join("generated/docker-compose.projects.yml"), "x").unwrap();
        std::fs::write(dir.join("generated/projects/app/Dockerfile"), "FROM x").unwrap();
        assert!(!project_generated_is_stale(&dir, "app"));

        // Edited afterwards: stale again. Set explicitly rather than slept
        // over, because mtime granularity is a whole second on some
        // filesystems.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join("projects/app/stackvo.json"))
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(project_generated_is_stale(&dir, "app"));

        // A snapshot runtime keeps its Dockerfile in its own source directory
        // (C-19), so there is nothing under `generated/projects/<name>`. That
        // absence must not read as "never generated" — the compose entry is
        // the output every project has.
        //
        // The compose mtime is set past the manifest's rather than just
        // rewritten: the manifest above was stamped into the future, and a
        // file written "now" is still older than that.
        std::fs::remove_dir_all(dir.join("generated/projects/app")).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join("generated/docker-compose.projects.yml"))
            .unwrap()
            .set_modified(later + std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            !project_generated_is_stale(&dir, "app"),
            "a node project has no generated Dockerfile and is not stale for it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generated_status_reports_missing_stale_and_fresh() {
        let dir = std::env::temp_dir().join(format!("stackvo-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects/app")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        // Never generated.
        assert_eq!(generated_status(&dir).state, State::Fail);

        // Fresh: outputs newer than inputs.
        std::fs::write(dir.join(".env"), "A=1").unwrap();
        std::fs::write(dir.join("projects/app/stackvo.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(dir.join("generated/stackvo.yml"), "services:").unwrap();
        let fresh = generated_status(&dir);
        assert_eq!(fresh.state, State::Ok, "detail: {:?}", fresh.detail);

        // Stale: an input touched after generation. Mtime granularity on some
        // filesystems is a full second, so set it explicitly instead of
        // sleeping across the boundary.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join("projects/app/stackvo.json"))
            .unwrap();
        file.set_modified(later).unwrap();
        let stale = generated_status(&dir);
        assert_eq!(stale.state, State::Warn);
        assert_eq!(stale.detail.as_deref(), Some("projects/app/stackvo.json"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
