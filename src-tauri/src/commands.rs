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

/// Record that the first-run setup finished.
///
/// Called by the screen that runs it, after its last step — not by any of the
/// steps. The difference is the whole reason this exists rather than deriving
/// the answer from a file one of them writes: a setup that generated the
/// compose files and then failed to issue a certificate used to look finished
/// for ever, and the stack it left could not serve a single domain.
#[tauri::command]
pub fn bootstrap_complete(state: State<'_, AppState>) -> Result<()> {
    let root = state.root()?;
    workspace::mark_bootstrapped(&root)
}

#[tauri::command]
pub fn workspace_get(state: State<'_, AppState>) -> Result<Workspace> {
    let ws = workspace::resolve();
    if let Ok(mut cached) = state.workspace.lock() {
        *cached = ws.clone();
    }
    Ok(ws)
}

/// Point the app at a project tree.
///
/// This used to choose the app's own directory as well, because there was one
/// directory and it held both. The app root is derived now, so the only thing
/// left to choose is where the user's code lives.
#[tauri::command]
pub fn workspace_set(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
    watcher: State<'_, crate::watcher::Handle>,
) -> Result<Workspace> {
    let ws = workspace::set_projects(&path)?;
    if let Ok(mut cached) = state.workspace.lock() {
        *cached = ws.clone();
    }
    // Move the file watcher with the choice, or it keeps reporting changes in
    // the tree the user just left. It takes the app root and reads the pointer
    // itself — handing it the project directory would make it watch a
    // `projects/` folder one level further down that nobody has.
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
    /// The manifest has been edited since anything was generated from it.
    ///
    /// Measured from the two files' timestamps rather than remembered from
    /// watcher events, so it is right on first load — an edit made while the
    /// app was closed used to go unreported — and it stops being true the
    /// moment a regenerate makes it untrue.
    pub generated_stale: bool,
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
    let projects_dir = crate::workspace::require_projects_root(root)?;

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
                        lang: None,
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
                generated_stale: crate::doctor::project_generated_is_stale(root, &dir_name),
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

/// One editable `SERVICE_<ID>_*` setting.
///
/// Distinct from [`Credential`], which exists to *display* what a service is
/// reachable with: it hides `ENABLE`, `VERSION` and `URL`, and drops anything
/// empty. An editor needs the opposite — every key the service has, empty ones
/// included, because an empty value is the one most likely to want filling in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSetting {
    /// The key without its `SERVICE_<ID>_` prefix — `ROOT_PASSWORD`.
    pub key: String,
    pub env_key: String,
    /// Masked when `secret`. Revealing one goes through `env_reveal`, the same
    /// path the credentials list uses.
    pub value: String,
    pub secret: bool,
    /// True when the value is what the binary ships, so the sheet can say so
    /// rather than presenting a default as somebody's decision.
    pub is_default: bool,
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
    /// How many extensions a manifest may carry.
    ///
    /// This used to be a hard 50 — the Bash extractor's `grep -A 50` window,
    /// CONFLICTS.md C-04, which promised to lift the moment the generator was
    /// ported. It has been: nothing reads `stackvo.json` with grep any more, so
    /// the ceiling is now simply the size of the catalog. Still exposed, and
    /// still derived rather than hardcoded, so the picker's counter cannot
    /// disagree with the list it is counting.
    pub max_extensions: usize,
}

/// Every runtime the Rust generator can build. The Bash CLI still knows only
/// php and node; since Sprint 17 the app generates for itself, so the four
/// lang runtimes exist here first (C-02, closed).
const IMPLEMENTED_RUNTIMES: [&str; 6] = ["php", "node", "python", "go", "ruby", "rust"];

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
    let php_ext: Vec<ExtensionOption> = catalog_names
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
        servers: env.get("SUPPORTED_SERVERS").map_or_else(
            || vec!["nginx".to_string()],
            |v| v.split(',').map(|s| s.trim().to_string()).collect(),
        ),
        default_server: env
            .get("SUPPORTED_SERVERS_DEFAULT")
            .unwrap_or("nginx")
            .to_string(),
        max_extensions: php_ext.len(),
        php_extensions: php_ext,
    })
}

/// A server's extra directives, as the user last saved them.
///
/// The raw file, comments and all — the stripping that keeps an untouched
/// workspace byte-identical happens at render time, not here. An editor that
/// showed the stripped version would delete the instructions the first time it
/// was saved.
///
/// A workspace with no file falls back to the copy in the binary, which is
/// eighteen lines of explanation and not one directive — so it renders to
/// nothing either way, and the editor opens on the instructions rather than on
/// an empty box. That mattered from the moment `install` stopped writing the
/// file: an empty editor does not tell anybody that nginx directives are a
/// thing they can add, and this pane is the only place that says so.
#[tauri::command]
pub fn server_config_get(state: State<'_, AppState>, server: String) -> Result<String> {
    let root = state.root()?;
    let path = checked_server_config(&root, &server)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(crate::skeleton::read_template(
            &root,
            &format!("core/servers/{server}.conf"),
        )
        .unwrap_or_default()),
        Err(e) => Err(Error::io(format!("reading {}", path.display()), e)),
    }
}

/// One shipped file, and whether this workspace has taken it over.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateFile {
    /// Relative to the workspace root, and the id every other call takes.
    pub path: String,
    /// There is a copy on disk, so the workspace's version is what renders.
    pub overridden: bool,
}

/// Every template the app ships, and which ones this workspace has changed.
///
/// This list could not be produced at all until installing stopped copying
/// everything to disk: with all thirty files present in every workspace, "has
/// a copy" meant "was installed" rather than "was changed".
#[tauri::command]
pub fn templates_list(state: State<'_, AppState>) -> Result<Vec<TemplateFile>> {
    let root = state.root()?;
    Ok(crate::skeleton::overridable()
        .into_iter()
        .map(|path| TemplateFile {
            overridden: root.join(&path).is_file(),
            path,
        })
        .collect())
}

/// Copy the shipped file into the workspace and return where it landed.
///
/// The absolute path is the useful return: the caller's next move is to open
/// the file in the user's own editor, which is a better place to edit compose
/// YAML than a box in a settings pane.
#[tauri::command]
pub fn template_override(state: State<'_, AppState>, path: String) -> Result<String> {
    let root = state.root()?;
    crate::skeleton::materialize(&root, &path)?;
    Ok(root.join(&path).display().to_string())
}

/// Drop the workspace's copy and go back to the version in the binary.
///
/// Destructive by definition — the file being deleted is the user's edit — so
/// the front end asks first. Nothing here is undoable.
#[tauri::command]
pub fn template_revert(state: State<'_, AppState>, path: String) -> Result<()> {
    let root = state.root()?;
    crate::skeleton::revert(&root, &path)
}

#[tauri::command]
pub fn server_config_set(
    state: State<'_, AppState>,
    server: String,
    content: String,
) -> Result<()> {
    let root = state.root()?;
    let path = checked_server_config(&root, &server)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    crate::atomic::write(&path, &content)
}

/// Only the servers whose config is generated as a file.
///
/// Apache is configured by `sed` inside its own Dockerfile and Swoole by an
/// inline script, so there is nothing for a snippet to be added to. Accepting
/// the name anyway would write a file that is never read — the exact shape of
/// `core/templates/servers/`, which is what this replaced.
fn checked_server_config(root: &std::path::Path, server: &str) -> Result<std::path::PathBuf> {
    if !matches!(server, "nginx" | "caddy" | "frankenphp") {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{server} is not configured through a file"),
        )
        .with_hint(crate::hints::SERVER_DIRECTIVES_UNSUPPORTED));
    }
    Ok(crate::generator::server_config_path(root, server))
}

// ---------------------------------------------------------------- env

#[tauri::command]
pub fn env_get(state: State<'_, AppState>) -> Result<std::collections::BTreeMap<String, String>> {
    let root = state.root()?;
    // Secret-suffixed values never cross the boundary; see env.schema.json.
    Ok(Env::load(&root)?.redacted())
}

/// The defaults the binary carries, so the UI can tell a decision from a
/// default.
///
/// `env_get` returns the merged view, which is what most callers want and
/// exactly the wrong thing for a settings form: every value looks equally
/// chosen, including the ones nobody chose. With this the form can say "this
/// is the default" and offer to go back to it, which is the difference between
/// a settings screen and a wall of populated text fields.
#[tauri::command]
pub fn env_defaults() -> std::collections::BTreeMap<String, String> {
    crate::config::EMBEDDED
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

// ================================================================ Phase 2
// Mutating commands. Everything above this line only reads.

use crate::env_writer;
use crate::events::{self, Lifecycle, SubjectEvent};
use crate::runner;
use tauri::AppHandle;

/// Shared body for the six start/stop/restart commands, which differ only by
/// verb, subject kind and event prefix.
#[tracing::instrument(skip(sink, phase), fields(action = phase.pending))]
async fn lifecycle(
    sink: &dyn crate::progress::ProgressSink,
    kind: &'static str,
    id: &str,
    phase: Lifecycle,
) -> Result<()> {
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

    crate::progress::emit(sink, &subject(phase.pending), make(id));

    let result = match phase.pending {
        "starting" => engine::start_container(id).await,
        "stopping" => engine::stop_container(id).await,
        _ => engine::restart_container(id).await,
    };

    match result {
        Ok(()) => {
            crate::progress::emit(
                sink,
                &subject(phase.done),
                make(id).running(phase.running_after),
            );
            Ok(())
        }
        Err(e) => {
            crate::progress::emit(sink, &subject("error"), make(id).error(e.message.clone()));
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn project_start(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&events::sink(&app), "project", &name, events::START).await
}

#[tauri::command]
pub async fn project_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&events::sink(&app), "project", &name, events::STOP).await
}

#[tauri::command]
pub async fn project_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&events::sink(&app), "project", &name, events::RESTART).await
}

#[tauri::command]
pub async fn service_start(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&events::sink(&app), "service", &name, events::START).await
}

#[tauri::command]
pub async fn service_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&events::sink(&app), "service", &name, events::STOP).await
}

#[tauri::command]
pub async fn service_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    lifecycle(&events::sink(&app), "service", &name, events::RESTART).await
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
                        historic: None,
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
                            historic: None,
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
                            // The seed, so the UI can draw the live boundary
                            // after it rather than passing old lines off as
                            // output that just arrived.
                            historic: line.historic.then_some(true),
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
        .with_hint(crate::hints::SERVICE_MUST_BE_IN_CATALOG))
}

/// Is every key in `patch` a setting this service owns, and is every value one
/// that can safely be written?
///
/// Two separate refusals, both of them things a UI can do by accident.
///
/// The prefix check keeps this from being a general `.env` writer that happens
/// to restart a container — it is reached from a sheet whose whole framing is
/// "these are Redis's settings", and it should mean that. `ENABLE` is excluded
/// because the services list owns that toggle; two controls for one key is how
/// they come to disagree.
///
/// The mask check is the sharper one. A read returns the bullet string for a
/// secret, so a form that round-trips what it was given would save the mask as
/// the password and lock the service out of its own database.
fn check_service_patch(
    name: &str,
    patch: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let prefix = Env::service_prefix(name);
    let enable = format!("{prefix}ENABLE");

    for (key, value) in patch {
        if !key.starts_with(&prefix) || key == &enable {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{key}\" is not a setting of service \"{name}\""),
            ));
        }
        if value == "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}" {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{key}\" would be saved as its own mask"),
            )
            .with_hint(crate::hints::REVEAL_VALUE_FIRST));
        }
    }
    Ok(())
}

/// Every `SERVICE_<ID>_*` setting, with the enable flag left out.
///
/// `ENABLE` is not here on purpose: it is the toggle in the services list, and
/// two controls for one key is how they end up disagreeing.
#[tauri::command]
pub fn service_settings(state: State<'_, AppState>, name: String) -> Result<Vec<ServiceSetting>> {
    checked_service(&name)?;
    let root = state.root()?;
    let env = Env::load(&root)?;
    let prefix = Env::service_prefix(&name);

    let defaults: std::collections::BTreeMap<&str, &str> =
        crate::config::EMBEDDED.iter().copied().collect();

    Ok(env
        .raw()
        .iter()
        .filter_map(|(env_key, value)| {
            let key = env_key.strip_prefix(&prefix)?;
            if key == "ENABLE" {
                return None;
            }
            let secret = Env::is_secret(env_key);
            Some(ServiceSetting {
                key: key.to_string(),
                env_key: env_key.clone(),
                value: if secret {
                    "••••••••".to_string()
                } else {
                    value.clone()
                },
                secret,
                is_default: defaults.get(env_key.as_str()) == Some(&value.as_str()),
            })
        })
        .collect())
}

