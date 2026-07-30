//! The IPC surface — see `contracts/ipc.json`.
//!
//! Every command returns `Result<T, Error>`. On success the payload crosses the
//! boundary directly; there is no `{ success, data }` envelope and therefore no
//! way to express the old "HTTP 200 with success:false" ambiguity.
//!
//! Phase 1 implements the read-only half; Phase 2 (below the marker near the
//! bottom of this file) adds the mutations. PTY lands with Phase 3.

use crate::applog;
use crate::certs;
use crate::config::Env;
use crate::contracts::{env_schema, php_extensions};
use crate::db;
use crate::detect;
use crate::engine::{self, ContainerInfo, EngineStatus, Port, SystemResources};
use crate::error::{Code, Error, Result};
use crate::hosts;
use crate::mail;
use crate::manifest::{self, Manifest};
use crate::stats::{HostStats, Sampler};
use crate::workspace::{self, Workspace};
use crate::xdebug;
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

/// container name -> (unix seconds, cpu %, memory %).
type StatsHistory = std::collections::HashMap<String, Vec<(u64, f64, f64)>>;

/// CPU percentages and network rates are deltas, so the sampler must persist
/// between calls. The workspace is cached to avoid re-walking the discovery
/// candidates on every command.
pub struct AppState {
    pub sampler: Mutex<Sampler>,
    pub workspace: Mutex<Workspace>,
    /// Live log tails, so `container_logs_close` can cancel the reader task
    /// instead of leaving it streaming into a window nobody is watching.
    pub log_streams: Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>,
    /// container name -> (timestamp, cpu %, memory %). Sampled in the
    /// background so the dashboard has history to draw on its first render.
    pub stats_history: Mutex<StatsHistory>,
    /// One operation per subject. The front end's busy flag is per view; this
    /// is the boundary that the tray, a second view and a shortcut all share.
    pub inflight: crate::inflight::Registry,
    /// Generation writes shared files — `docker-compose.projects.yml` and
    /// everything under `generated/`. Many commands regenerate as one of their
    /// steps, so these queue rather than fail: refusing a build because another
    /// command regenerated at that instant would be wrong, but letting two bash
    /// processes write the same compose file is worse. A Tokio mutex because it
    /// is held across the await.
    pub generate_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sampler: Mutex::new(Sampler::new()),
            workspace: Mutex::new(workspace::resolve()),
            log_streams: Mutex::new(std::collections::HashMap::new()),
            stats_history: Mutex::new(std::collections::HashMap::new()),
            inflight: crate::inflight::Registry::new(),
            generate_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn root(&self) -> Result<std::path::PathBuf> {
        self.workspace
            .lock()
            .map_err(|_| Error::new(crate::error::Code::IoError, "state lock poisoned"))?
            .require_root()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------- workspace

#[tauri::command]
pub fn workspace_get(state: State<'_, AppState>) -> Result<Workspace> {
    let ws = workspace::resolve();
    if let Ok(mut cached) = state.workspace.lock() {
        *cached = ws.clone();
    }
    Ok(ws)
}

#[tauri::command]
pub fn workspace_set(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
    watcher: State<'_, crate::watcher::Handle>,
) -> Result<Workspace> {
    let ws = workspace::set(&path)?;
    if let Ok(mut cached) = state.workspace.lock() {
        *cached = ws.clone();
    }
    // Move the file watcher with the workspace, or it keeps reporting changes
    // in the checkout the user just left.
    watcher.retarget(&app, ws.require_root().ok());
    Ok(ws)
}

// ---------------------------------------------------------------- engine

#[tauri::command]
pub async fn engine_status() -> Result<EngineStatus> {
    // Deliberately infallible: "Docker is down" is a displayable state, not an
    // error. Returning Err here would leave the UI unable to say why.
    Ok(engine::status().await)
}

#[tauri::command]
pub fn engine_start() -> Result<()> {
    engine::start()
}

// ---------------------------------------------------------------- metrics

#[tauri::command]
pub fn host_stats(state: State<'_, AppState>) -> Result<HostStats> {
    let mut sampler = state
        .sampler
        .lock()
        .map_err(|_| Error::new(crate::error::Code::IoError, "sampler lock poisoned"))?;
    Ok(sampler.sample())
}

#[tauri::command]
pub async fn docker_system_resources() -> Result<SystemResources> {
    engine::system_resources().await
}

/// Which stack member holds the bytes — the per-project answer the aggregate
/// numbers in `docker_system_resources` cannot give.
#[tauri::command]
pub async fn docker_disk_usage() -> Result<Vec<engine::DiskOwner>> {
    engine::disk_attribution().await
}

// ---------------------------------------------------------------- projects

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub domain: Option<String>,
    pub runtime: String,
    pub path: String,
    pub container_name: String,
    pub running: bool,
    pub built: bool,
    pub manifest: Manifest,
    /// Mirrors `manifest.valid`, hoisted so list views do not have to dig.
    pub manifest_valid: bool,
    pub domain_configured: bool,
    pub ports: Vec<Port>,
}

#[tauri::command]
pub async fn projects_list(state: State<'_, AppState>) -> Result<Vec<Project>> {
    let root = state.root()?;
    list_projects(&root).await
}

/// The command's logic, free of Tauri `State` so it can be exercised from tests
/// and from the `diagnose` example.
pub async fn list_projects(root: &std::path::Path) -> Result<Vec<Project>> {
    let projects_dir = root.join("projects");

    // A dead engine must not hide the project list — the manifests are on disk
    // and readable either way. Container state simply degrades to "not running".
    let containers = engine::stackvo_containers().await.unwrap_or_default();

    let entries = std::fs::read_dir(&projects_dir)
        .map_err(|e| Error::io(format!("reading {}", projects_dir.display()), e))?;

    let mut manifests = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if dir_name.starts_with('.') {
            continue;
        }
        let manifest_path = path.join("stackvo.json");
        if !manifest_path.is_file() {
            continue;
        }

        match manifest::read(&manifest_path, dir_name) {
            Ok(m) => manifests.push((dir_name.to_string(), path.clone(), m)),
            Err(e) => {
                // Unparseable JSON still yields a row, so a broken project is
                // visible instead of silently absent.
                manifests.push((
                    dir_name.to_string(),
                    path.clone(),
                    Manifest {
                        name: dir_name.to_string(),
                        domain: None,
                        runtime: "php".into(),
                        server: None,
                        document_root: None,
                        php: None,
                        node: None,
                        valid: false,
                        errors: vec![manifest::Finding {
                            code: "PARSE_ERROR".into(),
                            path: "stackvo.json".into(),
                            message: e.message,
                        }],
                        warnings: Vec::new(),
                    },
                ));
            }
        }
    }

    let domains: Vec<String> = manifests
        .iter()
        .filter_map(|(_, _, m)| m.domain.clone())
        .collect();
    let hosts_status = hosts::status_for(&domains);

    let mut out: Vec<Project> = manifests
        .into_iter()
        .map(|(dir_name, path, m)| {
            let container = containers.get(&dir_name);
            let domain_configured = m
                .domain
                .as_ref()
                .and_then(|d| hosts_status.iter().find(|h| &h.domain == d))
                .is_some_and(|h| h.configured);

            Project {
                container_name: format!("{}{}", engine::CONTAINER_PREFIX, dir_name),
                running: container.is_some_and(|c| c.running),
                built: container.is_some(),
                ports: container.map(|c| c.ports.clone()).unwrap_or_default(),
                path: path.display().to_string(),
                domain: m.domain.clone(),
                runtime: m.runtime.clone(),
                manifest_valid: m.valid,
                name: dir_name,
                manifest: m,
                domain_configured,
            }
        })
        .collect();

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

// ---------------------------------------------------------------- services

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    /// The key with its `SERVICE_<ID>_` prefix removed — `ROOT_PASSWORD`.
    pub key: String,
    /// The full `.env` key, so revealing one does not mean rebuilding the
    /// prefix transform in the frontend (CONFLICTS.md C-09 is about exactly
    /// that kind of round trip).
    pub env_key: String,
    /// Masked when `secret`; the real value comes from `env_reveal`.
    pub value: String,
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub category: String,
    pub enabled: bool,
    pub running: bool,
    pub built: bool,
    pub version: Option<String>,
    pub container_name: String,
    pub url: Option<String>,
    pub host_port: Option<u16>,
    pub ports: Vec<Port>,
    /// `SERVICE_<ID>_*` values, secrets masked. See `Env::service_credentials`.
    pub credentials: Vec<Credential>,
    pub required: Vec<String>,
    pub optional: Vec<String>,
    /// A required dependency that is not running. The web UI only knew about
    /// three services' dependencies, so it started admin UIs against nothing.
    pub unmet_dependencies: Vec<String>,
}

#[tauri::command]
pub async fn services_list(state: State<'_, AppState>) -> Result<Vec<Service>> {
    let root = state.root()?;
    list_services(&root).await
}

pub async fn list_services(root: &std::path::Path) -> Result<Vec<Service>> {
    let env = Env::load(root)?;
    let containers = engine::stackvo_containers().await.unwrap_or_default();
    let schema = env_schema();

    let is_running = |id: &str| {
        containers
            .get(id)
            .is_some_and(|c: &ContainerInfo| c.running)
    };

    let mut out: Vec<Service> = schema
        .service_catalog()
        .into_iter()
        .map(|(id, category)| {
            let deps = schema.dependencies_for(&id);
            let unmet: Vec<String> = deps
                .required
                .iter()
                .filter(|d| !is_running(d))
                .cloned()
                .collect();
            let container = containers.get(&id);

            Service {
                container_name: format!("{}{}", engine::CONTAINER_PREFIX, id),
                enabled: env.service_enabled(&id),
                running: container.is_some_and(|c| c.running),
                built: container.is_some(),
                version: env.service_version(&id).map(str::to_string),
                url: env.service_url(&id).map(str::to_string),
                host_port: env.service_host_port(&id),
                credentials: env
                    .service_credentials(&id)
                    .into_iter()
                    .map(|(key, value, secret)| Credential {
                        env_key: format!("{}{}", Env::service_prefix(&id), key),
                        key,
                        value,
                        secret,
                    })
                    .collect(),
                ports: container.map(|c| c.ports.clone()).unwrap_or_default(),
                required: deps.required,
                optional: deps.optional,
                unmet_dependencies: unmet,
                id,
                category,
            }
        })
        .collect();

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

// ---------------------------------------------------------------- catalog

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOption {
    pub id: String,
    pub versions: Vec<String>,
    pub default: Option<String>,
    /// False for the four runtimes `.env` advertises with no generator behind
    /// them (CONFLICTS.md C-02). The UI greys these out instead of offering a
    /// choice that silently produces nothing.
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionOption {
    pub name: String,
    pub install: String,
    pub in_default_set: bool,
    pub min_php: Option<String>,
    pub removed_in: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub runtimes: Vec<RuntimeOption>,
    pub servers: Vec<String>,
    pub default_server: String,
    pub php_extensions: Vec<ExtensionOption>,
    /// The Bash parser's hard ceiling on `php.extensions` (CONFLICTS.md C-04).
    /// Exposed so the picker can stop the user at 50 rather than let the
    /// generator drop the tail in silence.
    pub max_extensions: usize,
}

/// Only these two have generators under
/// `core/cli/lib/generators/project/{compose,dockerfile}/`.
const IMPLEMENTED_RUNTIMES: [&str; 2] = ["php", "node"];

#[tauri::command]
pub fn catalog_get(state: State<'_, AppState>) -> Result<Catalog> {
    let root = state.root()?;
    build_catalog(&root)
}

pub fn build_catalog(root: &std::path::Path) -> Result<Catalog> {
    let env = Env::load(root)?;

    let default_set: Vec<String> = env.list("SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT");

    let runtimes = env
        .list("SUPPORTED_LANGUAGES")
        .into_iter()
        .map(|lang| {
            // `.env` spells it nodejs; the manifest key is node (C-01).
            let id = if lang == "nodejs" {
                "node".to_string()
            } else {
                lang.clone()
            };
            let key = lang.to_uppercase();
            RuntimeOption {
                versions: env.list(&format!("SUPPORTED_LANGUAGES_{key}_VERSIONS")),
                default: env
                    .get(&format!("SUPPORTED_LANGUAGES_{key}_DEFAULT"))
                    .map(str::to_string),
                available: IMPLEMENTED_RUNTIMES.contains(&id.as_str()),
                id,
            }
        })
        .collect();

    let catalog_names = env.list("SUPPORTED_LANGUAGES_PHP_EXTENSIONS");
    let matrix = &php_extensions().extensions;
    let php_ext = catalog_names
        .into_iter()
        .filter_map(|name| {
            let spec = matrix.get(&name)?;
            Some(ExtensionOption {
                in_default_set: default_set.contains(&name),
                install: spec.install.clone(),
                min_php: spec.min_php.clone(),
                removed_in: spec.removed_in.clone(),
                name,
            })
        })
        .collect();

    Ok(Catalog {
        runtimes,
        servers: env
            .first_of(&["SUPPORTED_SERVERS", "SUPPORTED_WEBSERVERS"])
            .map_or_else(
                || vec!["nginx".to_string()],
                |v| v.split(',').map(|s| s.trim().to_string()).collect(),
            ),
        default_server: env
            .first_of(&["SUPPORTED_SERVERS_DEFAULT", "DEFAULT_SERVER"])
            .unwrap_or("nginx")
            .to_string(),
        php_extensions: php_ext,
        max_extensions: 50,
    })
}

// ---------------------------------------------------------------- env

#[tauri::command]
pub fn env_get(state: State<'_, AppState>) -> Result<std::collections::BTreeMap<String, String>> {
    let root = state.root()?;
    // Secret-suffixed values never cross the boundary; see env.schema.json.
    Ok(Env::load(&root)?.redacted())
}

// ================================================================ Phase 2
// Mutating commands. Everything above this line only reads.

use crate::env_writer;
use crate::events::{self, Lifecycle, SubjectEvent};
use crate::runner;
use tauri::AppHandle;

/// Shared body for the six start/stop/restart commands, which differ only by
/// verb, subject kind and event prefix.
#[tracing::instrument(skip(app, phase), fields(action = phase.pending))]
async fn lifecycle(app: &AppHandle, kind: &'static str, id: &str, phase: Lifecycle) -> Result<()> {
    // Validated even though no path is built here: the id becomes a container
    // name and a compose service name, and one rule applied at every entry
    // point is easier to keep true than five rules applied at some of them.
    // Service ids come from the catalog and are checked against it elsewhere.
    if kind == "project" {
        if !workspace::is_safe_name(id) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{id}\" is not a valid project name"),
            ));
        }
    } else {
        checked_service(id)?;
    }

    let subject = |ev: &str| format!("{kind}:{ev}");
    let make = |id: &str| {
        if kind == "project" {
            SubjectEvent::project(id)
        } else {
            SubjectEvent::service(id)
        }
    };

    events::emit(app, &subject(phase.pending), make(id));

    let result = match phase.pending {
        "starting" => engine::start_container(id).await,
        "stopping" => engine::stop_container(id).await,
        _ => engine::restart_container(id).await,
    };

    match result {
        Ok(()) => {
            events::emit(
                app,
                &subject(phase.done),
                make(id).running(phase.running_after),
            );
            Ok(())
        }
        Err(e) => {
            events::emit(app, &subject("error"), make(id).error(e.message.clone()));
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn project_start(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&app, "project", &name, events::START).await
}

#[tauri::command]
pub async fn project_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&app, "project", &name, events::STOP).await
}

