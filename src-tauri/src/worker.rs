//! Queue and scheduler workers per project, with Docker doing the healing.
//!
//! A Laravel app in production runs three long processes beside the web
//! server — `queue:work`, the scheduler, Horizon — and locally they usually
//! run in a forgotten terminal tab, which is why "my job never ran" is a
//! support staple. Here each worker is a **sidecar container built from the
//! project's own image**: same PHP, same extensions, same bind mount, same
//! network, so `.env` and the database resolve exactly as they do for the web
//! container.
//!
//! Self-healing is deliberately not reimplemented: the sidecar runs with
//! `--restart unless-stopped`, which is Docker's own supervisor. A worker
//! that dies is restarted by the engine whether or not this app is open; the
//! restart count is read back and shown, so a crash-looping worker is a
//! number on screen rather than a mystery.
//!
//! Detection is file-based and honest: `artisan` at the project root offers
//! the queue and scheduler workers; `laravel/horizon` in `composer.json`
//! offers Horizon. No artisan, no offer — a Node project gets an empty list,
//! not an error.

use serde::Serialize;
use std::path::Path;

/// Sidecar containers are `stackvo-worker-<project>-<kind>`.
pub const ID_PREFIX: &str = "worker-";

/// One worker kind this project could run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Queue,
    Scheduler,
    Horizon,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Queue, Kind::Scheduler, Kind::Horizon];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Queue => "queue",
            Kind::Scheduler => "scheduler",
            Kind::Horizon => "horizon",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The process the sidecar runs.
    ///
    /// `queue:work` restarts hourly on purpose: a worker holds the code it
    /// booted with, and an hour is the ceiling on serving yesterday's code
    /// after an edit. `schedule:work` is Laravel's own foreground scheduler —
    /// no host cron entry, nothing to clean up.
    pub fn command(self) -> &'static [&'static str] {
        match self {
            Kind::Queue => &[
                "php",
                "artisan",
                "queue:work",
                "--sleep=3",
                "--tries=3",
                "--max-time=3600",
            ],
            Kind::Scheduler => &["php", "artisan", "schedule:work"],
            Kind::Horizon => &["php", "artisan", "horizon"],
        }
    }
}

/// The engine-facing id of one worker sidecar.
pub fn container_id(project: &str, kind: Kind) -> String {
    format!("{ID_PREFIX}{project}-{}", kind.as_str())
}

/// `worker-<project>-<kind>` back into its parts. The kind is matched as a
/// suffix because project names may themselves contain `-`.
pub fn parse_id(id: &str) -> Option<(String, Kind)> {
    let rest = id.strip_prefix(ID_PREFIX)?;
    for kind in Kind::ALL {
        if let Some(project) = rest.strip_suffix(&format!("-{}", kind.as_str())) {
            if !project.is_empty() {
                return Some((project.to_string(), kind));
            }
        }
    }
    None
}

/// Which workers this project can offer, from its files alone.
pub fn available(root: &Path, project: &str) -> Vec<Kind> {
    let Some(projects) = crate::workspace::projects_root(root) else {
        return Vec::new();
    };
    let dir = projects.join(project);
    if !dir.join("artisan").is_file() {
        return Vec::new();
    }

    let mut out = vec![Kind::Queue, Kind::Scheduler];

    let horizon = std::fs::read_to_string(dir.join("composer.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|json| {
            json.get("require")
                .and_then(|r| r.get("laravel/horizon"))
                .is_some()
        });
    if horizon {
        out.push(Kind::Horizon);
    }
    out
}

/// One worker's live state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    pub project: String,
    pub kind: Kind,
    pub running: bool,
    /// How often Docker has had to bring it back — the self-heal made
    /// visible. A large number is a crash loop, not a success story.
    pub restarts: Option<i64>,
    pub container: String,
}