/// Write a service's settings and rebuild its container with them.
///
/// The rebuild is the point. `service_restart` restarts the container that is
/// already there, which keeps the environment it was created with — so a
/// setting saved and then "restarted" appears to have been applied and has
/// not. This regenerates the compose file and forces a recreate, which is the
/// only sequence where the new value actually reaches the process.
#[tauri::command]
pub async fn service_apply_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    patch: std::collections::BTreeMap<String, String>,
) -> Result<String> {
    checked_service(&name)?;
    let _busy = state.inflight.acquire(format!("service:{name}"))?;
    let root = state.root()?;
    let operation_id = events::next_operation_id("service-settings");

    check_service_patch(&name, &patch)?;
    events::emit(&app, "service:enabling", SubjectEvent::service(&name));

    let outcome = async {
        env_writer::apply(&root, &patch)?;
        generate(&app, &root, &operation_id, "projects_and_services").await?;

        let mut args = runner::compose_base_args(&root);
        args.extend(runner::profile_args("custom", std::slice::from_ref(&name))?);
        args.extend([
            "up".into(),
            "-d".into(),
            "--no-build".into(),
            // Without this, compose recreates only when it sees the compose
            // file change. A setting that lands in a rendered config file the
            // container mounts leaves the compose file identical, and the old
            // container would be left running with the old value.
            "--force-recreate".into(),
        ]);

        runner::run_operation(
            &events::sink(&app),
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "service:progress",
                finished_event: "service:enabled",
                program: "docker",
                args: &args,
                cwd: &root,
                env: &[],
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
            &events::sink(&app),
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "service:progress",
                finished_event: "service:enabled",
                program: "docker",
                args: &args,
                cwd: &root,
                env: &[],
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
    // The name has to resolve for the service to be reachable, and stop
    // resolving when it is not — asked for only when the file would change.
    if outcome.is_ok() {
        if let Err(e) = sync_service_host(&root, &name, true).await {
            tracing::warn!(service = %name, error = %e.message, "hosts entry not updated");
        }
    }

    outcome.map(|_| operation_id)
}

/// The named volumes one service's template declares, as the engine has them.
///
/// Read from the template rather than matched by name prefix, and the
/// difference is not cosmetic: `stackvo-mongo-` is a prefix of
/// `stackvo-mongo-express-…`, so switching off Mongo would have taken
/// Mongo Express's data with it. The template says exactly which volumes are
/// this service's, and nothing else does.
///
/// Compose prefixes a declared volume with the project name — `base.yml` sets
/// `name: stackvo`, so `stackvo-mysql-data` is `stackvo_stackvo-mysql-data` on
/// the engine. Both spellings are returned because a workspace that has taken
/// the template over can pin a `name:` and get the bare one.
fn declared_volumes(root: &std::path::Path, service: &str) -> Vec<String> {
    let relative = format!("core/templates/services/{service}/docker-compose.{service}.tpl");
    let Some(text) = crate::skeleton::read_template(root, &relative) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut in_volumes = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if indent == 0 {
            in_volumes = trimmed == "volumes:";
            continue;
        }
        // Only the top-level `volumes:` block: a service's own `volumes:` list
        // is bind mounts and named references, not declarations.
        if in_volumes && indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            let declared = trimmed.trim_end_matches(':');
            // A name still carrying `{{ }}` was never substituted, which means
            // this is not a file the generator wrote and guessing is worse than
            // leaving it.
            if declared.contains("{{") {
                continue;
            }
            out.push(format!("stackvo_{declared}"));
            out.push(declared.to_string());
        }
    }
    out
}

/// What a disabled service leaves behind, removed.
///
/// **This deletes data.** A service's named volume is its databases, and there
/// is no undo — re-enabling MySQL after this gives an empty MySQL. That is the
/// behaviour asked for, and the front end confirms it before calling; the
/// destructive step must not become reachable without that dialog.
///
/// Best effort per item, and it returns what it managed rather than failing:
/// the service is already gone by the time this runs, so an image another
/// container happens to hold is a reason to leave that image, not a reason to
/// report the whole operation as failed.
async fn discard_service(
    root: &std::path::Path,
    service: &str,
    image: Option<&str>,
) -> Vec<String> {
    let mut removed = Vec::new();

    for volume in declared_volumes(root, service) {
        // Ask the engine which of the two spellings exists rather than
        // deleting blind; `remove_volume` treats a 404 as success, so a wrong
        // guess would be indistinguishable from a real removal in the report.
        let exists = engine::volumes_named(&volume)
            .await
            .is_ok_and(|found| found.iter().any(|v| v == &volume));
        if exists && engine::remove_volume(&volume).await.is_ok() {
            removed.push(format!("volume {volume}"));
        }
    }

    if let Some(tag) = image {
        if engine::remove_image(tag).await.unwrap_or(false) {
            removed.push(format!("image {tag}"));
        }
    }

    // The log directory the template bind-mounts. Confined by construction:
    // `checked_service` has already vetted the name against the catalogue, so
    // it cannot carry a separator.
    let logs = root.join("logs").join("services").join(service);
    if logs.is_dir() && std::fs::remove_dir_all(&logs).is_ok() {
        removed.push(format!("logs {}", logs.display()));
    }

    removed
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
        // The hosts entry first, and it is allowed to fail the whole thing.
        //
        // It needs an administrator password, and until now a cancelled prompt
        // was written to the app log and nowhere else — the service went "off"
        // while its domain kept resolving, which is the residue that is hardest
        // to notice and hardest to explain. Doing it first means a cancelled
        // password leaves everything intact rather than half-destroyed: nothing
        // below this line has run yet.
        sync_service_host(&root, &name, false).await?;

        // Stop *and remove*, then unconfigure. The order is load-bearing: the
        // reverse regenerates the compose file without the service, and compose
        // can no longer see the container it is supposed to be taking down.
        //
        // Removal, not just a stop. Stopping left the container behind, and the
        // next regenerate wrote it out of the compose file — so it stopped
        // being anything's responsibility while still occupying its name, its
        // disk and every container list in the app. Turning a service off has
        // to mean it is not there, or "off" is a label rather than a state.
        let image = engine::inspect(&name).await.ok().and_then(|c| c.image);
        let _ = engine::stop_container(&name).await;
        let _ = engine::remove_container(&name).await;

        // Then everything the container leaves behind. Read `discard_service`
        // before changing any of this: it deletes data on purpose.
        let discarded = discard_service(&root, &name, image.as_deref()).await;
        tracing::info!(service = %name, ?discarded, "service disabled and its leftovers removed");

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
    // be held across the await below. Two generators writing
    // docker-compose.projects.yml at once produce a file that is neither.
    let lock = app.state::<AppState>().generate_lock.clone();
    let _serialised = lock.lock().await;

    // Everything below the lock is the reporting half, and it needs no window —
    // see the function it now calls. This one keeps the two things only Tauri
    // can give it: the managed state the lock lives in, and the window sink.
    generate_reported(&events::sink(app), root, operation_id, scope)
}

/// Write the generated files and narrate it, with no `AppHandle` in sight.
///
/// Split out of [`generate`] so it can be tested. What is being pinned is the
/// *event contract*, not the file writing — `write_generated` is a separate,
/// already-testable function, and this is the layer the UI's progress pane
/// actually consumes: one `generate:progress` per file, then exactly one
/// `generate:done` carrying the outcome.
///
/// That contract had never been verified anywhere, and it has a failure mode
/// that no type catches: returning `Err` without emitting `generate:done`
/// leaves the console showing an operation that never finishes. The tests below
/// assert the terminal event on **both** paths for that reason.
///
/// In-process since the Bash CLI was retired. It used to shell out to
/// `stackvo generate`, which is why this still reports through the operation
/// events: callers await it and watch the same stream either way.
fn generate_reported(
    sink: &dyn crate::progress::ProgressSink,
    root: &std::path::Path,
    operation_id: &str,
    scope: &str,
) -> Result<()> {
    let report = write_generated(root, scope, |label| {
        crate::progress::emit(
            sink,
            "generate:progress",
            events::ProgressEvent {
                operation_id: operation_id.to_string(),
                subject: scope.to_string(),
                line: label.to_string(),
            },
        );
    });

    let (success, error) = match &report {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.message.clone())),
    };

    crate::progress::emit(
        sink,
        "generate:done",
        events::FinishedEvent {
            operation_id: operation_id.to_string(),
            subject: scope.to_string(),
            success,
            duration_ms: 0,
            error,
            log_path: None,
        },
    );

    report.map(|_| ())
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

    // `subject` as well as `scope`, and both the same string. The progress and
    // finished events this operation goes on to emit are subjected on the
    // scope; without it here the opening event fell through the reader's
    // `subject ?? project ?? service ?? "stack"` chain and opened the operation
    // against "stack" — a subject its own finish then never closed.
    events::emit(
        &app,
        "generate:start",
        serde_json::json!({
            "operationId": operation_id,
            "scope": scope,
            "subject": scope,
        }),
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
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: &mode,
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
            env: &[],
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
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: "stack",
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
            env: &[],
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
            &events::sink(&app),
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "build:progress",
                finished_event: "build:built",
                program: "docker",
                args: &args,
                cwd: &root,
                env: &[],
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
            &events::sink(&app),
            runner::Operation {
                operation_id: &operation_id,
                subject: &name,
                progress_event: "build:progress",
                finished_event: "build:success",
                program: "docker",
                args: &up,
                cwd: &root,
                env: &[],
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

/// Every StackVo domain and whether `/etc/hosts` resolves it, plus the entries
/// StackVo wrote that nothing wants any more.
///
/// The pieces existed — `hosts_status` answers the first half and
/// `mapped_domains` the second — but nothing put them together, so the file
/// could only be corrected one broken domain at a time from the page that
/// happened to notice. A deleted project's line, in particular, had no way of
/// being found at all: it points at 127.0.0.1 forever and nothing looks for it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsOverview {
    /// Every domain the stack serves, in one list with its state.
    pub entries: Vec<hosts::HostsEntry>,
    /// Inside StackVo's own block, but no longer serving anything.
    pub stale: Vec<String>,
}

#[tauri::command]
pub async fn hosts_overview(state: State<'_, AppState>) -> Result<HostsOverview> {
    let root = state.root()?;
    let wanted = wanted_domains(&root).await;

    // Only StackVo's own block is offered for removal. A line somebody added
    // by hand is theirs, and a tool that tidies away entries it did not write
    // is a tool nobody trusts with the file again.
    let (_, managed) = hosts::mapped_domains();
    let keep: std::collections::HashSet<String> =
        wanted.iter().map(|d| d.to_ascii_lowercase()).collect();
    let mut stale: Vec<String> = managed.into_iter().filter(|d| !keep.contains(d)).collect();
    stale.sort();

    Ok(HostsOverview {
        entries: hosts::status_for(&wanted),
        stale,
    })
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

/// Rewrite the hosts file, with the system asking for a password first.
///
/// `(async)` for the same reason as `workspace_pick`: a synchronous command
/// runs on the main thread, and this one blocks on `osascript … with
/// administrator privileges` — a prompt that stays up for as long as somebody
/// takes to find and type their password. Every second of that was a second the
/// window behind it could not repaint.
#[tauri::command(async)]
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

/// Every domain this stack answers on that has no hosts entry.
///
/// Projects are the obvious half and were the only half. The rest reach the
/// browser exactly the same way — through Traefik, by name — and had no entry
/// offered for them, so an admin UI or the proxy's own dashboard simply failed
/// to resolve with nothing in the app to say why. The checkout this was
/// written against had those lines only because the retired Bash CLI once
/// wrote them; a workspace created by this app would not.
#[tauri::command]
pub async fn hosts_missing(state: State<'_, AppState>) -> Result<Vec<String>> {
    let root = state.root()?;
    Ok(missing_hosts(&root).await)
}

/// Only the two names the stack is addressed through.
///
/// A separate command rather than a filter the caller applies, because the two
/// questions have different right answers and the wrong one shipped: the
/// preflight gate blocks on `<suffix>` and `traefik.<suffix>`, and its button
/// wrote every missing name it could find — so a machine that had asked for two
/// entries got four, including the admin UI of a service the user had not
/// mentioned. A prompt that appears for one reason must not do a second thing
/// while it is up.
///
/// The dashboard keeps using `hosts_missing`, which is the "fix everything"
/// surface and is asked for as such. A service's own line still arrives when
/// that service is switched on — see `sync_service_host`.
#[tauri::command]
pub async fn hosts_missing_core(state: State<'_, AppState>) -> Result<Vec<String>> {
    let root = state.root()?;
    Ok(missing_hosts_by_owner(&root).await.core)
}

/// Whether a service's hosts line should be added, removed, or left alone.
///
/// Split out because it decides whether the user is asked for a password. Every
/// hosts write shows the system prompt and there is no way around it, so a
/// toggle that would change nothing must reach no further than this function.
///
/// Returns `(add, remove)` as a pair of flags, or `None` for "do nothing".
fn host_sync_action(
    enabled: bool,
    configured: bool,
    managed: bool,
) -> Option<(Option<()>, Option<()>)> {
    match (enabled, configured, managed) {
        // On and unresolvable: the admin UI would open on nothing.
        (true, false, _) => Some((Some(()), None)),
        // Off, resolvable, and ours to remove.
        (false, true, true) => Some((None, Some(()))),
        // Everything else — including a line somebody wrote by hand, which
        // stays even when the service is switched off.
        _ => None,
    }
}

/// Add or remove a service's hosts line as it is switched on and off.
///
/// Enabling wrote the route and started the container but left the name
/// unresolvable, so the admin UI opened on nothing. Listing every catalogue
/// service instead was the other extreme: thirteen lines for a stack running
/// three, which is the clutter this avoids.
///
/// Elevation is the constraint. Every write shows the system's authentication
/// prompt and there is no way around it, so this asks only when the file would
/// actually change — toggling a service whose line is already right costs
/// nothing, and the prompt lands while the user is still looking at the button
/// they pressed.
///
/// A failure here is reported, not fatal: the service is running either way,
/// and the Domain pane lists what is still missing.
async fn sync_service_host(root: &std::path::Path, service: &str, enabled: bool) -> Result<()> {
    let env = Env::load(root)?;
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");
    let Some(url) = env.service_url(service) else {
        return Ok(());
    };
    let domain = format!("{url}.{tld}");

    let configured = hosts::status_for(std::slice::from_ref(&domain))
        .first()
        .is_some_and(|e| e.configured);

    // Only what StackVo wrote comes back out. A line somebody added by hand
    // stays, even for a service being turned off.
    let managed = hosts::mapped_domains()
        .1
        .contains(&domain.to_ascii_lowercase());

    let Some((add, remove)) = host_sync_action(enabled, configured, managed) else {
        return Ok(());
    };

    hosts::apply(
        &add.map(|_| domain.clone()).into_iter().collect::<Vec<_>>(),
        &remove.map(|_| domain).into_iter().collect::<Vec<_>>(),
    )
    .map(|_| ())
}

/// The domains this stack serves that `/etc/hosts` does not resolve.
///
/// Shared with the doctor, which had its own copy of the projects-only version.
/// Two answers to "what is missing" is how the panel people open when something
/// is wrong ends up reporting less than the dashboard does.
///
/// `status_for` is what the hosts dialog itself reads, so "missing" here cannot
/// mean something different from "missing" there either.
pub(crate) async fn missing_hosts(root: &std::path::Path) -> Vec<String> {
    let split = missing_hosts_by_owner(root).await;
    split.core.into_iter().chain(split.rest).collect()
}

/// The same list, split by whether the stack can be reached at all without it.
#[derive(Debug, Default)]
pub(crate) struct MissingHosts {
    /// The two names the stack itself is addressed through.
    pub core: Vec<String>,
    /// Everything else: a service's admin UI, a project's domain.
    pub rest: Vec<String>,
}

/// Which missing names hold the gate, and which are just missing.
///
/// The line is drawn at "is the stack reachable at all". `<suffix>` and
/// `traefik.<suffix>` are the address of the thing itself and there is no
/// getting to anything without them, which is why a gate that numbered them as
/// a step and then closed over them had listed a requirement it did not
/// require.
///
/// Everything else is a specific thing being unreachable. A service's admin UI
/// is offered when that service is switched on (`sync_service_host`), a
/// project's domain on the pages that own the project — both belong in the
/// file, neither is a reason to refuse to open the app. That distinction was
/// wrong here once already, in the other direction: the first version of this
/// split blocked on every enabled service's UI too, which would have held the
/// whole app shut over phpMyAdmin.
pub(crate) async fn missing_hosts_by_owner(root: &std::path::Path) -> MissingHosts {
    let core = core_domains(root);
    let is_core: std::collections::HashSet<String> =
        core.iter().map(|d| d.to_ascii_lowercase()).collect();

    let mut out = MissingHosts::default();
    for entry in crate::hosts::status_for(&wanted_domains(root).await) {
        if entry.configured {
            continue;
        }
        if is_core.contains(&entry.domain.to_ascii_lowercase()) {
            out.core.push(entry.domain);
        } else {
            out.rest.push(entry.domain);
        }
    }
    out
}

/// The admin UI of every service whose name is worth asking about.
///
/// **Running, or already written down.** Not "enabled" — that was the bug, and
/// it was the same bug in three places before it was stated properly.
/// `SERVICE_X_ENABLE` decides whether a service is *in* the compose profile,
/// and phpMyAdmin and RabbitMQ ship switched on, so a fresh install with
/// nothing ever started was told two hosts entries were missing. They were, in
/// the sense that the file did not contain them; they were also the addresses
/// of two containers that did not exist.
///
/// The "or already written down" half is what keeps this one list rather than
/// two. A stopped service that has a line stays in the answer, so nothing ever
/// offers to delete it as stale, and the file's own contents never become a
/// thing to argue with. Without that, "what should be here" and "what is not
/// junk" needed separate definitions — and the version of this that had two
/// definitions is exactly how the settings pane went on listing names the
/// dashboard had stopped listing.
///
/// Toggling a service on still writes its line eagerly (`sync_service_host`),
/// and that is not the same thing: a button somebody pressed may act on intent,
/// an unsolicited list may not.
async fn service_domains(root: &std::path::Path) -> Vec<String> {
    let Ok(env) = Env::load(root) else {
        return Vec::new();
    };

    // A dead engine means nothing is running, which is the right answer here
    // rather than a reason to fail.
    let running: std::collections::HashSet<String> = engine::stackvo_containers()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, c)| c.running)
        .map(|(id, _)| id)
        .collect();
    let (_, managed) = crate::hosts::mapped_domains();

    service_domains_from(&env, &running, &managed)
}