#[tauri::command]
pub async fn project_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&app, "project", &name, events::RESTART).await
}

#[tauri::command]
pub async fn service_start(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&app, "service", &name, events::START).await
}

#[tauri::command]
pub async fn service_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&app, "service", &name, events::STOP).await
}

#[tauri::command]
pub async fn service_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&app, "service", &name, events::RESTART).await
}

// ---------------------------------------------------------------- inspect

#[tauri::command]
pub async fn container_inspect(name: String) -> Result<engine::ContainerDetails> {
    engine::inspect(&name).await
}

#[tauri::command]
pub async fn container_stats(name: String) -> Result<engine::ContainerStats> {
    engine::container_stats(&name).await
}

// ---------------------------------------------------------------- logs

#[tauri::command]
pub async fn container_logs_open(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    tail: Option<u32>,
    follow: Option<bool>,
) -> Result<String> {
    use futures_util::StreamExt;

    let stream_id = events::next_operation_id("logs");
    let stream = engine::logs_stream(&name, tail.unwrap_or(200), follow.unwrap_or(true))?;

    let handle = {
        let app = app.clone();
        let stream_id = stream_id.clone();
        let container = name.clone();

        tokio::spawn(async move {
            futures_util::pin_mut!(stream);
            while let Some(line) = stream.next().await {
                events::emit(
                    &app,
                    "logs:line",
                    events::LogLineEvent {
                        stream_id: stream_id.clone(),
                        container: container.clone(),
                        line: line.text,
                        stream: line.stream.to_string(),
                        source: None,
                    },
                );
            }
            // A non-following tail ends on its own; tell the UI so it can stop
            // showing a live indicator.
            events::emit(
                &app,
                "logs:closed",
                serde_json::json!({ "streamId": stream_id }),
            );
        })
        .abort_handle()
    };

    if let Ok(mut streams) = state.log_streams.lock() {
        streams.insert(stream_id.clone(), handle);
    }

    Ok(stream_id)
}

#[tauri::command]
pub fn container_logs_close(state: State<'_, AppState>, stream_id: String) -> Result<()> {
    if let Ok(mut streams) = state.log_streams.lock() {
        if let Some(handle) = streams.remove(&stream_id) {
            handle.abort();
        }
    }
    Ok(())
}

/// The log files this project writes, as opposed to what its container prints.
///
/// Deliberately engine-free: these are read from the host, and a container that
/// died during boot is exactly when its log matters and exactly when there is
/// nothing left to `docker exec` into.
#[tauri::command]
pub fn app_logs(state: State<'_, AppState>, name: String) -> Result<Vec<applog::LogFile>> {
    applog::candidates(&state.root()?, &name)
}

/// How often a followed file is checked for new bytes.
///
/// Polled rather than watched: a filesystem notification still only tells you
/// *that* something changed, so the read-the-delta path has to exist either
/// way, and one `stat` twice a second is cheaper than a watcher per open file.
const APP_LOG_POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// Follow one of those files, emitting the same events a container stream does.
///
/// The event shape is shared with `container_logs_open` on purpose: the viewer
/// renders one kind of line, and giving files their own event pair would have
/// meant a second listener in the frontend that could drift from the first.
#[tauri::command]
pub async fn app_log_open(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    id: String,
    tail_bytes: Option<u64>,
) -> Result<String> {
    let root = state.root()?;
    // Resolved before the task is spawned, so a bad id is an error the caller
    // gets rather than a stream that opens and immediately says nothing.
    let path = applog::resolve(&root, &name, &id)?;

    let stream_id = events::next_operation_id("applog");
    let (text, mut offset) = applog::tail(&path, tail_bytes.unwrap_or(64 * 1024))?;

    let handle = {
        let app = app.clone();
        let stream_id = stream_id.clone();
        let subject = name.clone();

        tokio::spawn(async move {
            let emit = |chunk: &str| {
                for line in chunk.lines() {
                    events::emit(
                        &app,
                        "logs:line",
                        events::LogLineEvent {
                            stream_id: stream_id.clone(),
                            container: subject.clone(),
                            line: line.to_string(),
                            // A file has one stream. Reporting stdout keeps the
                            // renderer's stderr colouring meaningful instead of
                            // painting whole files red.
                            stream: "stdout".to_string(),
                            // One file, named once when the stream opened.
                            source: None,
                        },
                    );
                }
            };

            emit(&text);

            loop {
                tokio::time::sleep(APP_LOG_POLL).await;
                match applog::read_since(&path, offset) {
                    Ok((chunk, next)) => {
                        offset = next;
                        if !chunk.is_empty() {
                            emit(&chunk);
                        }
                    }
                    // The file was deleted under us — a rotation that renamed
                    // rather than truncated. Stop rather than spin on an error
                    // once every poll for as long as the pane stays open.
                    Err(_) => break,
                }
            }

            events::emit(
                &app,
                "logs:closed",
                serde_json::json!({ "streamId": stream_id }),
            );
        })
        .abort_handle()
    };

    if let Ok(mut streams) = state.log_streams.lock() {
        streams.insert(stream_id.clone(), handle);
    }

    Ok(stream_id)
}

// --------------------------------------------------- across every project

/// Every log file every project writes, newest first.
///
/// The picker for the cross-project tail, and — because it needs no engine —
/// the one list in the app that is complete with Docker stopped.
#[tauri::command]
pub fn app_logs_all(state: State<'_, AppState>) -> Result<Vec<applog::ProjectLogFile>> {
    applog::candidates_all(&state.root()?)
}

/// How often the fanout re-discovers files.
///
/// Rediscovery is not free the way a `stat` is — it walks the log directories
/// of every project — so it runs on its own, much slower clock than the read.
/// Thirty seconds is the largest gap that still feels immediate when a daily
/// channel rolls over or a project is created while the pane is open.
const FANOUT_SCAN: std::time::Duration = std::time::Duration::from_secs(30);

/// Follow every project at once.
///
/// Live only, and deliberately: see `applog::Fanout`. Nothing here parses a
/// timestamp, so the only ordering this can honestly claim across files is the
/// order the bytes arrive in — true for new output, invented for old. History
/// stays in the per-project viewer, which reads one file and can show all of
/// it. Closed with `container_logs_close`, like every other stream.
#[tauri::command]
pub async fn app_logs_all_open(
    app: AppHandle,
    state: State<'_, AppState>,
    projects: Option<Vec<String>>,
) -> Result<FanoutStream> {
    let root = state.root()?;
    let only = projects.unwrap_or_default();

    // The first scan happens before the task is spawned, and its result is
    // *returned* rather than emitted. An event would race the caller: the task
    // can emit before the frontend has the stream id it filters events by, and
    // the coverage line would then stay blank until the next rediscovery
    // thirty seconds later. Only updates are events, because only updates have
    // somewhere to arrive.
    let mut fanout = applog::Fanout::new(&root);
    let first = fanout.scan(&only);

    let stream_id = events::next_operation_id("applog");

    let handle = {
        let app = app.clone();
        let stream_id = stream_id.clone();

        tokio::spawn(async move {
            let mut since_scan = std::time::Duration::ZERO;
            loop {
                tokio::time::sleep(APP_LOG_POLL).await;

                for line in fanout.poll() {
                    events::emit(
                        &app,
                        "logs:line",
                        events::LogLineEvent {
                            stream_id: stream_id.clone(),
                            container: line.project,
                            line: line.text,
                            stream: "stdout".to_string(),
                            source: Some(line.id),
                        },
                    );
                }

                since_scan += APP_LOG_POLL;
                if since_scan >= FANOUT_SCAN {
                    since_scan = std::time::Duration::ZERO;
                    // Scanned *after* the poll, so the files being dropped this
                    // round have already given up their last lines.
                    let scan = fanout.scan(&only);
                    events::emit(
                        &app,
                        "logs:sources",
                        serde_json::json!({
                            "streamId": stream_id,
                            "followed": scan.followed,
                            "total": scan.total,
                            "projects": scan.projects,
                        }),
                    );
                }
            }
        })
        .abort_handle()
    };

    if let Ok(mut streams) = state.log_streams.lock() {
        streams.insert(stream_id.clone(), handle);
    }

    Ok(FanoutStream {
        stream_id,
        followed: first.followed,
        total: first.total,
        projects: first.projects,
    })
}

/// An open fanout, with the coverage it starts on.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutStream {
    pub stream_id: String,
    /// Files being followed now.
    pub followed: usize,
    /// Files that exist. Larger than `followed` means the 60-file cap bit.
    pub total: usize,
    pub projects: usize,
}

// ---------------------------------------------------------------- .env writes

#[tauri::command]
pub fn env_set(
    state: State<'_, AppState>,
    patch: std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let root = state.root()?;
    env_writer::apply(&root, &patch)
}

/// Enable a service: flip the .env key, regenerate, then bring its profile up.
///
/// The profile comes from the service id itself, NOT from lowercasing the env
/// key — doing the latter is what leaves `mongo-express` unstartable (C-09).
/// Reject a service id the contract does not define, before it reaches the
/// user's .env or a compose profile. `services_list` only ever offers catalog
/// ids, so this fires on a stale caller or a typo — cases where writing a key
/// nobody reads is worse than an error.
fn checked_service(name: &str) -> Result<()> {
    if crate::contracts::env_schema().knows_service(name) {
        return Ok(());
    }
    Err(Error::not_found(format!("service {name}"))
        .with_hint("Only services listed in contracts/env.schema.json can be managed."))
}

#[tauri::command]
pub async fn service_enable(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    checked_service(&name)?;
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    let root = state.root()?;
    let operation_id = events::next_operation_id("enable");

    events::emit(&app, "service:enabling", SubjectEvent::service(&name));

    let outcome = async {
        env_writer::set_service_enabled(&root, &name, true)?;
        generate(&app, &root, &operation_id, "projects_and_services").await?;

        let mut args = runner::compose_base_args(&root);
        args.extend(runner::profile_args("custom", std::slice::from_ref(&name))?);
        args.extend(["up".into(), "-d".into(), "--no-build".into()]);

        runner::run_operation(
            &app,
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "service:progress",
                finished_event: "service:enabled",
                program: "docker",
                args: &args,
                cwd: &root,
            },
        )
        .await
    }
    .await;

    if let Err(e) = &outcome {
        events::emit(
            &app,
            "service:error",
            SubjectEvent::service(&name).error(e.message.clone()),
        );
    }
    outcome.map(|_| operation_id)
}

#[tauri::command]
pub async fn service_disable(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    checked_service(&name)?;
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    let root = state.root()?;
    let operation_id = events::next_operation_id("disable");

    events::emit(&app, "service:disabling", SubjectEvent::service(&name));

    let outcome = async {
        // Stop first, then unconfigure: the reverse order regenerates the
        // compose file without the service, leaving its container orphaned.
        let _ = engine::stop_container(&name).await;
        env_writer::set_service_enabled(&root, &name, false)?;
        generate(&app, &root, &operation_id, "projects_and_services").await
    }
    .await;

    match &outcome {
        Ok(()) => events::emit(
            &app,
            "service:disabled",
            SubjectEvent::service(&name).running(false),
        ),
        Err(e) => events::emit(
            &app,
            "service:error",
            SubjectEvent::service(&name).error(e.message.clone()),
        ),
    }
    outcome.map(|_| operation_id)
}

// ---------------------------------------------------------------- generate

#[tracing::instrument(skip(app, root), fields(root = %root.display()))]
async fn generate(
    app: &AppHandle,
    root: &std::path::Path,
    operation_id: &str,
    scope: &str,
) -> Result<()> {
    use tauri::Manager;

    // Cloned out of the state so the guard is independent of the borrow and can
    // be held across the await below. Two bash generators writing
    // docker-compose.projects.yml at once produce a file that is neither.
    let lock = app.state::<AppState>().generate_lock.clone();
    let _serialised = lock.lock().await;

    let script = runner::cli_script(root)?;
    let mut args = vec![script.display().to_string(), "generate".to_string()];
    match scope {
        "projects" => args.push("projects".into()),
        "services" => args.push("services".into()),
        // The CLI has no combined subcommand; a bare `generate` does everything.
        _ => {}
    }

    runner::run_operation(
        app,
        runner::Operation {
            operation_id,
            subject: scope,
            progress_event: "generate:progress",
            finished_event: "generate:done",
            program: "bash",
            args: &args,
            cwd: root,
        },
    )
    .await
}

#[tauri::command]
pub async fn generate_run(
    app: AppHandle,
    state: State<'_, AppState>,
    scope: Option<String>,
) -> Result<String> {
    let _busy = state.inflight.acquire("stack")?;
    let root = state.root()?;
    let scope = scope.unwrap_or_else(|| "all".into());
    let operation_id = events::next_operation_id("generate");

    events::emit(
        &app,
        "generate:start",
        serde_json::json!({ "operationId": operation_id, "scope": scope }),
    );

    generate(&app, &root, &operation_id, &scope).await?;
    Ok(operation_id)
}

// ---------------------------------------------------------------- compose

