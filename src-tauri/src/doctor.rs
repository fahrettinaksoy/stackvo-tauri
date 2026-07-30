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
    if let Ok(entries) = std::fs::read_dir(root.join("projects")) {
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

/// The full report: the boot gate's rows plus everything above.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doctor {
    pub preflight: crate::preflight::Preflight,
    pub ports: Vec<PortCheck>,
    /// Project domains with no hosts entry. Repaired through the reviewed
    /// `hosts_plan` / `hosts_apply` flow, never blindly.
    pub hosts_missing: Vec<String>,
    pub generated: GeneratedStatus,
    /// Unused image/volume counts and bytes; `None` with the engine down.
    pub space: Option<crate::engine::SystemResources>,
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

    let hosts_missing = match root {
        Some(root) => crate::commands::list_projects(root)
            .await
            .map(|projects| {
                projects
                    .into_iter()
                    .filter(|p| p.domain.is_some() && !p.domain_configured)
                    .filter_map(|p| p.domain)
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let space = if engine_up {
        crate::engine::system_resources().await.ok()
    } else {
        None
    };

    Doctor {
        preflight,
        ports,
        hosts_missing,
        generated,
        space,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn generated_status_reports_missing_stale_and_fresh() {
        let dir = std::env::temp_dir().join(format!("stackvo-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects/app")).unwrap();

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