/// The decision itself, with both ambient reads passed in.
///
/// Split out because the test that guards this rule **was not hermetic** and
/// nobody noticed for as long as it only ever ran where the rule was already
/// satisfied. It called the whole chain, which reaches the real Docker daemon
/// and the real `/etc/hosts`, and asserted that a fresh install asks for
/// nothing but the two core names. That is true on a CI runner with no StackVo
/// containers. On the machine of anyone actually running the stack, phpMyAdmin
/// and RabbitMQ *are* running, so they are correctly included and the test
/// fails — reporting a bug in the code when the bug is in the test.
///
/// A test that only passes where the daemon is idle is a test the maintainer
/// cannot run. Both inputs are arguments now, so the rule can be checked
/// against a stated world rather than against whichever one the machine
/// happens to be in.
fn service_domains_from(
    env: &Env,
    running: &std::collections::HashSet<String>,
    managed: &std::collections::HashSet<String>,
) -> Vec<String> {
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");

    env_schema()
        .service_catalog()
        .into_iter()
        .filter_map(|(id, _)| env.service_url(&id).map(|url| (id, format!("{url}.{tld}"))))
        .filter(|(id, domain)| {
            running.contains(id) || managed.contains(&domain.to_ascii_lowercase())
        })
        .map(|(_, domain)| domain)
        .collect()
}

/// The rule under test, reachable without a Docker daemon or an `/etc/hosts`.
#[cfg(test)]
pub(crate) fn service_domains_for_test(
    env: &Env,
    running: &std::collections::HashSet<String>,
    managed: &std::collections::HashSet<String>,
) -> Vec<String> {
    service_domains_from(env, running, managed)
}

/// The two names the stack answers on before anything else exists.
///
/// `<suffix>` because `certs::required_domains` issues for the bare name as
/// well as the wildcard, so the app already holds that it should answer, and
/// `traefik.<suffix>` because `routes.yml` has always written that router while
/// nothing ever offered the entry that makes it reachable.
///
/// Exactly these two. Anything that can be switched off is not the address of
/// the stack.
#[cfg(test)]
pub(crate) fn core_domains_for_test(root: &std::path::Path) -> Vec<String> {
    core_domains(root)
}

// `wanted_domains_for_test` used to live here, so a test could check that the
// settings pane and the dashboard banner agreed on the list. They cannot
// disagree any more: `missing_hosts_by_owner` is written in terms of
// `wanted_domains` rather than restating it, so the two-definitions bug that
// helper was added to catch is now prevented by construction. A test that can
// only fail if someone reintroduces the duplication is a test of a shape the
// compiler already holds.

fn core_domains(root: &std::path::Path) -> Vec<String> {
    let Ok(env) = Env::load(root) else {
        return Vec::new();
    };
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");

    let mut out = vec![tld.to_string(), format!("traefik.{tld}")];
    out.retain(|d| crate::hosts::is_valid_domain(d));
    out
}

/// Every domain the file should carry.
///
/// The one answer, used by the settings pane, the dashboard banner and the
/// preflight gate alike. There were briefly two — a wide one for deciding what
/// in the file is stale and a narrow one for what to warn about — and the two
/// disagreed in the way two definitions of the same thing always do: the
/// dashboard stopped naming phpMyAdmin and the settings pane went on listing
/// it, on a machine where it had never run.
async fn wanted_domains(root: &std::path::Path) -> Vec<String> {
    let mut wanted: Vec<String> = list_projects(root)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.domain)
        .collect();

    // Everything the stack itself answers on, from the two functions that know.
    // Restating the suffix and the proxy here — as this used to — is how the
    // gate and the dashboard come to disagree about what the stack needs.
    wanted.extend(core_domains(root));
    wanted.extend(service_domains(root).await);

    // A malformed domain would be refused by the writer for the whole batch,
    // taking every valid line with it.
    wanted.retain(|d| crate::hosts::is_valid_domain(d));
    wanted.sort();
    wanted.dedup();
    wanted
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

/// Delete one message.
///
/// Separate from `mail_clear` deliberately: emptying the inbox and removing
/// the one message you were looking at are different intentions, and a UI that
/// offers only the first makes people clear everything to get rid of one.
#[tauri::command]
pub async fn mail_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let root = state.root()?;
    mail::delete(&root, &id).await
}

/// Search the inbox. An empty query is a plain listing rather than an error,
/// so a cleared search box shows the inbox again instead of nothing.
#[tauri::command]
pub async fn mail_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<mail::MailMessage>> {
    let root = state.root()?;
    let limit = limit.unwrap_or(100);
    if query.trim().is_empty() {
        return mail::messages(&root, limit).await;
    }
    mail::search(&root, &query, limit).await
}