#[tauri::command]
pub async fn compose_up(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: Option<String>,
    profiles: Option<Vec<String>>,
) -> Result<String> {
    let _busy = state.inflight.acquire("stack")?;
    let root = state.root()?;
    let mode = mode.unwrap_or_else(|| "minimal".into());
    let operation_id = events::next_operation_id("up");

    let mut args = runner::compose_base_args(&root);
    args.extend(runner::profile_args(&mode, &profiles.unwrap_or_default())?);
    args.extend([
        "up".into(),
        "-d".into(),
        "--build".into(),
        "--pull=missing".into(),
        "--remove-orphans".into(),
    ]);

    runner::run_operation(
        &app,
        runner::Operation {
            operation_id: &operation_id,
            subject: &mode,
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
        },
    )
    .await?;
    Ok(operation_id)
}

/// Bring the whole stack down.
///
/// The web UI could not offer this at all: stopping the stack would have
/// stopped the container serving the dashboard, so the button could not exist.
#[tauri::command]
pub async fn compose_down(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    let _busy = state.inflight.acquire("stack")?;
    let root = state.root()?;
    let operation_id = events::next_operation_id("down");

    let mut args = runner::compose_base_args(&root);
    args.extend([
        "--profile".into(),
        "core".into(),
        "--profile".into(),
        "services".into(),
        "--profile".into(),
        "projects".into(),
        "down".into(),
    ]);

    runner::run_operation(
        &app,
        runner::Operation {
            operation_id: &operation_id,
            subject: "stack",
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
        },
    )
    .await?;
    Ok(operation_id)
}

// ---------------------------------------------------------------- build

#[tauri::command]
pub async fn project_build(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    no_cache: Option<bool>,
) -> Result<String> {
    let root = state.root()?;
    // Rejected before the build starts rather than after compose fails: the
    // name reaches `docker compose build <name>` as a service selector.
    workspace::project_dir(&root, &name)?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    let operation_id = events::next_operation_id("build");

    events::emit(
        &app,
        "build:start",
        serde_json::json!({ "operationId": operation_id, "project": name }),
    );

    let outcome = async {
        // Step 1 — regenerate, so the Dockerfile matches the current manifest.
        generate(&app, &root, &operation_id, "projects").await?;

        // Step 2 — build just this project's service. The web UI ran a bare
        // `docker-compose build`, which rebuilt every project on disk.
        let mut args = runner::compose_base_args(&root);
        args.push("build".into());
        if no_cache.unwrap_or(false) {
            args.push("--no-cache".into());
        }
        args.push(name.clone());

        runner::run_operation(
            &app,
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "build:progress",
                finished_event: "build:built",
                program: "docker",
                args: &args,
                cwd: &root,
            },
        )
        .await?;

        // Step 3 — (re)create the container from the fresh image.
        let mut up = runner::compose_base_args(&root);
        up.extend([
            "up".into(),
            "-d".into(),
            "--no-build".into(),
            "--no-deps".into(),
            name.clone(),
        ]);

        runner::run_operation(
            &app,
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "build:progress",
                finished_event: "build:success",
                program: "docker",
                args: &up,
                cwd: &root,
            },
        )
        .await
    }
    .await;

    if let Err(e) = &outcome {
        events::emit(
            &app,
            "build:error",
            serde_json::json!({
                "operationId": operation_id,
                "project": name,
                "error": e.message,
            }),
        );
    }
    outcome.map(|_| operation_id)
}

// ================================================================ Phase 3
// Desktop integration: hosts file, terminals, notifications.

use crate::pty::{self, PtyTarget};

// ---------------------------------------------------------------- hosts

#[tauri::command]
pub fn hosts_status(domains: Vec<String>) -> Result<Vec<hosts::HostsEntry>> {
    Ok(hosts::status_for(&domains))
}

/// Compute what a hosts change would do, WITHOUT elevating.
///
/// The UI shows this diff and asks before `hosts_apply` raises the auth prompt.
/// Elevating first and explaining afterwards would be the wrong order for the
/// one operation in this app that touches a system file.
#[tauri::command]
pub fn hosts_plan(add: Vec<String>, remove: Vec<String>) -> Result<hosts::HostsPlan> {
    hosts::plan(&add, &remove)
}

#[tauri::command]
pub fn hosts_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<hosts::HostsPlan> {
    // Two of these at once means two elevation prompts racing over one file,
    // and the loser's marker block is overwritten by a plan computed before it
    // existed.
    let _busy = state.inflight.acquire("hosts")?;
    let plan = hosts::apply(&add, &remove)?;
    events::emit(
        &app,
        "hosts:changed",
        serde_json::json!({ "added": plan.add, "removed": plan.remove }),
    );
    Ok(plan)
}

/// Every project domain that has no hosts entry. Drives the one-click fix.
#[tauri::command]
pub async fn hosts_missing(state: State<'_, AppState>) -> Result<Vec<String>> {
    let root = state.root()?;
    let projects = list_projects(&root).await?;
    Ok(projects
        .into_iter()
        .filter(|p| p.domain.is_some() && !p.domain_configured)
        .filter_map(|p| p.domain)
        .collect())
}

// -------------------------------------------------------------------- mail

/// Which catcher this checkout has, and how full it is.
#[tauri::command]
pub async fn mail_status(state: State<'_, AppState>) -> Result<mail::MailStatus> {
    let root = state.root()?;
    mail::status(&root).await
}

#[tauri::command]
pub async fn mail_messages(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<mail::MailMessage>> {
    let root = state.root()?;
    mail::messages(&root, limit.unwrap_or(50)).await
}

#[tauri::command]
pub async fn mail_message(state: State<'_, AppState>, id: String) -> Result<mail::MailBody> {
    let root = state.root()?;
    mail::message(&root, &id).await
}

/// Empty the inbox.
#[tauri::command]
pub async fn mail_clear(state: State<'_, AppState>) -> Result<()> {
    let root = state.root()?;
    mail::clear(&root).await
}

// --------------------------------------------------------------- databases

/// Which database services can be dumped, and whether they are up.
#[tauri::command]
pub async fn db_targets(state: State<'_, AppState>) -> Result<Vec<db::DbTarget>> {
    let root = state.root()?;
    db::targets(&root).await
}

/// Read a database out to a file the user chose.
#[tauri::command]
pub async fn db_dump(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    path: String,
) -> Result<String> {
    db_operation(app, state, service, path, "dump").await
}

/// Put a file back into a database, replacing what is there.
#[tauri::command]
pub async fn db_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    path: String,
) -> Result<String> {
    db_operation(app, state, service, path, "restore").await
}

/// Both directions differ only in which way the bytes travel.
async fn db_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    path: String,
    action: &str,
) -> Result<String> {
    let root = state.root()?;

    // One at a time per service: a dump racing a restore on the same database
    // produces a backup of a half-restored state, which is the one file you
    // would least want to be wrong.
    let _busy = state.inflight.acquire(format!("db:{service}"))?;

    let operation_id = events::next_operation_id(action);
    events::emit(
        &app,
        "db:start",
        serde_json::json!({
            "operationId": operation_id, "service": service,
            "action": action, "path": path,
        }),
    );

    let target = std::path::PathBuf::from(&path);
    let progress = {
        let app = app.clone();
        let id = operation_id.clone();
        move |line: String| {
            events::emit(
                &app,
                "db:progress",
                serde_json::json!({ "operationId": id, "line": line }),
            );
        }
    };

    let outcome = if action == "dump" {
        db::dump(&root, &service, &target, progress).await
    } else {
        db::restore(&root, &service, &target, progress).await
    };

    match outcome {
        Ok(bytes) => {
            events::emit(
                &app,
                "db:done",
                serde_json::json!({
                    "operationId": operation_id, "service": service,
                    "action": action, "path": path, "bytes": bytes,
                }),
            );
            Ok(operation_id)
        }
        Err(e) => {
            events::emit(
                &app,
                "db:error",
                serde_json::json!({
                    "operationId": operation_id, "service": service,
                    "error": e.message,
                }),
            );
            Err(e)
        }
    }
}

// ------------------------------------------------------------------ xdebug

/// Whether Xdebug is asked for, compiled in, and live — three separate answers.
#[tauri::command]
pub async fn xdebug_status(
    state: State<'_, AppState>,
    name: String,
) -> Result<xdebug::XdebugStatus> {
    let root = state.root()?;
    xdebug::status(&root, &name).await
}

/// Turn it on or off for one project.
#[tauri::command]
pub async fn xdebug_set(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<xdebug::XdebugStatus> {
    let root = state.root()?;

    // The manifest write and the overlay render have to land together: a
    // second toggle interleaving between them would render an overlay from a
    // half-written set of manifests.
    let _busy = state.inflight.acquire("xdebug")?;

    let status = xdebug::set(&root, &name, enabled).await?;
    events::emit(
        &app,
        "xdebug:changed",
        serde_json::json!({ "project": name, "enabled": status.enabled }),
    );
    Ok(status)
}

// ------------------------------------------------------------ dump catcher

#[tauri::command]
pub async fn dumps_status(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::dumps::Status> {
    crate::dumps::status(&state.root()?, &name).await
}

/// Start Symfony's own dump server in the project's container and stream what
/// it renders.
///
/// The rendering is Symfony's, not this app's — see `dumps.rs`. Reusing the log
/// stream's events on purpose: the viewer already renders one kind of line, and
/// a second event pair would be a second listener that could drift from the
/// first.
#[tauri::command]
pub async fn dumps_open(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    let root = state.root()?;
    let argv = crate::dumps::prepare(&root, &name).await?;

    let stream_id = events::next_operation_id("dumps");

    let handle = {
        let app = app.clone();
        let stream_id = stream_id.clone();
        let container = name.clone();

        tokio::spawn(async move {
            // `stream` returns when the process exits; aborting this task drops
            // the child, and `kill_on_drop` stops the collector with it.
            let _ = runner::stream("docker", &argv, std::path::Path::new("."), |line| {
                if crate::dumps::is_banner(line) {
                    return;
                }
                events::emit(
                    &app,
                    "logs:line",
                    events::LogLineEvent {
                        stream_id: stream_id.clone(),
                        container: container.clone(),
                        line: line.to_string(),
                        stream: "stdout".to_string(),
                        source: Some("dumps".to_string()),
                    },
                );
            })
            .await;

            events::emit(
                &app,
                "logs:closed",
                serde_json::json!({ "streamId": stream_id }),
            );
        })
        .abort_handle()
    };

    if let Ok(mut streams) = state.log_streams.lock() {
        streams.insert(stream_id.clone(), handle);
    }

    Ok(stream_id)
}

/// Stop the collector and the stream together.
///
/// Not `container_logs_close`: aborting the task kills the local `docker exec`
/// client, and the PHP process inside the container carries on holding the
/// port. The next `dumps_open` then fails with Symfony's own "Address already
/// in use", which is how this was found.
#[tauri::command]
pub async fn dumps_close(
    state: State<'_, AppState>,
    name: String,
    stream_id: String,
) -> Result<()> {
    if let Ok(mut streams) = state.log_streams.lock() {
        if let Some(handle) = streams.remove(&stream_id) {
            handle.abort();
        }
    }
    crate::dumps::stop(&crate::engine::container_name(&name)).await;
    Ok(())
}

// ------------------------------------------------------- production images

/// What building a production image would do, before it does it.
#[tauri::command]
pub fn release_plan(
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
) -> Result<crate::release::Plan> {
    crate::release::plan(&state.root()?, &name, tag)
}

/// The result of a build: the plan that produced it, and what the image was
/// found to contain.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseResult {
    pub plan: crate::release::Plan,
    pub verification: crate::release::Verification,
}

/// Build it, then open the image and check the two things that matter.
///
/// The verification is not optional and not a separate button. This feature's
/// safety property — no `.env`, no active debugger — is exactly the kind that
/// is easy to state in a Dockerfile and quietly wrong in the result, and the
/// method this project keeps finding those with is asking the running thing.
#[tauri::command]
pub async fn release_build(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
) -> Result<ReleaseResult> {
    let root = state.root()?;
    let _busy = state.inflight.acquire("release")?;

    let plan = crate::release::plan(&root, &name, tag)?;
    let context = workspace::project_dir(&root, &name)?;
    let operation_id = events::next_operation_id("release");

    events::emit(
        &app,
        "build:start",
        serde_json::json!({ "project": name, "operationId": operation_id, "tag": plan.tag }),
    );

    match plan.strategy {
        crate::release::Strategy::Layer => {
            let dockerfile = crate::release::write(&root, &name, &plan)?;
            let argv = crate::release::build_argv(&context, &dockerfile, &plan.tag);

            runner::run_operation(
                &app,
                runner::Operation {
                    operation_id: &operation_id,
                    subject: &name,
                    progress_event: "build:progress",
                    finished_event: "build:success",
                    program: "docker",
                    args: &argv,
                    cwd: &root,
                },
            )
            .await?;
        }
        crate::release::Strategy::Retag => {
            // Nothing to add: the node image already carries the code and the
            // build. Rebuilding from it would replace a Linux `node_modules`
            // with whatever the host has.
            let argv = vec!["tag".to_string(), plan.base_image.clone(), plan.tag.clone()];
            runner::run_operation(
                &app,
                runner::Operation {
                    operation_id: &operation_id,
                    subject: &name,
                    progress_event: "build:progress",
                    finished_event: "build:success",
                    program: "docker",
                    args: &argv,
                    cwd: &root,
                },
            )
            .await?;
        }
    }

    let verification = crate::release::verify(&plan.tag, &plan.runtime).await?;
    Ok(ReleaseResult { plan, verification })
}

/// Write a built image out as a tarball. Returns its size.
#[tauri::command]
pub async fn release_save(
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
    path: String,
) -> Result<u64> {
    let root = state.root()?;
    let plan = crate::release::plan(&root, &name, tag)?;
    crate::release::save(&plan.tag, std::path::Path::new(&path)).await
}

// ------------------------------------------------------------------ profiler

/// What Xdebug is set up to do for this project, and what it has recorded.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilerStatus {
    /// The Xdebug state this is layered on. Profiling is a *mode* of the
    /// existing toggle, not a second switch: the extension has to be compiled
    /// in either way, and two switches for one extension is two states to
    /// explain instead of one.
    pub xdebug: xdebug::XdebugStatus,
    pub mode: xdebug::Mode,
    /// The header a request needs when profiling is on: Xdebug is left on
    /// `start_with_request=trigger` so an idle stack does not write a
    /// multi-megabyte file per page load.
    pub trigger: String,
    pub profiles: Vec<crate::profile::ProfileFile>,
    /// Total bytes the profiles hold — this fills a disk fast.
    pub bytes: u64,
    pub directory: String,
}