/// Every worker sidecar the engine knows about.
pub async fn status_all() -> crate::error::Result<Vec<WorkerStatus>> {
    let containers = crate::engine::stackvo_containers().await?;
    let mut out = Vec::new();

    for (id, info) in containers {
        let Some((project, kind)) = parse_id(&id) else {
            continue;
        };
        // The restart count lives in inspect, not the list; workers are few.
        let restarts = crate::engine::inspect(&id)
            .await
            .ok()
            .map(|d| d.restart_count);

        out.push(WorkerStatus {
            project,
            kind,
            running: info.running,
            restarts,
            container: info.name,
        });
    }

    out.sort_by(|a, b| (&a.project, a.kind.as_str()).cmp(&(&b.project, b.kind.as_str())));
    Ok(out)
}

/// The `docker run` invocation for one worker sidecar.
///
/// `image` and the bind mount come from the project's own web container, so
/// the worker sees exactly what the site sees. `--restart unless-stopped` is
/// the whole self-heal story; `stop` (which removes) is the only way it stays
/// down.
pub fn run_args(
    project: &str,
    kind: Kind,
    image: &str,
    host_root: &str,
    network: &str,
) -> Vec<String> {
    let mount = format!(
        "{}/projects/{project}:/var/www/html",
        crate::paths::to_docker_mount(host_root).trim_end_matches('/')
    );

    let mut args: Vec<String> = [
        "run",
        "-d",
        "--name",
        &format!("stackvo-{}", container_id(project, kind)),
        "--network",
        network,
        "--restart",
        "unless-stopped",
        "-v",
        &mount,
        "-w",
        "/var/www/html",
        image,
    ]
    .into_iter()
    .map(String::from)
    .collect();

    args.extend(kind.command().iter().map(|s| s.to_string()));
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_even_when_the_project_name_has_dashes_and_dots() {
        for (project, kind) in [
            ("myapp", Kind::Queue),
            ("parser.ajans", Kind::Scheduler),
            ("api-v2.shop", Kind::Horizon),
        ] {
            let id = container_id(project, kind);
            assert_eq!(parse_id(&id), Some((project.to_string(), kind)));
        }
        assert_eq!(parse_id("worker-"), None);
        assert_eq!(parse_id("worker--queue"), None);
        assert_eq!(parse_id("tunnel-myapp"), None);
        // A project that merely ends in a kind word is not a worker.
        assert_eq!(parse_id("worker-queue"), None);
    }

    #[test]
    fn detection_offers_nothing_without_artisan_and_horizon_only_when_required() {
        let dir = std::env::temp_dir().join(format!("stackvo-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("projects/app");
        std::fs::create_dir_all(&project).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        assert!(available(&dir, "app").is_empty());

        std::fs::write(project.join("artisan"), "#!/usr/bin/env php").unwrap();
        assert_eq!(available(&dir, "app"), vec![Kind::Queue, Kind::Scheduler]);

        std::fs::write(
            project.join("composer.json"),
            r#"{ "require": { "php": "^8.2", "laravel/horizon": "^5.0" } }"#,
        )
        .unwrap();
        assert_eq!(
            available(&dir, "app"),
            vec![Kind::Queue, Kind::Scheduler, Kind::Horizon]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sidecar_runs_the_projects_image_with_docker_as_supervisor() {
        let args = run_args(
            "myapp",
            Kind::Queue,
            "stackvo-myapp:latest",
            "/Users/x/stackvo",
            "stackvo-net",
        );
        let line = args.join(" ");
        assert!(line.contains("--name stackvo-worker-myapp-queue"));
        assert!(line.contains("--restart unless-stopped"));
        assert!(line.contains("-v /Users/x/stackvo/projects/myapp:/var/www/html"));
        assert!(line.contains("stackvo-myapp:latest php artisan queue:work"));
        assert!(line.ends_with("--max-time=3600"));

        let horizon = run_args(
            "myapp",
            Kind::Horizon,
            "stackvo-myapp:latest",
            "/Users/x/stackvo",
            "stackvo-net",
        );
        assert!(horizon.join(" ").ends_with("php artisan horizon"));
    }
}