/// What this HTML would do in the clients people actually read mail in.
///
/// `None` when the message has no HTML part — a plain-text mail has nothing to
/// check, which is not the same as passing.
#[tauri::command]
pub async fn mail_html_check(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<mail::HtmlCheck>> {
    let root = state.root()?;
    mail::html_check(&root, &id).await
}

/// Follow the links in a message and report what answers.
///
/// The common failure this catches is a link built from a misconfigured base
/// URL — `http://localhost/verify?token=…` in a mail that a container sent,
/// which works when clicked on the developer's machine and nowhere else.
#[tauri::command]
pub async fn mail_link_check(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<mail::LinkCheck>> {
    let root = state.root()?;
    mail::link_check(&root, &id).await
}

/// Write an attachment to disk, returning how many bytes landed.
#[tauri::command]
pub async fn mail_attachment_save(
    state: State<'_, AppState>,
    id: String,
    part_id: String,
    path: String,
) -> Result<u64> {
    let root = state.root()?;
    mail::save_attachment(&root, &id, &part_id, std::path::Path::new(&path)).await
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

// ------------------------------------------------------------ debug bridge

/// Turn capture on or off, with no container involved.
///
/// A file appears in a directory that is already mounted, and the next request
/// reads it. That is the whole operation — no compose command, no recreate, no
/// waiting for a worker to cycle. It is the difference this feature exists for.
#[tauri::command]
pub fn debug_bridge_set(state: State<'_, AppState>, name: String, enabled: bool) -> Result<()> {
    crate::debugbridge::set_enabled(&state.root()?, &name, enabled)
}

/// Events recorded after the `since`th one.
///
/// A cursor rather than a stream, because the producer is a file a container
/// appends to and there is nothing to subscribe to. The count the caller last
/// saw is enough: events are only ever appended, so everything past that index
/// is new, and a caller that missed a poll catches up rather than losing them.
#[tauri::command]
pub fn debug_bridge_events(
    state: State<'_, AppState>,
    name: String,
    since: Option<usize>,
) -> Result<serde_json::Value> {
    let root = state.root()?;
    crate::debugbridge::rotate_if_large(&root, &name);

    let all = crate::debugbridge::read_events(&root, &name);
    let since = since.unwrap_or(0);

    // A cursor past the end means the file was cleared or rotated under the
    // caller. Starting again beats returning nothing for ever.
    let start = if since > all.len() { 0 } else { since };
    Ok(serde_json::json!({
        "total": all.len(),
        "events": &all[start..],
    }))
}

#[tauri::command]
pub fn debug_bridge_clear(state: State<'_, AppState>, name: String) -> Result<()> {
    crate::debugbridge::clear(&state.root()?, &name)
}

/// Every project the bridge could serve, and which of them are capturing.
///
/// The question the per-project pane cannot answer: *which* of eight projects
/// just dumped something. That is the same reason the log viewer grew a page —
/// you ask it before you know which project to open — and it is what a page
/// needs in order to poll only the projects worth polling.
///
/// Reads files and one container inspection per project; no engine, no
/// compose. With Docker down every row still reports whether capture is on,
/// because that is a file on the host and true either way.
#[tauri::command]
pub async fn debug_bridge_overview(state: State<'_, AppState>) -> Result<serde_json::Value> {
    let root = state.root()?;
    let mut out = Vec::new();

    // The first thing the pane does is ask for this, so it is where a bridge
    // left behind by an older build gets replaced — before the first poll comes
    // back, and without anybody having to restart a container or know that a
    // bridge is a file at all.
    crate::debugbridge::refresh(&root);

    let Some(projects) = workspace::projects_root(&root) else {
        return Ok(serde_json::json!([]));
    };
    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return Ok(serde_json::json!([]));
    };

    let mut names: Vec<String> = dirs
        .flatten()
        .filter(|d| d.path().is_dir())
        .filter_map(|d| d.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();

    for name in names {
        // A project the bridge cannot serve is not a row: the page is a list of
        // places dumps can come from, and a Node project is not one.
        let Ok(status) = crate::debugbridge::status(&root, &name).await else {
            continue;
        };
        if !status.supported {
            continue;
        }
        out.push(serde_json::json!({
            "project": name,
            "enabled": status.enabled,
            "mounted": status.mounted,
            "running": status.running,
            "events": status.events,
        }));
    }

    Ok(serde_json::Value::Array(out))
}

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
                &events::sink(&app),
                runner::Operation {
                    operation_id: &operation_id,
                    subject: &name,
                    progress_event: "build:progress",
                    finished_event: "build:success",
                    program: "docker",
                    args: &argv,
                    cwd: &root,
                    env: &[],
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
                &events::sink(&app),
                runner::Operation {
                    operation_id: &operation_id,
                    subject: &name,
                    progress_event: "build:progress",
                    finished_event: "build:success",
                    program: "docker",
                    args: &argv,
                    cwd: &root,
                    env: &[],
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
            .with_hint(crate::hints::START_PROJECT_FOR_COMMANDS));
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
            &events::sink(&handle),
            runner::Operation {
                operation_id: &op_id,
                subject: &subject,
                progress_event: "build:progress",
                finished_event: "build:success",
                program: "docker",
                args: &argv,
                cwd: &root,
                env: &[],
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
        return Err(Error::not_found(format!("a compose file in {name}"))
            .with_hint(crate::hints::COMPOSE_FILE_NOT_FOUND));
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
        // The compose importer builds a full spec, domain included.
        project_adopt(app.clone(), state.clone(), name.clone(), Some(spec), None).await?;
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

/// Trust the certificate authority, in the user's own terminal.
///
/// The one job macOS will not let this app do for itself — see
/// `certs::trust_ca`. `mkcert -install` asks `sudo` for a password, which works
/// in a terminal somebody is looking at and nowhere else, so the app opens one
/// and hands it the command rather than pretending.
///
/// `CAROOT` is passed explicitly because the terminal is a fresh login shell
/// that knows nothing about this app's environment, and without it mkcert would
/// install a certificate authority from its own default directory — not the one
/// that signed this stack's certificate.
#[tauri::command]
pub fn cert_trust_in_terminal() -> Result<()> {
    let preferred = prefs_get().ok().and_then(|p| {
        p.get("terminalApp")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });

    // The command mkcert itself runs, without mkcert's own pre-check.
    //
    // `mkcert -install` was here first and it does nothing: it decides the CA
    // is already installed and returns happy. Its check is Go's
    // `x509.Verify` against the system roots, and on macOS that is satisfied by
    // the certificate merely being *in* a keychain — the same trap this app
    // fell into a few hours earlier. Measured either side of a run that printed
    // "The local CA is already installed in the system trust store! 👍":
    //
    //   security verify-cert -p basic -c <leaf>  →  CSSMERR_TP_NOT_TRUSTED
    //
    // So it is the underlying write instead, which is exactly what mkcert
    // shells out to when it does decide to act:
    //
    //   sudo -- security add-trusted-cert -d -k /Library/Keychains/System.keychain <ca>
    //
    // `sudo` needs a terminal to ask for a password in, which is the entire
    // reason this opens one rather than doing it in the background.
    let command = format!(
        "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '{}'",
        crate::certs::ca_file().display()
    );
    crate::pty::open_external_shell(&command, preferred.as_deref())
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
    // `normalize_spec`, not `normalize`: an incoming spec is a JSON value with
    // no meaningful key order, so the layout rule (W-01) is checked against the
    // bytes `manifest::write` will actually produce.
    let m = manifest::normalize_spec(value, expected_name);

    if !m.valid {
        return Err(
            Error::new(Code::InvalidManifest, "the project definition is not valid")
                .with_details(serde_json::json!({ "errors": m.errors })),
        );
    }
    Ok(m)
}

#[tauri::command]
pub fn project_validate(name: String, spec: serde_json::Value) -> Result<serde_json::Value> {
    // The same rule as `parse_spec`: layout is judged on the bytes that would
    // be written, not on a pretty-printed `Value` whose keys `serde_json` has
    // sorted. Otherwise the New Project sheet reports W-01 against every PHP
    // spec that carries extensions — and its Create button stays disabled for
    // a project that `project_create` would have accepted.
    let m = manifest::normalize_spec(&spec, &name);

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
    // Canonicalised before anything is created: the directory, the manifest's
    // `name` and the image reference have to be the same string, and only one
    // of the three is allowed capitals. See `workspace::canonical_name`.
    let name = workspace::canonical_name(
        spec.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::new(Code::InvalidInput, "`name` is required"))?,
    );

    let mut spec = spec;
    spec["name"] = serde_json::json!(name);

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

    // Resolvable, then trusted. Everything else creation needs is generated
    // in-process; these two were the steps left to the README and to a trip
    // through Settings.
    if outcome.is_ok() {
        sync_project_host(&app, m.domain.as_deref()).await;
        sync_certificate(&app, &state, &root).await;
    }

    outcome.map(|_| operation_id)
}

/// Make a new project's domain resolve, the moment the project exists.
///
/// The counterpart of `sync_service_host`, and the same argument: routing was
/// written and the container was ready, but the name went nowhere, so the
/// browser answered `ERR_NAME_NOT_RESOLVED` for a project the app had just
/// reported as created. Everything else creation needs — the Dockerfile, the
/// compose entry, the Traefik labels — is generated in-process; this was the
/// one step left to the README.
///
/// Only when the line is actually missing. Every hosts write shows the system's
/// authentication prompt, and asking for a password to write something already
/// there is the kind of prompt people learn to click through.
///
/// Never fatal, and deliberately not part of the create transaction: the
/// project is on disk and generated either way, a rollback here would delete
/// work over a file the user can also fix from the Domains pane, and refusing
/// the password prompt is a choice, not a failure.
async fn sync_project_host(app: &AppHandle, domain: Option<&str>) {
    let Some(domain) = domain.filter(|d| hosts::is_valid_domain(d)) else {
        return;
    };

    let configured = hosts::status_for(std::slice::from_ref(&domain.to_string()))
        .first()
        .is_some_and(|e| e.configured);
    if configured {
        return;
    }

    match hosts::apply(&[domain.to_string()], &[]) {
        Ok(plan) => events::emit(
            app,
            "hosts:changed",
            serde_json::json!({ "added": plan.add, "removed": plan.remove }),
        ),
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e.message, "hosts entry not written")
        }
    }
}

/// Make the certificate describe the workspace as it is now.
///
/// Called after a project appears and after one goes away, because both change
/// the answer. A new domain that resolves and routes but is not on the
/// certificate is a full-page browser interstitial, and the fix was a trip to
/// Settings → Certificates to press Reissue — a step the app knew was needed
/// the moment the manifest was written, and made the user discover. A deleted
/// project leaves the opposite: a name the certificate still vouches for that
/// nothing serves.
///
/// `certs::plan` already answers "does this need reissuing" — new names, stale
/// names, expired, missing — so this asks it rather than deciding again. When
/// nothing changed, nothing runs: every reissue rewrites the key pair and makes
/// Traefik reread it, and the common case (`shop.<suffix>`, already inside the
/// wildcard) changes nothing at all.
///
/// `install_ca: false`, deliberately. Issuing writes inside the workspace and
/// needs nothing; installing the CA touches four system trust stores and can
/// raise an authentication prompt, which is a once-per-machine setup step the
/// requirements gate owns and not something to spring on someone who pressed
/// Create. Never fatal for the same reason as the hosts entry.
async fn sync_certificate(app: &AppHandle, state: &AppState, root: &std::path::Path) {
    // The same guard `cert_apply` takes, for the same reason: two reissues at
    // once write one pair of files from two argument lists. A reissue already
    // running is not worth queueing behind — the Certificates pane reports
    // whatever it leaves behind.
    let Ok(_busy) = state.inflight.acquire("certs") else {
        tracing::warn!("a reissue was already running; certificate left alone");
        return;
    };

    match certs::plan(root, false).await {
        Ok(plan) if !plan.changed => return,
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e.message, "certificate not reissued");
            return;
        }
    }

    match certs::apply(root, false).await {
        Ok(plan) => events::emit(
            app,
            "certs:changed",
            serde_json::json!({ "added": plan.add, "removed": plan.remove }),
        ),
        Err(e) => tracing::warn!(error = %e.message, "certificate not reissued"),
    }
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
/// What adoption asks the user for, because detection cannot answer it.
///
/// Each field replaces one value in the detected spec; an absent field leaves
/// detection's answer alone. Not a partial `ProjectSpec`: a spec passed whole
/// *replaces* detection, which is the right thing for an importer and the
/// wrong thing for a form that only means to change the PHP version.
///
/// Why these four. The domain is a choice — detection can say "this is
/// Laravel, its document root is public", never that the user wanted
/// `shop.loc`. The other three are the scaffolding gap: a `composer.json`
/// states the PHP version the framework *needs* (`"php": "^8.3"`), read as an
/// answer it pins a brand-new Laravel to the floor of its own range; nothing
/// in a checkout names a web server; and detection has no opinion at all about
/// extensions, so an adopted project got the generator's seven-entry fallback.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptOverrides {
    pub domain: Option<String>,
    pub php_version: Option<String>,
    pub server: Option<String>,
    pub extensions: Option<Vec<String>>,
}

#[tauri::command]
pub async fn project_adopt(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    spec: Option<serde_json::Value>,
    overrides: Option<AdoptOverrides>,
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
        .with_hint(crate::hints::EDIT_FROM_MANIFEST_TAB));
    }

    // Detection fills the form; it does not bypass validation. An adopted
    // project has to satisfy exactly the contract a created one does.
    let mut spec = match spec {
        Some(spec) => spec,
        None => detected_spec(&name, &detect::detect(&dir)),
    };

    // Overrides on top of detection, not a replacement for it: everything the
    // installed code can answer for itself still comes from the code.
    let overrides = overrides.unwrap_or_default();
    if let Some(domain) = overrides.domain.filter(|d| !d.trim().is_empty()) {
        spec["domain"] = serde_json::json!(domain.trim());
    }
    // Only onto a PHP project. A Node template carrying a stale PHP version
    // from a form the user never saw would be a second runtime block (W-02).
    if spec.get("php").is_some() {
        if let Some(server) = overrides.server.filter(|s| !s.trim().is_empty()) {
            spec["server"] = serde_json::json!(server.trim());
        }
        if let Some(version) = overrides.php_version.filter(|v| !v.trim().is_empty()) {
            spec["php"]["version"] = serde_json::json!(version.trim());
        }
        if let Some(extensions) = overrides.extensions {
            spec["php"]["extensions"] = serde_json::json!(extensions);
        }
    }
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

    // An adopted project is reached by name exactly like a created one.
    if outcome.is_ok() {
        sync_project_host(&app, m.domain.as_deref()).await;
        sync_certificate(&app, &state, &root).await;
    }

    outcome.map(|_| operation_id)
}

/// Bring a project that already carries its own `stackvo.json` online.
///
/// The case `project_adopt` deliberately refuses, and refuses correctly: a
/// directory that already has a manifest must not have one written over it.
/// But refusing is only half an answer, because the *rest* of adoption — the
/// compose files, the hosts entry, the certificate — has not happened either,
/// and nothing else does it. The manifest watcher only reports a change; it
/// regenerates nothing on purpose.
///
/// This is the other half. It is the intended path for a repository that ships
/// its manifest, which is the arrangement the file was designed for — it is
/// commit-friendly precisely so a teammate's clone arrives configured. Before
/// this existed, cloning such a repository ended in "already has a
/// stackvo.json" and a project that was never generated.
///
/// Writes nothing to the manifest. The repository's settings are the team's
/// answer and win over anything the form was pre-filled with; the Manifest tab
/// is where they are changed.
#[tauri::command]
pub async fn project_register(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<String> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;

    let manifest_path = dir.join("stackvo.json");
    if !manifest_path.is_file() {
        return Err(
            Error::new(Code::NotFound, format!("{name} has no stackvo.json"))
                .with_hint(crate::hints::ADOPT_INSTEAD),
        );
    }

    // Read and validate before anything is generated from it. A manifest that
    // came off a remote is not one this app wrote, and the schema check is the
    // same one every other path runs.
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| Error::io(format!("reading {}", manifest_path.display()), e))?;
    let spec: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{name}/stackvo.json is not valid JSON: {e}"),
        )
        .with_hint(crate::hints::FIX_OR_ADOPT)
    })?;
    // A manifest that came off a remote is far likelier to fail the schema than
    // one this app wrote, and the generic rejection says nothing a user can act
    // on. The findings ride along in `details` either way; this adds where to
    // go — the doctor already lists every unbuildable extension with the button
    // that removes it, and an extension is the common failure by a distance.
    let m =
        parse_spec(&spec, &name).map_err(|e| e.with_hint(crate::hints::RUN_DOCTOR_THEN_RETRY))?;

    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    let operation_id = events::next_operation_id("register");
    events::emit(&app, "project:creating", SubjectEvent::project(&name));

    let outcome = generate(&app, &root, &operation_id, "projects").await;

    match &outcome {
        Ok(()) => events::emit(&app, "project:created", SubjectEvent::project(&name)),
        Err(e) => events::emit(
            &app,
            "project:error",
            SubjectEvent::project(&name).error(e.message.clone()),
        ),
    }
    // Nothing to roll back: the manifest was the repository's before this ran
    // and is untouched either way.

    if outcome.is_ok() {
        sync_project_host(&app, m.domain.as_deref()).await;
        sync_certificate(&app, &state, &root).await;
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
    } else if let Some(defaults) = manifest::lang_defaults(detected.runtime) {
        // The ecosystem defaults, written out so the adopted manifest is
        // explicit about what it will run rather than relying on the reader
        // knowing them.
        let mut block = serde_json::Map::new();
        block.insert("version".into(), serde_json::json!(defaults.version));
        if let Some(install) = defaults.install {
            block.insert("install".into(), serde_json::json!(install));
        }
        if let Some(build) = defaults.build {
            block.insert("build".into(), serde_json::json!(build));
        }
        block.insert("start".into(), serde_json::json!(defaults.start));
        block.insert("port".into(), serde_json::json!(defaults.port));
        spec[detected.runtime] = serde_json::Value::Object(block);
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

/// Delete a project, and everything the app made because of it.
///
/// `remove_files` defaults to FALSE and must be opted into explicitly. The web
/// UI's deleteProject() removed the directory outright — a desktop app deleting
/// someone's source code needs a deliberate second step, not a default.
///
/// That flag guards **the user's code and nothing else**. Everything else here
/// exists only because the project did, and a deleted project used to leave all
/// of it behind: a stopped `stackvo-<name>` container, a two-gigabyte
/// `stackvo-<name>` image, its rendered Dockerfile under `generated/projects/`,
/// its log directory, its `/etc/hosts` line and its name on the certificate.
/// None of that is recoverable value — it is the debris of something that no
/// longer exists, and the user has to find it in `docker images` to know it is
/// there.
///
/// Docker-side cleanup is best effort by design: the engine being down is a
/// perfectly good moment to stop managing a project, and refusing to delete
/// until Docker comes back would make a stopped daemon into a lock. What fails
/// is logged and named, never silent.
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

    // Read before anything is removed: the domain is in the manifest, and the
    // hosts line and the certificate are both keyed by it. Absent or
    // unreadable, those two steps are simply skipped — a project whose
    // manifest is already gone has no domain to clean up after.
    let domain = manifest::read(&dir.join("stackvo.json"), &name)
        .ok()
        .and_then(|m| m.domain);

    let operation_id = events::next_operation_id("delete");
    events::emit(&app, "project:deleting", SubjectEvent::project(&name));

    let outcome = async {
        remove_project_containers(&name).await;

        match engine::remove_project_images(&name).await {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!(project = %name, images = ?removed, "project images removed")
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(project = %name, error = %e.message, "project images not removed")
            }
        }

        // The build cache the image was holding, now that the image is gone.
        //
        // Dangling, not all. Docker offers no per-project handle on the build
        // cache — the filters are `until`, `id`, `parent`, `type` — so the way
        // to reclaim one project's cache is to delete its image and then
        // collect what nothing references any more, which is what this does.
        //
        // The rest is not this project's to take. Every StackVo project image
        // starts from the same PHP base and runs the same extension installs,
        // so most of those layers are one cache shared by every project on the
        // machine; `BuildCache::All` here would charge the projects the user
        // kept for the one they deleted. That level exists, deliberately, in
        // the prune panel where its cost can be stated before it is paid.
        match engine::prune(false, false, engine::BuildCache::Dangling).await {
            Ok(report) if report.space_reclaimed > 0 => tracing::info!(
                project = %name,
                records = report.caches_deleted,
                bytes = report.space_reclaimed,
                "orphaned build cache reclaimed"
            ),
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(project = %name, error = %e.message, "build cache not pruned")
            }
        }

        if remove_files.unwrap_or(false) {
            remove_project_dir(&dir)
                .await
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

        // App-owned output, removed either way. `remove_files` is about the
        // user's code; a rendered Dockerfile and a container log directory for
        // a project that no longer exists are neither code nor the user's.
        for output in [
            root.join("generated/projects").join(&name),
            root.join("logs/projects").join(&name),
        ] {
            if output.is_dir() {
                if let Err(e) = remove_project_dir(&output).await {
                    tracing::warn!(path = %output.display(), error = %e, "generated output not removed");
                }
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

    // The two the rest of the machine shares. Both run on success only: a
    // failed delete leaves the project in place, and taking its name out of
    // the hosts file would make the project it did not delete unreachable.
    if outcome.is_ok() {
        drop_project_host(&app, domain.as_deref()).await;
        sync_certificate(&app, &state, &root).await;
    }

    outcome.map(|_| operation_id)
}

/// Every container this project owns: the web one, its worker sidecars, and a
/// tunnel if one was ever opened.
///
/// Stop-only was the old behaviour, and a stopped container is still a name
/// Docker will refuse to reuse, still a row in `docker ps -a`, and still the
/// thing that makes recreating a project under the same name fail.
async fn remove_project_containers(name: &str) {
    let mut ids = vec![name.to_string(), crate::tunnel::container_id(name)];
    ids.extend(
        crate::worker::Kind::ALL
            .iter()
            .map(|kind| crate::worker::container_id(name, *kind)),
    );

    for id in ids {
        if let Err(e) = engine::remove_container(&id).await {
            tracing::warn!(container = %id, error = %e.message, "container not removed");
        }
    }
}

/// Take a deleted project's name back out of `/etc/hosts`.
///
/// The mirror of `sync_project_host`, and it inherits that function's one hard
/// rule from `sync_service_host`: only lines StackVo wrote come back out. A
/// line somebody added by hand stays, even for a project being deleted — a
/// tool that removes entries it did not write is a tool nobody trusts with
/// that file again.
async fn drop_project_host(app: &AppHandle, domain: Option<&str>) {
    let Some(domain) = domain.filter(|d| hosts::is_valid_domain(d)) else {
        return;
    };

    let managed = hosts::mapped_domains()
        .1
        .contains(&domain.to_ascii_lowercase());
    if !managed {
        return;
    }

    match hosts::apply(&[], &[domain.to_string()]) {
        Ok(plan) => events::emit(
            app,
            "hosts:changed",
            serde_json::json!({ "added": plan.add, "removed": plan.remove }),
        ),
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e.message, "hosts entry not removed")
        }
    }
}