/// The name Xdebug looks for in a cookie, GET, POST or the environment.
const TRIGGER: &str = "XDEBUG_TRIGGER";

#[tauri::command]
pub async fn profiler_status(state: State<'_, AppState>, name: String) -> Result<ProfilerStatus> {
    let root = state.root()?;
    let profiles = crate::profile::list(&root, &name)?;

    Ok(ProfilerStatus {
        bytes: profiles.iter().map(|p| p.bytes).sum(),
        directory: crate::profile::host_dir(&root, &name).display().to_string(),
        mode: xdebug::read_mode(&root, &name),
        trigger: TRIGGER.to_string(),
        xdebug: xdebug::status(&root, &name).await?,
        profiles,
    })
}

/// Switch between stepping and profiling.
///
/// Not a set: the two modes want opposite start triggers — stepping wants to
/// connect on the next request, profiling wants a trigger so an idle stack does
/// not fill the disk — so `debug,profile` would have to break one of them.
#[tauri::command]
pub async fn profiler_set_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    mode: xdebug::Mode,
) -> Result<ProfilerStatus> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.join("stackvo.json").is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }

    let _busy = state.inflight.acquire("xdebug")?;

    let path = xdebug::mode_path(&root, &name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    let text = serde_json::to_string_pretty(&xdebug::ModeConfig { mode })
        .map_err(|e| Error::new(Code::IoError, format!("serialising the mode: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))?;

    // So the ini and the overlay exist before the next compose call rather than
    // being written by it — and so the reply describes a state that is real.
    xdebug::sync(&root);

    events::emit(
        &app,
        "xdebug:changed",
        serde_json::json!({ "project": name, "mode": mode }),
    );
    profiler_status(state, name).await
}

/// One recorded profile, aggregated.
#[tauri::command]
pub fn profiler_read(
    state: State<'_, AppState>,
    name: String,
    id: String,
) -> Result<crate::profile::Report> {
    crate::profile::read(&state.root()?, &name, &id)
}

#[tauri::command]
pub fn profiler_delete(state: State<'_, AppState>, name: String, id: String) -> Result<()> {
    crate::profile::delete(&state.root()?, &name, &id)
}

/// Remove every recorded profile. Returns how many, and how much was freed.
#[tauri::command]
pub fn profiler_clear(state: State<'_, AppState>, name: String) -> Result<serde_json::Value> {
    let (removed, freed) = crate::profile::clear(&state.root()?, &name)?;
    Ok(serde_json::json!({ "removed": removed, "freed": freed }))
}

// ---------------------------------------------------------- quick commands

/// The commands this project has the files to run.
#[tauri::command]
pub fn quick_commands(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<crate::quickcmd::QuickCommand>> {
    crate::quickcmd::for_project(&state.root()?, &name)
}

/// Run one of them, by id.
///
/// The id is looked up in the catalog and the argv is built here; the frontend
/// never names a program. Interactive commands open the user's own terminal and
/// return no operation id — there is nothing to stream, and an in-app REPL next
/// to the terminal they already configured would be the worse of the two.
#[tauri::command]
pub async fn quick_command_run(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    id: String,
) -> Result<Option<String>> {
    let root = state.root()?;
    let spec = crate::quickcmd::resolve(&id)?;

    // Validated before the container name is built from it, as everywhere else.
    workspace::project_dir(&root, &name)?;
    let container = crate::engine::container_name(&name);

    // `docker exec` needs something to exec into. Without this the failure is
    // Docker's "No such container", which reads as a broken button rather than
    // as a project that is not running.
    let running = crate::engine::inspect(&name)
        .await
        .map(|d| d.running)
        .unwrap_or(false);
    if !running {
        return Err(Error::new(Code::Conflict, format!("{name} is not running"))
            .with_hint("Start the project first — these commands run inside its container."));
    }

    if spec.interactive {
        let preferred = prefs_get().ok().and_then(|p| {
            p.get("terminalApp")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
        crate::pty::open_external_command(&container, spec, preferred.as_deref())?;
        return Ok(None);
    }

    // One-shot: through the operation console, like every other long-running
    // thing in this app. Reported under the `build:` family because that is the
    // one the project detail page already listens to for its own operations.
    let operation_id = events::next_operation_id("cmd");
    let argv = crate::quickcmd::exec_argv(&container, spec);

    events::emit(
        &app,
        "build:start",
        serde_json::json!({
            "project": name, "operationId": operation_id, "command": spec.display
        }),
    );

    let handle = app.clone();
    let subject = name.clone();
    let op_id = operation_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = runner::run_operation(
            &handle,
            runner::Operation {
                operation_id: &op_id,
                subject: &subject,
                progress_event: "build:progress",
                finished_event: "build:success",
                program: "docker",
                args: &argv,
                cwd: &root,
            },
        )
        .await;
    });

    Ok(Some(operation_id))
}

// -------------------------------------------------------------- dev server

/// Whether a node project runs its dev server with the source mounted live.
#[tauri::command]
pub async fn devserver_status(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::devserver::DevServerStatus> {
    let root = state.root()?;
    crate::devserver::status(&root, &name).await
}

/// Turn it on or off.
#[tauri::command]
pub async fn devserver_set(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
    command: Option<String>,
) -> Result<crate::devserver::DevServerStatus> {
    let root = state.root()?;

    // The config write and the overlay render have to land together, for the
    // same reason as Xdebug's toggle: a second change interleaving between them
    // would render an overlay from a half-written set of files.
    let _busy = state.inflight.acquire("devserver")?;

    let status = crate::devserver::set(&root, &name, enabled, command).await?;
    events::emit(
        &app,
        "devserver:changed",
        serde_json::json!({ "project": name, "enabled": status.enabled }),
    );
    Ok(status)
}

// ------------------------------------------------- migrating a compose file

/// What reading a project's `docker-compose.yml` would produce.
///
/// Everything the review needs, in one round trip: what was read, the manifest
/// it implies, and the `.env` diff enabling its services would make.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub migration: crate::migrate::Migration,
    /// The proposed `stackvo.json`, already validated against the schema —
    /// reviewing a spec that would then be refused is a review of nothing.
    pub spec: serde_json::Value,
    /// Services to enable, as the same reviewed diff a preset import shows.
    pub env: crate::preset::Plan,
    /// True when the directory already has a manifest, so this is a comparison
    /// rather than an adoption.
    pub already_managed: bool,
}

/// Detection, then the compose file on top of it.
///
/// Order matters and is the whole point: detection reads the *code* and gets
/// runtime, framework and document root; the compose file records what the
/// person who wrote it *decided* — the PHP version, the domain, the extensions,
/// and the services, none of which any marker file states. Where both have an
/// answer the compose file wins, because a guess loses to a declaration.
fn migrated_spec(
    name: &str,
    detected: &detect::Detected,
    m: &crate::migrate::Migration,
) -> serde_json::Value {
    let mut spec = detected_spec(name, detected);

    if let Some(domain) = &m.domain {
        spec["domain"] = serde_json::json!(domain);
    }

    let runtime = m.runtime.as_deref().unwrap_or(detected.runtime);
    if runtime == "node" {
        spec["runtime"] = serde_json::json!("node");
        // The three PHP-only keys have to go with it, or the spec describes two
        // runtimes at once and the contract rejects it (W-02).
        if let Some(object) = spec.as_object_mut() {
            for key in ["server", "document_root", "php"] {
                object.remove(key);
            }
        }

        // Built rather than patched: `detected_spec` only emits a node block
        // when *detection* said node, and the case that brings us here is
        // precisely the one where it did not — a Laravel repository whose
        // compose file runs the Vite container. Patching a block that is not
        // there silently produced a node project with no node settings.
        let node = spec
            .get("node")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let field =
            |key: &str, fallback: serde_json::Value| node.get(key).cloned().unwrap_or(fallback);

        spec["node"] = serde_json::json!({
            "version": m
                .node_version
                .clone()
                .map(serde_json::Value::from)
                .unwrap_or_else(|| field("version", serde_json::json!("22"))),
            "install": field("install", serde_json::json!("npm install")),
            "start": field("start", serde_json::json!("npm run dev")),
            "port": m
                .port
                .map(serde_json::Value::from)
                .unwrap_or_else(|| field("port", serde_json::json!(3000))),
        });
        return spec;
    }

    if let Some(server) = &m.server {
        spec["server"] = serde_json::json!(server);
    }
    if let Some(root) = &m.document_root {
        spec["document_root"] = serde_json::json!(root);
    }
    if let Some(php) = spec.get_mut("php").and_then(|v| v.as_object_mut()) {
        if let Some(version) = &m.php_version {
            php.insert("version".into(), serde_json::json!(version));
        }
        // `extensions` last: the contract's write rules put it at the end of
        // the php block, and a form that reorders it produces valid JSON the
        // differential check still fails on.
        if !m.extensions.is_empty() {
            php.insert("extensions".into(), serde_json::json!(m.extensions));
        }
    }

    spec
}

/// Read a project's compose file and say what importing it would do.
#[tauri::command]
pub async fn migrate_scan(state: State<'_, AppState>, name: String) -> Result<MigrationPlan> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("directory {name}")));
    }

    let Some(compose) = detect::compose_file(&dir) else {
        return Err(
            Error::not_found(format!("a compose file in {name}")).with_hint(
                "Looked for compose.yaml, compose.yml, docker-compose.yaml and docker-compose.yml.",
            ),
        );
    };

    let migration = crate::migrate::read(&compose).await?;
    let detected = detect::detect(&dir);
    let spec = migrated_spec(&name, &detected, &migration);

    // Validated here rather than at adopt time: a review of a spec that would
    // then be refused is a review of nothing.
    parse_spec(&spec, &name)?;

    let env = Env::load(&root)
        .map(|env| {
            crate::preset::plan(
                &env,
                &crate::contracts::env_schema().service_catalog(),
                &crate::migrate::to_preset(&migration, Some(name.clone())),
            )
        })
        .unwrap_or_else(|_| {
            crate::preset::plan(
                &Env::parse(""),
                &crate::contracts::env_schema().service_catalog(),
                &crate::migrate::to_preset(&migration, Some(name.clone())),
            )
        });

    Ok(MigrationPlan {
        migration,
        spec,
        env,
        already_managed: dir.join("stackvo.json").is_file(),
    })
}

/// Import it: adopt the project, then enable the services it named.
///
/// The two halves in that order. Adoption is the one that can fail on a schema
/// violation, and enabling services for a project that then did not get created
/// leaves the stack carrying a database nothing uses.
#[tauri::command]
pub async fn migrate_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    spec: Option<serde_json::Value>,
    services: Option<bool>,
) -> Result<MigrationPlan> {
    let root = state.root()?;
    let _busy = state.inflight.acquire("migrate")?;

    let plan = migrate_scan(state.clone(), name.clone()).await?;

    if !plan.already_managed {
        // The reviewed spec, or the one just computed. Passing it back is what
        // lets the user correct a document root before it is written.
        let spec = spec.unwrap_or_else(|| plan.spec.clone());
        project_adopt(app.clone(), state.clone(), name.clone(), Some(spec)).await?;
    }

    if services.unwrap_or(true) && !plan.env.changes.is_empty() {
        crate::env_writer::apply(&root, &crate::preset::patch(&plan.env))?;
        events::emit(
            &app,
            "preset:applied",
            serde_json::json!({ "changed": plan.env.changes.len() }),
        );
    }

    // Re-read, so what comes back describes the state that now exists rather
    // than the one that did before the write.
    migrate_scan(state, name).await
}

// ------------------------------------------------------------ stack presets

/// This stack as a preset, for preview and for copying.
#[tauri::command]
pub fn preset_export(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<crate::preset::Preset> {
    crate::preset::export_current(&state.root()?, name)
}

/// Write it to a file the user picked.
#[tauri::command]
pub fn preset_save(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<String> {
    let root = state.root()?;
    let target = std::path::PathBuf::from(&path);
    crate::preset::save(&root, &target, name)?;
    Ok(path)
}

/// What importing this file would change, without changing anything.
#[tauri::command]
pub fn preset_plan(state: State<'_, AppState>, path: String) -> Result<crate::preset::Plan> {
    crate::preset::plan_file(&state.root()?, std::path::Path::new(&path))
}

/// Import it. Re-planned inside `apply`, so a `.env` that moved between the
/// review and the click is not overwritten with a stale diff.
#[tauri::command]
pub fn preset_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::preset::Plan> {
    let root = state.root()?;

    // `.env` has several writers — env_set, service_enable/disable — and this
    // one rewrites many keys at once. The lock inside env_writer serialises the
    // read-modify-write; this serialises the plan against it too, so the diff
    // that is applied is the diff that was planned.
    let _busy = state.inflight.acquire("preset")?;

    let plan = crate::preset::apply_file(&root, std::path::Path::new(&path))?;
    if !plan.changes.is_empty() {
        events::emit(
            &app,
            "preset:applied",
            serde_json::json!({ "changed": plan.changes.len() }),
        );
    }
    Ok(plan)
}

// ----------------------------------------------------------------- php.ini

/// The project's PHP overrides: what is on disk, what the container has, and
/// whether the two agree — three separate answers, like Xdebug's.
#[tauri::command]
pub async fn php_ini_status(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::phpini::PhpIniStatus> {
    let root = state.root()?;
    crate::phpini::status(&root, &name).await
}

/// Write directives. `null` removes one; removing the last removes the file.
#[tauri::command]
pub async fn php_ini_set(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    patch: std::collections::BTreeMap<String, Option<String>>,
) -> Result<crate::phpini::PhpIniStatus> {
    let root = state.root()?;

    // The file write and the overlay render have to land together, for the same
    // reason as Xdebug: a second edit interleaving between them would render an
    // overlay from a half-written set of files.
    let _busy = state.inflight.acquire("php_ini")?;

    let status = crate::phpini::set(&root, &name, &patch).await?;
    events::emit(
        &app,
        "php_ini:changed",
        serde_json::json!({ "project": name, "exists": status.exists }),
    );
    Ok(status)
}

// ------------------------------------------------------------- certificates

/// What the wildcard certificate covers, and whether anything trusts its CA.
///
/// Reads only, and deliberately does not need the Docker engine: the state
/// worth reporting most urgently — a certificate that predates a project — is
/// just as true with the stack down.
#[tauri::command]
pub async fn cert_status(state: State<'_, AppState>) -> Result<certs::CertStatus> {
    let root = state.root()?;
    Ok(certs::status(&root).await)
}

/// What reissuing would change, without running mkcert.
#[tauri::command]
pub async fn cert_plan(
    state: State<'_, AppState>,
    install_ca: Option<bool>,
) -> Result<certs::CertPlan> {
    let root = state.root()?;
    certs::plan(&root, install_ca.unwrap_or(true)).await
}

/// Reissue the certificate, and install the CA when nothing trusts it yet.
#[tauri::command]
pub async fn cert_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    install_ca: Option<bool>,
) -> Result<certs::CertPlan> {
    let root = state.root()?;

    // Two reissues at once write the same two files from different argument
    // lists, and the loser leaves a certificate covering domains the winner
    // computed before they existed — the same race `hosts` guards against, for
    // the same reason.
    let _busy = state.inflight.acquire("certs")?;

    let plan = certs::apply(&root, install_ca.unwrap_or(true)).await?;
    events::emit(
        &app,
        "certs:changed",
        serde_json::json!({ "added": plan.add, "removed": plan.remove }),
    );
    Ok(plan)
}

// ---------------------------------------------------------------- terminals

/// Open a PTY, in a container or on the host.
///
/// `Host { cwd }` deliberately accepts any directory. Confining it would be
/// theatre: the shell it opens can `cd` anywhere the user can the moment it
/// starts, so restricting the *starting* directory restricts nothing while
/// breaking the legitimate "open a shell here" case. That is unlike
/// `open_in_editor`, where the path is the whole payload of the action.
#[tauri::command]
pub fn pty_open(
    app: AppHandle,
    registry: State<'_, pty::Registry>,
    target: PtyTarget,
    cols: u16,
    rows: u16,
) -> Result<String> {
    pty::open(&app, &registry, target, cols, rows)
}

#[tauri::command]
pub fn pty_write(
    registry: State<'_, pty::Registry>,
    session_id: String,
    data: String,
) -> Result<()> {
    pty::write(&registry, &session_id, &data)
}

#[tauri::command]
pub fn pty_resize(
    registry: State<'_, pty::Registry>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    pty::resize(&registry, &session_id, cols, rows)
}

#[tauri::command]
pub fn pty_close(registry: State<'_, pty::Registry>, session_id: String) -> Result<()> {
    pty::close(&registry, &session_id)
}

#[tauri::command]
pub fn terminal_open_external(target: PtyTarget) -> Result<()> {
    let preferred = prefs_get().ok().and_then(|p| {
        p.get("terminalApp")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });
    pty::open_external(&target, preferred.as_deref())
}

// ================================================================ Gap fill
// Commands the contract declared but earlier phases left unimplemented.

// ---------------------------------------------------------------- projects

#[tauri::command]
pub async fn project_get(state: State<'_, AppState>, name: String) -> Result<Project> {
    let root = state.root()?;
    list_projects(&root)
        .await?
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| Error::not_found(format!("project {name}")))
}

#[tauri::command]
pub fn project_manifest_read(state: State<'_, AppState>, name: String) -> Result<Manifest> {
    let root = state.root()?;
    manifest::read(
        &workspace::project_dir(&root, &name)?.join("stackvo.json"),
        &name,
    )
}

#[tauri::command]
pub fn project_manifest_write(
    state: State<'_, AppState>,
    name: String,
    manifest: serde_json::Value,
) -> Result<Manifest> {
    let root = state.root()?;
    let spec = parse_spec(&manifest, &name)?;

    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let _busy = state.inflight.acquire(format!("project:{name}"))?;

    manifest::write(&dir.join("stackvo.json"), &spec)?;
    Ok(spec)
}

/// Turn an incoming JSON spec into a validated Manifest.
///
/// The old POST body was flat (`runtime`, `version`, `extensions` at the top
/// level) and `ProjectService` reassembled it — wrongly for Node, which is
/// CONFLICTS.md C-01. Here the payload IS the manifest, so there is nothing to
/// reassemble and nothing to get wrong.
fn parse_spec(value: &serde_json::Value, expected_name: &str) -> Result<Manifest> {
    let raw = serde_json::to_string_pretty(value)?;
    let m = manifest::normalize(value, &raw, expected_name);

    if !m.valid {
        // Two of the contract's rules are about the *layout of a file* — W-01
        // (`php.extensions` must be the last key, because the Bash extractor
        // swallows whatever follows the array) and C-04 (the 50-line parser
        // window). An incoming spec is a JSON value, and a value has no
        // meaningful key order: `serde_json`'s map is sorted, so `extensions`
        // lands before `version` and W-01 fires on a spec that is perfectly
        // fine. Checking a layout rule against something that has no layout is
        // checking nothing.
        //
        // What *will* have a layout is `manifest::to_json`, which exists
        // precisely to satisfy these rules. So when the only complaints are
        // layout ones, re-validate the bytes that are actually going to be
        // written. Found by the first spec to carry `php.extensions` — every
        // caller before the compose importer happened to omit them.
        const LAYOUT_ONLY: [&str; 2] = ["W-01", "C-04"];
        if m.errors
            .iter()
            .all(|e| LAYOUT_ONLY.contains(&e.code.as_str()))
        {
            let canonical = manifest::to_json(&m);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&canonical) {
                let rechecked = manifest::normalize(&parsed, &canonical, expected_name);
                if rechecked.valid {
                    return Ok(rechecked);
                }
            }
        }

        return Err(
            Error::new(Code::InvalidManifest, "the project definition is not valid")
                .with_details(serde_json::json!({ "errors": m.errors })),
        );
    }
    Ok(m)
}

#[tauri::command]
pub fn project_validate(name: String, spec: serde_json::Value) -> Result<serde_json::Value> {
    let raw = serde_json::to_string_pretty(&spec)?;
    let m = manifest::normalize(&spec, &raw, &name);

    // Also pre-flight the extension list, so a bad name is caught here rather
    // than minutes into a Docker build.
    let mut errors = m.errors.clone();
    if let Some(php) = &m.php {
        if let Err(message) = crate::generator::resolve(&php.version, &php.extensions, true) {
            errors.push(manifest::Finding {
                code: "UNSUPPORTED".into(),
                path: "php.extensions".into(),
                message,
            });
        }
    }

    Ok(serde_json::json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": m.warnings,
    }))
}

