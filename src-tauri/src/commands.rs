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
use crate::connect;
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

/// container name -> (unix seconds, cpu %, memory %). Owned by `stats_store`,
/// which is also what loads it at startup and writes it back after each round.
use crate::stats_store::StatsHistory;

/// Take a lock on app state, recovering from poisoning instead of skipping.
///
/// Poisoning means some other thread panicked while holding this mutex. None of
/// the state behind these locks is left *structurally* invalid by that: they are
/// a cached `Workspace`, a `HashMap` of abort handles and a `HashMap` of sample
/// series, and the operations are `insert`, `remove` and `retain`. So the real
/// choice is between carrying on with the data and refusing to touch it — and
/// refusing was, until now, spelled `if let Ok(mut x) = …lock()`, which does
/// nothing at all and says nothing about it.
///
/// Each of those silences had a specific cost. A `workspace_set` whose cache
/// write is skipped leaves every later command resolving the directory the user
/// just navigated away from. A log stream whose handle is never recorded cannot
/// be aborted, and `container_logs_close` still answers `Ok(())` — the tail runs
/// until the process exits. A skipped `stats_history.retain` never drops dead
/// containers, so the series grow without bound.
///
/// `prefs_set` already chose recovery for its write lock. This is the rest of
/// the file agreeing with it rather than each call site deciding alone.
pub(crate) fn recover<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
    /// The DNS responder, when it is running (E-1).
    ///
    /// A handle rather than a "should it run" setting, because whether the
    /// socket is bound is the only honest answer to "is this working" — a
    /// preference saying yes over a port something else took would be a screen
    /// that lies. The flag stops the thread; the join handle is kept so
    /// stopping actually waits for it rather than leaving a socket bound behind
    /// a `false`.
    pub dns: Mutex<Option<DnsResponder>>,
}

/// A running responder, and the two things needed to stop one.
pub struct DnsResponder {
    pub suffix: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl DnsResponder {
    fn stop(mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sampler: Mutex::new(Sampler::new()),
            workspace: Mutex::new(workspace::resolve()),
            log_streams: Mutex::new(std::collections::HashMap::new()),
            // Read back rather than started empty, so the first detail view
            // after a launch has a sparkline instead of one point. Anything
            // that expired while the app was closed is dropped on the way in —
            // see `stats_store`, where that filter is the whole difficulty.
            stats_history: Mutex::new(
                crate::stats_store::path()
                    .map(|p| crate::stats_store::load_from(&p, crate::stats_store::now()))
                    .unwrap_or_default(),
            ),
            inflight: crate::inflight::Registry::new(),
            generate_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            dns: Mutex::new(None),
        }
    }

    fn root(&self) -> Result<std::path::PathBuf> {
        recover(&self.workspace).require_root()
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
    *recover(&state.workspace) = ws.clone();
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
    *recover(&state.workspace) = ws.clone();
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
    let mut sampler = recover(&state.sampler);
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
                        aliases: Vec::new(),
                        lan_share: false,
                        services: Vec::new(),
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
                        hooks: Default::default(),
                        local: Vec::new(),
                    },
                ));
            }
        }
    }

    let domains: Vec<String> = manifests
        .iter()
        .flat_map(|(_, _, m)| {
            m.domain
                .iter()
                .cloned()
                .chain(
                    m.aliases
                        .iter()
                        .filter(|a| crate::manifest::resolves_through_hosts(a))
                        .cloned(),
                )
                .collect::<Vec<_>>()
        })
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

/// One editable setting of one instance, as its manifest declares it.
///
/// Distinct from [`Credential`], which exists to *display* what a service is
/// reachable with and drops anything empty. An editor needs the opposite —
/// every key the manifest declares, empty ones included, because an empty value
/// is the one most likely to want filling in.
///
/// There is no `.env` key here and that is the whole difference from what this
/// replaced. A setting belongs to an instance, lives in `instances.json` (or the
/// keystore when it is secret), and is read from there. `SERVICE_MYSQL_DATABASE`
/// names a service that two versions of can be running.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSetting {
    /// As the manifest spells it — `ROOT_PASSWORD`, `DATABASE`.
    pub key: String,
    /// The manifest's `type`. Only `string` and `secret` occur today; carried
    /// through rather than collapsed to a boolean so a manifest that adds one
    /// does not need this struct changed.
    pub kind: String,
    /// Masked when `secret`. Revealing one goes through `instance_reveal`.
    pub value: String,
    pub secret: bool,
    /// True when the value is the manifest's own default, so the sheet can say
    /// so rather than presenting a default as somebody's decision.
    pub is_default: bool,
    /// The manifest's own default, so the form can offer to put it back.
    ///
    /// `None` for a secret, and that is deliberate rather than an omission: the
    /// value would cross the boundary unasked and sit in a field the same sheet
    /// takes care to mask. A secret is put back through `instance_reveal`,
    /// which is a request the user makes.
    pub default_value: Option<String>,
    pub required: bool,
    /// The values worth offering, or empty when there is no sensible list.
    ///
    /// Deliberately a property of the row rather than a second command: the
    /// sheet renders what it is given, in order, without knowing what any of it
    /// means. Non-empty does not mean closed — the sheet offers a combobox, so
    /// a value the manifest did not think of stays typeable.
    pub options: Vec<String>,
    /// Locale → human label, straight from the manifest. Absent for a locale
    /// means the front end falls back to its own vocabulary and then to the key.
    pub label: std::collections::BTreeMap<String, String>,
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
    /// The whole domain this is reached at — `phpmyadmin.stackvo.loc` — and not
    /// the subdomain on its own. A caller pastes it into a browser or a hosts
    /// file; neither wants half of it, and a half that has to be completed is a
    /// half two callers will complete differently.
    pub url: Option<String>,
    /// `healthy`, `unhealthy`, `starting`, or `None` when the image declares no
    /// healthcheck.
    ///
    /// Distinct from `running`, and the distinction is the point: twenty-four
    /// packages in the catalogue declare a healthcheck, and until this field
    /// existed a container whose database was refusing every connection was
    /// reported to the user with the same green chip as a healthy one.
    pub health: Option<String>,
    pub host_port: Option<u16>,
    pub ports: Vec<Port>,
    /// Every port the package declares, by the handle it declares it under,
    /// with the host number in force.
    ///
    /// `ports` above is what the *container* publishes, which is nothing at all
    /// until one exists — so a service that had been installed and never
    /// started showed no ports, and a running MinIO showed `9000, 9001` with
    /// nothing saying which of them is the console. Empty on a workspace that
    /// has not migrated: there is no manifest there to declare anything.
    pub declared_ports: Vec<DeclaredPort>,
    /// The names this instance answers to on the Docker network, its own first.
    ///
    /// The second one is the whole reason `primary` exists: every project's
    /// `DB_HOST=stackvo-mysql` reaches whichever instance holds it, and until
    /// now the only place that said so was a chip reading "Primary" on another
    /// page, which does not tell you the name.
    pub aliases: Vec<String>,
    /// What upstream says about this version — `supported`, `deprecated`, `eol`
    /// — and when it ends. `None` before the migration, which has no manifests.
    ///
    /// It was readable only in the catalogue tree, on the page you install
    /// from. The person who hits a bug in an end-of-life database is usually
    /// not the person who installed it.
    pub support: Option<String>,
    pub eol_date: Option<String>,
    /// Containers this instance cannot run without and that are not separately
    /// installable — Kafka's Zookeeper, the only one in the catalogue today.
    ///
    /// They were rendered into the compose file and then invisible: no row, no
    /// status, no way to reach their logs. When Kafka does not come up the
    /// answer is usually in one of these.
    pub companions: Vec<CompanionRow>,
    /// `SERVICE_<ID>_*` values, secrets masked. See `Env::service_credentials`.
    pub credentials: Vec<Credential>,
    pub required: Vec<DependencyRow>,
    pub optional: Vec<DependencyRow>,
    /// The subject of every required dependency that is not answered — nothing
    /// provides it, or something does and is not running. The web UI only knew
    /// about three services' dependencies, so it started admin UIs against
    /// nothing.
    pub unmet_dependencies: Vec<String>,
}

/// One port a package declares, under the name it declares it under.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredPort {
    /// The manifest's handle — `main`, `console`, `smtp`. This is the half that
    /// was being thrown away, and it is the half that says what the port is
    /// for.
    pub name: String,
    pub container: u16,
    /// The host number in force: what the container actually publishes when
    /// there is one, and the allocation recorded in `instances.json` when there
    /// is not. `None` only when nothing has been allocated at all.
    pub host: Option<u16>,
    pub protocol: String,
}

/// A container that comes with an instance rather than being installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionRow {
    /// The manifest's handle, and the suffix of the derived container name.
    pub name: String,
    /// `stackvo-<instance>-<name>`, derived exactly as `render::context`
    /// derives it. A companion named per service rather than per instance is
    /// how two Kafkas came to fight over one Zookeeper.
    pub container_name: String,
    pub image: String,
    pub built: bool,
    pub running: bool,
    pub health: Option<String>,
}

/// One line of "what this service needs", answered or not.
///
/// It used to be a bare instance id, resolved through `provider_instance` and
/// dropped when that returned `None` — so Kibana with no Elasticsearch
/// installed rendered **"No dependencies."**, which is the opposite of the
/// truth and is exactly the state somebody opens this panel in. A dependency
/// nothing answers is the one worth a row; the row is what carries the reason.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyRow {
    /// What the manifest asks for — `sql`, `search`. Stated as a capability
    /// rather than a service so MariaDB can answer a package that says `sql`.
    pub capability: String,
    /// The one service that will do, when only one will (Kafbat genuinely
    /// needs Kafka, not any queue).
    pub service: Option<String>,
    /// The installed instance answering it, or `None` when none does.
    pub provider: Option<String>,
    pub required: bool,
    /// Whether the provider is up. False whenever there is no provider — the
    /// two states are told apart by `provider`, and collapsing them here would
    /// make "not installed" and "installed but stopped" the same sentence.
    pub running: bool,
}

impl DependencyRow {
    /// What to call this dependency when there is no provider to name: the
    /// service the manifest narrowed to, else the capability it asked for.
    fn subject(&self) -> String {
        self.provider
            .clone()
            .or_else(|| self.service.clone())
            .unwrap_or_else(|| self.capability.clone())
    }

    fn unmet(&self) -> bool {
        self.required && !self.running
    }
}

#[tauri::command]
pub async fn services_list(state: State<'_, AppState>) -> Result<Vec<Service>> {
    let root = state.root()?;
    list_services(&root).await
}