/// `remove_dir_all`, retried when the tree gains an entry while it is emptied.
///
/// Reported as `Directory not empty (os error 66)` deleting a project nothing
/// was running. `remove_dir_all` reads a directory, unlinks what it read, then
/// `rmdir`s it — and anything that writes into that directory in between makes
/// the final `rmdir` fail. On macOS the usual author is Finder putting
/// `.DS_Store` back into a folder it has open; an editor's swap file and an
/// indexer do the same thing. Nothing is wrong with the deletion, it simply
/// lost a race, and the second pass finds the directory almost empty.
///
/// Bounded, and only for the two errors that are actually races. A permission
/// error — a `storage/` tree a container wrote as root, say — is a real refusal
/// and is reported on the first attempt rather than three seconds later.
async fn remove_project_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::io::ErrorKind;
    const ATTEMPTS: u32 = 3;

    for attempt in 1..=ATTEMPTS {
        let error = match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };

        let racy = matches!(
            error.kind(),
            ErrorKind::DirectoryNotEmpty | ErrorKind::ResourceBusy
        );
        if attempt == ATTEMPTS || !racy {
            return Err(error);
        }

        tracing::warn!(
            dir = %dir.display(),
            attempt,
            error = %error,
            "the project directory gained an entry while it was being removed; retrying"
        );
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    unreachable!("the loop returns on the last attempt")
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
        &events::sink(app),
        runner::Operation {
            operation_id: &operation_id,
            subject,
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: root,
            env: &[],
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
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: "stack",
            progress_event: "compose:progress",
            finished_event: "compose:done",
            program: "docker",
            args: &args,
            cwd: &root,
            env: &[],
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
        "browsers": crate::apps::browsers(),
    })
}

/// Open a URL in the browser the user chose, falling back to the system's.
///
/// A command of this app's own rather than the opener plugin, for two reasons
/// that turned out to be one: the plugin has no notion of *which* browser, and
/// its `open_url` is scope-checked — a `allow-open-url` permission granted
/// without a scope matches nothing and answers `ForbiddenUrl`, which is
/// exactly why every "visit" button in this app did nothing at all. The scope
/// is fixed too, but a project's own domain deserves the browser the user
/// works in, not whatever the OS last associated with `https`.
#[tauri::command]
pub fn open_in_browser(url: String) -> Result<()> {
    // Only web URLs. Everything reaching this is built by the app from a
    // project or service domain, and a launcher that accepts `file://` or a
    // custom scheme from its own front end is a way to start arbitrary things.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(Error::new(
            Code::InvalidInput,
            "only http and https URLs can be opened",
        ));
    }

    let configured = prefs_get()
        .ok()
        .and_then(|p| {
            p.get("browserCommand")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty());

    if let Some(launch) = crate::apps::resolve_browser(configured.as_deref()) {
        let spawned = match launch {
            crate::apps::Launch::Command(cmd) => {
                std::process::Command::new(cmd).arg(&url).spawn().is_ok()
            }
            crate::apps::Launch::Bundle(bundle) => std::process::Command::new("open")
                .args(["-a", bundle])
                .arg(&url)
                .spawn()
                .is_ok(),
        };
        if spawned {
            return Ok(());
        }
        // Chosen browser could not start — fall through to the system default
        // rather than leaving the click with nothing to show for it.
    }

    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(&url)
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(&url).spawn()
    };

    opened.map(|_| ()).map_err(|e| {
        Error::new(Code::NotFound, format!("could not open a browser: {e}"))
            .with_hint(crate::hints::CHOOSE_A_BROWSER)
    })
}

/// Show a directory in the system's file manager.
///
/// Its own command rather than the opener plugin's `open_path`, for the reason
/// this app's capability file already gives: the filesystem is reached through
/// typed commands, not blanket plugin permissions. The plugin's permission is
/// documented as enabling the command "without any pre-configured scope", and
/// a scope that would cover an arbitrary workspace is a scope that covers
/// everything.
///
/// The check here is narrower and means something: it must be a directory that
/// exists. A path that does not is a bug in the caller, and reporting it beats
/// spawning a file manager on nothing.
#[tauri::command]
pub fn open_folder(path: String) -> Result<()> {
    let dir = std::path::Path::new(&path);
    if !dir.is_dir() {
        return Err(Error::new(
            Code::NotFound,
            format!("{path} is not a directory"),
        ));
    }

    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(dir).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(dir).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(dir).spawn()
    };

    opened.map(|_| ()).map_err(|e| {
        Error::new(
            Code::NotFound,
            format!("could not open a file manager: {e}"),
        )
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
            if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
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

    // Same canonicalisation as `project_create`, and for the same reason: the
    // adoption that follows scaffolding keys off the directory this makes.
    let name = workspace::canonical_name(&name);

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
        .with_hint(crate::hints::ADOPT_EXISTING_CODE));
    }
    std::fs::create_dir_all(&dir).map_err(|e| Error::io("creating the project directory", e))?;

    let user = crate::scaffold::current_user().await;
    let operation_id = events::next_operation_id("scaffold");
    let sink = events::sink(&app);

    // A template either runs an installer or is written directly. The six
    // written ones — Gin, Echo, Flask, FastAPI, Sinatra, Rocket — have no
    // scaffolder in their ecosystem, and pulling an image to write thirty
    // lines would be a download for nothing. Their dependencies are installed
    // by the project's own Dockerfile, for the container's platform.
    let outcome =
        match crate::scaffold::run_args(template, &dir.display().to_string(), user.as_deref()) {
            Some(args) => {
                runner::run_operation(
                    &sink,
                    runner::Operation {
                        operation_id: &operation_id,
                        subject: &name,
                        progress_event: "scaffold:progress",
                        finished_event: "scaffold:done",
                        program: "docker",
                        args: &args,
                        cwd: &root,
                        env: &[],
                    },
                )
                .await
            }
            None => {
                let written = crate::scaffold::write_files(template, &dir);
                let ok = written.is_ok();
                if let Ok(files) = &written {
                    for file in files {
                        sink.emit(
                            "scaffold:progress",
                            events::ProgressEvent {
                                operation_id: operation_id.clone(),
                                subject: name.clone(),
                                line: format!("wrote {file}"),
                            },
                        );
                    }
                }
                sink.emit(
                    "scaffold:done",
                    events::FinishedEvent {
                        operation_id: operation_id.clone(),
                        subject: name.clone(),
                        success: ok,
                        duration_ms: 0,
                        error: written.as_ref().err().map(|e| e.message.clone()),
                        log_path: None,
                    },
                );
                written.map(|_| ())
            }
        };

    if outcome.is_err() {
        // A failed install that wrote nothing should not leave a husk that
        // blocks the retry; a partial write is kept for inspection.
        let _ = std::fs::remove_dir(&dir);
    }
    outcome?;
    Ok(operation_id)
}

/// Is `git` on this machine? The clone option is hidden without it.
///
/// A query rather than something the front end infers, because "is a program
/// installed" is not a question a webview can answer, and because the answer
/// has to survive an app launched from the Dock — see [`crate::git::available`].
#[tauri::command]
pub fn git_available() -> bool {
    crate::git::available()
}