#[tauri::command]
pub async fn project_create(
    app: AppHandle,
    state: State<'_, AppState>,
    spec: serde_json::Value,
) -> Result<String> {
    let root = state.root()?;
    let name = spec
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?
        .to_string();

    let m = parse_spec(&spec, &name)?;
    let dir = workspace::project_dir(&root, &name)?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    if dir.exists() {
        return Err(Error::new(
            Code::AlreadyExists,
            format!("project \"{name}\" already exists"),
        ));
    }

    let operation_id = events::next_operation_id("create");
    events::emit(&app, "project:creating", SubjectEvent::project(&name));

    let outcome = async {
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::io("creating the project directory", e))?;
        manifest::write(&dir.join("stackvo.json"), &m)?;

        // A document root with a placeholder page, so the project serves
        // something the moment it is built.
        if m.runtime == "php" {
            let doc_root = dir.join(m.document_root.as_deref().unwrap_or("public"));
            std::fs::create_dir_all(&doc_root)
                .map_err(|e| Error::io("creating the document root", e))?;
            let index = doc_root.join("index.php");
            if !index.exists() {
                std::fs::write(
                    &index,
                    format!("<?php\nphpinfo();\n// {name} — replace this with your application.\n"),
                )
                .map_err(|e| Error::io("writing index.php", e))?;
            }
        }

        generate(&app, &root, &operation_id, "projects").await
    }
    .await;

    match &outcome {
        Ok(()) => events::emit(&app, "project:created", SubjectEvent::project(&name)),
        Err(e) => {
            // Roll back the directory we made, so a failed create does not
            // leave a half-project that the list then reports as broken.
            let _ = std::fs::remove_dir_all(&dir);
            events::emit(
                &app,
                "project:error",
                SubjectEvent::project(&name).error(e.message.clone()),
            );
        }
    }

    outcome.map(|_| operation_id)
}

// ------------------------------------------------------- adopting a folder

/// Directories under `projects/` that StackVo is not managing yet.
#[tauri::command]
pub fn project_adoptable(state: State<'_, AppState>) -> Result<Vec<detect::Adoptable>> {
    let root = state.root()?;
    Ok(detect::adoptable(&root))
}

/// Bring an existing directory under management.
///
/// The counterpart of `project_create`, which requires the directory to be
/// absent. Nothing here writes application files: the code is already there,
/// and the only thing missing is the manifest that makes StackVo see it.
#[tauri::command]
pub async fn project_adopt(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    spec: Option<serde_json::Value>,
) -> Result<String> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;

    if !dir.is_dir() {
        return Err(Error::not_found(format!("directory {name}")));
    }

    let manifest_path = dir.join("stackvo.json");
    if manifest_path.exists() {
        // Adopting something already managed would overwrite settings the user
        // chose, which is a different operation with a different confirmation.
        return Err(Error::new(
            Code::AlreadyExists,
            format!("\"{name}\" already has a stackvo.json"),
        )
        .with_hint("Edit it from the project's Manifest tab instead."));
    }

    // Detection fills the form; it does not bypass validation. An adopted
    // project has to satisfy exactly the contract a created one does.
    let spec = match spec {
        Some(spec) => spec,
        None => detected_spec(&name, &detect::detect(&dir)),
    };
    let m = parse_spec(&spec, &name)?;

    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    let operation_id = events::next_operation_id("adopt");
    events::emit(&app, "project:creating", SubjectEvent::project(&name));

    let outcome = async {
        manifest::write(&manifest_path, &m)?;
        generate(&app, &root, &operation_id, "projects").await
    }
    .await;

    match &outcome {
        Ok(()) => events::emit(&app, "project:created", SubjectEvent::project(&name)),
        Err(e) => {
            // Remove only the manifest we just wrote. Unlike project_create
            // there is no directory of ours to roll back — the code was the
            // user's before this ran and stays theirs if it fails.
            let _ = std::fs::remove_file(&manifest_path);
            events::emit(
                &app,
                "project:error",
                SubjectEvent::project(&name).error(e.message.clone()),
            );
        }
    }

    outcome.map(|_| operation_id)
}

/// Turn a detection into a manifest the schema accepts.
fn detected_spec(name: &str, detected: &detect::Detected) -> serde_json::Value {
    let mut spec = serde_json::json!({
        "name": name,
        // The convention the generator and the hosts helper both assume.
        "domain": format!("{name}.loc"),
        "runtime": detected.runtime,
    });

    if detected.runtime == "node" {
        spec["node"] = serde_json::json!({
            "version": detected.node_version.clone().unwrap_or_else(|| "22".into()),
            "install": "npm install",
            "start": detected.node_start.clone().unwrap_or_else(|| "npm run dev".into()),
            "port": detected.node_port.unwrap_or(3000),
        });
    } else {
        spec["server"] = serde_json::json!(detected.server);
        spec["document_root"] = serde_json::json!(detected
            .document_root
            .clone()
            .unwrap_or_else(|| "public".into()));
        spec["php"] = serde_json::json!({
            "version": detected.php_version.clone().unwrap_or_else(|| "8.4".into()),
        });
    }

    spec
}

/// Delete a project.
///
/// `remove_files` defaults to FALSE and must be opted into explicitly. The web
/// UI's deleteProject() removed the directory outright — a desktop app deleting
/// someone's source code needs a deliberate second step, not a default.
#[tauri::command]
pub async fn project_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    remove_files: Option<bool>,
) -> Result<String> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let _busy = state.inflight.acquire(format!("project:{name}"))?;

    let operation_id = events::next_operation_id("delete");
    events::emit(&app, "project:deleting", SubjectEvent::project(&name));

    let outcome = async {
        let _ = engine::stop_container(&name).await;

        if remove_files.unwrap_or(false) {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| Error::io("removing the project directory", e))?;
        } else {
            // Keep the source; drop only the manifest, which is what makes
            // StackVo consider it a project at all.
            let manifest_path = dir.join("stackvo.json");
            if manifest_path.exists() {
                std::fs::remove_file(&manifest_path)
                    .map_err(|e| Error::io("removing stackvo.json", e))?;
            }
        }

        generate(&app, &root, &operation_id, "projects").await
    }
    .await;

    match &outcome {
        Ok(()) => events::emit(&app, "project:deleted", SubjectEvent::project(&name)),
        Err(e) => events::emit(
            &app,
            "project:error",
            SubjectEvent::project(&name).error(e.message.clone()),
        ),
    }

    outcome.map(|_| operation_id)
}

// ---------------------------------------------------------------- bulk + compose

async fn bulk(app: &AppHandle, phase: Lifecycle) -> Result<Vec<String>> {
    let containers = engine::stackvo_containers().await?;
    let mut touched = Vec::new();

    for (id, info) in containers {
        // Skip work that would be a no-op anyway.
        let needed = match phase.pending {
            "starting" => !info.running,
            "stopping" => info.running,
            _ => true,
        };
        if !needed {
            continue;
        }

        let result = match phase.pending {
            "starting" => engine::start_container(&id).await,
            "stopping" => engine::stop_container(&id).await,
            _ => engine::restart_container(&id).await,
        };

        if result.is_ok() {
            events::emit(
                app,
                &format!("service:{}", phase.done),
                SubjectEvent::service(&id).running(phase.running_after),
            );
            touched.push(id);
        }
    }

    Ok(touched)
}

#[tauri::command]
pub async fn containers_start_all(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let _busy = state.inflight.acquire("stack")?;
    bulk(&app, events::START).await
}

#[tauri::command]
pub async fn containers_stop_all(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let _busy = state.inflight.acquire("stack")?;
    bulk(&app, events::STOP).await
}

#[tauri::command]
pub async fn containers_restart_all(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let _busy = state.inflight.acquire("stack")?;
    bulk(&app, events::RESTART).await
}