/// The Services page's rows, built from the instance table.
///
/// `None` when this workspace has not migrated, which is the caller's signal to
/// use `.env`.
///
/// The row's `id` is the **instance** id — `mysql-8-0`, not `mysql` — because
/// everything downstream keys off it: the detail sheet asks for logs by
/// container, `container_inspect` wants a name, and two versions of one service
/// are two rows that must not collapse into one. `category` comes from the
/// package's own identity, so the page groups the way the Market page does.
fn instance_services(
    root: &std::path::Path,
    env: &Env,
    containers: &std::collections::HashMap<String, ContainerInfo>,
) -> Option<Vec<Service>> {
    if !crate::instances::path(root).exists() {
        return None;
    }
    let table = crate::instances::Table::load(root).ok()?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(root)).ok()?;
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");

    let mut out: Vec<Service> = table
        .instances
        .iter()
        .map(|instance| {
            let manifest = tree.load(&instance.service, &instance.version).ok();
            let container = containers.get(&instance.id);

            // A dependency is stated by capability in a manifest, and answered
            // by whichever instance provides it — which is the whole point of
            // stating it that way: phpMyAdmin is satisfied by MariaDB.
            //
            // Every declared dependency yields a row, answered or not. The
            // unanswered one used to be dropped here, which meant the panel
            // said "no dependencies" in precisely the case where the sentence
            // needed to be "Elasticsearch is required and is not installed".
            let rows: Vec<DependencyRow> = manifest
                .as_ref()
                .map(|m| {
                    m.depends_on
                        .iter()
                        .map(|d| {
                            let provider = provider_instance(&table, &tree, d);
                            DependencyRow {
                                capability: d.capability.clone(),
                                service: d.service.clone(),
                                running: provider.as_ref().is_some_and(|id| {
                                    containers.get(id).is_some_and(|c| c.running)
                                }),
                                provider,
                                required: d.required,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let (required, optional): (Vec<_>, Vec<_>) =
                rows.into_iter().partition(|row| row.required);

            // The host number in force, which is two different facts depending
            // on whether a container exists. A running one publishes what it
            // publishes and that is the truth; a stopped one has only the
            // allocation, and reporting nothing for it is how a service that
            // has never been started came to show no ports at all.
            let declared_ports: Vec<DeclaredPort> = manifest
                .as_ref()
                .map(|m| {
                    m.ports
                        .iter()
                        .map(|port| DeclaredPort {
                            host: container
                                .and_then(|c| {
                                    c.ports
                                        .iter()
                                        .find(|p| p.container == port.container)
                                        .and_then(|p| p.host)
                                })
                                .or_else(|| instance.ports.get(&port.name).copied()),
                            name: port.name.clone(),
                            container: port.container,
                            protocol: port.protocol.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Named the way `render::context` names them, and that is not a
            // coincidence to be maintained by hand: a companion the compose
            // file calls one thing and this panel calls another is a row that
            // reports the wrong container's health.
            let companions: Vec<CompanionRow> = manifest
                .as_ref()
                .map(|m| {
                    m.companions
                        .iter()
                        .map(|companion| {
                            let id = format!("{}-{}", instance.id, companion.name);
                            let container = containers.get(&id);
                            CompanionRow {
                                container_name: format!("stackvo-{id}"),
                                name: companion.name.clone(),
                                image: companion.image.reference(),
                                built: container.is_some(),
                                running: container.is_some_and(|c| c.running),
                                health: container.and_then(|c| c.health.clone()),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            Service {
                declared_ports,
                companions,
                aliases: instance.aliases(),
                support: manifest.as_ref().map(|m| m.support.status.clone()),
                eol_date: manifest.as_ref().and_then(|m| m.support.eol_date.clone()),
                container_name: instance.container(),
                enabled: instance.enabled,
                running: container.is_some_and(|c| c.running),
                built: container.is_some(),
                health: container.and_then(|c| c.health.clone()),
                version: Some(instance.version.clone()),
                url: manifest
                    .as_ref()
                    .and_then(|m| m.url.as_ref())
                    .map(|u| instance.domain(&u.subdomain, tld)),
                // The published number, which is the one somebody pastes into a
                // client. The manifest's `primary` port names which that is.
                host_port: manifest
                    .as_ref()
                    .and_then(|m| m.ports.iter().find(|p| p.primary).or(m.ports.first()))
                    .and_then(|p| instance.ports.get(&p.name).copied()),
                // Settings, never secrets: the value of a `secret` setting lives
                // in the keystore and `instances.json` holds a reference (ADR
                // 0010). Reporting the reference as a credential would put a
                // keystore path on a screen that masks passwords.
                credentials: manifest
                    .as_ref()
                    .map(|m| {
                        m.settings
                            .iter()
                            .map(|setting| {
                                let secret = setting.kind == "secret";
                                Credential {
                                    env_key: format!(
                                        "{}{}",
                                        Env::service_prefix(&instance.service),
                                        setting.key
                                    ),
                                    key: setting.key.clone(),
                                    value: if secret {
                                        crate::config::MASK.to_string()
                                    } else {
                                        instance
                                            .settings
                                            .get(&setting.key)
                                            .cloned()
                                            .or_else(|| setting.default_text())
                                            .unwrap_or_default()
                                    },
                                    secret,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ports: container.map(|c| c.ports.clone()).unwrap_or_default(),
                unmet_dependencies: required
                    .iter()
                    .filter(|row| row.unmet())
                    .map(DependencyRow::subject)
                    .collect(),
                required,
                optional,
                category: tree
                    .identity(&instance.service)
                    .map(|i| i.category.clone())
                    .unwrap_or_else(|| "services".into()),
                id: instance.id.clone(),
            }
        })
        .collect();

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Some(out)
}

/// Which installed instance answers a declared capability.
fn provider_instance(
    table: &crate::instances::Table,
    tree: &crate::pkg::Tree,
    dependency: &crate::pkg::Dependency,
) -> Option<String> {
    table
        .instances
        .iter()
        .find(|other| {
            if let Some(named) = &dependency.service {
                if &other.service != named {
                    return false;
                }
            }
            tree.load(&other.service, &other.version)
                .map(|m| m.capabilities.iter().any(|c| c == &dependency.capability))
                .unwrap_or(false)
        })
        .map(|other| other.id.clone())
}

pub async fn list_services(root: &std::path::Path) -> Result<Vec<Service>> {
    let env = Env::load(root)?;
    let containers = engine::stackvo_containers().await.unwrap_or_default();

    // A migrated workspace's services are instances, and this function did not
    // know that. It walked the compiled-in catalogue and built every container
    // name as `stackvo-<id>` — so after a handover the Services page listed
    // twenty-five services, reported all of them stopped (the containers are
    // `stackvo-mysql-8-0` now), and the detail sheet with its connection
    // string, its dumps and its **logs** was reachable for none of them.
    //
    // Same rule as `service_source` and `service_domains`: the table when there
    // is one, `.env` when there is not. Not a union — a page built from both
    // would list a service twice under two names, and one of them would be a
    // container that has not existed since the migration.
    if let Some(services) = instance_services(root, &env, &containers) {
        return Ok(services);
    }

    let schema = env_schema();
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");

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
            // Before packages a dependency was a service id and nothing else —
            // there are no capabilities in `env.schema.json`. The row is filled
            // out with the id in all three places rather than left half empty,
            // because the panel reading it must not need to know which of the
            // two models produced the row.
            let row = |service: &String, required: bool| DependencyRow {
                capability: service.clone(),
                service: Some(service.clone()),
                provider: Some(service.clone()),
                running: is_running(service),
                required,
            };
            let required: Vec<DependencyRow> = deps.required.iter().map(|d| row(d, true)).collect();
            let optional: Vec<DependencyRow> =
                deps.optional.iter().map(|d| row(d, false)).collect();
            let unmet: Vec<String> = required
                .iter()
                .filter(|row| row.unmet())
                .map(DependencyRow::subject)
                .collect();
            let container = containers.get(&id);

            Service {
                container_name: format!("{}{}", engine::CONTAINER_PREFIX, id),
                enabled: env.service_enabled(&id),
                running: container.is_some_and(|c| c.running),
                built: container.is_some(),
                health: container.and_then(|c| c.health.clone()),
                version: env.service_version(&id).map(str::to_string),
                // The whole domain, as the instance branch above already
                // returns. This used to be the subdomain alone, and the two
                // callers made up the difference by appending the suffix
                // themselves — which is why a migrated phpMyAdmin rendered as
                // `phpmyadmin.stackvo.loc.stackvo.loc` and its Open button led
                // nowhere. One field cannot mean two things.
                url: env.service_url(&id).map(|url| format!("{url}.{tld}")),
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
                // Nothing to declare them: before the migration there is no
                // manifest, the container name is the only network name, and
                // the compiled-in catalogue never carried a support status.
                declared_ports: Vec::new(),
                companions: Vec::new(),
                aliases: vec![format!("{}{}", engine::CONTAINER_PREFIX, id)],
                support: None,
                eol_date: None,
                required,
                optional,
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
    /// False for a runtime `.env` advertises with no generator behind it
    /// (CONFLICTS.md C-02). The UI greys those out instead of offering a
    /// choice that silently produces nothing.
    ///
    /// True is not the same as "`.env` lists it": `build_catalog` also appends
    /// the runtimes this binary can generate for that the workspace's `.env`
    /// predates, which is how Bun and Deno reach a picker on a machine set up
    /// before they existed here.
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
const IMPLEMENTED_RUNTIMES: [&str; 8] =
    ["php", "node", "python", "go", "ruby", "rust", "bun", "deno"];

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

    // Runtimes this build can generate for that `.env` has never heard of.
    //
    // `SUPPORTED_LANGUAGES` is a workspace setting written when the workspace
    // was created, so a machine set up before Bun and Deno existed here lists
    // neither — and keying the picker off it alone would mean the feature
    // arrives only for people who edit `.env` by hand, or not at all. What the
    // app can build is a fact about this binary, not about a file on disk.
    //
    // No versions and no default from `.env`, because neither has a key there:
    // the answer comes from `manifest::lang_defaults`, which is where the rest
    // of the app already reads it. Deno's is a full patch version and has to
    // be — see that function.
    let mut runtimes: Vec<RuntimeOption> = runtimes;
    for id in IMPLEMENTED_RUNTIMES {
        if runtimes.iter().any(|r| r.id == id) {
            continue;
        }
        runtimes.push(RuntimeOption {
            id: id.to_string(),
            versions: Vec::new(),
            default: crate::manifest::lang_defaults(id).map(|l| l.version),
            available: true,
        });
    }

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
    if kind == "project" || kind == "instance" {
        // An instance id is not in the service catalog and never will be — it
        // is `mysql-8-0`, derived from a pair. The shape check is the same one
        // a project name gets, and the id itself has already been looked up in
        // the instance table by the caller.
        if !workspace::is_safe_name(id) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{id}\" is not a valid {kind} name"),
            ));
        }
    } else {
        checked_service(id)?;
    }

    let subject = |ev: &str| format!("{kind}:{ev}");
    // An instance rides in the `service` field rather than getting one of its
    // own: the front end keys these on the id string, and a fourth shape of
    // event would be a fourth thing every listener has to know about for no
    // information it does not already have.
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
    let root = state.root()?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    lifecycle(&events::sink(&app), "project", &name, events::START).await?;
    run_hooks(&app, &root, &name, crate::hooks::Event::PostStart).await;
    Ok(())
}

#[tauri::command]
pub async fn project_stop(app: AppHandle, state: State<'_, AppState>, name: String) -> Result<()> {
    let root = state.root()?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    // Before the stop, which is what `pre-stop` means and is also the only
    // moment it can work: a container that has gone down has nothing to exec
    // into, so a step that ran afterwards would fail every time.
    run_hooks(&app, &root, &name, crate::hooks::Event::PreStop).await;
    lifecycle(&events::sink(&app), "project", &name, events::STOP).await
}

#[tauri::command]
pub async fn project_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let root = state.root()?;
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    // Both ends, in the order they happen. A restart is a stop and a start, and
    // a project whose hooks fired on neither would be one where "restart" and
    // "stop then start" quietly did different things.
    run_hooks(&app, &root, &name, crate::hooks::Event::PreStop).await;
    lifecycle(&events::sink(&app), "project", &name, events::RESTART).await?;
    run_hooks(&app, &root, &name, crate::hooks::Event::PostStart).await;
    Ok(())
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

    recover(&state.log_streams).insert(stream_id.clone(), handle);

    Ok(stream_id)
}

#[tauri::command]
pub fn container_logs_close(state: State<'_, AppState>, stream_id: String) -> Result<()> {
    if let Some(handle) = recover(&state.log_streams).remove(&stream_id) {
        handle.abort();
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

    recover(&state.log_streams).insert(stream_id.clone(), handle);

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

    recover(&state.log_streams).insert(stream_id.clone(), handle);

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
    let outcome = env_writer::apply(&root, &patch);

    // The keys, never the values. `.env` is where the passwords are, and a
    // trail that carries them is one nobody can hand to anybody — the same rule
    // `logging.rs` states about payloads.
    crate::audit::record(
        "env_set",
        patch.keys().cloned().collect::<Vec<_>>().join(", "),
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );
    outcome
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

// ---------------------------------------------------------- lifecycle hooks

/// Run a project's hooks for one lifecycle event (B-3).
///
/// Everything about *whether* a step runs is `hooks::plan`'s; this reads the
/// three inputs it needs and reports what happened.
///
/// **A hook failure does not fail the operation it hangs off.** A container
/// that started is started, and reporting the start as failed because a
/// convenience afterwards did not work would leave nobody able to tell which
/// half broke. The failure is emitted on the hook's own events and logged here.
async fn run_hooks(
    app: &AppHandle,
    root: &std::path::Path,
    name: &str,
    event: crate::hooks::Event,
) {
    let dir = match workspace::project_dir(root, name) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    // The effective manifest, deliberately: `stackvo.local.json` may override
    // hooks the same way it overrides anything else, and a machine that wants
    // a different post-start step is exactly the case B-2 exists for.
    let Ok(manifest) = manifest::read(&dir.join("stackvo.json"), name) else {
        return;
    };
    if manifest.hooks.is_empty() {
        return;
    }

    let consent = crate::hooks::consent_path()
        .map(|path| crate::hooks::read_consent(&path))
        .unwrap_or_default();

    let operation_id = events::next_operation_id("hook");
    if let Err(e) = crate::hooks::run(
        &events::sink(app),
        crate::hooks::Run {
            operation_id: &operation_id,
            project: name,
            dir: &dir,
            container: &format!("stackvo-{}", name.to_ascii_lowercase()),
            hooks: &manifest.hooks,
            event,
            policy: crate::policy::current().hooks(),
            consent: &consent,
        },
    )
    .await
    {
        tracing::warn!(project = name, event = event.key(), error = %e.message, "a hook failed");
    }
}

/// What would run for a project's hooks, and what would not.
#[tauri::command]
pub fn project_hooks_plan(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<crate::hooks::Plan>> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    let manifest = manifest::read(&dir.join("stackvo.json"), &name)?;
    let consent = crate::hooks::consent_path()
        .map(|path| crate::hooks::read_consent(&path))
        .unwrap_or_default();
    let policy = crate::policy::current().hooks();

    Ok(crate::hooks::Event::ALL
        .iter()
        .map(|event| crate::hooks::plan(&name, &manifest.hooks, *event, policy, &consent))
        .collect())
}

/// Agree to this project's host commands, exactly as they are now.
///
/// The digest is sent back by the caller rather than recomputed here, and that
/// is the point of the round trip: it is a receipt for the list the person
/// actually read. If the manifest changed between the screen being drawn and
/// the button being pressed, the digest no longer matches and the grant is
/// refused — which is the whole property that makes this consent rather than a
/// checkbox.
#[tauri::command]
pub fn project_hooks_approve(
    state: State<'_, AppState>,
    name: String,
    digest: String,
) -> Result<Vec<crate::hooks::Plan>> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    let manifest = manifest::read(&dir.join("stackvo.json"), &name)?;

    let current = manifest.hooks.host_digest().ok_or_else(|| {
        Error::new(
            Code::InvalidInput,
            format!("{name} has no host commands to approve"),
        )
    })?;
    if current != digest {
        return Err(Error::new(
            Code::Conflict,
            "the commands changed since they were shown; read them again before approving"
                .to_string(),
        ));
    }

    let path = crate::hooks::consent_path().ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            "no application directory on this platform",
        )
    })?;
    let mut consent = crate::hooks::read_consent(&path);
    consent.grant(&name, &current);
    crate::hooks::write_consent(&path, &consent)?;

    project_hooks_plan(state, name)
}

/// Withdraw approval. Takes no digest — you may always revoke.
#[tauri::command]
pub fn project_hooks_revoke(
    state: State<'_, AppState>,
    name: String,
) -> Result<Vec<crate::hooks::Plan>> {
    let path = crate::hooks::consent_path().ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            "no application directory on this platform",
        )
    })?;
    let mut consent = crate::hooks::read_consent(&path);
    consent.revoke(&name);
    crate::hooks::write_consent(&path, &consent)?;

    project_hooks_plan(state, name)
}

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

    // After the container exists, not after the image: `post-build` steps are
    // things like `composer install`, and a step that runs inside a container
    // needs one to be there. Only on success — a build that failed leaves
    // whatever the previous image was, and running the new manifest's hooks
    // against it is a state nobody described.
    if outcome.is_ok() {
        run_hooks(&app, &root, &name, crate::hooks::Event::PostBuild).await;
    }

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

// ------------------------------------------------ idle projects (I-2)

/// How long since each running project was last asked for anything.
#[tauri::command]
pub async fn projects_idle(state: State<'_, AppState>) -> Result<Vec<crate::idle::Idle>> {
    let root = state.root()?;
    let threshold = crate::idle::threshold_seconds(&root)?;

    let projects: Vec<(String, bool)> = list_projects(&root)
        .await?
        .into_iter()
        .map(|p| (p.name, p.running))
        .collect();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Ok(crate::idle::assess(&root, &projects, threshold, now))
}

/// Stop every project past the threshold, and say which.
///
/// Returns what it stopped rather than a count. "3 projects suspended" is a
/// number somebody then has to go and match against a list; the names are the
/// thing they actually want, and this is a background action whose whole risk
/// is being surprising.
#[tauri::command]
pub async fn projects_suspend_idle(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let idle = projects_idle(state.clone()).await?;
    let mut stopped = Vec::new();

    for entry in idle.iter().filter(|entry| entry.suspendable) {
        // Skipped rather than queued when something else holds the project: a
        // sweep that waited behind a build would stop a container the moment
        // the build finished, which is the opposite of idle.
        let Ok(_busy) = state.inflight.acquire(format!("project:{}", entry.project)) else {
            continue;
        };
        if lifecycle(&events::sink(&app), "project", &entry.project, events::STOP)
            .await
            .is_ok()
        {
            stopped.push(entry.project.clone());
        }
    }

    Ok(stopped)
}

// ------------------------------------------- moving a database (G-4)

/// Every database instance, which is what a move names.
#[tauri::command]
pub async fn db_instances(state: State<'_, AppState>) -> Result<Vec<crate::db::DbInstance>> {
    let root = state.root()?;
    crate::db::instances(&root).await
}

/// What moving one instance's contents into another would do, and whether it
/// may happen at all.
#[tauri::command]
pub fn db_move_plan(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<crate::dbmove::Plan> {
    let root = state.root()?;
    crate::dbmove::plan(&root, &from, &to)
}

/// Dump one instance and restore it into another, reporting as an operation.
///
/// Both ends are held busy for the whole thing. A move reads one database and
/// replaces another, and either of them being started, stopped or dumped
/// underneath it is a torn result nobody would be able to explain afterwards.
#[tauri::command]
pub async fn db_move_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<crate::dbmove::Moved> {
    let root = state.root()?;
    let _source = state.inflight.acquire(format!("instance:{from}"))?;
    let _target = state.inflight.acquire(format!("instance:{to}"))?;

    let operation_id = events::next_operation_id("dbmove");
    let sink = events::sink(&app);
    let subject = format!("{from} → {to}");

    events::emit(
        &app,
        "db:progress",
        serde_json::json!({ "operationId": operation_id, "subject": subject, "line": "planning" }),
    );

    let id = operation_id.clone();
    let who = subject.clone();
    let outcome = crate::dbmove::run(&root, &from, &to, move |line| {
        crate::progress::emit(
            &sink,
            "db:progress",
            crate::events::ProgressEvent {
                operation_id: id.clone(),
                subject: who.clone(),
                line,
            },
        );
    })
    .await;

    events::emit(
        &app,
        "db:done",
        serde_json::json!({
            "operationId": operation_id,
            "subject": subject,
            "success": outcome.is_ok(),
            "error": outcome.as_ref().err().map(|e| e.message.clone()),
        }),
    );
    outcome
}

// --------------------------------------------------- user routes (E-4)

/// The saved routes, each with what it would actually do.
///
/// Checked on the way out rather than stored checked: `host.docker.internal` is
/// the app's answer to `localhost`, and freezing it into the file would mean a
/// route written today keeping today's answer after the rule changed. The file
/// holds what the user typed.
#[tauri::command]
pub fn routes_list(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>> {
    let root = state.root()?;
    let env = crate::config::Env::load(&root)?;
    let suffix = env
        .get("DEFAULT_TLD_SUFFIX")
        .unwrap_or("stackvo.loc")
        .to_string();

    Ok(crate::routes::read(&root)
        .into_iter()
        .map(|route| match route.normalise(&suffix) {
            Ok(checked) => serde_json::to_value(checked).unwrap_or_default(),
            // Reported, not dropped: a route the renderer skips must be visible
            // here, or the screen shows a route and the proxy has none.
            Err(e) => serde_json::json!({
                "domain": route.domain,
                "target": route.target,
                "enabled": route.enabled,
                "notes": [],
                "error": e.message,
            }),
        })
        .collect())
}

/// Replace the whole list, then regenerate so Traefik picks it up.
///
/// The whole list rather than add/remove/edit: the file is a handful of pairs
/// the user is looking at in a table, and three commands over one small
/// document is three chances for the file and the screen to disagree about
/// order. Every route is checked before anything is written, so one bad row
/// fails the save instead of writing half of it.
#[tauri::command]
pub async fn routes_save(
    app: AppHandle,
    state: State<'_, AppState>,
    routes: Vec<crate::routes::Route>,
) -> Result<Vec<serde_json::Value>> {
    let root = state.root()?;
    let env = crate::config::Env::load(&root)?;
    let suffix = env
        .get("DEFAULT_TLD_SUFFIX")
        .unwrap_or("stackvo.loc")
        .to_string();

    for route in &routes {
        route.normalise(&suffix)?;
    }

    // Two routes on one name is a router Traefik loads twice and resolves by
    // whichever it read last — a coin toss the user cannot see.
    let mut seen = std::collections::BTreeSet::new();
    for route in &routes {
        let domain = route.domain.trim().to_ascii_lowercase();
        if !seen.insert(domain.clone()) {
            return Err(Error::new(
                Code::Conflict,
                format!("{domain} is listed twice; one name routes to one place"),
            ));
        }
    }

    crate::routes::write(&root, &routes)?;

    let operation_id = events::next_operation_id("routes");
    generate(&app, &root, &operation_id, "services").await?;

    routes_list(state)
}

// ------------------------------------------------------------- DNS (E-1)

/// The suffix this workspace's names end in.
fn dns_suffix(state: &State<'_, AppState>) -> String {
    state
        .root()
        .ok()
        .map(|root| crate::certs::suffix(&root))
        .unwrap_or_else(|| "stackvo.loc".to_string())
}

/// Whether the responder is answering, and what the machine still needs.
#[tauri::command]
pub fn dns_status(state: State<'_, AppState>) -> Result<crate::dns::Status> {
    let suffix = dns_suffix(&state);
    let listening = recover(&state.dns)
        .as_ref()
        .is_some_and(|responder| responder.suffix == suffix);
    Ok(crate::dns::status(&suffix, listening))
}

/// Start answering, or say why the socket could not be had.
///
/// Idempotent for the same suffix and a restart for a different one: a
/// workspace whose TLD changed must not leave a responder serving the old one,
/// which would answer for names nothing renders any more and refuse the ones it
/// does.
#[tauri::command]
pub fn dns_start(state: State<'_, AppState>) -> Result<crate::dns::Status> {
    let suffix = dns_suffix(&state);
    {
        let mut slot = recover(&state.dns);
        if let Some(running) = slot.take() {
            if running.suffix == suffix {
                *slot = Some(running);
                return Ok(crate::dns::status(&suffix, true));
            }
            running.stop();
        }
    }

    let socket = crate::dns::bind()?;
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker = {
        let stop = std::sync::Arc::clone(&stop);
        let suffix = suffix.clone();
        std::thread::Builder::new()
            .name("stackvo-dns".into())
            .spawn(move || crate::dns::serve(socket, suffix, stop))
            .map_err(|e| Error::io("starting the DNS responder", e))?
    };

    *recover(&state.dns) = Some(DnsResponder {
        suffix: suffix.clone(),
        stop,
        worker: Some(worker),
    });
    Ok(crate::dns::status(&suffix, true))
}

#[tauri::command]
pub fn dns_stop(state: State<'_, AppState>) -> Result<crate::dns::Status> {
    let suffix = dns_suffix(&state);
    if let Some(running) = recover(&state.dns).take() {
        running.stop();
    }
    Ok(crate::dns::status(&suffix, false))
}

/// Write `/etc/resolver/<tld>`, with a password.
///
/// Separate from `dns_start` on purpose, and it is the same separation
/// `hosts_plan`/`hosts_apply` has: one of these is a socket this app owns and
/// the other changes how the whole machine resolves names. Folding them into
/// one button would mean a password prompt appearing from something that reads
/// like "turn on a feature".
#[tauri::command]
pub fn dns_resolver_install(state: State<'_, AppState>) -> Result<crate::dns::Status> {
    let suffix = dns_suffix(&state);
    crate::dns::install_resolver(&suffix)?;
    dns_status(state)
}

#[tauri::command]
pub fn dns_resolver_remove(state: State<'_, AppState>) -> Result<crate::dns::Status> {
    let suffix = dns_suffix(&state);
    crate::dns::remove_resolver(&suffix)?;
    dns_status(state)
}

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

    // Audited on both paths. A refused elevation prompt is the outcome most
    // worth having a record of: nothing changed, and the fact that somebody
    // tried is the whole content of the entry.
    let plan = hosts::apply(&add, &remove).inspect_err(|e| {
        crate::audit::record_with(
            "hosts_apply",
            add.join(", "),
            crate::audit::Outcome::Failed,
            Some(e.message.clone()),
        )
    })?;
    crate::audit::record_with(
        "hosts_apply",
        plan.add.join(", "),
        crate::audit::Outcome::Ok,
        Some(format!("removed: {}", plan.remove.join(", "))),
    );
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

    // A migrated workspace's services are in the table, not in `.env`, and this
    // function only ever read `.env`. So on such a machine phpMyAdmin got a
    // Traefik router and a certificate SAN and **no hosts line** — the browser
    // never resolved the name, and everything downstream looked correct.
    //
    // Same rule as `service_source`: the table when there is one, `.env` when
    // there is not. No union of the two — a name that came from state the user
    // has already replaced is the drift the table exists to end.
    match instance_domains(root, &env, &running, &managed) {
        Some(domains) => domains,
        None => service_domains_from(&env, &running, &managed),
    }
}

/// The domains a migrated workspace answers on, or `None` when it has not
/// migrated.
///
/// The subdomain comes from the package manifest and the *name* from
/// [`crate::instances::Instance::domain`], so a second instance of phpMyAdmin is
/// `phpmyadmin-5-2` rather than a second claim on `phpmyadmin` — the thing that
/// kept twelve packages single-instance.
fn instance_domains(
    root: &std::path::Path,
    env: &Env,
    running: &std::collections::HashSet<String>,
    managed: &std::collections::HashSet<String>,
) -> Option<Vec<String>> {
    if !crate::instances::path(root).exists() {
        return None;
    }
    let table = crate::instances::Table::load(root).ok()?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(root)).ok()?;
    let tld = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");

    Some(
        table
            .instances
            .iter()
            .filter(|instance| instance.enabled)
            .filter_map(|instance| {
                let manifest = tree.load(&instance.service, &instance.version).ok()?;
                let url = manifest.url.as_ref()?;
                Some((instance.id.clone(), instance.domain(&url.subdomain, tld)))
            })
            // The same two reasons a name is wanted as on the `.env` path: the
            // container is up, or the hosts file already carries the line and
            // taking it away would break a tab somebody has open.
            .filter(|(id, domain)| {
                running.contains(id) || managed.contains(&domain.to_ascii_lowercase())
            })
            .map(|(_, domain)| domain)
            .collect(),
    )
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
        .flat_map(|p| {
            // The main domain and every alias a hosts file can express. A
            // wildcard is deliberately not here and is not an omission: no
            // hosts file resolves one, so listing it would put a line in the
            // "missing" report that no button could ever fix. It is reported
            // as unresolvable in its own right — see `project_hostnames`.
            p.domain
                .into_iter()
                .chain(
                    p.manifest
                        .aliases
                        .iter()
                        .filter(|a| crate::manifest::resolves_through_hosts(a))
                        .cloned(),
                )
                .collect::<Vec<_>>()
        })
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

/// The string a client is pasted into, for one service.
///
/// `null` for a service nobody connects to with one — an admin UI is opened at
/// its domain, which the sheet already shows. Two addresses come back rather
/// than one because a service has two and they are not interchangeable: see
/// [`crate::connect`], which exists because the obvious guess (`stackvo-mongo`
/// in Compass, on the host) is the one that cannot work.
///
/// `reveal` is the same act `env_reveal` is: the password is bullets until
/// somebody asks for it, on a click, for one service.
#[tauri::command]
pub async fn service_connection(
    state: State<'_, AppState>,
    service: String,
    reveal: bool,
) -> Result<Option<connect::Connection>> {
    let root = state.root()?;
    connect::of(&root, &service, reveal).await
}

/// Which desktop clients could open this service, on this machine.
///
/// Empty for most rows and that is the answer, not a failure: a service with no
/// connection string has nothing to open, and AMQP or SMTP has a string that no
/// desktop database client takes. The caller shows the button only when this
/// comes back with something in it.
#[tauri::command]
pub async fn service_db_clients(
    state: State<'_, AppState>,
    service: String,
) -> Result<Vec<crate::apps::App>> {
    let root = state.root()?;
    let Some(connection) = connect::of(&root, &service, false).await? else {
        return Ok(Vec::new());
    };
    Ok(crate::apps::db_clients(connect::scheme_of(connection.kind)))
}

/// Hand this service's address to a database client.
///
/// The last of G-3, and the smaller half by a distance: the correct string has
/// been available since `connect.rs` was written and the sheet has offered to
/// copy it since. What was missing was the step that pastes it, which is what
/// everybody was doing by hand.
///
/// ## Which of the two addresses
///
/// The host one, always. The container address is a name on a Docker network
/// and a client on this desktop has never heard of it — that confusion is the
/// reason `connect` returns two endpoints in the first place, and choosing the
/// wrong one here would reintroduce it at the one call site that cannot be
/// corrected by reading. A running container that publishes nothing has no host
/// address at all, and this says so rather than inventing `127.0.0.1`.
///
/// ## The password is in the string
///
/// It has to be — a URI with bullets in it is a URI that fails to connect, and
/// the point of the button is that nothing is retyped. So this is `reveal` in
/// the sense of `env_reveal` and `service_connection`: an act, on a click, for
/// one service, and it is recorded like one.
///
/// What it costs is that the secret is an argument to `open`, so it is visible
/// in `ps` for as long as that process lives. That is a real exposure and it is
/// written down rather than left for someone to find: the alternative is a
/// temporary file holding the same secret with the same reachability and a
/// lifetime nobody controls, which is worse. On a single-user machine holding
/// development credentials it is the right trade; on a shared one it is a
/// reason to copy the string instead, and the copy button is still there.
#[tauri::command]
pub async fn service_open_in_client(
    state: State<'_, AppState>,
    service: String,
    client: String,
) -> Result<()> {
    let root = state.root()?;

    let Some(connection) = connect::of(&root, &service, true).await? else {
        return Err(Error::new(
            Code::NotFound,
            format!("{service} has no connection string to open"),
        ));
    };
    let Some(endpoint) = connection.from_host else {
        return Err(Error::new(
            Code::NotFound,
            format!("{service} is not publishing a port to this machine"),
        )
        .with_hint(crate::hints::SERVICE_PUBLISHES_NOTHING));
    };

    // A client is offered only when it declares the scheme, so a request naming
    // one that does not is either a stale UI or a caller inventing values. Both
    // are refused here rather than launched and left to fail silently, which is
    // the failure mode that made Redis Insight worth writing a test about.
    let scheme = connect::scheme_of(connection.kind);
    if !client.is_empty()
        && !crate::apps::db_clients(scheme)
            .iter()
            .any(|a| a.id == client && a.available)
    {
        return Err(Error::new(
            Code::NotFound,
            format!("{client} is not installed here, or does not open {scheme} addresses"),
        )
        .with_hint(crate::hints::CHOOSE_A_DB_CLIENT));
    }

    // Deliberately not audited, and the first draft of this had it the other
    // way. `audit.rs` states its bar — acts that change something outside this
    // app and cannot be undone — and says in the same breath that reading
    // anything is not one, nor is starting a process. This is both of those:
    // the same disclosure `env_reveal` makes, followed by a launch. Auditing it
    // while `env_reveal` and `instance_reveal` are not audited would leave a
    // trail where a password shown on screen is invisible and the same password
    // handed to TablePlus is a line, which is a worse record than either choice.
    // If reveals ever become auditable, this belongs with them and not before.

    if let Some(launch) = crate::apps::resolve_db_client(&client) {
        let spawned = match launch {
            crate::apps::Launch::Command(cmd) => std::process::Command::new(cmd)
                .arg(&endpoint.uri)
                .spawn()
                .is_ok(),
            crate::apps::Launch::Bundle(bundle) => std::process::Command::new("open")
                .args(["-a", bundle])
                .arg(&endpoint.uri)
                .spawn()
                .is_ok(),
        };
        if spawned {
            return Ok(());
        }
        // Fall through to the system handler rather than leaving the click with
        // nothing to show for it, exactly as `open_in_browser` does.
    }

    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(&endpoint.uri)
            .spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(&endpoint.uri)
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(&endpoint.uri)
            .spawn()
    };

    opened.map(|_| ()).map_err(|e| {
        Error::new(
            Code::NotFound,
            format!("nothing on this machine opens {scheme} addresses: {e}"),
        )
        .with_hint(crate::hints::CHOOSE_A_DB_CLIENT)
    })
}

// ------------------------------------------------------- query log (F-1)

/// What the database was asked, and what it was asked repeatedly.
///
/// F-1. The row in §2 said this needed a collector inside the container, and
/// for MySQL and MariaDB it does not: their general query log can be pointed at
/// a table and switched on with two statements, at runtime, on a stock image.
/// See [`crate::querylog`] for what that buys and what it costs.
///
/// A kind with no such log answers `supported: false` rather than an error —
/// the screen asks whichever database the project uses and has to be able to
/// say "not this one" without looking broken.
#[tauri::command]
pub async fn query_log(
    state: State<'_, AppState>,
    service: String,
) -> Result<crate::querylog::Session> {
    let root = state.root()?;
    crate::querylog::read(&root, &service).await
}

/// Start or stop recording.
///
/// Deliberately one command with a boolean rather than two: the two are a pair,
/// and the interesting failure — turning it on and forgetting — is one a single
/// switch makes visible. Stopping also truncates the log, because what it holds
/// is statement text and in a development database that is the data.
#[tauri::command]
pub async fn query_log_record(
    state: State<'_, AppState>,
    service: String,
    recording: bool,
) -> Result<crate::querylog::Session> {
    let root = state.root()?;
    if recording {
        crate::querylog::enable(&root, &service).await?;
    } else {
        crate::querylog::disable(&root, &service).await?;
    }
    crate::querylog::read(&root, &service).await
}

/// Throw away what has been collected, without stopping.
///
/// The "start again from here" somebody reaches for before reloading the page
/// they are investigating — without it every read is the whole session and the
/// one request under study is buried in it.
#[tauri::command]
pub async fn query_log_clear(
    state: State<'_, AppState>,
    service: String,
) -> Result<crate::querylog::Session> {
    let root = state.root()?;
    crate::querylog::clear(&root, &service).await?;
    crate::querylog::read(&root, &service).await
}

/// One request, from the code's side and the database's side, on one axis.
///
/// F-2. The row said "dump/mail/log three separate screens, no correlation",
/// and the two halves that matter — what the code thought it had, and what it
/// actually asked the database for — were readable only by comparing clocks by
/// eye across two panes.
///
/// `service` is optional: without one this is the dumps alone, which is still
/// the timeline it always could have been. See [`crate::timeline`] for what
/// correlates (dumps, by the request they name) and what only sorts (queries,
/// because nothing in a general log says which request caused a statement).
#[tauri::command]
pub async fn request_timeline(
    state: State<'_, AppState>,
    project: String,
    service: Option<String>,
) -> Result<crate::timeline::Timeline> {
    let root = state.root()?;
    let dumps = crate::debugbridge::read_events(&root, &project);

    let (queries, recording) = match service {
        Some(service) => match crate::querylog::read(&root, &service).await {
            Ok(session) => (session.entries, session.recording),
            // A database that cannot be reached is not a reason to withhold the
            // dumps: half a timeline is worth more than an error page, and the
            // screen says the query half is absent.
            Err(_) => (Vec::new(), false),
        },
        None => (Vec::new(), false),
    };

    // Best effort, and deliberately not a reason to fail: a catcher that is not
    // running is a common state and half a timeline is worth more than an error
    // page. The same rule the query half follows two lines up.
    let mail = crate::mail::messages(&root, 100).await.unwrap_or_default();

    Ok(crate::timeline::build(&dumps, &queries, &mail, recording))
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

// ----------------------------------------------------- snapshots (G-1, G-2)

/// Every snapshot in the workspace, newest first.
#[tauri::command]
pub fn db_snapshots(state: State<'_, AppState>) -> Result<Vec<crate::snapshot::Snapshot>> {
    Ok(crate::snapshot::list(&state.root()?))
}

/// Take one, under a name somebody chose.
///
/// The same dump `db_dump` performs, into a path this app owns rather than one
/// picked in a save dialog — which is the whole difference between raw material
/// and a feature you come back to.
#[tauri::command]
pub async fn db_snapshot_take(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    name: String,
) -> Result<String> {
    let root = state.root()?;
    let name = crate::snapshot::safe_name(&name)?;
    let path = crate::snapshot::path_for(&root, &service, &name)?;

    if path.exists() {
        return Err(Error::new(
            Code::AlreadyExists,
            format!("a {service} snapshot called `{name}` already exists"),
        )
        .with_hint(crate::hints::SNAPSHOT_NAME_IN_USE));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }

    db_operation(app, state, service, path.display().to_string(), "dump").await
}

/// Put one back, replacing what is in the database.
#[tauri::command]
pub async fn db_snapshot_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    name: String,
) -> Result<String> {
    let root = state.root()?;
    let path = checked_snapshot(&root, &service, &name)?;

    if !path.is_file() {
        return Err(Error::not_found(format!("snapshot {name}")));
    }
    db_operation(app, state, service, path.display().to_string(), "restore").await
}

/// Delete one. Deliberately not audited: this removes a copy, and the thing
/// that would have to be accounted for is the restore, which already is.
#[tauri::command]
pub fn db_snapshot_delete(state: State<'_, AppState>, service: String, name: String) -> Result<()> {
    crate::snapshot::remove(&state.root()?, &service, &name)
}

/// The one place a snapshot name becomes a path outside `snapshot.rs`.
fn checked_snapshot(
    root: &std::path::Path,
    service: &str,
    name: &str,
) -> Result<std::path::PathBuf> {
    // A scheduled snapshot carries the reserved prefix, so it cannot go through
    // `safe_name` — which refuses that prefix precisely so nobody can create
    // one. Both spellings are checked for the characters that matter.
    let checked = if name.starts_with(crate::snapshot::AUTO_PREFIX) {
        name.strip_prefix(crate::snapshot::AUTO_PREFIX)
            .filter(|rest| {
                !rest.is_empty()
                    && !rest.starts_with('.')
                    && rest
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
            .map(|_| name.to_string())
            .ok_or_else(|| {
                Error::new(
                    Code::InvalidInput,
                    "not a scheduled snapshot name".to_string(),
                )
                .with_hint(crate::hints::SNAPSHOT_NAME_CHARSET)
            })?
    } else {
        crate::snapshot::safe_name(name)?
    };

    crate::snapshot::path_for(root, service, &checked)
}

/// The scheduler's one tick: take what is due, then prune what has expired.
///
/// Runs from a background loop and never from a command. Everything it can fail
/// at is a reason to do nothing rather than to report: the engine is down, the
/// database is not running, the workspace has moved. A backup feature that
/// raises a dialog because Docker was closed is one people switch off.
pub async fn run_due_snapshots(app: &AppHandle) {
    let (schedule, keep) = snapshot_settings();
    if schedule == crate::snapshot::Schedule::Off {
        return;
    }

    // `Manager` names `state`, and this module imports `State` rather than the
    // trait — spelled out here so the one background caller does not put a
    // trait import at the top of a file where nothing else needs it.
    let state = <AppHandle as tauri::Manager<_>>::state::<AppState>(app);
    let Ok(root) = state.root() else { return };

    // Only what is actually running. Dumping a stopped database produces a
    // failed `docker exec` and, before `db::dump` removes it, a zero-byte file
    // that looks exactly like a backup.
    let Ok(targets) = crate::db::targets(&root).await else {
        return;
    };

    for target in targets.into_iter().filter(|t| t.running) {
        let service = target.service.clone();
        let last = crate::snapshot::last_automatic(&root, &service);
        if !crate::snapshot::is_due(schedule, last, std::time::SystemTime::now()) {
            continue;
        }

        let name =
            crate::snapshot::auto_name(&crate::snapshot::stamp(std::time::SystemTime::now()));
        let Ok(path) = crate::snapshot::path_for(&root, &service, &name) else {
            continue;
        };
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }

        // Serialised against the manual buttons through the same key, so a
        // scheduled dump cannot start while somebody is restoring.
        let Ok(_busy) = state.inflight.acquire(format!("db:{service}")) else {
            continue;
        };

        match crate::db::dump(&root, &service, &path, |_| {}).await {
            Ok(bytes) => {
                tracing::info!(service = %service, bytes, "scheduled snapshot taken");
                events::emit(
                    app,
                    "db:snapshot",
                    serde_json::json!({ "service": service, "name": name, "bytes": bytes }),
                );
            }
            Err(e) => {
                tracing::warn!(service = %service, error = %e.message, "scheduled snapshot failed");
                continue;
            }
        }

        for stale in crate::snapshot::expired(&crate::snapshot::list(&root), keep) {
            if crate::snapshot::remove(&root, &service, &stale).is_ok() {
                tracing::info!(service = %service, snapshot = %stale, "expired snapshot removed");
            }
        }
    }
}

/// The schedule and the retention window, from preferences.
///
/// Defaults are `off` and 7 — off because a feature that starts writing
/// hundreds of megabytes without being asked is one people find out about when
/// a disk fills, and 7 because that is a week of daily copies.
fn snapshot_settings() -> (crate::snapshot::Schedule, usize) {
    let prefs = prefs_path().map(|p| read_prefs(&p)).unwrap_or_default();
    let schedule = prefs
        .get("backupSchedule")
        .and_then(|v| v.as_str())
        .map(crate::snapshot::Schedule::parse)
        .unwrap_or_default();
    let keep = prefs
        .get("backupKeep")
        .and_then(|v| v.as_u64())
        .unwrap_or(7)
        .clamp(1, 100) as usize;
    (schedule, keep)
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

    // A restore replaces data that was there; a dump only reads it. Only the
    // first is audited — the bar is "would somebody have to account for this?",
    // and a backup is the answer to that question rather than an instance of it.
    if action == "restore" {
        crate::audit::record_with(
            "db_restore",
            &service,
            if outcome.is_ok() {
                crate::audit::Outcome::Ok
            } else {
                crate::audit::Outcome::Failed
            },
            Some(format!("from {path}")),
        );
    }

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

/// Whether a built image may be pushed (H-1).
///
/// Verification is re-run rather than remembered from the build. It is a
/// question about an image that is on this machine now, and the answer between
/// the build and the push is exactly the interval in which somebody could have
/// retagged something else onto the name.
#[tauri::command]
pub async fn release_push_plan(
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
) -> Result<crate::release::PushPlan> {
    let root = state.root()?;
    let plan = crate::release::plan(&root, &name, tag)?;
    let verification = crate::release::verify(&plan.tag, &plan.runtime).await.ok();
    Ok(crate::release::push_plan(&plan.tag, verification.as_ref()))
}

/// Push it, refusing again on the way out.
#[tauri::command]
pub async fn release_push(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
) -> Result<crate::release::PushPlan> {
    let root = state.root()?;
    let _busy = state.inflight.acquire("release")?;

    let plan = crate::release::plan(&root, &name, tag.clone())?;
    let verification = crate::release::verify(&plan.tag, &plan.runtime).await.ok();
    // Re-checked here rather than trusting the plan the caller was shown: that
    // plan crossed the IPC boundary and came back, and a check that only runs
    // on the way out is not a check.
    let push = crate::release::push_plan(&plan.tag, verification.as_ref());
    if !push.possible {
        return Err(Error::new(
            Code::Forbidden,
            push.refused
                .unwrap_or_else(|| "this image may not be pushed".into()),
        ));
    }

    let operation_id = events::next_operation_id("push");
    runner::run_operation(
        &events::sink(&app),
        runner::Operation {
            operation_id: &operation_id,
            subject: &name,
            progress_event: "build:progress",
            finished_event: "build:success",
            program: "docker",
            args: &crate::release::push_argv(&plan.tag),
            cwd: &root,
            env: &[],
        },
    )
    .await?;

    Ok(push)
}

/// A compose file for running the built image somewhere else (H-1).
///
/// Returned as text rather than written to a path the caller names. Where a
/// deployment recipe belongs is the user's decision — a repository, a server, a
/// paste — and a command that wrote it somewhere would be this app guessing.
#[tauri::command]
pub fn release_recipe(
    state: State<'_, AppState>,
    name: String,
    tag: Option<String>,
) -> Result<String> {
    let root = state.root()?;
    let plan = crate::release::plan(&root, &name, tag)?;
    let dir = workspace::project_dir(&root, &name)?;
    let manifest = manifest::read(&dir.join("stackvo.json"), &name)?;

    // The port the project actually answers on, per runtime. A recipe that
    // published the wrong one is a deployment that starts and serves nothing.
    let port = manifest
        .node
        .as_ref()
        .map(|n| n.port)
        .or_else(|| manifest.lang.as_ref().map(|l| l.port))
        .unwrap_or(80);

    // Named from the project's own `.env`, not from the workspace's: the
    // workspace file holds this machine's stack, and half of it is credentials
    // for containers that are not going anywhere.
    let env_keys = project_env_keys(&dir);

    Ok(crate::release::recipe(&name, &plan.tag, port, &env_keys))
}

/// The variable *names* a project's own `.env` files use.
///
/// Names only, and read rather than copied — the whole point of the recipe is
/// that it carries no values. A project with no `.env` yields nothing, which is
/// a recipe with no `environment:` block rather than an error.
fn project_env_keys(dir: &std::path::Path) -> Vec<String> {
    let mut keys = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".env") || name.ends_with(".bak") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, _)) = line.split_once('=') {
                let key = key.trim();
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                {
                    keys.insert(key.to_string());
                }
            }
        }
    }
    keys.into_iter().collect()
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

/// Read a tarball back in. Returns the image names the daemon adopted.
///
/// No workspace and no project name: a bundle is loaded on the machine that
/// received it, which is precisely the machine that may have neither.
#[tauri::command]
pub async fn release_load(path: String) -> Result<Vec<String>> {
    let loaded = crate::release::load(std::path::Path::new(&path)).await;

    // What landed, by name, because that is the part a bundle's file name does
    // not tell anyone — and installing an image from elsewhere is the act this
    // trail exists for.
    crate::audit::record_with(
        "release_load",
        &path,
        match &loaded {
            Ok(_) => crate::audit::Outcome::Ok,
            Err(_) => crate::audit::Outcome::Failed,
        },
        loaded.as_ref().ok().map(|images| images.join(", ")),
    );
    loaded
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

/// The same profile as a call tree, for the flame view.
///
/// F-3. `profiler_read` answers "where did the time go" — a table of the
/// costliest functions — and cannot answer "what called that", which is the
/// question a flame view exists for. Separate from `profiler_read` rather than
/// a field on it: the table is what the pane opens with and the tree is
/// thousands of nodes, so a page that only wanted the top sixty would carry the
/// whole graph across the boundary to ignore it.
///
/// A call tree and not, strictly, a flame graph — see [`crate::profile::call_tree`]
/// for what cachegrind holds and what it does not.
#[tauri::command]
pub fn profiler_tree(
    state: State<'_, AppState>,
    name: String,
    id: String,
) -> Result<Vec<crate::profile::Frame>> {
    let report = crate::profile::read(&state.root()?, &name, &id)?;
    Ok(crate::profile::call_tree(&report))
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

    let plan = certs::apply(&root, install_ca.unwrap_or(true))
        .await
        .inspect_err(|e| {
            crate::audit::record_with(
                "cert_apply",
                "*",
                crate::audit::Outcome::Failed,
                Some(e.message.clone()),
            )
        })?;
    crate::audit::record_with(
        "cert_apply",
        plan.domains.join(", "),
        crate::audit::Outcome::Ok,
        install_ca
            .unwrap_or(true)
            .then(|| "CA trust requested".to_string()),
    );
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

/// The committed manifest, which is what the editor edits.
///
/// `read_committed`, not `read`: this text round-trips straight back through
/// `project_manifest_write`, so handing it the effective manifest would offer
/// somebody this machine's overrides to save into the file the team shares.
/// `manifest::write` refuses that, which means the editor would simply stop
/// being able to save — correct, and a bad way to find out. The overrides have
/// their own pair of commands below.
#[tauri::command]
pub fn project_manifest_read(state: State<'_, AppState>, name: String) -> Result<Manifest> {
    let root = state.root()?;
    manifest::read_committed(
        &workspace::project_dir(&root, &name)?.join("stackvo.json"),
        &name,
    )
}

/// This machine's overrides for a project (B-2).
#[tauri::command]
pub fn project_local_read(
    state: State<'_, AppState>,
    name: String,
) -> Result<manifest::LocalOverride> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    manifest::read_local(&dir, &name)
}

/// Write this machine's overrides, or remove them when the text is empty.
///
/// Text rather than a parsed object, unlike `project_manifest_write`. That one
/// takes a `ProjectManifest` because the form builds it field by field; this is
/// a file somebody types, and round-tripping it through a struct would drop the
/// comments — no, JSON has none — but it would also silently reorder and
/// reformat what they wrote, and this is the one file in the project nobody
/// else reads.
#[tauri::command]
pub fn project_local_write(
    state: State<'_, AppState>,
    name: String,
    text: String,
) -> Result<manifest::LocalOverride> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let _busy = state.inflight.acquire(format!("project:{name}"))?;
    manifest::write_local(&dir, &name, &text)
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

// ----------------------------------------------------- LAN sharing (E-3)

/// What another device on this network can reach, and what it cannot.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    /// This machine's private address, or `None` when there is none to offer.
    ///
    /// `None` is two different situations with the same answer: no network at
    /// all, and a machine whose address is public. The second is the one worth
    /// refusing — a development site under a name anybody on the internet can
    /// resolve is not what a switch called "share on this network" promises.
    pub address: Option<String>,
    /// The suffix, so the screen can say where the name comes from rather than
    /// showing a domain nobody recognises and hoping it is trusted.
    pub suffix: String,
    /// Projects that asked, each with the host it currently answers on.
    pub projects: Vec<LanProject>,
    /// A name rendered into the compose file or the certificate that this
    /// machine would no longer produce — a laptop that changed networks.
    ///
    /// The one thing a derived address cannot tell you by being derived: the
    /// value on disk is a copy, and a copy of an expired lease points at
    /// whatever machine took it next.
    pub stale: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanProject {
    pub name: String,
    /// `None` when the project asked but this machine has no address to build
    /// one from — the switch is on and there is nothing to show for it, which
    /// is a state the screen has to be able to say out loud.
    pub host: Option<String>,
}

/// Where this workspace is reachable from the rest of the network.
///
/// E-3. `shop.loc` lives in exactly one `/etc/hosts` and that is this machine's,
/// so a real phone has never been able to open it. See [`crate::lan`] for why
/// the answer is a wildcard DNS suffix rather than a resolver of our own, and
/// for the warning the visiting browser will show.
#[tauri::command]
pub fn lan_status(state: State<'_, AppState>) -> Result<LanStatus> {
    let root = state.root()?;
    let address = crate::lan::address();

    let mut projects = Vec::new();
    if let Some(entries) =
        crate::workspace::projects_root(&root).and_then(|p| std::fs::read_dir(p).ok())
    {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !path.join("stackvo.json").is_file() {
                continue;
            }
            let Ok(manifest) = crate::manifest::read(&path.join("stackvo.json"), name) else {
                continue;
            };
            if manifest.lan_share {
                projects.push(LanProject {
                    name: name.to_string(),
                    host: address.map(|ip| crate::lan::domain_for(name, ip)),
                });
            }
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    // Read from what was actually written, not from what would be written now
    // — the whole question is whether those two still agree.
    let rendered = std::fs::read_to_string(root.join("generated/docker-compose.projects.yml"))
        .unwrap_or_default();
    let baked: Vec<String> = rendered
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .filter(|token| token.ends_with(crate::lan::SUFFIX))
        .map(str::to_string)
        .collect();

    Ok(LanStatus {
        address: address.map(|ip| ip.to_string()),
        suffix: crate::lan::SUFFIX.to_string(),
        projects,
        stale: crate::lan::stale(&baked, address),
    })
}

/// Turn LAN sharing on or off for one project.
///
/// Writes the intent and nothing else. The hostname is not stored — it is
/// derived from this machine's address every time it is asked for, which is the
/// whole of `lan.rs`'s argument — so the caller still has to regenerate for the
/// router and the certificate to carry it. That is the same shape every other
/// manifest change has and deliberately not special-cased here: a command that
/// quietly regenerated the whole workspace would be doing an expensive thing
/// behind a switch that reads as cheap.
#[tauri::command]
pub fn project_lan_share(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<Manifest> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let _busy = state.inflight.acquire(format!("project:{name}"))?;

    // Committed, not effective: the next line edits it and the line after
    // writes it back. See `manifest::read`.
    let mut manifest = manifest::read_committed(&dir.join("stackvo.json"), &name)?;
    manifest.lan_share = enabled;
    manifest::write(&dir.join("stackvo.json"), &manifest)?;
    Ok(manifest)
}

// ------------------------------------------------- declared services (B-1)

/// One service a project's manifest asks for, and what this stack does about it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredService {
    pub id: String,
    /// There is a template for it. False means the manifest names something
    /// this version has never heard of — reported, not silently dropped.
    pub known: bool,
    /// `SERVICE_<NAME>_ENABLE` is true in the workspace `.env`.
    pub enabled: bool,
}

/// What a project declares, what the stack currently gives it, and the diff.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    pub declared: Vec<DeclaredService>,
    /// Services the project's own `.env` implies but its manifest does not
    /// declare, each with the key that implied it. The input to writing the
    /// declaration in the first place.
    pub suggested: Vec<crate::detect::ServiceHint>,
    /// The reviewed diff, from the same planner a preset import uses. Empty
    /// changes means the stack already satisfies the declaration.
    pub plan: crate::preset::Plan,
}

/// Read the declaration and compare it with the stack. Changes nothing.
///
/// Two sources on purpose. `declared` is what the repository says, which is the
/// statement a teammate cloned; `suggested` is what the project's own `.env`
/// implies, which is how the declaration gets written the first time without
/// anybody typing a list. Keeping them apart matters — merging them would make
/// a guess indistinguishable from a commitment, and only one of the two is
/// something a colleague agreed to.
#[tauri::command]
pub fn project_requirements(state: State<'_, AppState>, name: String) -> Result<Requirements> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    let manifest = manifest::read(&dir.join("stackvo.json"), &name)?;

    let env = Env::load(&root)?;
    let catalog = crate::contracts::env_schema();

    let declared = manifest
        .services
        .iter()
        .map(|id| DeclaredService {
            known: catalog.knows_service(id),
            enabled: env.service_enabled(id),
            id: id.clone(),
        })
        .collect();

    // Only what is not already written down: repeating a declared service as a
    // suggestion would invite somebody to "add" what is already there.
    let suggested = crate::detect::services_of(&dir)
        .into_iter()
        .filter(|hint| !manifest.services.contains(&hint.service))
        .collect();

    Ok(Requirements {
        declared,
        suggested,
        plan: crate::preset::plan_declared(&root, &manifest.services)?,
    })
}

/// Enable everything the project declares that is not on yet.
///
/// Writes `.env` and stops there, exactly as `preset_apply` does, and for the
/// same reason: the plan says `needsRegenerate` and regenerating is a visible
/// step with its own progress. Doing it silently here would make one click
/// rewrite every compose file in the workspace.
#[tauri::command]
pub fn project_requirements_apply(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::preset::Plan> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    let manifest = manifest::read(&dir.join("stackvo.json"), &name)?;

    // `.env` has several writers; the preset import takes the same lock for the
    // same reason — the diff that is applied must be the diff that was planned.
    let _busy = state.inflight.acquire("preset")?;
    crate::preset::apply_declared(&root, &manifest.services)
}

/// Write the declaration into `stackvo.json`.
///
/// A focused command rather than asking the front end to rebuild a manifest and
/// post it through `project_manifest_write`: that path round-trips every other
/// field through the webview, and a field the UI has not learned about yet
/// would come back missing. Here the manifest is read, one list is replaced,
/// and the rest is the bytes that were already there.
#[tauri::command]
pub fn project_requirements_declare(
    state: State<'_, AppState>,
    name: String,
    services: Vec<String>,
) -> Result<Manifest> {
    let root = state.root()?;
    let dir = workspace::project_dir(&root, &name)?;
    if !dir.is_dir() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let _busy = state.inflight.acquire(format!("project:{name}"))?;

    let path = dir.join("stackvo.json");
    // Committed: this is read to be written back.
    let mut manifest = manifest::read_committed(&path, &name)?;

    // Normalised here as well as in `read`, because this list comes from the
    // webview rather than from the file: same trim, same lower-case, same
    // de-duplication, so what is written back reads identically to what a
    // person would have typed.
    let mut wanted: Vec<String> = Vec::new();
    for id in services {
        let id = id.trim().to_ascii_lowercase();
        if !id.is_empty() && !wanted.contains(&id) {
            wanted.push(id);
        }
    }
    manifest.services = wanted;

    manifest::write(&path, &manifest)?;
    Ok(manifest)
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
        sync_project_host(&app, &m).await;
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
async fn sync_project_host(app: &AppHandle, manifest: &Manifest) {
    // Every name this project answers on that a hosts file can carry. One
    // elevation prompt for all of them rather than one per name: a project with
    // three tenant subdomains asking three times for the same password is a
    // project people stop adding subdomains to.
    let wanted: Vec<String> = manifest
        .domain
        .iter()
        .cloned()
        .chain(
            manifest
                .aliases
                .iter()
                .filter(|a| crate::manifest::resolves_through_hosts(a))
                .cloned(),
        )
        .filter(|d| hosts::is_valid_domain(d))
        .collect();

    let configured = hosts::status_for(&wanted);
    let missing: Vec<String> = wanted
        .into_iter()
        .filter(|d| !configured.iter().any(|e| &e.domain == d && e.configured))
        .collect();

    if missing.is_empty() {
        return;
    }

    match hosts::apply(&missing, &[]) {
        Ok(plan) => events::emit(
            app,
            "hosts:changed",
            serde_json::json!({ "added": plan.add, "removed": plan.remove }),
        ),
        Err(e) => {
            tracing::warn!(error = %e.message, "hosts entries not written")
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

// ------------------------------------------------ importing from a rival

/// What XAMPP and Laragon have on this machine.
///
/// Reads only, and never the tool's own configuration for anything but the
/// hostname it serves a site at. Works with no workspace selected, because the
/// answer is about the machine — but reports which names are taken when there
/// is one, so the list can say so before somebody clicks.
#[tauri::command]
pub fn imports_scan(state: State<'_, AppState>) -> Result<Vec<crate::imports::Install>> {
    let projects = state
        .root()
        .ok()
        .and_then(|root| workspace::projects_root(&root));
    Ok(crate::imports::scan(projects.as_deref()))
}

/// The same, for an installation somewhere this app did not think to look.
#[tauri::command]
pub fn imports_scan_at(
    state: State<'_, AppState>,
    source: String,
    path: String,
) -> Result<Option<crate::imports::Install>> {
    let source = match source.as_str() {
        "xampp" => crate::imports::Source::Xampp,
        "laragon" => crate::imports::Source::Laragon,
        other => {
            return Err(Error::new(
                Code::InvalidInput,
                format!("{other} is not a tool this app can read"),
            ))
        }
    };

    let projects = state
        .root()
        .ok()
        .and_then(|root| workspace::projects_root(&root));
    Ok(crate::imports::scan_at(
        source,
        std::path::Path::new(&path),
        projects.as_deref(),
    ))
}

/// Bring one site into the workspace. Copies by default; moves when asked.
///
/// This is the file half only. The manifest is written by `project_adopt`
/// afterwards, from the front end, with the detected domain filled in — the
/// same path an ordinary adoption takes, so an imported project is validated by
/// the same rules and is not a second class of project.
///
/// Nothing is written into the other installation in either mode. `move` copies
/// first and removes the original only once the copy is complete: the reverse
/// order turns a full disk into a site that exists in neither place.
#[tauri::command]
pub async fn imports_take(
    state: State<'_, AppState>,
    path: String,
    name: String,
    r#move: bool,
) -> Result<String> {
    let root = state.root()?;
    let name = workspace::canonical_name(&name);
    let target = workspace::project_dir(&root, &name)?;

    let source = std::path::PathBuf::from(&path);
    if !source.is_dir() {
        return Err(Error::not_found(path));
    }
    if target.exists() {
        return Err(Error::new(
            Code::AlreadyExists,
            format!("a project directory called `{name}` already exists"),
        )
        .with_hint(crate::hints::CHOOSE_ANOTHER_NAME));
    }
    // The one thing a copy must not do: write a tree into itself. It happens
    // when somebody points the projects directory at their own htdocs, which is
    // a perfectly reasonable thing to have tried.
    if target.starts_with(&source) {
        return Err(Error::new(
            Code::InvalidInput,
            "the projects directory is inside the site being imported".to_string(),
        ));
    }

    let _busy = state.inflight.acquire(format!("project:{name}"))?;

    let from = source.clone();
    let to = target.clone();
    let copied =
        tauri::async_runtime::spawn_blocking(move || crate::imports::copy_tree(&from, &to))
            .await
            .map_err(|e| Error::new(Code::IoError, format!("the copy did not finish: {e}")))?;

    if let Err(e) = copied {
        // A half-copied tree is worse than none: adoption would find it, write
        // a manifest, and produce a project missing files nobody can name.
        let _ = std::fs::remove_dir_all(&target);
        return Err(Error::io(format!("copying {} ", source.display()), e));
    }

    if r#move {
        // Only now. And a failure here is reported rather than rolled back —
        // the copy is complete and correct, and deleting it to honour "move"
        // would destroy the successful half of the operation.
        if let Err(e) = std::fs::remove_dir_all(&source) {
            tracing::warn!(
                path = %source.display(),
                error = %e,
                "the site was copied but the original could not be removed"
            );
        }
    }

    crate::audit::record_with(
        "project_import",
        &name,
        crate::audit::Outcome::Ok,
        Some(format!(
            "{} from {}",
            if r#move { "moved" } else { "copied" },
            source.display()
        )),
    );

    Ok(target.display().to_string())
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
        sync_project_host(&app, &m).await;
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
        sync_project_host(&app, &m).await;
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

    // Read before anything is removed: every name this project answers on is in
    // the manifest, and the hosts lines and the certificate are keyed by them.
    // Absent or unreadable, those two steps are simply skipped — a project
    // whose manifest is already gone has no names to clean up after.
    let manifest = manifest::read(&dir.join("stackvo.json"), &name).ok();

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

        // Recorded before the tree goes, not after: if `remove_dir_all` dies
        // half way there is no "after", and a partially deleted project is
        // exactly the state somebody comes back asking about.
        crate::audit::record_with(
            "project_delete",
            &name,
            crate::audit::Outcome::Ok,
            Some(if remove_files.unwrap_or(false) {
                "source removed".into()
            } else {
                "manifest only, source kept".to_string()
            }),
        );

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
        if let Some(manifest) = &manifest {
            drop_project_host(&app, manifest).await;
        }
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
async fn drop_project_host(app: &AppHandle, manifest: &Manifest) {
    let managed = hosts::mapped_domains().1;

    // Every name that was written for this project, not just the main one:
    // deleting a project and leaving its three tenant subdomains behind is how
    // a hosts file fills up with names for directories that are gone.
    let remove: Vec<String> = manifest
        .domain
        .iter()
        .cloned()
        .chain(manifest.aliases.iter().cloned())
        .filter(|d| hosts::is_valid_domain(d))
        .filter(|d| managed.contains(&d.to_ascii_lowercase()))
        .collect();

    if remove.is_empty() {
        return;
    }

    match hosts::apply(&[], &remove) {
        Ok(plan) => events::emit(
            app,
            "hosts:changed",
            serde_json::json!({ "added": plan.add, "removed": plan.remove }),
        ),
        Err(e) => {
            tracing::warn!(error = %e.message, "hosts entries not removed")
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
    let history = recover(&state.stats_history);

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
    let _serialised = recover(&WRITE_LOCK);

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
pub fn env_reveal(state: State<'_, AppState>, key: String) -> Result<String> {
    let env = Env::load(&state.root()?)?;

    env.get(&key)
        .map(str::to_string)
        .ok_or_else(|| Error::new(Code::NotFound, format!("{key} is not set in .env")))
}

/// The value behind one masked credential on the services list.
///
/// Dispatches the way every other reader of service state does — the table when
/// there is one, `.env` when there is not. The detail sheet used to call
/// `env_reveal` directly, which is right for a workspace that keeps its
/// services in `.env` and wrong for a migrated one: the key it would ask for is
/// `SERVICE_MYSQL_ROOT_PASSWORD`, and after a handover nothing sets that. The
/// eye reported "not set in .env" over a password that exists.
///
/// `key` is the *setting* key for an instance (`ROOT_PASSWORD`) and the `.env`
/// key for the legacy path (`SERVICE_MYSQL_ROOT_PASSWORD`), which is what the
/// row already carries in each case.
#[tauri::command]
pub fn service_reveal(state: State<'_, AppState>, service: String, key: String) -> Result<String> {
    let root = state.root()?;
    if crate::instances::path(&root).exists() {
        return instance_reveal(state, service, key);
    }
    env_reveal(state, key)
}

// ---------------------------------------------------------------- secrets

/// Which credentials are in the keystore, which are in the file, and whether
/// this machine has a keystore at all.
///
/// One call rather than three because the pane needs all of it to render a
/// single row per key, and because "there is no keystore here" has to reach the
/// UI before it offers a button that cannot work — a headless Linux box with no
/// Secret Service is a real machine somebody runs this on.
#[tauri::command]
pub fn secrets_status(state: State<'_, AppState>) -> Result<serde_json::Value> {
    let root = state.root()?;

    // The *file*, not `Env::load`: loading resolves a reference into the value
    // it points at, which is precisely the fact this command reports on.
    let text = std::fs::read_to_string(root.join(".env")).unwrap_or_default();
    let on_disk = Env::parse(&text);

    let keys: Vec<serde_json::Value> = on_disk
        .raw()
        .iter()
        .filter(|(key, _)| crate::secrets::is_movable(key))
        .map(|(key, value)| {
            let entry = crate::secrets::entry_of(value);
            serde_json::json!({
                "key": key,
                "moved": entry.is_some(),
                // Only asked of keys that claim to be in the store, and never
                // the value itself — this is a status call, and `env_reveal` is
                // the one deliberate way a password crosses the boundary.
                "resolvable": match entry {
                    Some(name) => crate::secrets::read(name).ok().flatten().is_some(),
                    None => !value.is_empty(),
                },
                "set": !value.is_empty(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "available": crate::secrets::available(),
        "keys": keys,
    }))
}

/// Move one credential out of `.env` and into the keystore.
///
/// Deliberately one key at a time and never automatic. The Bash CLI reads
/// `.env` directly and would take `keychain:…` for the password itself, so this
/// is a decision with a consequence somebody has to be told about — see
/// [`crate::secrets`] and ADR 0010. A sweep that moved every credential at
/// once would be the same decision made silently, twelve times.
#[tauri::command]
pub fn secret_move(state: State<'_, AppState>, key: String) -> Result<()> {
    let root = state.root()?;

    if !crate::secrets::is_movable(&key) {
        return Err(
            Error::new(Code::InvalidInput, format!("`{key}` is not a credential"))
                .with_hint(crate::hints::ONLY_CREDENTIALS_MOVE),
        );
    }

    let text = std::fs::read_to_string(root.join(".env")).unwrap_or_default();
    let on_disk = Env::parse(&text);
    let value = on_disk.get(&key).unwrap_or_default();

    if crate::secrets::is_reference(value) {
        // Not an error: the end state the caller asked for is the state it is
        // already in, and failing here would make a double click a failure.
        return Ok(());
    }
    if value.is_empty() {
        return Err(Error::new(
            Code::NotFound,
            format!("`{key}` has no value in .env to move"),
        ));
    }

    let entry = crate::secrets::new_entry(&key, &root);
    crate::secrets::write(&entry, value)?;

    // The reference replaces the value only once the value is safely stored.
    // The other order loses the password if the write fails.
    let outcome = env_writer::apply(
        &root,
        &std::collections::BTreeMap::from([(
            key.clone(),
            crate::secrets::reference_for(&key, &root),
        )]),
    );

    if outcome.is_err() {
        // Leave nothing behind: an entry nothing points at is a password in the
        // user's keychain that no screen in this app will ever show them again.
        let _ = crate::secrets::delete(&entry);
    }

    crate::audit::record(
        "secret_move",
        &key,
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );
    outcome
}

/// Put a credential back in `.env` and forget the keystore entry.
///
/// The way out, and it exists because the way in has a cost the user may only
/// discover afterwards — the first time they run `stackvo.sh` on the same
/// workspace. A one-way door here would mean hand-editing `.env` with a value
/// the app will not show, which is not a way out at all.
#[tauri::command]
pub fn secret_restore(state: State<'_, AppState>, key: String) -> Result<()> {
    let root = state.root()?;

    let text = std::fs::read_to_string(root.join(".env")).unwrap_or_default();
    let on_disk = Env::parse(&text);
    let Some(entry) = crate::secrets::entry_of(on_disk.get(&key).unwrap_or_default()) else {
        return Err(Error::new(
            Code::NotFound,
            format!("`{key}` is not stored in the keystore"),
        ));
    };

    let Some(value) = crate::secrets::read(entry)? else {
        return Err(Error::new(
            Code::NotFound,
            format!("the keystore has no entry named `{entry}`"),
        )
        .with_hint(crate::hints::KEYSTORE_ENTRY_IS_GONE));
    };

    // `apply_verbatim`, because `apply` would see the key is currently a
    // reference and send this value straight back to the keystore — which is
    // the correct rule everywhere except in the one command whose job is to
    // undo it.
    let outcome = env_writer::apply_verbatim(
        &root,
        &std::collections::BTreeMap::from([(key.clone(), value)]),
    );
    if outcome.is_ok() {
        // Only after the file has it. Deleting first and failing to write would
        // destroy the password.
        crate::secrets::delete(entry)?;
    }

    crate::audit::record(
        "secret_restore",
        &key,
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );
    outcome
}

// ---------------------------------------------------------------- assistants

/// Which assistants are on this machine, and which already know about the
/// server.
///
/// Deliberately tolerant of having no workspace. Every other command here
/// starts with `state.root()?`, which is right when the answer is about a
/// stack — but this one is about the machine, and refusing to list the editors
/// installed on it because no folder has been chosen yet would hide the pane
/// exactly when somebody is setting the app up for the first time. The root is
/// reported when there is one, because it goes into the registration.
#[tauri::command]
pub fn agents_status(state: State<'_, AppState>) -> Result<crate::agents::Status> {
    let root = state.root().ok().map(|r| r.display().to_string());
    Ok(crate::agents::status(root.as_deref()))
}

/// Write the server into one client's configuration file.
///
/// `allow_writes` is the whole security decision and it is passed from the UI
/// rather than defaulted here: it puts `--allow-writes` in the argument list,
/// which grants that assistant `stack_down` and `project_stop` along with the
/// tools people actually want. The default in the pane is off, matching the
/// server's own default.
///
/// Audited, because this writes to a file outside the workspace and outside
/// this app's own directories — the same reason `/etc/hosts` is audited.
#[tauri::command]
pub fn agents_install(
    state: State<'_, AppState>,
    client: String,
    allow_writes: bool,
) -> Result<String> {
    let root = state.root().ok().map(|r| r.display().to_string());
    let outcome = crate::agents::install(&client, allow_writes, root.as_deref());

    crate::audit::record_with(
        "agent_install",
        &client,
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
        // The flag is the detail worth having later: "an assistant could stop
        // the stack from this date" is answerable only if it was written down.
        Some(
            if allow_writes {
                "reads and writes"
            } else {
                "read-only"
            }
            .to_string(),
        ),
    );
    outcome
}

/// Take the server back out of one client's configuration file.
#[tauri::command]
pub fn agents_remove(client: String) -> Result<String> {
    let outcome = crate::agents::uninstall(&client);
    crate::audit::record(
        "agent_remove",
        &client,
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );
    outcome
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

/// What an administrator has decided on this machine, if anything.
///
/// Every field here exists so a Settings pane can explain itself rather than
/// just refusing. A greyed-out field with no reason reads as a broken app; one
/// that says which file it came from is something the user can act on, even if
/// the action is to go and ask somebody.
///
/// `error` is returned rather than logged for the reason [`crate::policy`]
/// gives: a policy that quietly did nothing is one the administrator who
/// deployed it believes is in force. Somebody has to see it, and the machine
/// where it is visible is this one.
#[tauri::command]
pub fn policy_status() -> serde_json::Value {
    let policy = crate::policy::current();
    serde_json::json!({
        "active": policy.is_active(),
        "source": policy.source().map(|p| p.display().to_string()),
        // The keys only. A managed value is not a secret, but this is a
        // status call and the values are already on the wire through
        // `env_get` — with its redaction, which this would be a way around.
        "managed": policy.settings().keys().collect::<Vec<_>>(),
        "locked": policy.locked().iter().collect::<Vec<_>>(),
        "registryPrefix": policy.registry_prefix(),
        // The market block, as an administrator set it. Reported in full rather
        // than as a single "constrained" flag: the whole reason the Settings
        // pane shows this is so a person who has just been refused an install
        // can read *which* rule refused it, and a boolean would send them to
        // support instead.
        "market": {
            "constrained": policy.constrains_market(),
            "registryUrl": policy.market().registry_url,
            "offlineBundle": policy.market().offline_bundle
                .as_ref().map(|p| p.display().to_string()),
            "requireSignature": policy.market().require_signature,
            "allowedPackages": policy.market().allowed_packages.iter().collect::<Vec<_>>(),
            "allowedRegistries": policy.market().allowed_registries.iter().collect::<Vec<_>>(),
            "autoUpdate": policy.market().auto_update,
            // The count, not the keys. A public key is not a secret and this is
            // still a status call; the number is what answers "did my key
            // arrive", which is the question an administrator has.
            "additionalKeys": policy.market().additional_keys.len(),
        },
        "error": policy.error(),
    })
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
pub fn tray_relabel(
    app: AppHandle,
    labels: Option<std::collections::BTreeMap<String, String>>,
) -> Result<()> {
    // Adopted before the redraw, not after: `relabel` reads the catalog while
    // it builds, so setting it afterwards would leave the menu one language
    // behind until the next call.
    crate::tray::set_labels(labels)?;
    crate::tray::relabel(&app);
    Ok(())
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
        "notifyOnBuild": true,
        // Off, because a feature that starts writing hundreds of megabytes
        // without being asked is one people find out about when a disk fills.
        "backupSchedule": "off",
        "backupKeep": 7
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
    *recover(&state.workspace) = ws.clone();
    watcher.retarget(&app, ws.require_root().ok());
    Ok(Some(ws))
}

/// How often to sample per-container stats, given whether anyone can see them.
///
/// Stretched rather than stopped when the window is away. Stopping would be the
/// obvious saving and the wrong one: this series exists so that opening a
/// project's detail view shows a sparkline instead of a single point, and a
/// sampler that idles while hidden hands back an empty chart at exactly the
/// moment somebody looks at it. Five times slower keeps the history continuous
/// — coarser, but continuous — for a fifth of the daemon calls.
///
/// The tray's own 15-second refresh is deliberately *not* gated on this. A tray
/// that stops updating while the window is closed has lost the one job it has.
pub fn stats_sample_interval(window_visible: bool) -> std::time::Duration {
    std::time::Duration::from_secs(if window_visible { 60 } else { 300 })
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
    recover(&state.stats_history).retain(|name, _| live.contains(name));

    for (id, info) in containers {
        if !info.running {
            continue;
        }
        let Ok(stats) = engine::container_stats(&id).await else {
            continue;
        };

        let mut history = recover(&state.stats_history);
        let series = history.entry(engine::container_name(&id)).or_default();
        series.push((now, stats.cpu_percent, stats.memory_percent));
        if series.len() > 120 {
            let excess = series.len() - 120;
            series.drain(0..excess);
        }
    }

    // Written after the whole round rather than after each container: one
    // atomic replacement per minute, of a file bounded by the same 120-sample
    // cap that bounds the map.
    //
    // On every round, not at shutdown. A cache only written on the way out is
    // one that is empty after precisely the exits worth surviving — a crash,
    // a force quit, a machine that lost power — and `crash.rs` exists because
    // this app does crash sometimes.
    //
    // The result is dropped on purpose. A history that cannot be written is a
    // sparkline that starts empty next launch; stopping the sampler over it
    // would trade that for a sparkline that is empty now.
    if let Some(path) = crate::stats_store::path() {
        let snapshot = recover(&state.stats_history).clone();
        let _ = crate::stats_store::save_to(&path, &snapshot);
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

/// The services half, rendered from `.env` and the compiled-in templates.
///
/// Lifted out of `render_generated` unchanged when the instance table became a
/// second source. Kept whole rather than merged with the new path: they share
/// an output and nothing else, and a single function with a branch through the
/// middle of it would be a function nobody could read either half of.
/// Where the services half of a render comes from — and there is only one.
///
/// Faz 6 of `docs/servis-market-mimarisi.md` is closed here. This used to be a
/// switch: no `instances.json` meant render from `.env` and the templates
/// compiled into the binary, an `instances.json` meant render from the table
/// and the package tree. Both branches existed so that every workspace in
/// existence could keep working while the second one was built.
///
/// The first branch is gone (ADR 0016). What made keeping it untenable was not
/// the code — it was that the two branches knew about **different catalogues**:
/// `.env` knew the twenty-five services that had a template inside the binary,
/// the table knows whatever the package tree holds. Adding Solr and ClickHouse
/// as packages made that concrete rather than theoretical — a project declaring
/// `services: ["solr"]` got a correct declaration met with a wrong warning, and
/// the warning could not be fixed without putting a templateless entry into the
/// `.env` catalogue, where it would have offered a switch that renders nothing.
///
/// A workspace with no table is now a workspace that has not migrated, and it
/// is met by `MigrationGate` before it ever reaches a render. Reaching here
/// without one is a bug in the gate, so it says so with a name rather than
/// producing an empty stack.
fn service_source(root: &std::path::Path) -> Result<crate::instances::Table> {
    if !crate::instances::path(root).exists() {
        // `Conflict` rather than a new code: the workspace's state and this
        // version's renderer disagree, which is what that code already means,
        // and a new variant is a contract change (ADR 0008) for a message.
        return Err(Error::new(
            Code::Conflict,
            "this workspace still keeps its services in .env, and this version renders them \
             from instances.json",
        )
        .with_hint(crate::hints::MIGRATE_THE_WORKSPACE));
    }
    crate::instances::Table::load(root)
}

pub fn render_generated(root: &std::path::Path) -> Result<Rendered> {
    use crate::generator;

    let env = Env::load(root)?;

    // Before anything is rendered, because the failure is silent otherwise.
    //
    // A key whose keystore entry did not answer is *absent* from the map, so
    // `{{ SERVICE_MYSQL_ROOT_PASSWORD | default('root') }}` renders `root` and
    // a database comes up on a password the user last set years ago and does
    // not know is in force. Every other consumer of `Env` can live with a
    // missing key; this one writes it into a file that starts a container.
    let unresolved = env.unresolved_secrets();
    if !unresolved.is_empty() {
        return Err(Error::new(
            Code::PermissionDenied,
            format!(
                "the keystore did not produce a value for {}",
                unresolved.join(", ")
            ),
        )
        .with_hint(crate::hints::UNLOCK_THE_KEYSTORE)
        .with_details(serde_json::json!({ "keys": unresolved })));
    }

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

    // Once, not per project: every project that shares on the LAN shares on the
    // same address, and asking the routing table in a loop would be asking the
    // same question once per manifest.
    let lan_address = crate::lan::address();

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
            let Ok(mut m) = crate::manifest::read(&path.join("stackvo.json"), name) else {
                continue;
            };

            // LAN sharing is an extra hostname and nothing more, so it joins
            // the list the manifest already has rather than growing a second
            // path through the renderer. Added here rather than in
            // `manifest::read`, because `read` answers what is on disk and this
            // name is not: it is computed from the address this machine has
            // right now, and a manifest that reported it would be reporting
            // something that expires with a DHCP lease.
            if m.lan_share {
                if let Some(ip) = lan_address {
                    m.aliases.push(crate::lan::domain_for(name, ip));
                }
            }

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

    // ---- services: configs and the dynamic compose ----
    //
    // One source. See `service_source` for what happened to the other.
    {
        let table = service_source(root)?;
        {
            let tree = crate::pkg::Tree::open(&crate::market::dir(root))?;
            let network = vars
                .get("DOCKER_DEFAULT_NETWORK")
                .cloned()
                .unwrap_or_else(|| "stackvo-net".into());
            let tld = vars
                .get("DEFAULT_TLD_SUFFIX")
                .cloned()
                .unwrap_or_else(|| "stackvo.loc".into());

            // The keystore, read through the same helper `.env` values go
            // through — one answer to "what is this secret", not two.
            let secrets = |reference: &str| {
                crate::secrets::entry_of(reference)
                    .and_then(|entry| crate::secrets::read(entry).ok().flatten())
            };

            let rendered =
                crate::render::dynamic_compose(root, &table, &tree, &network, &tld, &secrets)?;

            for config in rendered.configs {
                files.push(GenFile {
                    label: format!(
                        "configs/{}",
                        config
                            .path
                            .strip_prefix(root.join("generated/configs"))
                            .unwrap_or(&config.path)
                            .display()
                    ),
                    path: config.path,
                    scope: "services",
                    content: config.contents,
                });
            }
            files.push(GenFile {
                label: "docker-compose.dynamic.yml".into(),
                path: root.join("generated/docker-compose.dynamic.yml"),
                scope: "services",
                content: rendered.compose,
            });
        }
    }

    // ---- traefik ----
    let catalog = env_schema().service_catalog();
    let services: Vec<(&str, bool, Option<&str>)> = catalog
        .iter()
        .map(|(id, _)| (id.as_str(), env.service_enabled(id), env.service_url(id)))
        .collect();

    // A route that no longer normalises — a target edited by hand into
    // something invalid — is dropped from the render rather than failing it.
    // The whole stack refusing to regenerate because of one optional route is a
    // worse outcome than that route being absent, and `routes_list` is where
    // somebody sees why.
    let suffix = env.get("DEFAULT_TLD_SUFFIX").unwrap_or("stackvo.loc");
    let user_routes: Vec<crate::routes::Checked> = crate::routes::read(root)
        .iter()
        .filter_map(|route| route.normalise(suffix).ok())
        .collect();

    let traefik = generator::TraefikOptions {
        tld_suffix: suffix,
        network: env.get("DOCKER_DEFAULT_NETWORK").unwrap_or("stackvo-net"),
        ssl_enabled: env.bool("SSL_ENABLE"),
        redirect_to_https: env.bool("REDIRECT_TO_HTTPS"),
        services,
        routes: user_routes,
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

    // ---- the registry mirror ----
    //
    // Last, and over the rendered text rather than inside each renderer. Every
    // image reference in the workspace passes through this function on its way
    // to disk, so one pass here covers the project Dockerfiles, both compose
    // files and every service template at once — and a renderer added next
    // year is covered without anybody remembering to do it.
    //
    // `crate::policy` carries the three references this deliberately leaves
    // alone, each of which would otherwise break a build.
    if let Some(prefix) = crate::policy::current().registry_prefix() {
        for file in &mut files {
            if crate::policy::rewrites(&file.label) {
                file.content = crate::policy::rewrite(&file.content, prefix);
            }
        }
    }

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
        // The warning this asks for is about entry points, not about routes.
        routes: Vec::new(),
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
/// Every managed project's directory and manifest, for the callers that walk
/// them. Broken ones are skipped: a project that cannot be read has nothing to
/// write a context for, and it is already reported by `list_projects`.
fn project_manifests(
    root: &std::path::Path,
) -> Vec<(String, std::path::PathBuf, manifest::Manifest)> {
    let Ok(projects_dir) = workspace::require_projects_root(root) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&projects_dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
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
        if let Ok(manifest) = manifest::read(&path.join("stackvo.json"), name) {
            out.push((name.to_string(), path, manifest));
        }
    }
    out
}

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

    // The agent context (K-2), written into each project's own directory
    // rather than into `generated/`. It is not a file the stack reads — the
    // reader is an assistant working inside the container, which sees the
    // project tree and not this app's.
    //
    // Best-effort, per project. A directory that has been deleted, or one the
    // app cannot write to, must not stop the stack being regenerated: this is
    // a convenience for a reader that may not exist, and the compose files are
    // the thing somebody is waiting for.
    if scope_includes(scope, "projects") {
        for (name, dir, manifest) in project_manifests(root) {
            if let Ok(context) = crate::agentctx::build(root, &manifest) {
                if crate::agentctx::write(&dir, &context).is_ok() {
                    written.push(format!(
                        "{name}/{}/{}",
                        crate::agentctx::DIR,
                        crate::agentctx::FILE
                    ));
                }
            }
        }
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
mod sampling_tests {
    use super::*;

    /// A hidden window samples slower, and still samples.
    ///
    /// The second half is the assertion that matters. "Stop polling when
    /// hidden" is the obvious reading of the saving, and it would empty the
    /// series this sampler exists to fill — the bug would appear as a detail
    /// view with one data point, blamed on the chart.
    #[test]
    fn a_hidden_window_stretches_the_interval_without_stopping_it() {
        let visible = stats_sample_interval(true);
        let hidden = stats_sample_interval(false);

        assert!(
            hidden > visible,
            "a hidden window must cost less: {hidden:?} is not longer than {visible:?}"
        );
        assert!(
            !hidden.is_zero(),
            "a hidden window still samples — the history has to stay continuous"
        );

        // Bounded on both sides. Too close together and the change buys
        // nothing; too far apart and reopening the window shows a sparkline
        // with a visible hole in it.
        let ratio = hidden.as_secs() / visible.as_secs();
        assert!(
            (2..=10).contains(&ratio),
            "the hidden interval is {ratio}× the visible one, which is outside \
             the range where this is worth doing"
        );
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    /// A poisoned lock still hands over the data, and the write lands.
    ///
    /// Written against a real poisoning rather than a simulated one: a thread
    /// panics while holding the guard, which is the only way the standard
    /// library sets the flag. The nine call sites this replaced would each have
    /// taken the `else` branch here and returned having done nothing.
    #[test]
    fn a_poisoned_lock_is_recovered_rather_than_skipped() {
        let lock = std::sync::Arc::new(Mutex::new(vec!["before"]));

        let poisoner = std::sync::Arc::clone(&lock);
        let died = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("while holding the lock");
        })
        .join();
        assert!(died.is_err(), "the thread was supposed to panic");
        assert!(
            lock.is_poisoned(),
            "a panic under the guard poisons the lock"
        );

        recover(&lock).push("after");

        assert_eq!(
            *recover(&lock),
            vec!["before", "after"],
            "the value survives the poisoning and the write is applied"
        );
    }

    /// The silent pattern does not come back.
    ///
    /// `if let Ok(mut x) = …lock()` compiles, reads as handling, and does
    /// nothing on the one path it exists for. It survived ten months and 556
    /// tests precisely because a skipped cache write and a skipped registry
    /// insert both look exactly like success from the outside — there is no
    /// observable behaviour for a test to assert on. So the gate is on the
    /// source: this file locks through `recover` or it does not lock.
    #[test]
    fn no_lock_in_this_file_is_taken_by_the_silently_skipping_pattern() {
        let source = include_str!("commands.rs");

        // Only the production half. The test modules below deliberately poison
        // a mutex by hand, and a scanner that reads its own source finds its own
        // search string — the first version of this test failed on the line
        // doing the searching.
        let production = source
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("commands.rs has test modules");

        let offenders: Vec<_> = production
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(".lock()"))
            // `recover`'s own body, and the doc comment naming the pattern it
            // replaced. The async `generate_lock` is a Tokio mutex, which has no
            // poisoning to skip.
            .filter(|(_, line)| !line.trim_start().starts_with("///"))
            .filter(|(_, line)| !line.contains("unwrap_or_else"))
            .filter(|(_, line)| !line.contains(".await"))
            .map(|(i, line)| format!("  line {}: {}", i + 1, line.trim()))
            .collect();

        assert!(
            offenders.is_empty(),
            "these take a std Mutex without going through `recover`:\n{}",
            offenders.join("\n")
        );
    }
}

// ================================================================ the market

use std::collections::BTreeMap;
//
// Faz 3 and Faz 4 of `docs/servis-market-mimarisi.md`, the half that touches
// neither Docker nor `.env`. Everything here reads or writes two things: the
// package cache under `<root>/market`, and the instance table at
// `<root>/services/instances.json`.
//
// The commands that *do* touch Docker — enabling an instance, starting it —
// are deliberately absent. They need the generate path to render from the
// instance table, and that swap is Faz 6. Shipping them now would mean a
// button that writes a row nothing renders.

/// Has this machine got a catalogue, where from, and how old is it?
///
/// Every field can be absent, and the absence is the useful part: ADR 0011
/// leaves a fresh install with nothing at all, and "no catalogue yet" is a
/// different sentence from "the catalogue is empty".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStatus {
    /// False before the first refresh. The market page shows a gate rather than
    /// an empty list.
    pub fetched: bool,
    pub sequence: Option<u64>,
    pub generated_at: Option<String>,
    pub expires: Option<String>,
    /// `local`, or absent when nothing has been fetched.
    pub source_kind: Option<String>,
    pub source_location: Option<String>,
    pub packages: usize,
    pub installed: usize,
    /// Whether signatures are being checked. Always false today, and named so
    /// the UI can say so rather than implying otherwise — see `market::Trust`.
    pub signed: bool,
    /// Whether `policy.market.requireSignature` is set. Reported separately
    /// from `signed` because the pair is the whole story: required and not
    /// happening is a refusal a user needs explained, and it is the state a
    /// managed machine is in until ADR 0015's key exists.
    pub signature_required: bool,
    /// The bundle an administrator pointed this machine at, if any. Shown so
    /// the source line does not look like a path the user chose.
    pub offline_bundle: Option<String>,
    /// Whether a policy says anything at all about the market, so the page can
    /// explain a refusal before it happens rather than after.
    pub constrained: bool,
}

/// One row of the market list: what is published, and what is here.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPackage {
    pub service: String,
    pub category: String,
    pub name: BTreeMap<String, String>,
    pub summary: BTreeMap<String, String>,
    pub capabilities: Vec<String>,
    /// Search terms the index publishes, so that `mysql` is findable by typing
    /// `database` and by typing `mariadb`.
    ///
    /// The registry has carried these since v1 and this struct dropped them, so
    /// the catalogue had no search at all — twenty-five services and a hundred
    /// versions, found by opening categories one at a time.
    pub keywords: Vec<String>,
    /// Whether two versions may run at once, so a card can say so before
    /// anything is downloaded.
    pub multiple: bool,
    pub versions: Vec<MarketVersion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketVersion {
    pub version: String,
    /// What `latest` resolves to (ADR 0014).
    pub recommended: bool,
    /// `supported`, `deprecated` or `eol`. An `eol` version is listed and
    /// installable; the picker hides it behind "show older versions" rather
    /// than withdrawing it, because somebody's workspace may name it.
    pub support: String,
    pub eol_date: Option<String>,
    pub size_bytes: Option<u64>,
    /// Whether the package is already on this machine.
    pub installed: bool,
    /// Whether an instance is using it, so the UI can refuse an uninstall
    /// before the filesystem does.
    pub in_use: bool,
}

/// One installed instance, as the front end needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceRow {
    pub id: String,
    pub service: String,
    pub version: String,
    pub enabled: bool,
    /// Holds the pre-package name, so a project's `DB_HOST=stackvo-mysql`
    /// reaches it.
    pub primary: bool,
    pub container: String,
    pub aliases: Vec<String>,
    pub ports: BTreeMap<String, u16>,
    /// Absent when the package this instance names is not installed — which is
    /// a state a user has to be able to see, not an error that hides the row.
    pub package_present: bool,
}

fn market_root(state: &State<'_, AppState>) -> Result<std::path::PathBuf> {
    state.root()
}

#[tauri::command]
pub fn market_status(state: State<'_, AppState>) -> Result<MarketStatus> {
    let root = market_root(&state)?;
    let registry = crate::market::cached(&root)?;
    let source = crate::market::remembered(&root)?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;
    let installed = crate::pkg::Catalogue::services(&tree)
        .iter()
        .map(|s| crate::pkg::Catalogue::versions(&tree, s).len())
        .sum();

    Ok(MarketStatus {
        fetched: registry.is_some(),
        sequence: registry.as_ref().map(|r| r.sequence),
        generated_at: registry.as_ref().map(|r| r.generated_at.clone()),
        expires: registry.as_ref().and_then(|r| r.expires.clone()),
        source_kind: source.as_ref().map(|s| s.kind.clone()),
        source_location: source.as_ref().map(|s| s.location.clone()),
        packages: registry.as_ref().map(|r| r.packages.len()).unwrap_or(0),
        installed,
        // `market::Trust::Signed` refuses rather than downgrades, so nothing
        // can report true here until there is a key to verify against. A
        // machine whose policy sets `requireSignature` therefore cannot
        // refresh at all, and saying so is the point: the alternative is a
        // page that looks the same on a machine where the check is off.
        signed: false,
        signature_required: crate::policy::current().market().require_signature,
        offline_bundle: crate::policy::current()
            .market()
            .offline_bundle
            .as_ref()
            .map(|p| p.display().to_string()),
        constrained: crate::policy::current().constrains_market(),
    })
}

/// Read a catalogue from a directory and cache it.
///
/// The only source this build has. `location` is a path the user chose — an
/// offline bundle, or a checkout of the packages repository — so it is treated
/// as input rather than as configuration: the source is remembered, not
/// obeyed, and `market::LocalSource` still refuses a path that walks out of it.
#[tauri::command]
pub async fn market_refresh(state: State<'_, AppState>, location: String) -> Result<MarketStatus> {
    let root = state.root()?;
    let market = crate::policy::current().market();

    // An administrator's bundle wins over the path the user picked, and that is
    // the whole of what `market.offlineBundle` does. ADR 0011 leaves nothing
    // embedded, so on an air-gapped machine this is not an enterprise extra —
    // it is the only way a catalogue ever arrives.
    // The bundle first, then the mirror, then what the user typed. An offline
    // bundle beats a `registryUrl` because a machine given one is a machine
    // that was not expected to reach anything.
    let location = market
        .offline_bundle
        .as_ref()
        .map(|bundle| bundle.display().to_string())
        .or_else(|| market.registry_url.clone())
        .unwrap_or(location);

    let reference = crate::market::SourceRef {
        kind: crate::market::kind_of(&location).to_string(),
        location,
    };
    let previous = crate::market::cached(&root)?;

    // The one policy key that is a lock rather than a note (ADR 0009): it can
    // only turn verification *on*. `Trust::Signed` refuses today rather than
    // downgrading, so a machine that sets this gets an honest failure instead
    // of an unsigned index under a name that promises otherwise.
    let trust = if market.require_signature {
        crate::market::Trust::Signed
    } else {
        crate::market::Trust::Unsigned
    };

    // Off the runtime thread, and this is a requirement rather than a courtesy.
    // `Source::fetch` is synchronous — the trait is read by `pkg` and `render`,
    // neither of which should learn what a runtime is — so `HttpSource` blocks
    // on the current handle, and doing that on a runtime thread panics.
    let moved = root.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let source = crate::market::open(&moved, &reference)?;
        crate::market::refresh(&moved, source.as_ref(), trust, previous.as_ref())?;
        crate::market::remember(&moved, &reference)
    })
    .await
    .map_err(|e| Error::new(Code::IoError, format!("the refresh could not be run: {e}")))??;

    market_status(state)
}

// ------------------------------------------------------- authoring (C-1)

/// Write a new package under this workspace's own package tree.
///
/// Under the workspace rather than at a path the caller names, and that is a
/// boundary rather than a convenience: `root` comes from the app state and the
/// rest of the path is built here from checked components, so the webview names
/// a service and a version and never a directory. The same handle-not-a-path
/// rule `applog` and `quickcmd` state as their security model.
#[tauri::command]
pub fn package_scaffold(
    state: State<'_, AppState>,
    category: String,
    service: String,
    version: String,
    image: String,
) -> Result<crate::authoring::Report> {
    let root = state.root()?;
    crate::authoring::scaffold(
        &crate::market::dir(&root),
        &category,
        &service,
        &version,
        &image,
    )
}

/// What sealing would change, and what would still be wrong afterwards.
#[tauri::command]
pub fn package_lint(
    state: State<'_, AppState>,
    category: String,
    service: String,
    version: String,
) -> Result<crate::authoring::Report> {
    let root = state.root()?;
    crate::authoring::lint(&crate::authoring::version_dir(
        &crate::market::dir(&root),
        &category,
        &service,
        &version,
    ))
}

/// Recompute the manifest's hashes after an edit, then check the package.
#[tauri::command]
pub fn package_seal(
    state: State<'_, AppState>,
    category: String,
    service: String,
    version: String,
) -> Result<crate::authoring::Report> {
    let root = state.root()?;
    crate::authoring::reseal(&crate::authoring::version_dir(
        &crate::market::dir(&root),
        &category,
        &service,
        &version,
    ))
}

#[tauri::command]
pub fn market_catalog(state: State<'_, AppState>) -> Result<Vec<MarketPackage>> {
    let root = state.root()?;
    let Some(registry) = crate::market::cached(&root)? else {
        return Ok(Vec::new());
    };
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;
    let table = crate::instances::Table::load(&root)?;

    Ok(registry
        .packages
        .iter()
        .map(|package| MarketPackage {
            service: package.service.clone(),
            category: package.category.clone(),
            name: package.name.clone(),
            summary: package.summary.clone(),
            capabilities: package.capabilities.clone(),
            keywords: package.keywords.clone(),
            multiple: package.instancing.map(|i| i.multiple).unwrap_or(false),
            versions: package
                .versions
                .iter()
                .map(|row| MarketVersion {
                    version: row.version.clone(),
                    recommended: row.recommended,
                    support: row.support.clone(),
                    eol_date: row.eol_date.clone(),
                    size_bytes: row.size_bytes,
                    installed: tree.dir(&package.service, &row.version).is_some(),
                    in_use: table
                        .instances
                        .iter()
                        .any(|i| i.service == package.service && i.version == row.version),
                })
                .collect(),
        })
        .collect())
}

#[tauri::command]
pub async fn market_install(
    state: State<'_, AppState>,
    service: String,
    version: String,
) -> Result<MarketStatus> {
    let root = state.root()?;
    let Some(registry) = crate::market::cached(&root)? else {
        return Err(
            Error::new(Code::NotFound, "no catalogue has been fetched yet")
                .with_hint(crate::hints::PACKAGE_NOT_IN_REGISTRY),
        );
    };
    let Some(reference) = crate::market::remembered(&root)? else {
        return Err(Error::new(
            Code::NotFound,
            "no source is remembered — refresh the catalogue first",
        ));
    };
    // Blocking, for the same reason the refresh is: a network source blocks on
    // the runtime handle and cannot do that from a runtime thread.
    let moved = root.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let source = crate::market::open(&moved, &reference)?;
        crate::market::install(
            &moved,
            source.as_ref(),
            &registry,
            &service,
            &version,
            crate::policy::current().market(),
        )
        .map(|_| ())
    })
    .await
    .map_err(|e| Error::new(Code::IoError, format!("the install could not be run: {e}")))??;

    market_status(state)
}

/// Remove a package. Refuses while an instance still names it.
///
/// The filesystem would allow it and `pkg::Tree` would then refuse to render
/// that instance — a service that is configured and cannot start, with the
/// reason two screens away. Saying no here is the same refusal, at the moment
/// somebody can act on it.
#[tauri::command]
pub async fn market_uninstall(
    state: State<'_, AppState>,
    service: String,
    version: String,
) -> Result<MarketStatus> {
    let root = state.root()?;
    let table = crate::instances::Table::load(&root)?;
    if let Some(instance) = table
        .instances
        .iter()
        .find(|i| i.service == service && i.version == version)
    {
        return Err(Error::new(
            Code::Conflict,
            format!(
                "{} is using this package. Remove the instance first",
                instance.id
            ),
        )
        .with_hint(crate::hints::REMOVE_THE_INSTANCE_FIRST));
    }

    let category = crate::market::cached(&root)?
        .and_then(|r| r.package(&service).map(|p| p.category.clone()))
        .ok_or_else(|| Error::not_found(format!("package {service}")))?;
    crate::market::uninstall(&root, &category, &service, &version)?;
    market_status(state)
}

/// What a candidate source turned out to be.
///
/// Every field is what a person needs to decide whether to use it, and the
/// unhappy ones are values rather than an error: "this address answers and its
/// index is older than yours" is a thing to know *before* pressing the button
/// that would be refused.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProbe {
    /// Exactly what was asked for, so the field and the answer cannot drift.
    pub location: String,
    /// Where the bytes were actually fetched from. Differs whenever a GitHub
    /// repository URL was translated, and showing it is the difference between
    /// "it works" and "it works, and here is why what you typed was not it".
    pub resolved: String,
    pub kind: String,
    pub reachable: bool,
    pub packages: usize,
    pub versions: usize,
    pub sequence: Option<u64>,
    pub generated_at: Option<String>,
    pub expires: Option<String>,
    /// The cached index's sequence, when there is one. With `sequence` this is
    /// the whole of whether a refresh would be refused.
    pub current_sequence: Option<u64>,
    /// True when this index is older than the one already here, which
    /// `market::refresh` refuses — T-6, replay.
    pub goes_backwards: bool,
    /// Why not, when `reachable` is false. Already translated by the front end
    /// through the error's own hint key.
    pub error: Option<String>,
    pub hint_key: Option<String>,
}

/// Try a source and report, without writing anything.
///
/// A separate command rather than a flag on `market_refresh`, because the two
/// differ in the one way that matters: this one **caches nothing and remembers
/// nothing**. A "test" that left the index behind would make the test and the
/// act the same button with different words on it.
///
/// It is also the only honest way to answer "is my address right", which was
/// otherwise a question you could only ask by doing the thing.
#[tauri::command]
pub async fn market_probe(state: State<'_, AppState>, location: String) -> Result<SourceProbe> {
    let root = state.root()?;
    let resolved = crate::market::resolve_location(&location);
    let kind = crate::market::kind_of(&location).to_string();
    let current = crate::market::cached(&root)?.map(|r| r.sequence);

    let reference = crate::market::SourceRef {
        kind: kind.clone(),
        location: location.clone(),
    };

    // Into a scratch directory, so a probe cannot touch the real cache even by
    // accident: `refresh` writes the index it accepted, and the version of this
    // that passed the workspace root replaced a good catalogue with a probe of
    // a bad one.
    let scratch = std::env::temp_dir().join(format!("stackvo-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);

    let outcome = {
        let scratch = scratch.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<crate::market::Registry> {
            let source = crate::market::open(&scratch, &reference)?;
            // `Trust::Unsigned` and `previous: None` on purpose. Trust is the
            // refresh's decision, and passing the cached index here would make
            // a backwards index an error instead of the fact this reports.
            crate::market::refresh(
                &scratch,
                source.as_ref(),
                crate::market::Trust::Unsigned,
                None,
            )
        })
        .await
        .map_err(|e| Error::new(Code::IoError, format!("the probe could not be run: {e}")))?
    };
    let _ = std::fs::remove_dir_all(&scratch);

    Ok(match outcome {
        Ok(registry) => SourceProbe {
            location,
            resolved,
            kind,
            reachable: true,
            packages: registry.packages.len(),
            versions: registry.packages.iter().map(|p| p.versions.len()).sum(),
            goes_backwards: current.is_some_and(|have| registry.sequence < have),
            sequence: Some(registry.sequence),
            generated_at: Some(registry.generated_at.clone()),
            expires: registry.expires.clone(),
            current_sequence: current,
            error: None,
            hint_key: None,
        },
        Err(e) => SourceProbe {
            location,
            resolved,
            kind,
            reachable: false,
            packages: 0,
            versions: 0,
            sequence: None,
            generated_at: None,
            expires: None,
            current_sequence: current,
            goes_backwards: false,
            error: Some(e.message.clone()),
            hint_key: e.hint_key.map(str::to_string),
        },
    })
}

// ---------------------------------------------------------------- handover

/// What the migration would do, or why it cannot.
///
/// The plan is computed and shown before anything is written, because the one
/// workspace this touches is one somebody is already using — `handover.rs` is
/// built as plan-then-apply for that reason, and a UI that only offered the
/// apply half would have thrown the reason away.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverPreview {
    /// `.env` has service state and no table exists yet.
    pub pending: bool,
    /// Already migrated: the table is there.
    pub migrated: bool,
    /// What the instances would be, in the order they would be written.
    pub instances: Vec<HandoverInstance>,
    /// Human-readable, already translated by the front end through `hint_key`
    /// where one applies — these carry the moving-tag resolutions and the
    /// adopted volumes, which are the two things a user should see *before*
    /// agreeing rather than in a log afterwards.
    pub notes: Vec<HandoverNote>,
    pub blockers: Vec<HandoverNote>,
    /// Whether `.env.pre-market.bak` is already on disk.
    pub backup: bool,
    /// Packages the handover needs before it can run. Empty on a workspace
    /// whose versions are all installed, which is what the happy path is.
    pub missing: Vec<MissingPackage>,
}

/// A package the handover needs and this machine does not have.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPackage {
    pub service: String,
    pub version: String,
    /// Whether the cached index offers it, which decides whether the UI can
    /// offer a button or only an explanation.
    pub installable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverInstance {
    pub id: String,
    pub service: String,
    pub version: String,
    pub ports: BTreeMap<String, u16>,
    pub volumes: BTreeMap<String, String>,
}

/// One line of the preview: a machine-readable `kind` and the subject it is
/// about, so the front end translates rather than parses.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverNote {
    pub kind: String,
    pub subject: String,
    pub detail: String,
}

fn preview_of(root: &std::path::Path) -> Result<HandoverPreview> {
    let migrated = crate::instances::path(root).exists();
    let env = crate::config::Env::load(root)?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(root))?;
    let pending = crate::handover::is_pending(root, &env, &tree);

    // A workspace that has already migrated has nothing to plan, and planning
    // it anyway is not merely wasted work — it is **wrong output**. The plan
    // reads `.env`, whose service keys are deliberately left behind as a record
    // (marked, never deleted), so it happily produces blockers about versions
    // this workspace stopped using the moment the table was written. The panel
    // upstream shows on `blockers.length > 0`, so a migrated machine was told
    // it "still keeps its services in .env" — while the Services page was
    // reading the table and the containers were running from it.
    //
    // `handover_apply` already refuses in this state. This is the same refusal
    // moved to where it is read rather than where it is acted on.
    if migrated {
        return Ok(HandoverPreview {
            pending: false,
            migrated: true,
            instances: Vec::new(),
            notes: Vec::new(),
            blockers: Vec::new(),
            backup: crate::handover::backup_path(root).exists(),
            missing: Vec::new(),
        });
    }

    // The timestamp the plan stamps on each package reference. Real, because it
    // is recorded as when this workspace adopted the package.
    let now = crate::snapshot::now_rfc3339();
    let plan = crate::handover::plan(root, &env, &tree, &crate::ports::is_free, &now);

    let notes = plan
        .notes
        .iter()
        .map(|note| match note {
            crate::handover::Note::ResolvedMovingTag { service, from, to } => HandoverNote {
                kind: "resolvedMovingTag".into(),
                subject: service.clone(),
                detail: format!("{from} → {to}"),
            },
            crate::handover::Note::PortMoved {
                instance,
                port,
                from,
                to,
            } => HandoverNote {
                kind: "portMoved".into(),
                subject: instance.clone(),
                detail: format!("{port}: {from} → {to}"),
            },
            crate::handover::Note::AdoptedVolume { instance, volume } => HandoverNote {
                kind: "adoptedVolume".into(),
                subject: instance.clone(),
                detail: volume.clone(),
            },
            crate::handover::Note::SettingHasNoHome { service, key } => HandoverNote {
                kind: "settingHasNoHome".into(),
                subject: service.clone(),
                detail: key.clone(),
            },
        })
        .collect();

    let blockers = plan
        .blockers
        .iter()
        .map(|blocker| match blocker {
            crate::handover::Blocker::UnknownService { service } => HandoverNote {
                kind: "unknownService".into(),
                subject: service.clone(),
                detail: String::new(),
            },
            crate::handover::Blocker::VersionNotInstalled {
                service,
                version,
                available,
            } => HandoverNote {
                kind: "versionNotInstalled".into(),
                subject: format!("{service}@{version}"),
                detail: available.join(", "),
            },
            crate::handover::Blocker::NothingToInstall { service } => HandoverNote {
                kind: "nothingToInstall".into(),
                subject: service.clone(),
                detail: String::new(),
            },
            crate::handover::Blocker::NoFreePort { instance, port } => HandoverNote {
                kind: "noFreePort".into(),
                subject: instance.clone(),
                detail: port.clone(),
            },
        })
        .collect();

    // What would unblock this, as data rather than as a sentence to read.
    //
    // The blocker above is the truthful statement of the problem; this is the
    // route out of it. Every version `.env` names has to be installed before
    // the table can point at it, and on a workspace that has never opened the
    // Market that is *every* version — so a preview that only refused was a
    // dead end with the answer one page away and unnamed.
    //
    // `installable` is the difference between "press this" and something else
    // entirely: the registry either publishes that version or it does not, and
    // ADR 0014 makes the second case a mistake somebody made rather than a
    // withdrawal.
    let registry = crate::market::cached(root)?;
    let missing: Vec<MissingPackage> = plan
        .blockers
        .iter()
        .filter_map(|blocker| match blocker {
            crate::handover::Blocker::VersionNotInstalled {
                service, version, ..
            } => Some((service.clone(), version.clone())),
            _ => None,
        })
        .map(|(service, version)| MissingPackage {
            installable: registry
                .as_ref()
                .is_some_and(|r| r.version(&service, &version).is_some()),
            service,
            version,
        })
        .collect();

    Ok(HandoverPreview {
        missing,
        pending,
        migrated,
        instances: plan
            .instances
            .iter()
            .map(|i| HandoverInstance {
                id: i.id.clone(),
                service: i.service.clone(),
                version: i.version.clone(),
                ports: i.ports.clone(),
                volumes: i.volumes.clone(),
            })
            .collect(),
        notes,
        blockers,
        backup: crate::handover::backup_path(root).exists(),
    })
}

#[tauri::command]
pub fn handover_preview(state: State<'_, AppState>) -> Result<HandoverPreview> {
    preview_of(&state.root()?)
}

/// Write the table, after backing `.env` up.
///
/// Recomputes the plan rather than taking one from the front end. A plan is a
/// decision about ports and volumes made against the machine as it was when it
/// was computed, and the machine moves — accepting one over IPC would let a
/// stale preview claim a port something else has since taken.
#[tauri::command]
pub async fn handover_apply(state: State<'_, AppState>) -> Result<HandoverPreview> {
    let root = state.root()?;

    if crate::instances::path(&root).exists() {
        return Err(Error::new(
            Code::Conflict,
            "this workspace has already been handed over. A second run would adopt the same \
             volumes into a table that already claims them",
        ));
    }

    let env = crate::config::Env::load(&root)?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;
    let now = crate::snapshot::now_rfc3339();
    let plan = crate::handover::plan(&root, &env, &tree, &crate::ports::is_free, &now);

    let count = plan.instances.len();
    crate::handover::apply(&root, &plan)?;
    crate::audit::record(
        "handover_apply",
        format!("{count} instance(s) carried over from .env"),
        crate::audit::Outcome::Ok,
    );

    preview_of(&root)
}

// ---------------------------------------------------------------- instances

#[tauri::command]
pub fn instance_list(state: State<'_, AppState>) -> Result<Vec<InstanceRow>> {
    let root = state.root()?;
    let table = crate::instances::Table::load(&root)?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;

    Ok(table
        .instances
        .iter()
        .map(|instance| InstanceRow {
            id: instance.id.clone(),
            service: instance.service.clone(),
            version: instance.version.clone(),
            enabled: instance.enabled,
            primary: instance.primary,
            container: instance.container(),
            aliases: instance.aliases(),
            ports: instance.ports.clone(),
            package_present: tree.dir(&instance.service, &instance.version).is_some(),
        })
        .collect())
}

/// What creating an instance of this package would produce, before it does.
///
/// The form that reads this is the answer to the worst trap in the app. An
/// image reads `MYSQL_ROOT_PASSWORD` only while its data directory is empty, so
/// the one moment a password can be set is *before* the first boot — and until
/// this existed the only route was create-with-defaults and then edit, which
/// rebuilds the container, reports success, and leaves the database on `root`.
///
/// Nothing is written and nothing is reserved. The ports here are what the
/// allocator would choose right now; by the time the user presses Create,
/// something else may hold one, and `instance_create` allocates again for real.
/// Showing a number that might move is still worth it — it is the number in
/// nearly every case, and a form that showed no ports would be one where the
/// first sight of them is in a table afterwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePlan {
    /// The id this would take — `mysql-8-0`.
    pub id: String,
    /// True when the service already has an instance and forbids a second.
    /// Reported rather than refused: the dialog can say why the button is off,
    /// which a thrown error on open cannot.
    pub refused: Option<String>,
    pub settings: Vec<InstanceSetting>,
    pub ports: Vec<DeclaredPort>,
}

#[tauri::command]
pub fn instance_plan(
    state: State<'_, AppState>,
    service: String,
    version: String,
) -> Result<InstancePlan> {
    let root = state.root()?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;
    let manifest = tree.load(&service, &version)?;
    let table = crate::instances::Table::load(&root)?;

    let refused = (table.of_service(&service).count() > 0 && !manifest.instancing.multiple)
        .then(|| format!("{service} declares that only one version may run at a time"));

    let reserved = table.reserved_ports();
    let mut claims = crate::ports::Claims::default();
    let ports = manifest
        .ports
        .iter()
        .map(|port| {
            Ok(DeclaredPort {
                name: port.name.clone(),
                container: port.container,
                // `ok()` rather than `?`: a machine with nothing free near the
                // preferred number should open a form saying so per port, not
                // fail to open at all.
                host: crate::ports::allocate(
                    port.preferred,
                    &reserved,
                    &mut claims,
                    &crate::ports::is_free,
                )
                .ok(),
                protocol: port.protocol.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(InstancePlan {
        id: crate::instances::slug(&service, &version)?,
        refused,
        ports,
        settings: manifest
            .settings
            .iter()
            .map(|setting| InstanceSetting {
                key: setting.key.clone(),
                kind: setting.kind.clone(),
                // Not masked, unlike every other reading of a secret in this
                // app, and the difference is what the value *is*. There is no
                // instance yet and no keystore entry: this is the manifest's
                // published first-boot default, sitting in a JSON file on
                // disk that anybody with the package can read. Masking it
                // would be theatre — and this form exists precisely so that
                // `root` is changed before it ever becomes a real credential.
                value: setting.default_text().unwrap_or_default(),
                secret: setting.is_secret(),
                is_default: true,
                default_value: setting.default_text(),
                required: setting.required,
                options: if setting.kind == "instanceRef" {
                    instances_providing(&table, &tree, setting.capability.as_deref())
                } else {
                    setting.options.clone()
                },
                label: setting.label.clone(),
            })
            .collect(),
    })
}

/// Create an instance of an installed package.
///
/// Ports are allocated here and written down, not recomputed per render: a
/// connection string that changes because an unrelated service was installed is
/// a string somebody had already pasted somewhere.
///
/// `settings` and `ports` are what the create form collected, and both are
/// optional — creating with the package's own defaults is still one call with
/// two nulls. A supplied secret goes into the keystore *before* the container
/// has ever run, which is the only moment an image will read it.
#[tauri::command]
pub async fn instance_create(
    state: State<'_, AppState>,
    service: String,
    version: String,
    settings: Option<BTreeMap<String, String>>,
    ports: Option<BTreeMap<String, u16>>,
) -> Result<String> {
    let root = state.root()?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;
    let manifest = tree.load(&service, &version)?;

    let mut table = crate::instances::Table::load(&root)?;
    let existing = table.of_service(&service).count();
    if existing > 0 && !manifest.instancing.multiple {
        return Err(Error::new(
            Code::Conflict,
            format!("{service} declares that only one version may run at a time"),
        )
        .with_hint(crate::hints::SERVICE_IS_SINGLE_INSTANCE));
    }

    let id = crate::instances::slug(&service, &version)?;
    let chosen = settings.unwrap_or_default();
    check_instance_patch(&id, &manifest.settings, &chosen)?;

    let requested = ports.unwrap_or_default();
    for handle in requested.keys() {
        if !manifest.ports.iter().any(|p| &p.name == handle) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{handle}\" is not a port of {service}@{version}"),
            ));
        }
    }

    let reserved = table.reserved_ports();
    let mut claims = crate::ports::Claims::default();
    let mut ports = BTreeMap::new();
    for port in &manifest.ports {
        let chosen_port = match requested.get(&port.name) {
            // A number the user typed is used or refused, never moved. The
            // allocator's job is to find *a* free port; asking for 3307 and
            // being handed 3407 without a word is a different thing, and the
            // person who typed it has a reason.
            Some(&wanted) => {
                if wanted == 0 {
                    return Err(Error::new(
                        Code::InvalidInput,
                        "port 0 asks the kernel to choose, which cannot be written down",
                    ));
                }
                if reserved.contains(&wanted) || claims.pending.contains(&wanted) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!("port {wanted} is already held by another instance"),
                    )
                    .with_hint(crate::hints::PORT_HELD_BY_INSTANCE));
                }
                if !crate::ports::is_free(wanted) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!("port {wanted} is in use on this machine"),
                    )
                    .with_hint(crate::hints::PORT_IN_USE));
                }
                claims.pending.insert(wanted);
                wanted
            }
            None => crate::ports::allocate(
                port.preferred,
                &reserved,
                &mut claims,
                &crate::ports::is_free,
            )?,
        };
        ports.insert(port.name.clone(), chosen_port);
    }

    let mut settings = BTreeMap::new();
    let mut secret_refs = BTreeMap::new();
    for setting in &manifest.settings {
        if setting.is_secret() {
            // `secrets::reference_for`, not a formatted string: it appends a
            // digest of the workspace path so two checkouts on one machine do
            // not share a keychain entry.
            let reference = crate::secrets::reference_for(&format!("{id}/{}", setting.key), &root);

            // A password chosen on the form goes into the store now, before the
            // container has ever run. That is the whole point of the form: an
            // image reads its root password while the data directory is empty
            // and never again, so this is the one moment it can be set.
            if let Some(value) = chosen.get(&setting.key) {
                if let Some(entry) = crate::secrets::entry_of(&reference) {
                    crate::secrets::write(entry, value)?;
                }
            }
            secret_refs.insert(setting.key.clone(), reference);
        } else if let Some(value) = chosen
            .get(&setting.key)
            .cloned()
            .or_else(|| setting.default_text())
        {
            settings.insert(setting.key.clone(), value);
        }
    }

    table.insert(crate::instances::Instance {
        id: id.clone(),
        service,
        version,
        package: crate::instances::PackageRef {
            source: "local".into(),
            sha256: crate::pkg::sha256_hex(
                serde_json::to_string(&manifest)
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            installed_at: crate::snapshot::now_rfc3339(),
        },
        // Off until somebody asks for it: creating an instance is not the same
        // decision as starting one, and the second belongs to a command that
        // can also bring the container up.
        enabled: false,
        // The first instance of a service takes the pre-package name, because
        // in a workspace with one version that name has to reach something.
        primary: existing == 0,
        ports,
        volumes: BTreeMap::new(),
        settings,
        secret_refs,
    })?;
    table.save(&root)?;
    Ok(id)
}

/// Forget an instance. Its volumes are not touched (ADR 0012).
#[tauri::command]
pub async fn instance_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    let root = state.root()?;
    let mut table = crate::instances::Table::load(&root)?;
    let removed = table.remove(&id)?;

    // A service left with instances and no primary is one whose pre-package
    // name resolves to nothing, and every project pointing at it breaks. The
    // oldest survivor takes it, which is the one most likely to be the adopted
    // one.
    if removed.primary {
        let next = table
            .of_service(&removed.service)
            .map(|i| i.id.clone())
            .next();
        if let Some(next) = next {
            table.promote(&next)?;
        }
    }
    table.save(&root)
}

/// Look an instance up, or say which one is missing.
fn instance_of(root: &std::path::Path, id: &str) -> Result<crate::instances::Instance> {
    crate::instances::Table::load(root)?
        .get(id)
        .cloned()
        .ok_or_else(|| Error::not_found(format!("instance {id}")))
}

/// The manifest an instance was created from, or a refusal naming the package.
///
/// An instance can outlive its package files — `instance_list` reports that as
/// `packagePresent: false` rather than hiding the row. Every settings call goes
/// through here so that state produces one sentence about the package instead
/// of an empty form that looks like a service with nothing to configure.
fn instance_manifest(
    root: &std::path::Path,
    instance: &crate::instances::Instance,
) -> Result<crate::pkg::Manifest> {
    crate::pkg::Tree::open(&crate::market::dir(root))?.load(&instance.service, &instance.version)
}

/// The value in force for one setting: what was stored, then the default.
///
/// The same order `render::context` resolves in, and it has to stay the same
/// order. A form that showed one value while the compose file rendered another
/// would be a form about nothing.
fn setting_value(
    instance: &crate::instances::Instance,
    setting: &crate::pkg::Setting,
) -> Option<String> {
    if setting.is_secret() {
        return instance
            .secret_refs
            .get(&setting.key)
            .and_then(|reference| crate::secrets::entry_of(reference))
            .and_then(|entry| crate::secrets::read(entry).ok().flatten());
    }
    instance.settings.get(&setting.key).cloned()
}

/// Every setting one instance's manifest declares, with the value in force.
///
/// Order is the manifest's, not sorted: a package author who put the database
/// name above the password meant that, and an alphabetical form would put
/// `DATABASE` under `BASEURL` for no reason a reader could see.
#[tauri::command]
pub fn instance_settings(state: State<'_, AppState>, id: String) -> Result<Vec<InstanceSetting>> {
    let root = state.root()?;
    let instance = instance_of(&root, &id)?;
    let manifest = instance_manifest(&root, &instance)?;
    let table = crate::instances::Table::load(&root)?;
    let tree = crate::pkg::Tree::open(&crate::market::dir(&root))?;

    Ok(manifest
        .settings
        .iter()
        .map(|setting| {
            let stored = setting_value(&instance, setting);
            let default = setting.default_text();
            InstanceSetting {
                key: setting.key.clone(),
                kind: setting.kind.clone(),
                // Masked without consulting the keystore: the mask is the same
                // eight bullets whether the entry holds a password or has never
                // been written, and a form that showed a shorter mask for an
                // unset secret would be leaking its length.
                value: if setting.is_secret() {
                    crate::config::MASK.to_string()
                } else {
                    stored
                        .clone()
                        .or_else(|| default.clone())
                        .unwrap_or_default()
                },
                secret: setting.is_secret(),
                // Nothing stored means the default is what runs. Stored *equal*
                // to the default counts too — the value in force is the same
                // thing, and the chip is about the value, not about whether
                // somebody once typed it.
                is_default: stored.is_none() || stored == default,
                default_value: if setting.is_secret() {
                    None
                } else {
                    default.clone()
                },
                required: setting.required,
                // The manifest's list, except for an `instanceRef`, whose list
                // is a question about this machine rather than about the
                // package: which installed instance can answer the capability
                // it names. Filled here so the form needs to know nothing about
                // it — a row that carries options already renders a combobox,
                // and this is that, with the candidates in it.
                //
                // A combobox and not a select, on the same terms as everything
                // else on the form: an instance the app has not heard of is
                // still a name somebody may need to type.
                options: if setting.kind == "instanceRef" {
                    instances_providing(&table, &tree, setting.capability.as_deref())
                } else {
                    setting.options.clone()
                },
                label: setting.label.clone(),
            }
        })
        .collect())
}

/// Which installed instances can answer a capability, for an `instanceRef`.
///
/// Every instance when the setting names no capability: the manifest is saying
/// "point me at another instance" and nothing narrower, so narrowing it here
/// would be this side inventing a constraint the package did not state.
fn instances_providing(
    table: &crate::instances::Table,
    tree: &crate::pkg::Tree,
    capability: Option<&str>,
) -> Vec<String> {
    table
        .instances
        .iter()
        .filter(|instance| match capability {
            // `primary` is the documented special value: it means whichever
            // instance currently holds the legacy alias, so it is offered as
            // itself rather than resolved to a name that moves when somebody
            // presses the star button on another page.
            None | Some("primary") => true,
            Some(wanted) => tree
                .load(&instance.service, &instance.version)
                .map(|m| m.capabilities.iter().any(|c| c == wanted))
                .unwrap_or(false),
        })
        .map(|instance| instance.id.clone())
        .collect()
}

/// The real value behind a masked secret.
///
/// The keystore when it holds one, the manifest's default otherwise — because
/// that is what the container is running with, and a reveal that showed an
/// empty box for a service reachable with `root` would be a lie of omission.
#[tauri::command]
pub fn instance_reveal(state: State<'_, AppState>, id: String, key: String) -> Result<String> {
    let root = state.root()?;
    let instance = instance_of(&root, &id)?;
    let manifest = instance_manifest(&root, &instance)?;

    let setting = manifest
        .settings
        .iter()
        .find(|s| s.key == key)
        .ok_or_else(|| Error::not_found(format!("setting {key} of instance {id}")))?;

    if !setting.is_secret() {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{key} is not a secret — its value is already on the form"),
        ));
    }

    Ok(setting_value(&instance, setting)
        .or_else(|| setting.default_text())
        .unwrap_or_default())
}

/// Is every key in `patch` one this instance's manifest declares, and is every
/// value one that can safely be written?
///
/// Two separate refusals, both of them things a UI can do by accident.
///
/// The first keeps this from being a general writer into `instances.json` that
/// happens to restart a container. It is reached from a sheet whose whole
/// framing is "these are this instance's settings", and it should mean that.
/// `enabled`, `primary`, `ports` and `version` are not settings and are not
/// reachable here — each has its own command, and two controls for one field is
/// how they come to disagree.
///
/// The second is the sharper one. A read returns the bullet string for a
/// secret, so a form that round-trips what it was given would save the mask as
/// the password and lock the service out of its own database.
///
/// The third is `required`. A manifest that marks a setting required means the
/// service cannot start without it, and emptying such a field used to be
/// written, committed and applied — the container was recreated and failed to
/// boot, and the form that caused it reported success. Checked here rather than
/// only in the sheet because the sheet is one caller of a command that writes to
/// disk and restarts a container.
fn check_instance_patch(
    id: &str,
    settings: &[crate::pkg::Setting],
    patch: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    for (key, value) in patch {
        let Some(setting) = settings.iter().find(|s| &s.key == key) else {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{key}\" is not a setting of instance \"{id}\""),
            ));
        };
        if value == crate::config::MASK {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{key}\" would be saved as its own mask"),
            )
            .with_hint(crate::hints::REVEAL_VALUE_FIRST));
        }
        // Trimmed, because a required field holding one space satisfies every
        // check that asks whether it is empty and none of the ones the service
        // makes of it.
        if setting.required && value.trim().is_empty() {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{key}\" is required by {id} and cannot be emptied"),
            )
            .with_hint(crate::hints::SETTING_IS_REQUIRED));
        }
    }
    Ok(())
}