/// Clone a repository into the project tree with the user's own git.
///
/// **This app does not do authentication.** No keys, no agent, no
/// `known_hosts`, no tokens, no host trust — `git` and `ssh` read the config
/// the user already has, and everything that makes their clone work in a
/// terminal is what makes it work here. The two environment variables in
/// [`crate::git::CLONE_ENV`] configure nothing except that the subprocess must
/// fail rather than wait for an answer, because there is no terminal to answer
/// in.
///
/// Ends where `project_scaffold` ends: with code on disk and no manifest. The
/// front end follows with `project_adopt`, so detection, the manifest, the
/// hosts entry and the certificate all come from the one path they already
/// came from — a clone must not become a second way to create a project.
#[tauri::command]
pub async fn project_clone(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    name: Option<String>,
) -> Result<serde_json::Value> {
    if !crate::git::available() {
        return Err(Error::new(Code::NotFound, "git is not installed.")
            .with_hint(crate::hints::INSTALL_GIT_OR_ADOPT));
    }

    let repo = crate::git::parse(&url)?;
    // An explicit name wins; otherwise the one git itself would use. Both go
    // through the same canonicalisation as every other creation path.
    let name = match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(given) => workspace::canonical_name(given),
        None => repo.name.clone(),
    };

    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    let root = state.root()?;
    // The path-safety gate, as everywhere else: a name is never joined directly.
    let dir = workspace::project_dir(&root, &name)?;

    if dir.exists() {
        return Err(Error::new(
            Code::AlreadyExists,
            format!("projects/{name} already exists"),
        )
        .with_hint(crate::hints::CHOOSE_ANOTHER_NAME));
    }

    // The parent has to exist; the target must not — git creates it, and
    // creating it here would mean git cloning into a directory we made, which
    // it accepts only while empty and which we would then have to clean up.
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io("creating the projects directory", e))?;
    }

    let operation_id = events::next_operation_id("clone");
    let args = crate::git::clone_args(&repo, &dir);

    let outcome = runner::run_operation(
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: &name,
            // The scaffold events, deliberately: this is the same step of the
            // same flow — put code in a directory — and the console already
            // subscribes to them.
            progress_event: "scaffold:progress",
            finished_event: "scaffold:done",
            program: "git",
            args: &args,
            cwd: &root,
            env: &crate::git::CLONE_ENV,
        },
    )
    .await;

    if outcome.is_err() {
        // Git removes the directory it created when a clone fails, so this is
        // only for the case where it left something behind. `remove_dir` and
        // not `remove_dir_all`: whatever is in there came off a remote nobody
        // has inspected yet, and a recursive delete on a path built from a
        // user-supplied URL is not a line worth writing.
        let _ = std::fs::remove_dir(&dir);
    }
    outcome?;

    // Which of the two follow-ups the caller owes.
    //
    // A repository may or may not carry its own `stackvo.json`, and the two
    // cases need opposite things: without one, adoption detects and writes;
    // with one, the settings are already the team's answer and only need
    // bringing online. Cloning used to end in "already has a stackvo.json" for
    // the second case — the one the file was designed for.
    let has_manifest = workspace::project_dir(&root, &name)
        .map(|d| d.join("stackvo.json").is_file())
        .unwrap_or(false);

    Ok(serde_json::json!({
        "operationId": operation_id,
        "name": name,
        "hasManifest": has_manifest,
    }))
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
        .with_hint(crate::hints::WORKERS_ARE_DETECTED));
    }

    // The image comes from the project's web container: the one image that is
    // guaranteed to carry the right PHP and extensions for this code.
    let containers = engine::stackvo_containers().await?;
    let image = containers
        .get(&name)
        .and_then(|c| c.image.clone())
        .ok_or_else(|| {
            Error::new(Code::Conflict, format!("{name} has no built container"))
                .with_hint(crate::hints::BUILD_AND_START_FOR_WORKER)
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
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: &name,
            progress_event: "tunnel:progress",
            finished_event: "tunnel:done",
            program: "docker",
            args: &args,
            cwd: &root,
            env: &[],
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
/// unused volumes and the build cache.
///
/// `build_cache` defaults to `Keep`. `Dangling` is what a project deletion
/// already does for itself; `All` is the one that reclaims the shared layers
/// every project image is built on, and costs each of them a full rebuild.
/// A level rather than a flag because those are two different bargains.
///
/// Volumes are opt-in per call rather than a default, because the engine's
/// "unused" means "not currently mounted": the database of a project that
/// happens to be stopped qualifies. The UI states this before offering it.
#[tauri::command]
pub async fn docker_prune(
    state: State<'_, AppState>,
    images: bool,
    volumes: bool,
    build_cache: Option<engine::BuildCache>,
) -> Result<engine::PruneReport> {
    // One prune at a time: two concurrent passes double-report the same bytes.
    let _busy = state.inflight.acquire("prune")?;
    engine::prune(images, volumes, build_cache.unwrap_or_default()).await
}

// ---------------------------------------------------------------- preferences

/// User preferences, stored beside the workspace pointer.
///
/// Replaces the localStorage-backed `usePreferences` composable: a webview's
/// localStorage is cleared by a cache reset, and the editor command needs to be
/// readable from Rust anyway.
/// The shape `preferences.json` is written in.
///
/// There was no version field, so there was no handle to migrate by: a future
/// release that renamed a key would have to guess whether an absent key meant
/// "old file" or "never set". One number now costs nothing and is the only
/// thing that makes the answer knowable later.
const PREFS_SCHEMA_VERSION: u64 = 1;

#[tauri::command]
pub fn prefs_get() -> Result<serde_json::Value> {
    Ok(read_prefs(&prefs_path()?))
}

/// The reading half, with the path passed in.
///
/// Split out only so it can be tested: `prefs_path()` resolves the real OS
/// config directory, and a test that exercised recovery through it would move
/// the preferences of whoever ran `cargo test`.
fn read_prefs(path: &std::path::Path) -> serde_json::Value {
    // No file is a fresh install, not a fault — the one case that must stay
    // silent.
    let Ok(text) = std::fs::read_to_string(path) else {
        return default_prefs();
    };

    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) if value.is_object() => migrated(value),

        // Two failures, one answer. The `Ok` arm is the one that was missing:
        // `from_str` accepts a bare `3` or `"x"` as valid JSON, and the old code
        // returned it. Every later `prefs_set` then found `as_object_mut() ==
        // None`, merged into nothing, and wrote the same scalar back — so every
        // setting the user changed was silently discarded, for ever, with a
        // parseable file on disk.
        Ok(_) | Err(_) => {
            preserve_corrupt(path);
            default_prefs()
        }
    }
}

/// Bring a stored preferences object up to the current shape.
///
/// Only stamps the version today. It exists now so that the release that *does*
/// need to rename a key has somewhere to put the migration, rather than
/// inventing this function under time pressure and guessing at the old shape.
fn migrated(mut value: serde_json::Value) -> serde_json::Value {
    let stored = value.get("schemaVersion").and_then(|v| v.as_u64());

    if let Some(object) = value.as_object_mut() {
        // Absent means "written before versioning existed", which is shape 1 —
        // no key has been renamed yet, so nothing has to move.
        if stored != Some(PREFS_SCHEMA_VERSION) {
            object.insert(
                "schemaVersion".into(),
                serde_json::json!(PREFS_SCHEMA_VERSION),
            );
        }
    }
    value
}

/// Move an unparseable preferences file aside instead of overwriting it.
///
/// The old behaviour was `unwrap_or_else(|_| default_prefs())`: not crashing was
/// right, and losing the file was not. Every setting the user had chosen went
/// back to default with no warning and no copy — and the first `prefs_set`
/// afterwards wrote defaults over the evidence.
///
/// Renamed rather than copied, deliberately. A copy would be re-made on every
/// launch for as long as the bad file sat there; a rename leaves no file at all,
/// so the next launch is an ordinary fresh start. It is safe because this only
/// runs on *malformed* JSON — a file from a future release carrying keys this
/// version does not know is still a valid object, so it parses and reaches
/// [`migrated`] untouched.
fn preserve_corrupt(path: &std::path::Path) {
    let stamp = crate::crash::stamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    );
    let backup = path.with_file_name(format!("preferences.corrupt-{stamp}.json"));

    match std::fs::rename(path, &backup) {
        Ok(()) => tracing::error!(
            from = %path.display(),
            to = %backup.display(),
            "preferences.json could not be parsed; it was kept and defaults were loaded"
        ),
        // Nothing else to do: returning defaults is still better than failing to
        // start, and the file is left where the user can find it.
        Err(e) => tracing::error!(
            path = %path.display(),
            error = %e,
            "preferences.json could not be parsed and could not be moved aside"
        ),
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

/// Everything a bug report needs, written to a file the user chose.
///
/// `logs_info` above can point at the log folder, and that was the whole of the
/// support story: find the newest of seven daily files, know that the doctor
/// output is a separate thing, remember the version and the platform. Most
/// people attach one log and the first reply asks for the other four things.
///
/// `path` comes from the system save dialog, like `mail_attachment_save`'s —
/// the front end names no destination this process did not receive from the
/// user. Everything that goes in is masked on the way; `diagnostics` explains
/// why it is masked twice.
#[tauri::command]
pub async fn diagnostics_bundle(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::diagnostics::Bundle> {
    // Best effort on the workspace: a bundle from a machine with no workspace
    // selected is exactly the bundle somebody needs when the app will not get
    // that far, so a missing root narrows the contents rather than refusing.
    let root = state.root().ok();
    crate::diagnostics::write(root.as_deref(), std::path::Path::new(&path)).await
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

/// The third-party licence notice this build was compiled with.
///
/// A command rather than a file the front end fetches, for the same reason
/// `updater_status` reads `include_str!`'d configuration: a notice read at run
/// time from a path is a notice that can be absent, and an app that quietly
/// ships no attribution is the state the notice exists to end. What this
/// returns is the text in the binary or nothing at all — there is no third
/// outcome. [`crate::licences`] carries the rest of the reasoning.
#[tauri::command]
pub fn licences_notice() -> &'static str {
    crate::licences::NOTICE
}

/// The user's language: what they chose, else what the machine is set to.
///
/// The order and the detection live in [`crate::locale`], because the window
/// needs the same answer as the tray and the two used to work it out
/// separately — with different fallbacks, which is how a Turkish machine ended
/// up with an English tray under a Turkish window.
pub fn preferred_locale() -> String {
    let stored = prefs_get()
        .ok()
        .and_then(|p| p.get("locale").and_then(|v| v.as_str()).map(str::to_string));
    crate::locale::resolve(stored.as_deref()).to_string()
}

/// The language the window should open in.
///
/// A command rather than letting the front end work it out from `prefs_get`:
/// the fallback is a reading of the operating system, which a webview cannot
/// do — `navigator.language` answers from the app bundle's localised
/// resources, and this app ships none.
#[tauri::command]
pub fn locale_get() -> String {
    preferred_locale()
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
    crate::appdir::config()
        .map(|d| d.join("preferences.json"))
        .ok_or_else(|| Error::new(Code::IoError, "cannot determine the OS config directory"))
}

fn default_prefs() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": PREFS_SCHEMA_VERSION,
        "locale": null,
        "theme": "system",
        "editorCommand": null,
        "terminalApp": null,
        "browserCommand": null,
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
        .with_hint(crate::hints::ONLY_PROJECT_FOLDERS));
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

    Err(Error::new(Code::NotFound, "No editor found.").with_hint(crate::hints::CHOOSE_AN_EDITOR))
}

/// Ask the OS for a folder.
///
/// `(async)` is load-bearing, and its absence is what froze the window. Tauri
/// runs a *synchronous* command on the main thread — there is no command
/// threadpool for one, which is what the note here used to claim. On macOS the
/// panel itself must run on the main thread too, so `blocking_pick_folder`
/// schedules it there and blocks the caller until it closes: called from the
/// main thread, that is the main thread waiting for work only the main thread
/// can do. The panel still appeared, because AppKit runs it on a nested run
/// loop, and everything behind it stopped drawing for as long as it was open.
///
/// The attribute moves the body onto a blocking task, which is the arrangement
/// the plugin documents: block a worker, leave the main thread to draw the
/// window and run the panel.
#[tauri::command(async)]
pub fn workspace_pick(
    app: AppHandle,
    state: State<'_, AppState>,
    watcher: State<'_, crate::watcher::Handle>,
) -> Result<Option<Workspace>> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app.dialog().file().blocking_pick_folder();

    let Some(folder) = picked else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|e| Error::new(Code::IoError, format!("could not resolve the folder: {e}")))?;

    let ws = workspace::set_projects(&path)?;
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
    let bash_path = if m.runtime != "php" {
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

/// One generated file, rendered in memory and not yet on disk.
pub struct GenFile {
    /// Human-facing label — `parser.ajans/Dockerfile`, `configs/mysql.cnf`.
    pub label: String,
    /// Absolute target path.
    pub path: std::path::PathBuf,
    /// `projects` or `services` — which generate scope owns it, mirroring the
    /// Bash orchestrator's two subcommands.
    pub scope: &'static str,
    pub content: String,
}

/// Render everything the generator owns, in memory.
///
/// The single source both `verify_generator` (compare against disk) and
/// `write_generated` (write to disk) consume — one enumeration, so the set
/// that is verified and the set that is written cannot drift apart.
///
/// Project render failures come back as `(label, error)` pairs rather than
/// failing the whole call: one broken manifest must neither hide the other
/// projects nor abort a stack-wide regenerate, which is also what the Bash
/// generator did.
/// What a render produced: the files, and the manifests that were skipped
/// paired with the reason. The second half is not an error channel — a broken
/// manifest is reported alongside the projects that rendered fine.
pub type Rendered = (Vec<GenFile>, Vec<(String, String)>);

pub fn render_generated(root: &std::path::Path) -> Result<Rendered> {
    use crate::generator;

    let env = Env::load(root)?;
    let limits = generator::ServerSettings::from_env(&env);
    let extras = generator::ServerExtras::load(root, &env);
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

    let mut files: Vec<GenFile> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();

    // ---- per-project files ----
    let mut manifests: Vec<(String, crate::manifest::Manifest)> = Vec::new();
    if let Some(entries) =
        crate::workspace::projects_root(root).and_then(|p| std::fs::read_dir(p).ok())
    {
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
            // C-19, generalised: every snapshot runtime builds from the
            // project source dir, so that is where its Dockerfile lives.
            let dockerfile_path = if m.runtime != "php" {
                path.join("Dockerfile")
            } else {
                root.join("generated/projects")
                    .join(name)
                    .join("Dockerfile")
            };

            match generator::render_from_manifest(&m, &opts, false) {
                Ok(content) => files.push(GenFile {
                    label: format!("{name}/Dockerfile"),
                    path: dockerfile_path,
                    scope: "projects",
                    content,
                }),
                Err(e) => errors.push((format!("{name}/Dockerfile"), e)),
            }

            // A node build context must never swallow host node_modules; the
            // Bash generator rewrites this beside the Dockerfile on every run.
            let dockerignore = match m.runtime.as_str() {
                "node" => Some(generator::NODE_DOCKERIGNORE),
                other => generator::lang_dockerignore(other),
            };
            if let Some(content) = dockerignore {
                files.push(GenFile {
                    label: format!("{name}/.dockerignore"),
                    path: path.join(".dockerignore"),
                    scope: "projects",
                    content: content.to_string(),
                });
            }

            // nginx.conf / supervisord.conf / Caddyfile per server; apache,
            // swoole and node correctly contribute nothing here.
            for (file, content) in generator::render_project_config_files_with(&m, &limits, &extras)
            {
                files.push(GenFile {
                    label: format!("{name}/{file}"),
                    path: root.join("generated/projects").join(name).join(file),
                    scope: "projects",
                    content,
                });
            }
            manifests.push((name.to_string(), m));
        }
    }

    // ---- the projects compose file ----
    let projects = generator::compose_projects_from(&manifests);
    files.push(GenFile {
        label: "docker-compose.projects.yml".into(),
        path: root.join("generated/docker-compose.projects.yml"),
        scope: "projects",
        content: generator::render_compose_projects(
            &projects,
            &root.display().to_string(),
            &crate::workspace::require_projects_root(root)?
                .display()
                .to_string(),
        ),
    });

    // ---- the base compose (stackvo.yml) ----
    //
    // Traefik and the network — `generate_base_compose` renders
    // `core/compose/base.yml` through the same substitution engine the
    // service templates use. This was the one file the Sprint 15 "verify
    // covers everything" claim missed; enumerated here, the claim is true.
    let vars = crate::template::variables(&env, root);
    if let Some(text) = crate::skeleton::read_template(root, "core/compose/base.yml") {
        files.push(GenFile {
            label: "stackvo.yml".into(),
            path: root.join("generated/stackvo.yml"),
            scope: "services",
            content: crate::template::render(&text, &vars),
        });
    }

    // ---- service configs ----
    {
        // (template, output) — the exact mapping in config.sh. The first five
        // are rendered; the last two are copied verbatim by Bash.
        const RENDERED: [(&str, &str); 5] = [
            ("redis/redis.conf.tpl", "redis.conf"),
            ("mysql/my.cnf.tpl", "mysql.cnf"),
            ("mongo/mongo.conf.tpl", "mongo.conf"),
            ("postgres/postgres.conf.tpl", "postgres.conf"),
            ("elasticsearch/elasticsearch.yml.tpl", "elasticsearch.yml"),
        ];
        const COPIED: [(&str, &str); 2] = [
            ("mariadb/my.cnf", "mariadb.cnf"),
            ("percona/my.cnf", "percona.cnf"),
        ];

        for (template, output) in RENDERED {
            // The workspace's copy wins, the compiled-in one is the fallback:
            // shipping templates must not take away the ability to edit them.
            let Some(text) = crate::skeleton::read_template(
                root,
                &format!("core/templates/services/{template}"),
            ) else {
                continue;
            };
            files.push(GenFile {
                label: format!("configs/{output}"),
                path: root.join("generated/configs").join(output),
                scope: "services",
                content: crate::template::render(&text, &vars),
            });
        }
        for (source, output) in COPIED {
            let Some(text) =
                crate::skeleton::read_template(root, &format!("core/templates/services/{source}"))
            else {
                continue;
            };
            files.push(GenFile {
                label: format!("configs/{output}"),
                path: root.join("generated/configs").join(output),
                scope: "services",
                content: text,
            });
        }
    }

    // ---- the dynamic compose ----
    files.push(GenFile {
        label: "docker-compose.dynamic.yml".into(),
        path: root.join("generated/docker-compose.dynamic.yml"),
        scope: "services",
        content: crate::template::render_dynamic_compose(root, &vars),
    });

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

    files.push(GenFile {
        label: "traefik/traefik.yml".into(),
        path: root.join("generated/traefik/traefik.yml"),
        scope: "services",
        content: generator::render_traefik_config(&traefik),
    });
    files.push(GenFile {
        label: "traefik/dynamic/routes.yml".into(),
        path: root.join("generated/traefik/dynamic/routes.yml"),
        scope: "services",
        content: generator::render_traefik_routes(&traefik),
    });

    Ok((files, errors))
}