async fn compose_profile_up(
    app: &AppHandle,
    root: &std::path::Path,
    subject: &str,
    profile: &str,
) -> Result<String> {
    let operation_id = events::next_operation_id("up");
    let mut args = runner::compose_base_args(root);
    args.extend(runner::profile_args("custom", &[profile.to_string()])?);
    args.extend([
        "up".into(),
        "-d".into(),
        "--build".into(),
        "--pull=missing".into(),
    ]);

    runner::run_operation(
        app,
        runner::Operation {
            operation_id: &operation_id,
            subject,
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: root,
        },
    )
    .await?;
    Ok(operation_id)
}

#[tauri::command]
pub async fn compose_up_service(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    checked_service(&name)?;
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    let root = state.root()?;
    compose_profile_up(&app, &root, &name, &name).await
}

#[tauri::command]
pub async fn compose_up_project(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    let root = state.root()?;
    workspace::project_dir(&root, &name)?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    // Project profiles are prefixed; service profiles are not.
    let profile = format!("project-{name}");
    compose_profile_up(&app, &root, &name, &profile).await
}

#[tauri::command]
pub async fn compose_restart(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    let _busy = state.inflight.acquire("stack")?;
    let root = state.root()?;
    let operation_id = events::next_operation_id("restart");

    let mut args = runner::compose_base_args(&root);
    args.extend([
        "--profile".into(),
        "core".into(),
        "--profile".into(),
        "services".into(),
        "--profile".into(),
        "projects".into(),
        "restart".into(),
    ]);

    runner::run_operation(
        &app,
        runner::Operation {
            operation_id: &operation_id,
            subject: "stack",
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
        },
    )
    .await?;
    Ok(operation_id)
}

// ---------------------------------------------------------------- remaining queries

#[tauri::command]
pub async fn service_dependencies(name: String) -> Result<serde_json::Value> {
    let schema = env_schema();
    let deps = schema.dependencies_for(&name);
    let containers = engine::stackvo_containers().await.unwrap_or_default();
    let running = |id: &str| containers.get(id).is_some_and(|c| c.running);

    let mut rows = Vec::new();
    for dep in &deps.required {
        rows.push(serde_json::json!({ "name": dep, "type": "required", "running": running(dep) }));
    }
    for dep in &deps.optional {
        rows.push(serde_json::json!({ "name": dep, "type": "optional", "running": running(dep) }));
    }

    Ok(serde_json::json!({
        "service": name,
        "description": deps.note.unwrap_or_default(),
        "dependencies": rows,
        "hasUnmetDependencies": deps.required.iter().any(|d| !running(d)),
        "internal": deps.internal,
    }))
}

// ---------------------------------------------------------------- stats history

/// Recorded CPU/memory samples for a container.
///
/// The web UI kept these in memory in the dashboard container, so the history
/// died whenever that container restarted. Here it lives in the app's own data
/// directory and survives restarts of both the app and the stack.
#[tauri::command]
pub fn container_stats_history(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<serde_json::Value>> {
    let history = state
        .stats_history
        .lock()
        .map_err(|_| Error::new(Code::IoError, "stats history lock poisoned"))?;

    Ok(history
        .get(&engine::container_name(&name))
        .map(|samples| {
            samples
                .iter()
                .map(|(t, cpu, mem)| serde_json::json!({ "t": t, "cpu": cpu, "memory": mem }))
                .collect()
        })
        .unwrap_or_default())
}

/// What terminals and editors this machine actually has.
///
/// Both lists include entries that are not installed, marked `available:
/// false`, so the picker can grey them out. A list that silently omits them
/// reads as "this app does not support iTerm", which is a different and wrong
/// message.
#[tauri::command]
pub fn apps_available() -> serde_json::Value {
    serde_json::json!({
        "terminals": crate::apps::terminals(),
        "editors": crate::apps::editors(),
    })
}

// ---------------------------------------------------------------- window life

/// What closing the window should do.
///
/// Four options rather than the three a tool with its own service processes
/// would offer, because StackVo's containers are Docker's, not ours: they
/// outlive the app perfectly well, so "close and leave the stack running" is a
/// real choice here and probably the common one. A tool whose services would be
/// orphaned could not offer it.
pub const CLOSE_ASK: &str = "ask";
pub const CLOSE_TRAY: &str = "tray";
pub const CLOSE_QUIT: &str = "quit";
pub const CLOSE_STOP_AND_QUIT: &str = "stopAndQuit";

/// Whether to open hidden, showing only the tray.
pub fn start_minimized() -> bool {
    prefs_get()
        .ok()
        .and_then(|p| p.get("startMinimized").and_then(|v| v.as_bool()))
        .unwrap_or(false)
}

/// The stored preference, or "ask".
pub fn close_behaviour() -> String {
    prefs_get()
        .ok()
        .and_then(|p| {
            p.get("closeBehaviour")
                .and_then(|v| v.as_str())
                .filter(|s| {
                    matches!(
                        *s,
                        CLOSE_ASK | CLOSE_TRAY | CLOSE_QUIT | CLOSE_STOP_AND_QUIT
                    )
                })
                .map(str::to_string)
        })
        .unwrap_or_else(|| CLOSE_ASK.to_string())
}

/// Carry out the choice the close dialog collected.
///
/// The dialog is in the front end rather than a native one because it has to
/// offer "remember this", and a remembered choice is a preference the Settings
/// page also edits — one control, not two that can disagree.
#[tauri::command]
pub async fn window_close_action(app: AppHandle, action: String, remember: bool) -> Result<()> {
    if remember && action != CLOSE_ASK {
        prefs_set(serde_json::json!({ "closeBehaviour": action }))?;
    }
    apply_close(app, action).await;
    Ok(())
}

/// Shared by the dialog and by the stored-preference path, so a remembered
/// choice behaves identically to the same choice made in the dialog.
pub async fn apply_close(app: AppHandle, action: String) {
    use tauri::Manager;

    match action.as_str() {
        CLOSE_TRAY => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            tracing::info!("window hidden to tray");
        }
        CLOSE_STOP_AND_QUIT => {
            // Stopping is the point of the choice, so it is awaited. A stack
            // half-stopped because the process exited first would be worse than
            // a second of delay on quit.
            tracing::info!("stopping the stack before exit");
            let stopped = bulk(&app, events::STOP).await;
            match stopped {
                Ok(names) => tracing::info!(count = names.len(), "stopped on exit"),
                Err(e) => tracing::error!(error = %e, "could not stop the stack on exit"),
            }
            app.exit(0);
        }
        // CLOSE_QUIT, and anything unrecognised: leave the containers running.
        _ => {
            tracing::info!("exiting, stack left running");
            app.exit(0);
        }
    }
}

// ---------------------------------------------------------------- preflight

/// Everything that has to be true before the app can do its job.
///
/// Called before the first screen: the alternative is an app that opens on an
/// empty dashboard and answers every click with a different error.
#[tauri::command]
pub async fn preflight() -> crate::preflight::Preflight {
    crate::preflight::run().await
}

/// Do the one thing a fixable requirement needs.
#[tauri::command]
pub async fn preflight_fix(id: String) -> Result<()> {
    crate::preflight::fix(&id).await
}

// ------------------------------------------------------------------ doctor

/// The full diagnosis: the boot gate's rows plus the failures that arrive
/// later — a port already taken (named), hosts entries missing, generated
/// config older than its inputs, disk held by unused images and volumes.
///
/// Each finding pairs with a repair the app already knows how to do:
/// `preflight_fix`, `hosts_apply` (behind its reviewed diff), `generate_run`,
/// `docker_prune`. The report only diagnoses; every repair stays behind its
/// own command so the confirmation flows are not bypassed.
#[tauri::command]
pub async fn doctor(state: State<'_, AppState>) -> Result<crate::doctor::Doctor> {
    let root = state.root().ok();
    Ok(crate::doctor::run(root.as_deref()).await)
}

/// Remove an extension the build cannot install, and re-report.
///
/// The one repair in this panel that changes a file the *user* wrote, which is
/// why it is worth being exact about what it does: **nothing about the running
/// stack changes.** The generator already drops the extension silently, so it
/// is already missing from every built container — this only stops the manifest
/// claiming something the container never had.
#[tauri::command]
pub async fn doctor_drop_extension(
    app: AppHandle,
    state: State<'_, AppState>,
    subject: String,
    extension: String,
) -> Result<crate::doctor::Doctor> {
    let root = state.root()?;

    // `.env` has several writers, and a manifest edit races the watcher.
    let _busy = state.inflight.acquire("doctor")?;

    crate::doctor::drop_extension(&root, &subject, &extension)?;

    events::emit(
        &app,
        "manifest:changed",
        serde_json::json!({ "project": subject, "reason": "extension-removed" }),
    );

    Ok(crate::doctor::run(Some(&root)).await)
}

// ---------------------------------------------------------------- scaffold