/// What the ports would be after applying `patch`, if they can be.
///
/// Until this existed there was no way to change an allocated port at all.
/// `instance_create` picks one, moves on when the preferred number is taken,
/// and nothing afterwards could move it — so somebody whose 3306 had been given
/// to something else had to edit `instances.json` by hand. That is a real
/// regression on the `.env` model, where `HOST_PORT_MYSQL` was a line in a file
/// the app already edited.
///
/// Three refusals, and they are not interchangeable:
///
/// - a handle the manifest does not declare, for the same reason
///   `check_instance_patch` refuses an undeclared key: this writes to
///   `instances.json` and restarts a container, and it should only be able to
///   write the things it is named after.
/// - a number another instance holds. The table is the record of that, and two
///   instances claiming one port is a compose file that fails to come up with a
///   message about neither of them.
/// - a number the machine is already using. `probe` is a parameter for the same
///   reason `ports::allocate` takes one: a test that binds real sockets fails on
///   whichever CI machine happens to be running something.
///
/// Ports this instance already holds are exempt from the last check, and have
/// to be: its own container is bound to them, so probing would report the
/// service's own port as taken and refuse a patch that does not touch it.
fn planned_ports(
    id: &str,
    manifest: &crate::pkg::Manifest,
    instance: &crate::instances::Instance,
    table: &crate::instances::Table,
    patch: &std::collections::BTreeMap<String, u16>,
    probe: &dyn Fn(u16) -> bool,
) -> Result<std::collections::BTreeMap<String, u16>> {
    let mine: std::collections::BTreeSet<u16> = instance.ports.values().copied().collect();
    let others: std::collections::BTreeSet<u16> = table
        .instances
        .iter()
        .filter(|other| other.id != id)
        .flat_map(|other| other.ports.values().copied())
        .collect();

    let mut planned = instance.ports.clone();
    for (handle, port) in patch {
        if !manifest.ports.iter().any(|p| &p.name == handle) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{handle}\" is not a port of instance \"{id}\""),
            ));
        }
        if *port == 0 {
            return Err(Error::new(
                Code::InvalidInput,
                "port 0 asks the kernel to choose, which cannot be written down",
            ));
        }
        if others.contains(port) {
            return Err(Error::new(
                Code::Conflict,
                format!("port {port} is already held by another instance"),
            )
            .with_hint(crate::hints::PORT_HELD_BY_INSTANCE));
        }
        // Unchanged is not a move, so it is not probed: this instance's own
        // container is bound to it, and asking the kernel would say "taken".
        if !mine.contains(port) && !probe(*port) {
            return Err(Error::new(
                Code::Conflict,
                format!("port {port} is in use on this machine"),
            )
            .with_hint(crate::hints::PORT_IN_USE));
        }
        planned.insert(handle.clone(), *port);
    }

    // Two handles on one number. Reachable by patching `console` to whatever
    // `main` already holds, which passes every check above — `main` is this
    // instance's own port, so it is neither another instance's nor probed.
    let mut seen = std::collections::BTreeSet::new();
    for (handle, port) in &planned {
        if !seen.insert(*port) {
            return Err(Error::new(
                Code::Conflict,
                format!("port {port} would be published twice, once as \"{handle}\""),
            ));
        }
    }

    Ok(planned)
}