/// The routing warning, computed the same way the render does — kept separate
/// so both the verify report and the write report can carry it.
fn generator_warnings(root: &std::path::Path) -> Vec<String> {
    let Ok(env) = Env::load(root) else {
        return Vec::new();
    };
    let catalog = env_schema().service_catalog();
    let services: Vec<(&str, bool, Option<&str>)> = catalog
        .iter()
        .map(|(id, _)| (id.as_str(), env.service_enabled(id), env.service_url(id)))
        .collect();
    let traefik = crate::generator::TraefikOptions {
        tld_suffix: env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc"),
        network: env.get("DOCKER_DEFAULT_NETWORK").unwrap_or("stackvo-net"),
        ssl_enabled: env.bool("SSL_ENABLE"),
        redirect_to_https: env.bool("REDIRECT_TO_HTTPS"),
        services,
    };
    crate::generator::traefik_routing_warning(&traefik)
        .map(|w| vec![w])
        .unwrap_or_default()
}

/// The command's logic, free of Tauri `State` so the `diagnose` example runs
/// exactly the same comparison the app does.
pub fn verify_generator(root: &std::path::Path) -> Result<serde_json::Value> {
    let (rendered, errors) = render_generated(root)?;

    let mut files: Vec<serde_json::Value> = Vec::new();
    for f in &rendered {
        let theirs = std::fs::read_to_string(&f.path).ok();
        let (status, at) = match &theirs {
            None => ("missing", None),
            Some(t) if *t == f.content => ("match", None),
            Some(t) => (
                "differ",
                f.content
                    .lines()
                    .zip(t.lines())
                    .position(|(a, b)| a != b)
                    .map(|i| i as u64 + 1),
            ),
        };
        files.push(serde_json::json!({
            "file": f.label,
            "path": f.path.display().to_string(),
            "status": status,
            "firstDifferenceLine": at,
        }));
    }
    for (label, error) in &errors {
        files.push(serde_json::json!({
            "file": label,
            "status": "error",
            "error": error,
        }));
    }

    let matched = files.iter().filter(|f| f["status"] == "match").count();
    let differed = files.iter().filter(|f| f["status"] == "differ").count();

    Ok(serde_json::json!({
        "files": files,
        "matched": matched,
        "differed": differed,
        "readyToTakeOver": differed == 0,
        // Surfaced here because the desktop app can say the routing is broken;
        // StackVo itself never does. See CONFLICTS.md C-20.
        "warnings": generator_warnings(root),
    }))
}

/// Does this generate scope include files of this kind?
///
/// The narrowing scopes are exactly `projects` and `services`; **anything
/// else means everything** — which is the Bash orchestrator's `case` falling
/// through to "generate all", and the semantics its callers still rely on:
/// `service_enable` passes `projects_and_services`, and the takeover
/// initially read that as "matches nothing", wrote zero files, and reported
/// success — an enabled service whose container could never come up, because
/// it was never written into the compose file being `up`'d.
fn scope_includes(scope: &str, file_scope: &str) -> bool {
    match scope {
        "projects" | "services" => scope == file_scope,
        _ => true,
    }
}