/// Fill a new project directory by running the framework's own installer in
/// a throwaway container, then leave the rest to `project_adopt` — the same
/// detection whether the code arrived by `git clone` or by this command.
///
/// An operation: `composer create-project` downloads a framework, which is
/// minutes on a slow line and belongs in the operation console.
#[tauri::command]
pub async fn project_scaffold(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    template: String,
) -> Result<String> {
    let template = crate::scaffold::Template::parse(&template).ok_or_else(|| {
        Error::new(
            Code::InvalidInput,
            format!("{template} is not a scaffold template"),
        )
    })?;

    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;

    // The installer refuses a non-empty target anyway, but with a worse
    // message and after a pull.
    if dir.exists()
        && dir
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(true)
    {
        return Err(Error::new(
            Code::AlreadyExists,
            format!("projects/{name} already exists and is not empty"),
        )
        .with_hint("Use adoption for existing code — scaffolding is for a brand-new project."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| Error::io("creating the project directory", e))?;

    let user = crate::scaffold::current_user().await;
    let args = crate::scaffold::run_args(template, &dir.display().to_string(), user.as_deref());

    let operation_id = events::next_operation_id("scaffold");
    let outcome = runner::run_operation(
        &app,
        runner::Operation {
            operation_id: &operation_id,
            subject: &name,
            progress_event: "scaffold:progress",
            finished_event: "scaffold:done",
            program: "docker",
            args: &args,
            cwd: &root,
        },
    )
    .await;

    if outcome.is_err() {
        // A failed install that wrote nothing should not leave a husk that
        // blocks the retry; a partial write is kept for inspection.
        let _ = std::fs::remove_dir(&dir);
    }
    outcome?;
    Ok(operation_id)
}

// ----------------------------------------------------------------- workers

/// Which workers this project can offer, from its files alone: `artisan`
/// offers queue and scheduler, `laravel/horizon` in composer.json adds
/// Horizon. A Node project gets an empty list, not an error.
#[tauri::command]
pub fn worker_options(state: State<'_, AppState>, name: String) -> Result<Vec<String>> {
    let root = state.root()?;
    workspace::project_dir(&root, &name)?;
    Ok(crate::worker::available(&root, &name)
        .into_iter()
        .map(|k| k.as_str().to_string())
        .collect())
}

/// Every worker sidecar and its state, restart count included — Docker does
/// the healing (`--restart unless-stopped`), this makes the healing visible.
#[tauri::command]
pub async fn worker_status() -> Result<Vec<crate::worker::WorkerStatus>> {
    crate::worker::status_all().await
}

/// Start one worker as a sidecar built from the project's own image — same
/// PHP, same extensions, same bind mount, same network, so `.env` and the
/// database resolve exactly as they do for the web container.
#[tauri::command]
pub async fn worker_start(state: State<'_, AppState>, name: String, kind: String) -> Result<()> {
    let kind = crate::worker::Kind::parse(&kind)
        .ok_or_else(|| Error::new(Code::InvalidInput, format!("{kind} is not a worker kind")))?;
    let _busy = state
        .inflight
        .acquire(format!("worker:{name}:{}", kind.as_str()))?;
    let root = state.root()?;
    workspace::project_dir(&root, &name)?;

    if !crate::worker::available(&root, &name).contains(&kind) {
        return Err(Error::new(
            Code::Unsupported,
            format!("{name} does not offer a {} worker", kind.as_str()),
        )
        .with_hint("Workers are detected from artisan and composer.json."));
    }

    // The image comes from the project's web container: the one image that is
    // guaranteed to carry the right PHP and extensions for this code.
    let containers = engine::stackvo_containers().await?;
    let image = containers
        .get(&name)
        .and_then(|c| c.image.clone())
        .ok_or_else(|| {
            Error::new(Code::Conflict, format!("{name} has no built container"))
                .with_hint("Build and start the project first — the worker runs its image.")
        })?;

    let network = Env::load(&root)
        .ok()
        .and_then(|env| env.get("DOCKER_DEFAULT_NETWORK").map(str::to_string))
        .unwrap_or_else(|| "stackvo-net".to_string());

    let args = crate::worker::run_args(&name, kind, &image, &root.display().to_string(), &network);

    let output = tokio::process::Command::new("docker")
        .args(&args)
        .output()
        .await
        .map_err(|e| Error::io("running docker", e))?;
    if !output.status.success() {
        return Err(Error::new(
            Code::Conflict,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// Stop one worker. Removal, not just stop: `--restart unless-stopped` means
/// a merely-stopped container is one engine restart away from coming back.
#[tauri::command]
pub async fn worker_stop(state: State<'_, AppState>, name: String, kind: String) -> Result<()> {
    let kind = crate::worker::Kind::parse(&kind)
        .ok_or_else(|| Error::new(Code::InvalidInput, format!("{kind} is not a worker kind")))?;
    let _busy = state
        .inflight
        .acquire(format!("worker:{name}:{}", kind.as_str()))?;

    let container = format!("stackvo-{}", crate::worker::container_id(&name, kind));
    let output = tokio::process::Command::new("docker")
        .args(["rm", "-f", &container])
        .output()
        .await
        .map_err(|e| Error::io("running docker", e))?;
    if !output.status.success() {
        return Err(Error::new(
            Code::NotFound,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ tunnel

/// Every tunnel sidecar and its assigned public URL, where one exists yet.
///
/// The URL is read from the sidecar's own log on every call rather than
/// cached: what the log says is what is actually live, across app restarts
/// and container crashes alike.
#[tauri::command]
pub async fn tunnel_status() -> Result<Vec<crate::tunnel::TunnelStatus>> {
    crate::tunnel::status_all().await
}

/// Start a cloudflared quick-tunnel sidecar for one project.
///
/// An operation, not a mutation: the first start pulls the cloudflared image,
/// which can take minutes and belongs in the operation console. The public
/// URL is not in the return value — Cloudflare assigns it after the container
/// is up, so the UI polls `tunnel_status` until it appears.
#[tauri::command]
pub async fn tunnel_start(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    // Per-project, not global: two projects may tunnel at once, the same one
    // must not race itself.
    let _busy = state.inflight.acquire(format!("tunnel:{name}"))?;
    let root = state.root()?;

    let manifest = manifest::read(
        &workspace::project_dir(&root, &name)?.join("stackvo.json"),
        &name,
    )?;
    crate::tunnel::ensure_project_running(&name).await?;

    let network = Env::load(&root)
        .ok()
        .and_then(|env| env.get("DOCKER_DEFAULT_NETWORK").map(str::to_string))
        .unwrap_or_else(|| "stackvo-net".to_string());
    let args = crate::tunnel::run_args(
        &name,
        manifest.domain.as_deref(),
        crate::tunnel::internal_port(&manifest),
        &network,
    );

    let operation_id = events::next_operation_id("tunnel");
    runner::run_operation(
        &app,
        runner::Operation {
            operation_id: &operation_id,
            subject: &name,
            progress_event: "tunnel:progress",
            finished_event: "tunnel:done",
            program: "docker",
            args: &args,
            cwd: &root,
        },
    )
    .await?;
    Ok(operation_id)
}

/// Stop a project's tunnel. The sidecar runs with `--rm`, so stopping is
/// also removal — nothing is left behind to leak the old URL.
#[tauri::command]
pub async fn tunnel_stop(state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("tunnel:{name}"))?;
    engine::stop_container(&crate::tunnel::container_id(&name)).await
}

/// Reclaim space from dangling images and — only when explicitly asked —
/// unused volumes.
///
/// Volumes are opt-in per call rather than a default, because the engine's
/// "unused" means "not currently mounted": the database of a project that
/// happens to be stopped qualifies. The UI states this before offering it.
#[tauri::command]
pub async fn docker_prune(
    state: State<'_, AppState>,
    images: bool,
    volumes: bool,
) -> Result<engine::PruneReport> {
    // One prune at a time: two concurrent passes double-report the same bytes.
    let _busy = state.inflight.acquire("prune")?;
    engine::prune(images, volumes).await
}

// ---------------------------------------------------------------- preferences

/// User preferences, stored beside the workspace pointer.
///
/// Replaces the localStorage-backed `usePreferences` composable: a webview's
/// localStorage is cleared by a cache reset, and the editor command needs to be
/// readable from Rust anyway.
#[tauri::command]
pub fn prefs_get() -> Result<serde_json::Value> {
    let path = prefs_path()?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_else(|_| default_prefs())),
        Err(_) => Ok(default_prefs()),
    }
}

#[tauri::command]
pub fn prefs_set(patch: serde_json::Value) -> Result<serde_json::Value> {
    // Same read-modify-write hazard as `.env`, and the same answer. Two settings
    // changed in quick succession — a theme toggle and a language change — would
    // otherwise each read the file, merge into their own copy, and write back;
    // the second write drops the first. Held only across this synchronous body,
    // so it never crosses an await.
    static WRITE_LOCK: Mutex<()> = Mutex::new(());
    let _serialised = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut current = prefs_get()?;

    // Shallow merge, so a caller can send one key without clobbering the rest.
    if let (Some(base), Some(incoming)) = (current.as_object_mut(), patch.as_object()) {
        for (k, v) in incoming {
            base.insert(k.clone(), v.clone());
        }
    }

    let path = prefs_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io("creating the config directory", e))?;
    }
    // Atomic, for the same reason the manifest is: a truncated preferences.json
    // is unparseable, and the app would silently fall back to defaults.
    crate::atomic::write(&path, &serde_json::to_string_pretty(&current)?)?;

    Ok(current)
}

/// Where the log is, how big it is, and whether there is one at all.
///
/// A support instruction of "send me your log" is only actionable if the app
/// can point at it. The path differs per platform and none of the three is
/// somewhere a user would think to look.
#[tauri::command]
pub fn logs_info() -> serde_json::Value {
    let dir = crate::logging::dir();
    let newest = crate::logging::newest_file();

    serde_json::json!({
        "directory": dir.as_ref().map(|d| d.display().to_string()),
        "newestFile": newest.as_ref().map(|f| f.display().to_string()),
        "totalBytes": dir.as_ref().map(|d| crate::logging::total_bytes(d)).unwrap_or(0),
    })
}

/// One `.env` value, unmasked, because the user asked for that one.
///
/// `env_get` and the service list hand over secrets as bullets on purpose: a
/// password that crosses the boundary by default is in every screenshot of the
/// page that shows it. This is the deliberate exception — a single key, on a
/// click, so revealing a database password is an act rather than a default.
#[tauri::command]
pub fn env_reveal(state: State<'_, AppState>, key: String) -> Result<String> {
    let env = Env::load(&state.root()?)?;

    env.get(&key)
        .map(str::to_string)
        .ok_or_else(|| Error::new(Code::NotFound, format!("{key} is not set in .env")))
}

// ---------------------------------------------------------------- system accent

/// The accent colour the user picked in System Settings.
///
/// Read rather than guessed so the app can match the rest of the desktop. macOS
/// stores the choice in the global preference domain; the value that names it is
/// `AppleHighlightColor`, whose last field is the accent's name — the leading
/// floats are the *selection* tint, a paler variant that would be unreadable as
/// a primary. The names map to the accent colours themselves.
///
/// Absent means "multicolour", which is macOS's default and resolves to blue.
///
/// Shelling out to `defaults` rather than linking AppKit: one process for a
/// value read a few times a session is cheaper than an Objective-C bridge in
/// the dependency tree, and it cannot panic inside someone else's runtime.
#[tauri::command]
pub fn system_accent() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        // The accent colours macOS itself draws with, keyed by the name it
        // writes into the preference.
        const ACCENTS: [(&str, &str); 8] = [
            ("Blue", "#007AFF"),
            ("Purple", "#A550A7"),
            ("Pink", "#F74F9E"),
            ("Red", "#FF5257"),
            ("Orange", "#F7821B"),
            ("Yellow", "#FFC600"),
            ("Green", "#62BA46"),
            ("Graphite", "#8C8C8C"),
        ];

        let output = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleHighlightColor"])
            .output();

        let name = match &output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .last()
                .unwrap_or("Blue")
                .to_string(),
            // Unset is not an error: it is the default multicolour accent.
            _ => "Blue".to_string(),
        };

        let hex = ACCENTS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, hex)| *hex)
            .unwrap_or("#007AFF");

        serde_json::json!({ "available": true, "name": name, "hex": hex })
    }

    // Windows has an accent colour too, but it lives in the registry and this
    // app is not built there yet; saying so beats returning a wrong blue.
    #[cfg(not(target_os = "macos"))]
    serde_json::json!({ "available": false, "name": null, "hex": null })
}