/// Write an instance's settings and rebuild its container with them.
///
/// The rebuild is the point. `instance_restart` restarts the container that is
/// already there, which keeps the environment it was created with — so a
/// setting saved and then "restarted" appears to have been applied and has not.
/// This regenerates the compose file and forces a recreate, which is the only
/// sequence where the new value actually reaches the process.
///
/// Ports come through here rather than through a command of their own, and that
/// is a deliberate exception to the rule stated on `check_instance_patch`. The
/// rule is about two *controls* for one field; this is one control, on one
/// sheet, behind one confirmation. Split into two commands, a user who changed
/// a password and a port would have their container stopped and recreated
/// twice, and the second failure would land on a container the first had
/// already rebuilt.
#[tauri::command]
pub async fn instance_apply_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: std::collections::BTreeMap<String, String>,
    ports: Option<std::collections::BTreeMap<String, u16>>,
) -> Result<String> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    let root = state.root()?;
    let instance = instance_of(&root, &id)?;
    let manifest = instance_manifest(&root, &instance)?;
    check_instance_patch(&id, &manifest.settings, &patch)?;

    // Planned before anything is written, so a refused port does not leave the
    // settings half applied.
    let ports = match ports.filter(|p| !p.is_empty()) {
        Some(patch) => Some(planned_ports(
            &id,
            &manifest,
            &instance,
            &crate::instances::Table::load(&root)?,
            &patch,
            &crate::ports::is_free,
        )?),
        None => None,
    };

    let operation_id = events::next_operation_id("instance-settings");
    events::emit(&app, "instance:enabling", SubjectEvent::service(&id));

    let outcome = async {
        let mut table = crate::instances::Table::load(&root)?;
        let row = table
            .instances
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| Error::not_found(format!("instance {id}")))?;

        for (key, value) in &patch {
            let setting = manifest
                .settings
                .iter()
                .find(|s| &s.key == key)
                .expect("check_instance_patch accepted this key");

            if !setting.is_secret() {
                row.settings.insert(key.clone(), value.clone());
                continue;
            }

            // An adopted instance can reach here with no reference: the
            // handover carries a service over without inventing keystore
            // entries for settings nobody has changed yet.
            let reference = row
                .secret_refs
                .entry(key.clone())
                .or_insert_with(|| crate::secrets::reference_for(&format!("{id}/{key}"), &root))
                .clone();
            let entry = crate::secrets::entry_of(&reference).ok_or_else(|| {
                Error::new(
                    Code::InvalidInput,
                    format!("{key} has a keystore reference that names no entry: {reference}"),
                )
            })?;

            // The value into the keystore before the table records where it
            // went — the same order `secret_move` writes in, for the same
            // reason. The other way round points the table at an entry that
            // failed to take the password, and the fallback quietly serves the
            // manifest default in its place.
            crate::secrets::write(entry, value)?;
        }

        if let Some(planned) = &ports {
            row.ports = planned.clone();
        }
        table.save(&root)?;

        generate(&app, &root, &operation_id, "projects_and_services").await?;

        let mut args = runner::compose_base_args(&root);
        args.extend(runner::profile_args("custom", std::slice::from_ref(&id))?);
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
                subject: &id,
                progress_event: "instance:progress",
                finished_event: "instance:enabled",
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
            "instance:error",
            SubjectEvent::service(&id).error(e.message.clone()),
        );
    }
    outcome.map(|_| operation_id)
}