/// Write the generated files — the Rust generator as the generator, not the
/// understudy.
///
/// Writes are **in place** (truncate-and-write, exactly the shell's `>`),
/// never staged-and-renamed: Traefik's file provider was measured to ignore an
/// atomic rename outright — see the `cert_apply` note — and the generated
/// tree is precisely the directory it watches.
///
/// `on_file` is called once per file written, which is what the operation
/// console shows as progress.
pub fn write_generated(
    root: &std::path::Path,
    scope: &str,
    mut on_file: impl FnMut(&str),
) -> Result<serde_json::Value> {
    let (rendered, errors) = render_generated(root)?;

    // The directories Bash's generators mkdir before writing. The log trees
    // matter beyond the writes below: the generated compose mounts them, and
    // compose does not create host directories for bind mounts.
    for dir in [
        "generated/projects",
        "generated/configs",
        "generated/traefik/dynamic",
        "logs/projects",
        "logs/services",
    ] {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|e| Error::io(format!("creating {dir}"), e))?;
    }

    let mut written: Vec<String> = Vec::new();
    for f in rendered {
        if !scope_includes(scope, f.scope) {
            continue;
        }
        if let Some(parent) = f.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        std::fs::write(&f.path, &f.content)
            .map_err(|e| Error::io(format!("writing {}", f.path.display()), e))?;
        on_file(&f.label);
        written.push(f.label);
    }

    Ok(serde_json::json!({
        "engine": "rust",
        "scope": scope,
        "written": written.len(),
        "files": written,
        "skipped": errors
            .iter()
            .map(|(label, error)| serde_json::json!({ "file": label, "error": error }))
            .collect::<Vec<_>>(),
        "warnings": generator_warnings(root),
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
    /// Retired. Kept in the enum so an old caller gets a sentence about what
    /// happened instead of a deserialisation error.
    Bash,
    /// Render without writing and report drift against what is on disk.
    Verify,
    /// Rust writes. The default and, since the takeover, the only writer.
    #[default]
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

    // The staged takeover is over: the Rust generator took over once the
    // parity check reached 28/28 on real data, and the Bash engine was
    // retired with it. The mode survives as two behaviours, not three.
    match mode {
        GeneratorEngine::Bash => Err(Error::new(
            Code::Unsupported,
            "The Bash engine was retired after the Rust port reached byte parity on every file.",
        )
        .with_hint(crate::hints::USE_GENERATE_RUN)),

        // Verify without writing: now a *drift* check — does what is on disk
        // still match what this generator would write? Catches hand-edited
        // generated files, which byte parity used to catch by accident.
        GeneratorEngine::Verify => Ok(serde_json::json!({
            "operationId": operation_id,
            "engine": "verify",
            "report": verify_generator(&root)?,
        })),

        GeneratorEngine::Rust => {
            generate(&app, &root, &operation_id, &scope).await?;
            Ok(serde_json::json!({
                "operationId": operation_id,
                "engine": "rust",
                "report": verify_generator(&root)?,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The volumes a disable deletes, from the shipped templates.
    ///
    /// The prefix approach this replaced would have taken Mongo Express's data
    /// with Mongo's, because `stackvo-mongo-` is a prefix of
    /// `stackvo-mongo-express-…`. That is the whole reason the names are read
    /// from the template, and it is asserted against the real ones in the
    /// binary rather than a fixture — a template gaining a volume must show up
    /// here.
    #[test]
    fn a_services_volumes_are_its_own_and_not_a_name_prefix_match() {
        let root = std::env::temp_dir().join("stackvo-declared-volumes-none");

        // Compose prefixes with the project name from base.yml; both spellings
        // are offered because a pinned `name:` produces the bare one.
        let mysql = declared_volumes(&root, "mysql");
        assert!(mysql.contains(&"stackvo_stackvo-mysql-data".to_string()));
        assert!(mysql.contains(&"stackvo-mysql-data".to_string()));

        // Mongo's list must not reach into Mongo Express's namespace.
        for volume in declared_volumes(&root, "mongo") {
            assert!(
                !volume.contains("mongo-express"),
                "{volume} belongs to another service"
            );
        }

        // A service that declares none gets none — not an empty-prefix match
        // that would sweep every volume on the machine.
        assert!(
            declared_volumes(&root, "phpmyadmin").is_empty(),
            "phpmyadmin declares no volumes"
        );
    }

    /// A service's own `volumes:` list is bind mounts and references to
    /// volumes declared elsewhere. Reading those as declarations would delete
    /// paths on the host and volumes belonging to other services.
    #[test]
    fn only_the_top_level_volumes_block_declares_anything() {
        for volume in declared_volumes(&std::env::temp_dir(), "mysql") {
            assert!(
                !volume.contains('/'),
                "{volume} is a bind mount, not a named volume"
            );
        }
    }

    /// The stack answers on more than its projects.
    ///
    /// `hosts_missing` offered project domains and nothing else, so an admin
    /// UI or the proxy's own dashboard failed to resolve with nothing in the
    /// app to say why — the checkout this was written against had those lines
    /// only because the retired Bash CLI once wrote them. This pins the three
    /// kinds of domain the stack serves, since only one of them was covered.
    /// Every hosts write shows the system's password prompt, so the question
    /// this table answers is "does the user get asked". A toggle that would
    /// change nothing must not.
    /// Deleting a project removes a populated tree, and says so honestly when
    /// there is nothing to remove rather than retrying its way to a timeout.
    #[tokio::test]
    async fn removing_a_project_directory_clears_a_populated_tree() {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-remove-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let nested = dir.join("vendor").join("laravel");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("composer.json"), "{}").unwrap();
        // The entry that starts the race in the first place.
        std::fs::write(dir.join(".DS_Store"), "").unwrap();

        remove_project_dir(&dir).await.unwrap();
        assert!(!dir.exists());

        // A directory that is not there is not a race, so it fails at once
        // with the reason rather than after three passes.
        let missing = remove_project_dir(&dir).await.unwrap_err();
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn a_hosts_prompt_only_happens_when_the_file_would_change() {
        // Enabled and unresolvable: add it — otherwise the admin UI opens on
        // a name that does not resolve, which is the bug this exists for.
        assert!(host_sync_action(true, false, false).is_some());
        assert!(host_sync_action(true, false, true).is_some());
        // Enabled and already there: nothing to do, so no prompt.
        assert!(host_sync_action(true, true, true).is_none());

        // Disabled and ours: take it out, so the file describes the stack.
        assert!(host_sync_action(false, true, true).is_some());
        // Disabled but written by hand: leave it. A tool that deletes lines it
        // did not write is a tool nobody trusts with the file again.
        assert!(host_sync_action(false, true, false).is_none());
        // Disabled and already absent: nothing to do.
        assert!(host_sync_action(false, false, true).is_none());
    }
    #[test]
    fn every_kind_of_domain_the_stack_serves_is_offered() {
        let env = Env::parse(
            "DEFAULT_TLD_SUFFIX=dev.test\n\
             SERVICE_PHPMYADMIN_ENABLE=true\n\
             SERVICE_PHPMYADMIN_URL=pma\n\
             SERVICE_ADMINER_ENABLE=false\n\
             SERVICE_ADMINER_URL=adminer\n",
        );

        let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap();
        let mut wanted: Vec<String> = Vec::new();
        for (id, _) in env_schema().service_catalog() {
            if env.service_enabled(&id) {
                if let Some(url) = env.service_url(&id) {
                    wanted.push(format!("{url}.{tld}"));
                }
            }
        }
        wanted.push(format!("traefik.{tld}"));
        wanted.push(tld.to_string());

        // The service at its own subdomain rather than its id.
        assert!(wanted.contains(&"pma.dev.test".to_string()));
        // A disabled one is not wanted: its line is added when it is enabled
        // and taken away when it is disabled, so the file describes the stack
        // rather than the catalogue.
        assert!(!wanted.contains(&"adminer.dev.test".to_string()));
        // The dashboard, whose router the generator has always written.
        assert!(wanted.contains(&"traefik.dev.test".to_string()));
        // And the suffix itself, which the certificate is already issued for.
        assert!(wanted.contains(&"dev.test".to_string()));
    }
    #[test]
    fn a_service_patch_cannot_reach_past_its_own_service() {
        let patch = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };

        assert!(
            check_service_patch("redis", &patch(&[("SERVICE_REDIS_HOST_PORT", "6380")])).is_ok()
        );

        // A sheet titled "redis" writing another service's password, or a
        // global setting, would be a general .env writer wearing a costume.
        for key in [
            "SERVICE_MYSQL_ROOT_PASSWORD",
            "DEFAULT_TLD_SUFFIX",
            "SERVICE_REDIS",
        ] {
            assert!(
                check_service_patch("redis", &patch(&[(key, "x")])).is_err(),
                "{key} should be refused"
            );
        }

        // The list owns the toggle. Two controls for one key is how they end
        // up disagreeing about whether the service is on.
        assert!(
            check_service_patch("redis", &patch(&[("SERVICE_REDIS_ENABLE", "false")])).is_err()
        );

        // The read masks secrets, so a form that returns what it was handed
        // would save the mask as the password.
        assert!(check_service_patch(
            "mysql",
            &patch(&[(
                "SERVICE_MYSQL_ROOT_PASSWORD",
                "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
            )])
        )
        .is_err());

        // Dashes in a service id become underscores in the key.
        assert!(check_service_patch(
            "mongo-express",
            &patch(&[("SERVICE_MONGO_EXPRESS_BASEURL", "/db")])
        )
        .is_ok());
    }
    #[test]
    fn every_generate_scope_its_callers_pass_writes_something() {
        // `projects` and `services` narrow; everything else is "all" — the
        // Bash case-fallthrough its callers still rely on. The regression this
        // pins: `service_enable` passes `projects_and_services`, and an exact
        // match wrote zero files and reported success, so the just-enabled
        // service was missing from the very compose file being `up`'d.
        for scope in ["all", "projects_and_services", "anything-future"] {
            assert!(scope_includes(scope, "projects"), "{scope}");
            assert!(scope_includes(scope, "services"), "{scope}");
        }
        assert!(scope_includes("projects", "projects"));
        assert!(!scope_includes("projects", "services"));
        assert!(scope_includes("services", "services"));
        assert!(!scope_includes("services", "projects"));
    }

    /// A command that waits for a person must not wait on the main thread.
    ///
    /// Tauri runs a synchronous command on the main thread; only `async fn` or
    /// `#[tauri::command(async)]` moves it off. Two commands here blocked there
    /// on something only a human could finish — the folder panel and the
    /// administrator prompt — and both froze the window for exactly as long as
    /// the person took. `workspace_pick` even carried a comment asserting the
    /// opposite ("Tauri's command threadpool is already off it"), which is why
    /// this is a test rather than a note: the belief was written down, and it
    /// was wrong.
    ///
    /// Reads the source because the property is about the attribute, and the
    /// attribute is invisible to anything else. The same trick `tray.rs` is
    /// checked with in `app-shell.spec.js`.
    #[test]
    fn a_command_that_waits_for_a_person_is_not_on_the_main_thread() {
        const SOURCE: &str = include_str!("commands.rs");

        // Calls that do not return until somebody has clicked, typed or
        // dismissed something. Not "slow" calls — slow is a different problem
        // with a different fix, and listing them here would make this test a
        // performance opinion instead of a correctness one.
        const WAITS_FOR_A_PERSON: [&str; 3] = [
            "blocking_pick_folder",
            "blocking_pick_file",
            // Elevation: `osascript … with administrator privileges` on macOS,
            // `pkexec` on Linux. Both put up a password prompt and block.
            "hosts::apply(",
        ];

        let mut offenders = Vec::new();

        for block in SOURCE.split("#[tauri::command").skip(1) {
            let Some((attribute, rest)) = block.split_once('\n') else {
                continue;
            };
            // `(async)]` — anything else on that line is a plain command.
            let off_main_thread =
                attribute.contains("(async)") || rest.trim_start().starts_with("pub async fn");

            let Some(name) = rest
                .split_once("fn ")
                .and_then(|(_, after)| after.split_once('('))
                .map(|(n, _)| n.trim())
            else {
                continue;
            };

            // Two things end up in a segment that are not part of its body, and
            // both were found by this test reporting `open_in_editor` for a
            // call it does not make.
            //
            // The next command's doc comment: it sits *before* the attribute
            // that terminates the segment, so prose about `blocking_pick_folder`
            // lands in the previous command. Comments are dropped, which is
            // right regardless — a call named in a sentence is not a call.
            //
            // And the test module, which trails the last command and holds the
            // list below as string literals.
            let body: String = block
                .split("\n#[cfg(test)]")
                .next()
                .unwrap_or(block)
                .lines()
                .map(|line| match line.find("//") {
                    Some(at) => &line[..at],
                    None => line,
                })
                .collect::<Vec<_>>()
                .join("\n");

            for call in WAITS_FOR_A_PERSON {
                if body.contains(call) && !off_main_thread {
                    offenders.push(format!("{name} calls {call}"));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "these block the main thread until a person acts — mark them \
             #[tauri::command(async)]: {offenders:?}"
        );

        // A guard over an empty set passes for the wrong reason, and this one
        // greps for strings that a refactor could rename out from under it.
        let seen = WAITS_FOR_A_PERSON
            .iter()
            .filter(|call| SOURCE.contains(*call))
            .count();
        assert!(
            seen >= 2,
            "expected to be checking real calls, matched {seen} of the list"
        );
    }

    // ------------------------------------------------- generate reporting
    //
    // The first tests for the generate operation's event contract. It could not
    // be reached before `progress::Recording` existed: `generate` took an
    // `AppHandle` for two unrelated reasons — the managed lock and the sink —
    // and neither is available outside a running app. `generate_reported` is
    // the half that needed neither.

    /// A workspace the generator can actually run in: the skeleton the binary
    /// carries, plus a projects pointer, which is exactly what `independence`
    /// asserts is enough to render from nothing.
    fn generated_workspace(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-generate-events-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::skeleton::install(&dir).expect("install the embedded skeleton");
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).expect("projects pointer");
        dir
    }

    #[test]
    fn generating_reports_progress_then_exactly_one_terminal_event() {
        let root = generated_workspace("ok");
        let sink = crate::progress::Recording::new();

        generate_reported(&sink, &root, "generate-7", "all").expect("a skeleton must render");

        let names = sink.names();
        assert!(
            names.len() > 1,
            "the generator wrote nothing worth reporting: {names:?}"
        );
        assert!(
            names[..names.len() - 1]
                .iter()
                .all(|n| n == "generate:progress"),
            "something other than progress arrived before the end: {names:?}"
        );
        assert_eq!(
            names.iter().filter(|n| *n == "generate:done").count(),
            1,
            "the terminal event must arrive exactly once"
        );
        assert_eq!(
            names.last().map(String::as_str),
            Some("generate:done"),
            "the terminal event must be last"
        );

        // Both fields are what the operation console keys on: without the
        // subject the opening event fell through its `subject ?? project ??
        // service ?? \"stack\"` chain and opened an operation its own finish
        // never closed. That bug is what this assertion exists to prevent.
        for event in sink.events() {
            assert_eq!(event.str("operationId"), Some("generate-7"));
            assert_eq!(event.str("subject"), Some("all"));
        }

        let done = sink.last("generate:done").unwrap();
        assert_eq!(done.get("success"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(done.get("error"), Some(&serde_json::Value::Null));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The failure mode no type catches: returning `Err` without emitting the
    /// terminal event leaves the console showing an operation that never ends.
    #[test]
    fn a_failed_generate_still_closes_its_operation() {
        // A path with no skeleton and no projects pointer — the generator has
        // nothing to read and no directory to write into.
        let root = std::env::temp_dir().join(format!(
            "stackvo-generate-events-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let sink = crate::progress::Recording::new();

        let error = generate_reported(&sink, &root, "generate-8", "all")
            .expect_err("a workspace that does not exist cannot render");

        let done = sink
            .last("generate:done")
            .expect("a failed operation must still emit its terminal event");
        assert_eq!(done.get("success"), Some(&serde_json::Value::Bool(false)));
        assert_eq!(
            done.str("error"),
            Some(error.message.as_str()),
            "the event and the returned error must say the same thing"
        );
    }

    /// The premise of the whole module: the same call with nowhere to report to
    /// does the same work. This is the path `stackvo-mcp` takes.
    #[test]
    fn generating_without_a_sink_produces_the_same_files() {
        let root = generated_workspace("headless");
        generate_reported(&crate::progress::Null, &root, "generate-9", "all")
            .expect("headless must not change the outcome");

        assert!(
            root.join("generated").join("stackvo.yml").is_file(),
            "the generator did not write with a silent sink"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------- preferences recovery

    fn prefs_scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-prefs-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A file that never existed is a fresh install. It must not leave a
    /// "corrupt" backup implying something went wrong.
    #[test]
    fn a_missing_preferences_file_is_not_a_corruption() {
        let dir = prefs_scratch("missing");
        let prefs = read_prefs(&dir.join("preferences.json"));

        assert_eq!(prefs["theme"], "system");
        assert_eq!(prefs["schemaVersion"], PREFS_SCHEMA_VERSION);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a fresh install wrote something"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The finding: settings went back to default with no warning and no copy,
    /// and the next write put defaults over the evidence.
    #[test]
    fn an_unparseable_file_is_kept_rather_than_lost() {
        let dir = prefs_scratch("corrupt");
        let path = dir.join("preferences.json");
        std::fs::write(&path, "{\"theme\": \"dark\", trunca").unwrap();

        let prefs = read_prefs(&path);
        assert_eq!(prefs["theme"], "system", "defaults are loaded");

        assert!(
            !path.exists(),
            "the bad file must be moved, not left in place"
        );
        let kept: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("preferences.corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "the user's settings were not preserved");
        assert!(std::fs::read_to_string(dir.join(&kept[0]))
            .unwrap()
            .contains("dark"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Valid JSON that is not an object was the silent one: it parsed, so the
    /// old code returned it, and every `prefs_set` afterwards merged into
    /// nothing and wrote the same scalar back. The user changed settings and
    /// none of them ever persisted.
    #[test]
    fn valid_json_that_is_not_an_object_is_treated_as_corrupt() {
        for content in ["3", "\"dark\"", "[1,2,3]", "null"] {
            let dir = prefs_scratch("scalar");
            let path = dir.join("preferences.json");
            std::fs::write(&path, content).unwrap();

            let prefs = read_prefs(&path);
            assert!(
                prefs.is_object(),
                "{content} was returned as-is and would swallow every later write"
            );
            assert_eq!(prefs["theme"], "system");

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// A file written before versioning existed is not corrupt — it is shape 1.
    /// Stamping it is what gives a later release something to migrate from.
    #[test]
    fn an_unversioned_file_is_stamped_and_otherwise_untouched() {
        let dir = prefs_scratch("unversioned");
        let path = dir.join("preferences.json");
        std::fs::write(&path, r#"{"theme":"dark","editorCommand":"code"}"#).unwrap();

        let prefs = read_prefs(&path);
        assert_eq!(prefs["schemaVersion"], PREFS_SCHEMA_VERSION);
        assert_eq!(prefs["theme"], "dark", "the user\'s choice survived");
        assert_eq!(prefs["editorCommand"], "code");
        assert!(path.exists(), "a readable file must not be moved aside");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file from a *newer* release carries keys this build does not know.
    /// It is still a valid object, so it must be read, not quarantined —
    /// quarantining it would delete a newer version\'s settings on a downgrade.
    #[test]
    fn an_unknown_future_shape_is_read_rather_than_quarantined() {
        let dir = prefs_scratch("future");
        let path = dir.join("preferences.json");
        std::fs::write(
            &path,
            r#"{"schemaVersion":99,"theme":"dark","somethingNew":true}"#,
        )
        .unwrap();

        let prefs = read_prefs(&path);
        assert!(path.exists(), "a newer file was destroyed");
        assert_eq!(prefs["theme"], "dark");
        assert_eq!(prefs["somethingNew"], true);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ------------------------------------------------- lifecycle validation

    /// The six start/stop/restart commands share one body, and its first act is
    /// to refuse a name it does not like. That gate had never been exercised:
    /// `lifecycle` took an `AppHandle`, so reaching it from a test meant
    /// running a Tauri app.
    ///
    /// Worth a test rather than trusted to the comment above it, because the id
    /// does not stay an id — it becomes a container name and a compose service
    /// name. Nothing downstream re-checks it.
    #[tokio::test]
    async fn a_rejected_name_touches_neither_docker_nor_the_event_stream() {
        for bad in [
            "../etc",
            "a; rm -rf ~",
            "shop project",
            "",
            "-leading-dash",
            "a/b",
        ] {
            let sink = crate::progress::Recording::new();
            let error = lifecycle(&sink, "project", bad, events::START)
                .await
                .expect_err(&format!("{bad:?} was accepted as a project name"));

            assert_eq!(error.code, Code::InvalidInput, "for {bad:?}");
            assert!(
                sink.is_empty(),
                "{bad:?} was announced to the UI before it was refused: {:?}",
                sink.names()
            );
        }
    }

    /// A service id has to be in the shipped catalog. A name that merely looks
    /// like one is not, and the difference is what stops an arbitrary string
    /// reaching `docker start`.
    ///
    /// `NotFound` rather than `InvalidInput`, and deliberately so: the name is
    /// well-formed, it just names nothing. The two codes reach the user as
    /// different translated headings, so which one this is counts as behaviour.
    #[tokio::test]
    async fn an_unknown_service_is_refused_before_anything_is_emitted() {
        let sink = crate::progress::Recording::new();
        let error = lifecycle(&sink, "service", "not-a-real-service", events::START)
            .await
            .expect_err("an id outside the catalog must be refused");

        assert_eq!(error.code, Code::NotFound);
        assert!(
            error.hint.is_some(),
            "a refusal the user can act on needs to say what is allowed"
        );
        assert!(sink.is_empty(), "got {:?}", sink.names());
    }
}