/// Is this build capable of verifying an update?
///
/// Tauri checks a bundle's signature against the public key compiled into the
/// app. With an empty `pubkey` there is nothing to check against, so every
/// update attempt fails — and it fails deep inside the plugin, with a message
/// about signatures that reads like a server problem rather than a build
/// problem. The UI needs to be able to say which one it is.
///
/// Read from the same file that gets compiled in, via `include_str!`, for the
/// reason `contracts.rs` does the same: a value parsed at runtime from
/// somewhere else could disagree with the one actually in the binary.
#[tauri::command]
pub fn updater_status() -> serde_json::Value {
    const CONF: &str = include_str!("../tauri.conf.json");

    let conf: serde_json::Value = serde_json::from_str(CONF).unwrap_or(serde_json::Value::Null);
    let updater = conf
        .get("plugins")
        .and_then(|p| p.get("updater"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let pubkey = updater
        .get("pubkey")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let endpoints = updater
        .get("endpoints")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    serde_json::json!({
        "configured": !pubkey.is_empty() && endpoints > 0,
        "hasPublicKey": !pubkey.is_empty(),
        "endpoints": endpoints,
    })
}

/// The user's language for the native surfaces (the tray).
///
/// Falls back to the OS locale and then English. The front end owns the
/// setting; this reads what it stored.
pub fn preferred_locale() -> String {
    prefs_get()
        .ok()
        .and_then(|p| {
            p.get("locale")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en".to_string())
}

/// Re-label the tray after a language change, so the setting takes effect
/// without a restart.
#[tauri::command]
pub fn tray_relabel(app: AppHandle) {
    crate::tray::relabel(&app);
}

#[cfg(test)]
mod migrate_tests {
    use super::*;

    fn detected(runtime: &'static str) -> detect::Detected {
        detect::Detected {
            framework: None,
            runtime,
            server: "nginx",
            document_root: Some("public".into()),
            php_version: Some("8.2".into()),
            node_version: Some("20".into()),
            node_port: Some(3000),
            node_start: Some("npm run dev".into()),
            confidence: detect::Confidence::Likely,
            evidence: vec![],
        }
    }

    fn migration() -> crate::migrate::Migration {
        crate::migrate::Migration {
            source: "/w/shop/docker-compose.yml".into(),
            app_service: Some("app".into()),
            runtime: Some("php".into()),
            server: Some("apache".into()),
            php_version: Some("8.3".into()),
            document_root: Some("web".into()),
            domain: Some("shop.test".into()),
            extensions: vec!["pdo_mysql".into(), "gd".into()],
            ..Default::default()
        }
    }

    /// The whole point of the merge, and the thing that decides whether the
    /// import is worth anything: where both have an answer, the compose file
    /// wins. Detection *guesses* from the shape of the code; the compose file
    /// records what its author decided.
    #[test]
    fn a_declaration_beats_a_guess() {
        let spec = migrated_spec("shop", &detected("php"), &migration());

        assert_eq!(spec["domain"], "shop.test");
        assert_eq!(spec["server"], "apache");
        assert_eq!(spec["document_root"], "web");
        assert_eq!(spec["php"]["version"], "8.3");
        assert_eq!(
            spec["php"]["extensions"],
            serde_json::json!(["pdo_mysql", "gd"])
        );
    }

    /// And the merged spec has to actually validate. A review of a spec that
    /// adoption would then refuse is a review of nothing — which is why
    /// migrate_scan runs this check before returning, and why it is worth an
    /// assertion here rather than a discovery at apply time.
    #[test]
    fn the_merged_spec_satisfies_the_same_contract_a_created_project_does() {
        let spec = migrated_spec("shop", &detected("php"), &migration());
        parse_spec(&spec, "shop").expect("the merged spec must validate");
    }

    /// Detection saying "php" and the compose file saying "node" is a real
    /// disagreement — a Laravel repo whose compose file runs only the Vite
    /// container. The runtime blocks are mutually exclusive in the contract, so
    /// switching has to take the PHP keys with it or the spec describes two
    /// runtimes and is refused.
    #[test]
    fn switching_runtime_removes_the_other_runtime_s_keys() {
        let node = crate::migrate::Migration {
            runtime: Some("node".into()),
            node_version: Some("22".into()),
            port: Some(5173),
            domain: Some("app.test".into()),
            ..Default::default()
        };

        let spec = migrated_spec("app", &detected("php"), &node);

        assert_eq!(spec["runtime"], "node");
        assert!(spec.get("php").is_none(), "php block survived: {spec}");
        assert!(spec.get("server").is_none());
        assert!(spec.get("document_root").is_none());
        assert_eq!(spec["node"]["version"], "22");
        assert_eq!(spec["node"]["port"], 5173);

        parse_spec(&spec, "app").expect("the switched spec must validate");
    }

    /// A compose file that states nothing extra must leave detection alone
    /// rather than overwrite it with nulls.
    #[test]
    fn an_empty_migration_changes_nothing() {
        let plain = detected_spec("shop", &detected("php"));
        let merged = migrated_spec("shop", &detected("php"), &Default::default());
        assert_eq!(plain, merged);
    }
}

#[cfg(test)]
mod prefs_tests {
    use super::*;

    /// The failure this guards: two settings changed at once, one silently lost.
    ///
    /// Exercised through the real merge-and-write path rather than a copy of it,
    /// so the lock being removed actually breaks the test.
    #[test]
    fn concurrent_preference_writes_do_not_lose_each_other() {
        // prefs_path() is a single fixed location per user, so this test writes
        // where the app writes. It restores whatever was there.
        let path = match prefs_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        let original = std::fs::read_to_string(&path).ok();

        let _ = prefs_set(serde_json::json!({
            "theme": "system", "locale": null, "editorCommand": null
        }));

        let keys = ["theme", "locale", "editorCommand", "notifyOnBuild"];
        let values = [
            serde_json::json!("dark"),
            serde_json::json!("tr"),
            serde_json::json!("code"),
            serde_json::json!(false),
        ];

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(keys.len()));
        let handles: Vec<_> = keys
            .iter()
            .zip(values.iter())
            .map(|(k, v)| {
                let (k, v) = ((*k).to_string(), v.clone());
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    prefs_set(serde_json::json!({ k: v })).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let after = prefs_get().unwrap();
        assert_eq!(after["theme"], "dark");
        assert_eq!(after["locale"], "tr");
        assert_eq!(after["editorCommand"], "code");
        assert_eq!(after["notifyOnBuild"], false);

        match original {
            Some(text) => std::fs::write(&path, text).unwrap(),
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

fn prefs_path() -> Result<std::path::PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("dev.stackvo.desktop").join("preferences.json"))
        .ok_or_else(|| Error::new(Code::IoError, "cannot determine the OS config directory"))
}

fn default_prefs() -> serde_json::Value {
    serde_json::json!({
        "locale": null,
        "theme": "system",
        "editorCommand": null,
        "terminalApp": null,
        "startMinimized": false,
        "closeBehaviour": "ask",
        "autostart": false,
        "notifyOnBuild": true
    })
}

// ---------------------------------------------------------------- OS integration

/// Open a path in the user's editor.
///
/// Unlike `open_path` (which the frontend does through the opener plugin), this
/// needs real logic: find an editor, and fall back to the OS handler rather
/// than failing when there is none.
#[tauri::command]
pub fn open_in_editor(state: State<'_, AppState>, path: String) -> Result<()> {
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(Error::not_found(path));
    }

    // Confined to the workspace. The only caller passes a project directory, and
    // an editor is launched as a subprocess with this path as its argument —
    // there is no reason for the boundary to accept anything else, and "the
    // front end only ever sends good values" is not a boundary.
    let root = state.root()?;
    let (Ok(resolved), Ok(root)) = (target.canonicalize(), root.canonicalize()) else {
        return Err(Error::new(
            Code::IoError,
            format!("could not resolve {}", target.display()),
        ));
    };
    if !resolved.starts_with(&root) {
        return Err(Error::new(
            Code::InvalidInput,
            "refusing to open a path outside the StackVo directory",
        )
        .with_hint("Only project folders inside the selected workspace can be opened."));
    }

    // An explicit preference wins; otherwise walk the catalogue in order. Both
    // paths go through `resolve_editor`, so an editor installed only as a macOS
    // bundle is launchable either way — spawning the launcher blindly, as this
    // used to, reports "no editor found" on a machine that has one.
    let configured = prefs_get()
        .ok()
        .and_then(|p| {
            p.get("editorCommand")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty());

    let ids: Vec<String> = match configured {
        Some(id) => vec![id],
        None => crate::apps::editors()
            .into_iter()
            .filter(|a| a.available)
            .map(|a| a.id)
            .collect(),
    };

    for id in ids {
        let Some(launch) = crate::apps::resolve_editor(&id) else {
            continue;
        };
        let spawned = match launch {
            crate::apps::Launch::Command(cmd) => {
                std::process::Command::new(cmd).arg(&target).spawn().is_ok()
            }
            // `open -a` is what Finder does; it needs no CLI helper installed.
            crate::apps::Launch::Bundle(bundle) => std::process::Command::new("open")
                .args(["-a", bundle])
                .arg(&target)
                .spawn()
                .is_ok(),
        };
        if spawned {
            return Ok(());
        }
    }

    Err(Error::new(Code::NotFound, "No editor found.")
        .with_hint("Choose an editor in Settings, or open the folder manually."))
}

#[tauri::command]
pub fn workspace_pick(
    app: AppHandle,
    state: State<'_, AppState>,
    watcher: State<'_, crate::watcher::Handle>,
) -> Result<Option<Workspace>> {
    use tauri_plugin_dialog::DialogExt;

    // blocking_pick_folder must not run on the main thread on macOS; Tauri's
    // command threadpool is already off it, so this is safe here.
    let picked = app.dialog().file().blocking_pick_folder();

    let Some(folder) = picked else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|e| Error::new(Code::IoError, format!("could not resolve the folder: {e}")))?;

    let ws = workspace::set(&path)?;
    if let Ok(mut cached) = state.workspace.lock() {
        *cached = ws.clone();
    }
    watcher.retarget(&app, ws.require_root().ok());
    Ok(Some(ws))
}

/// One round of per-container sampling, called from the background timer.
///
/// Bounded to the last 120 samples (two hours at the 60s interval) — an app
/// left open for a week must not accumulate an unbounded series per container.
pub async fn sample_container_stats(app: &AppHandle) {
    use tauri::Manager;

    let Ok(containers) = engine::stackvo_containers().await else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Hold the state handle across the loop: taking it per iteration creates a
    // temporary that the MutexGuard outlives.
    let state = app.state::<AppState>();

    // Drop series for containers that no longer exist. Each series is capped at
    // 120 samples, but nothing was capping the number of series: a project
    // deleted, or a container renamed, left its history behind for the lifetime
    // of the process. Slow, but unbounded, and this is a long-running app.
    //
    // `stackvo_containers` lists with all(true), so a stopped container is still
    // here and keeps its history — the detail page draws it for a container that
    // is not running.
    let live: std::collections::HashSet<String> = containers
        .keys()
        .map(|id| engine::container_name(id))
        .collect();
    if let Ok(mut history) = state.stats_history.lock() {
        history.retain(|name, _| live.contains(name));
    }

    for (id, info) in containers {
        if !info.running {
            continue;
        }
        let Ok(stats) = engine::container_stats(&id).await else {
            continue;
        };

        if let Ok(mut history) = state.stats_history.lock() {
            let series = history.entry(engine::container_name(&id)).or_default();
            series.push((now, stats.cpu_percent, stats.memory_percent));
            if series.len() > 120 {
                let excess = series.len() - 120;
                series.drain(0..excess);
            }
        }
    }
}

// ---------------------------------------------------------------- generator preview

/// Render a project's Dockerfile with the Rust generator, without writing it.
///
/// The Bash generator remains the one that actually produces build inputs; this
/// is the port running alongside it so its output can be compared before it
/// takes over. Strict mode is the point: where Bash silently drops an
/// incompatible extension, this refuses and says which one.
#[tauri::command]
pub fn project_dockerfile_preview(
    state: State<'_, AppState>,
    name: String,
    strict: Option<bool>,
) -> Result<serde_json::Value> {
    let root = state.root()?;
    let m = manifest::read(
        &workspace::project_dir(&root, &name)?.join("stackvo.json"),
        &name,
    )?;
    let env = Env::load(&root)?;

    let opts = crate::generator::ToolchainOptions {
        tools: env.list("PHP_DEFAULT_TOOLS"),
        apt_packages: env.list("PHP_DEFAULT_APT_PACKAGES"),
        composer_version: env
            .get("PHP_TOOL_COMPOSER_VERSION")
            .unwrap_or("latest")
            .to_string(),
        nodejs_version: env
            .get("PHP_TOOL_NODEJS_VERSION")
            .unwrap_or("20")
            .to_string(),
    };

    let strict = strict.unwrap_or(true);
    let rendered = crate::generator::render_from_manifest(&m, &opts, strict)
        .map_err(|e| Error::new(Code::Unsupported, e))?;

    // What Bash would drop without telling anyone.
    let skipped = m
        .php
        .as_ref()
        .and_then(|php| crate::generator::resolve(&php.version, &php.extensions, false).ok())
        .map(|plan| plan.skipped)
        .unwrap_or_default();

    // Where the Bash generator puts its version, so the two can be diffed.
    let bash_path = if m.runtime == "node" {
        workspace::project_dir(&root, &name)?.join("Dockerfile")
    } else {
        root.join("generated/projects")
            .join(&name)
            .join("Dockerfile")
    };

    Ok(serde_json::json!({
        "project": name,
        "runtime": m.runtime,
        "server": m.server,
        "dockerfile": rendered,
        "skipped": skipped.into_iter().map(|(ext, reason)| {
            serde_json::json!({ "extension": ext, "reason": reason })
        }).collect::<Vec<_>>(),
        "bashOutputPath": bash_path.display().to_string(),
        "matchesBashOutput": std::fs::read_to_string(&bash_path)
            .map(|existing| existing == rendered)
            .unwrap_or(false),
    }))
}

// ---------------------------------------------------------------- generator verification

/// Render every generated file with the Rust generator and compare it against
/// what the Bash generator actually wrote.
///
/// This is the migration path, not a curiosity. The Rust port cannot replace
/// the Bash generator on the strength of a fixture suite alone — fixtures cover
/// the cases someone thought to write down. This runs the comparison against
/// the user's real projects and real `.env`, so a divergence shows up on their
/// machine before anything depends on it.
///
/// Reads only. It never writes a generated file.
#[tauri::command]
pub fn generator_verify(state: State<'_, AppState>) -> Result<serde_json::Value> {
    verify_generator(&state.root()?)
}

/// The command's logic, free of Tauri `State` so the `diagnose` example runs
/// exactly the same comparison the app does.
pub fn verify_generator(root: &std::path::Path) -> Result<serde_json::Value> {
    use crate::generator;

    let env = Env::load(root)?;

    let opts = generator::ToolchainOptions {
        tools: env.list("PHP_DEFAULT_TOOLS"),
        apt_packages: env.list("PHP_DEFAULT_APT_PACKAGES"),
        composer_version: env
            .get("PHP_TOOL_COMPOSER_VERSION")
            .unwrap_or("latest")
            .to_string(),
        nodejs_version: env
            .get("PHP_TOOL_NODEJS_VERSION")
            .unwrap_or("20")
            .to_string(),
    };

    let mut files = Vec::new();

    // A free function rather than a closure: the closure would hold a mutable
    // borrow of `files` for its whole lifetime, and the Dockerfile loop needs
    // to push error entries of its own.
    fn compare(
        files: &mut Vec<serde_json::Value>,
        label: String,
        path: std::path::PathBuf,
        ours: String,
    ) {
        let theirs = std::fs::read_to_string(&path).ok();
        let (status, at) = match &theirs {
            None => ("missing", None),
            Some(t) if *t == ours => ("match", None),
            Some(t) => (
                "differ",
                ours.lines()
                    .zip(t.lines())
                    .position(|(a, b)| a != b)
                    .map(|i| i as u64 + 1),
            ),
        };
        files.push(serde_json::json!({
            "file": label,
            "path": path.display().to_string(),
            "status": status,
            "firstDifferenceLine": at,
        }));
    }

    // ---- per-project Dockerfiles ----
    let mut manifests: Vec<(String, crate::manifest::Manifest)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("projects")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') || !path.join("stackvo.json").is_file() {
                continue;
            }
            let Ok(m) = crate::manifest::read(&path.join("stackvo.json"), name) else {
                continue;
            };

            // Node writes into the project source dir, PHP into generated/ (C-19).
            let bash_path = if m.runtime == "node" {
                path.join("Dockerfile")
            } else {
                root.join("generated/projects")
                    .join(name)
                    .join("Dockerfile")
            };

            match generator::render_from_manifest(&m, &opts, false) {
                Ok(ours) => compare(&mut files, format!("{name}/Dockerfile"), bash_path, ours),
                Err(e) => files.push(serde_json::json!({
                    "file": format!("{name}/Dockerfile"),
                    "status": "error",
                    "error": e,
                })),
            }
            manifests.push((name.to_string(), m));
        }
    }

    // ---- compose ----
    let projects = generator::compose_projects_from(&manifests);
    compare(
        &mut files,
        "docker-compose.projects.yml".into(),
        root.join("generated/docker-compose.projects.yml"),
        generator::render_compose_projects(&projects, &root.display().to_string()),
    );

    // ---- traefik ----
    let catalog = env_schema().service_catalog();
    let services: Vec<(&str, bool, Option<&str>)> = catalog
        .iter()
        .map(|(id, _)| (id.as_str(), env.service_enabled(id), env.service_url(id)))
        .collect();

    let traefik = generator::TraefikOptions {
        tld_suffix: env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc"),
        network: env.get("DOCKER_DEFAULT_NETWORK").unwrap_or("stackvo-net"),
        ssl_enabled: env.bool("SSL_ENABLE"),
        redirect_to_https: env.bool("REDIRECT_TO_HTTPS"),
        services,
    };

    compare(
        &mut files,
        "traefik/traefik.yml".into(),
        root.join("generated/traefik/traefik.yml"),
        generator::render_traefik_config(&traefik),
    );
    compare(
        &mut files,
        "traefik/dynamic/routes.yml".into(),
        root.join("generated/traefik/dynamic/routes.yml"),
        generator::render_traefik_routes(&traefik),
    );

    let matched = files.iter().filter(|f| f["status"] == "match").count();
    let differed = files.iter().filter(|f| f["status"] == "differ").count();

    Ok(serde_json::json!({
        "files": files,
        "matched": matched,
        "differed": differed,
        "readyToTakeOver": differed == 0,
        // Surfaced here because the desktop app can say the routing is broken;
        // StackVo itself never does. See CONFLICTS.md C-20.
        "warnings": generator::traefik_routing_warning(&traefik)
            .map(|w| vec![w])
            .unwrap_or_default(),
    }))
}

// ---------------------------------------------------------------- staged takeover

/// Which generator produces the files.
///
/// The port cannot simply replace the Bash generator: its output is the input
/// to every container the user runs, so "probably identical" is not a standard
/// worth shipping. These modes make the handover reversible and self-checking.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeneratorEngine {
    /// Bash writes the files. The default, and what StackVo does today.
    #[default]
    Bash,
    /// Bash writes, then the Rust port renders the same files and the two are
    /// compared. Divergence is reported, never acted on.
    Verify,
    /// Rust writes — but only after Bash has produced the same bytes. If any
    /// file differs, nothing is written and the operation fails with the diff.
    /// This is a takeover that cannot silently change anyone's images.
    Rust,
}

#[tauri::command]
pub async fn generate_with(
    app: AppHandle,
    state: State<'_, AppState>,
    scope: Option<String>,
    engine_mode: Option<GeneratorEngine>,
) -> Result<serde_json::Value> {
    // Writes the same files as generate_run, so it shares the same key.
    let _busy = state.inflight.acquire("stack")?;
    let root = state.root()?;
    let scope = scope.unwrap_or_else(|| "all".into());
    let mode = engine_mode.unwrap_or_default();
    let operation_id = events::next_operation_id("generate");

    events::emit(
        &app,
        "generate:start",
        serde_json::json!({ "operationId": operation_id, "scope": scope, "engine": format!("{mode:?}") }),
    );

    // Bash runs in every mode. In Rust mode it is the reference the port has to
    // reproduce before it is allowed to write anything.
    generate(&app, &root, &operation_id, &scope).await?;

    if mode == GeneratorEngine::Bash {
        return Ok(serde_json::json!({ "operationId": operation_id, "engine": "bash" }));
    }

    let report = verify_generator(&root)?;
    let differed = report["differed"].as_u64().unwrap_or(0);

    if mode == GeneratorEngine::Verify {
        return Ok(serde_json::json!({
            "operationId": operation_id,
            "engine": "verify",
            "report": report,
        }));
    }

    // Rust mode. Refuse rather than write over a disagreement.
    if differed > 0 {
        return Err(Error::new(
            Code::GenerateFailed,
            format!("The Rust generator disagrees with Bash on {differed} file(s); nothing was written."),
        )
        .with_hint("Bash output is still in place. Report the diff before switching engines.")
        .with_details(report));
    }

    // Every file already matches, so writing is a no-op on content — which is
    // exactly the property that makes this safe to turn on.
    Ok(serde_json::json!({
        "operationId": operation_id,
        "engine": "rust",
        "report": report,
        "note": "Rust output is byte-identical to Bash; the files on disk are unchanged.",
    }))
}