#[tauri::command]
pub async fn instance_start(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    instance_of(&state.root()?, &id)?;
    lifecycle(&events::sink(&app), "instance", &id, events::START).await
}

#[tauri::command]
pub async fn instance_stop(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    instance_of(&state.root()?, &id)?;
    lifecycle(&events::sink(&app), "instance", &id, events::STOP).await
}

#[tauri::command]
pub async fn instance_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    instance_of(&state.root()?, &id)?;
    lifecycle(&events::sink(&app), "instance", &id, events::RESTART).await
}

/// Switch an instance on: write it down, regenerate, bring its profile up.
///
/// The order is the same one `service_enable` uses and for the same reason —
/// the compose file has to describe the container before compose is asked to
/// start it.
#[tauri::command]
pub async fn instance_enable(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    let root = state.root()?;
    let instance = instance_of(&root, &id)?;
    let operation_id = events::next_operation_id("enable");

    events::emit(&app, "instance:enabling", SubjectEvent::service(&id));

    let outcome = async {
        let mut table = crate::instances::Table::load(&root)?;
        if let Some(row) = table.instances.iter_mut().find(|i| i.id == id) {
            row.enabled = true;
        }
        table.save(&root)?;

        generate(&app, &root, &operation_id, "projects_and_services").await?;

        let mut args = runner::compose_base_args(&root);
        args.extend(runner::profile_args("custom", std::slice::from_ref(&id))?);
        args.extend(["up".into(), "-d".into(), "--no-build".into()]);

        runner::run_operation(
            &events::sink(&app),
            runner::Operation {
                operation_id: &operation_id,
                subject: &id,
                progress_event: "instance:progress",
                finished_event: "instance:enabled",
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
            "instance:error",
            SubjectEvent::service(&id).error(e.message.clone()),
        );
    }
    // The name has to resolve while the service is on and stop resolving when
    // it is not. Keyed on the SERVICE rather than the instance because a
    // service with a domain declares `instancing.multiple: false` — there is
    // exactly one instance of it, and per-instance subdomains are a separate,
    // smaller job.
    if outcome.is_ok() {
        if let Err(e) = sync_service_host(&root, &instance.service, true).await {
            tracing::warn!(instance = %id, error = %e.message, "hosts entry not updated");
        }
    }

    outcome.map(|_| operation_id)
}

/// Switch an instance off. **Nothing is deleted** (ADR 0012).
///
/// `service_disable` removes the container, the image and the named volumes,
/// and in a single-version world that was right: "off" should be a state rather
/// than a label. It stops being right per version. Somebody switching MySQL 8.0
/// off to try 9.4 wants 8.0's rows when they switch back, and a disable that
/// took the datadir with it would be the most expensive way to learn that.
///
/// Deleting now lives behind `instance_remove` and `market_uninstall`, where
/// the word on the button matches what happens.
#[tauri::command]
pub async fn instance_disable(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String> {
    let _busy = state.inflight.acquire(format!("instance:{id}"))?;
    let root = state.root()?;
    let instance = instance_of(&root, &id)?;
    let operation_id = events::next_operation_id("disable");

    events::emit(&app, "instance:disabling", SubjectEvent::service(&id));

    let outcome = async {
        // The hosts entry first, and it is allowed to fail the whole thing: it
        // needs a password, and a cancelled prompt must leave everything intact
        // rather than half-undone.
        sync_service_host(&root, &instance.service, false).await?;

        // Stop and remove the container — but only the container. The next
        // regenerate writes it out of the compose file, so leaving it running
        // would make it nobody's responsibility while it still held its name.
        let _ = engine::stop_container(&id).await;
        let _ = engine::remove_container(&id).await;

        let mut table = crate::instances::Table::load(&root)?;
        if let Some(row) = table.instances.iter_mut().find(|i| i.id == id) {
            row.enabled = false;
        }
        table.save(&root)?;

        generate(&app, &root, &operation_id, "projects_and_services").await
    }
    .await;

    match &outcome {
        Ok(()) => events::emit(
            &app,
            "instance:disabled",
            SubjectEvent::service(&id).running(false),
        ),
        Err(e) => events::emit(
            &app,
            "instance:error",
            SubjectEvent::service(&id).error(e.message.clone()),
        ),
    }

    outcome.map(|_| operation_id)
}

/// Move the pre-package name to another instance of the same service.
#[tauri::command]
pub async fn instance_promote(state: State<'_, AppState>, id: String) -> Result<()> {
    let root = state.root()?;
    let mut table = crate::instances::Table::load(&root)?;
    table.promote(&id)?;
    table.save(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn an_instance_patch_cannot_reach_past_its_own_manifest() {
        let setting = |key: &str, kind: &str| crate::pkg::Setting {
            key: key.into(),
            kind: kind.into(),
            default: None,
            required: false,
            options: Vec::new(),
            capability: None,
            label: std::collections::BTreeMap::new(),
        };
        let declared = [
            setting("DATABASE", "string"),
            setting("ROOT_PASSWORD", "secret"),
        ];
        let patch = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };

        assert!(
            check_instance_patch("mysql-8-0", &declared, &patch(&[("DATABASE", "shop")])).is_ok()
        );

        // The fields that are not settings. Each has its own command, and a
        // sheet that could write them here would be a second control for a
        // state something else owns — `primary` above all, which decides which
        // instance answers to the bare service name.
        for key in [
            "enabled",
            "primary",
            "version",
            "ports",
            "SERVICE_MYSQL_DATABASE",
        ] {
            assert!(
                check_instance_patch("mysql-8-0", &declared, &patch(&[(key, "x")])).is_err(),
                "{key} should be refused"
            );
        }

        // The read masks secrets, so a form that returns what it was handed
        // would save the mask as the password and lock MySQL out of its own
        // database.
        assert!(check_instance_patch(
            "mysql-8-0",
            &declared,
            &patch(&[("ROOT_PASSWORD", crate::config::MASK)])
        )
        .is_err());

        // A setting the package says the service cannot start without. No
        // package in the catalogue marks one today, which is precisely why
        // this is pinned here: emptying it used to be written, committed and
        // applied, and the container came back up refusing to boot while the
        // form that did it reported success.
        let mut needed = setting("MASTER_KEY", "string");
        needed.required = true;
        let with_required = [needed];

        assert!(check_instance_patch(
            "meilisearch-1-0",
            &with_required,
            &patch(&[("MASTER_KEY", "")])
        )
        .is_err());
        // Trimmed: one space satisfies every check that asks whether a string
        // is empty and none of the ones the service makes of it.
        assert!(check_instance_patch(
            "meilisearch-1-0",
            &with_required,
            &patch(&[("MASTER_KEY", "   ")])
        )
        .is_err());
        assert!(check_instance_patch(
            "meilisearch-1-0",
            &with_required,
            &patch(&[("MASTER_KEY", "a-real-key")])
        )
        .is_ok());

        // And an optional one is still allowed to be emptied — the refusal is
        // about what the manifest declared, not about empty strings.
        assert!(check_instance_patch("mysql-8-0", &declared, &patch(&[("DATABASE", "")])).is_ok());
    }

    /// Changing a port, which until now could not be done from the app at all:
    /// `instance_create` allocated one and nothing afterwards could move it, so
    /// a taken 3306 meant editing `instances.json` by hand.
    #[test]
    fn a_port_may_move_but_not_onto_one_that_is_taken() {
        // Built through serde rather than by naming every field: these are the
        // shapes the boundary actually parses, and a literal would have to be
        // updated by hand every time one of them grows a field.
        let instance = |id: &str, ports: serde_json::Value| -> crate::instances::Instance {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "service": id.split('-').next().unwrap(),
                "version": "1",
                "package": { "source": "official", "sha256": "", "installedAt": "" },
                "enabled": true,
                "primary": true,
                "ports": ports,
            }))
            .unwrap()
        };

        let manifest: crate::pkg::Manifest = serde_json::from_value(serde_json::json!({
            "apiVersion": "stackvo.dev/package/v1",
            "service": "minio",
            "version": "1",
            "image": { "repository": "minio/minio", "tag": "1" },
            "instancing": { "multiple": true },
            "ports": [
                { "name": "main", "container": 9000, "preferred": 9000, "primary": true },
                { "name": "console", "container": 9001, "preferred": 9001 },
            ],
            "compose": { "file": "compose.yml.tpl", "sha256": "0".repeat(64) },
            "support": { "status": "supported" },
        }))
        .unwrap();

        let mine = instance(
            "minio-1",
            serde_json::json!({ "main": 9000, "console": 9001 }),
        );
        let table = crate::instances::Table {
            instances: vec![
                mine.clone(),
                instance("mysql-8-0", serde_json::json!({ "main": 3306 })),
            ],
            ..Default::default()
        };
        let patch = |pairs: &[(&str, u16)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<BTreeMap<_, _>>()
        };
        let free = |_: u16| true;
        let taken = |_: u16| false;
        let plan = |patch: &BTreeMap<String, u16>, probe: &dyn Fn(u16) -> bool| {
            planned_ports("minio-1", &manifest, &mine, &table, patch, probe)
        };

        // The ordinary move: a free number, and the untouched handle keeps what
        // it had rather than being dropped from the map.
        let moved = plan(&patch(&[("main", 9500)]), &free).unwrap();
        assert_eq!(moved.get("main"), Some(&9500));
        assert_eq!(moved.get("console"), Some(&9001));

        // A handle this package does not declare. Same rule as an undeclared
        // setting key: this writes to instances.json and restarts a container.
        assert!(plan(&patch(&[("grpc", 9500)]), &free).is_err());
        assert!(plan(&patch(&[("main", 0)]), &free).is_err());

        // Another instance's port. The table is the record of that, and two
        // instances on one number is a compose file that fails to come up with
        // a message about neither of them.
        assert!(plan(&patch(&[("main", 3306)]), &free).is_err());

        // Taken on the machine by something that is not StackVo at all.
        assert!(plan(&patch(&[("main", 9500)]), &taken).is_err());

        // Its own port, unchanged, is never probed — the instance's container
        // is bound to it, so asking the kernel would refuse a patch that does
        // not touch it. This one moves `console` while `main` stays put, with
        // every probe answering "taken".
        assert!(plan(&patch(&[("console", 9001)]), &taken).is_ok());

        // Both handles onto one number. It passes every check above — 9000 is
        // this instance's own port, so it is neither another instance's nor
        // probed — and would publish the same port twice.
        assert!(plan(&patch(&[("console", 9000)]), &free).is_err());
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
        // An empty instance table, because a workspace without one is refused
        // rather than rendered (ADR 0016). Empty rather than populated: these
        // tests are about the generator reporting progress and writing files,
        // not about what a service renders to, and an empty table exercises the
        // same path with nothing to install.
        crate::instances::Table::default()
            .save(&dir)
            .expect("an empty instance table");
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
