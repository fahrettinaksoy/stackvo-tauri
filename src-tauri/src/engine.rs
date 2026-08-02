//! Talking to the Docker engine from the host.
//!
//! The web UI got its connection handed to it: a bind-mounted `/var/run/docker.sock`
//! plus `chmod 666` at container start. On the host there is no mount and no
//! chmod — we resolve the endpoint the way the `docker` CLI does, and connect as
//! the invoking user.
//!
//! Resolution order, matching the CLI: `DOCKER_HOST`, then the current
//! `docker context`, then the well-known socket paths.

use crate::error::{Code, Error, Result};
use bollard::Docker;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    DockerDesktop,
    Colima,
    Orbstack,
    Engine,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub reachable: bool,
    pub version: Option<String>,
    pub api_version: Option<String>,
    pub context: Option<String>,
    pub platform: Platform,
    pub socket_path: Option<String>,
    pub error: Option<String>,
}

impl EngineStatus {
    fn unreachable(socket: Option<String>, err: impl std::fmt::Display) -> Self {
        Self {
            reachable: false,
            version: None,
            api_version: None,
            context: None,
            platform: socket.as_deref().map(classify).unwrap_or(Platform::Unknown),
            socket_path: socket,
            error: Some(err.to_string()),
        }
    }
}

/// Guess the runtime from the socket path. Only used for presentation — the
/// connection itself does not care — but "Docker Desktop is not running" is a
/// far more actionable message than "connection refused".
fn classify(socket: &str) -> Platform {
    if socket.contains(r"\\.\pipe\") {
        // The named pipe is Docker Desktop's endpoint on Windows.
        Platform::DockerDesktop
    } else if socket.contains(".colima") {
        Platform::Colima
    } else if socket.contains(".orbstack") {
        Platform::Orbstack
    } else if socket.contains(".docker/run") || socket.contains("docker.desktop") {
        Platform::DockerDesktop
    } else if socket.contains("docker.sock") {
        Platform::Engine
    } else {
        Platform::Unknown
    }
}

/// The `docker context` currently selected, and its endpoint.
///
/// Contexts live in `~/.docker/contexts/meta/<sha>/meta.json`. Rather than
/// hashing the name ourselves, we scan the directory and match on the `Name`
/// field — cheap (a handful of files) and immune to the hashing scheme changing.
fn context_endpoint() -> Option<(String, String)> {
    let current = std::fs::read_to_string(dirs::home_dir()?.join(".docker/config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|cfg| cfg.get("currentContext")?.as_str().map(str::to_string))?;

    let meta_root = dirs::home_dir()?.join(".docker/contexts/meta");
    for entry in std::fs::read_dir(meta_root).ok()? {
        let meta = entry.ok()?.path().join("meta.json");
        let Ok(raw) = std::fs::read_to_string(&meta) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };

        if json.get("Name").and_then(|v| v.as_str()) != Some(current.as_str()) {
            continue;
        }
        let host = json
            .pointer("/Endpoints/docker/Host")
            .and_then(|v| v.as_str())?
            .to_string();
        return Some((current, host));
    }
    None
}

/// Strip the scheme the CLI and `DOCKER_HOST` use.
///
/// Returns None for endpoints we cannot connect to as a local socket or pipe
/// (a `tcp://` remote daemon), so callers fall through to the well-known paths.
fn socket_from_host(host: &str) -> Option<String> {
    if host.starts_with("unix://") || host.starts_with("npipe://") {
        return Some(crate::paths::strip_endpoint_scheme(host).to_string());
    }
    // A bare named pipe with no scheme is what Docker Desktop on Windows sets.
    if crate::paths::is_named_pipe(host) {
        return Some(host.to_string());
    }
    None
}

fn well_known_sockets() -> Vec<PathBuf> {
    let mut out = Vec::new();

    // Windows has no unix socket: the daemon listens on a named pipe.
    #[cfg(target_os = "windows")]
    {
        out.push(PathBuf::from(crate::paths::WINDOWS_NAMED_PIPE));
        return out;
    }

    #[allow(unreachable_code)]
    if let Some(home) = dirs::home_dir() {
        out.push(home.join(".docker/run/docker.sock")); // Docker Desktop
        out.push(home.join(".colima/default/docker.sock")); // Colima
        out.push(home.join(".orbstack/run/docker.sock")); // OrbStack
    }
    out.push(PathBuf::from("/var/run/docker.sock")); // Docker Engine
    out
}

/// The resolved endpoint plus the context name it came from, if any.
pub fn resolve_endpoint() -> (Option<String>, Option<String>) {
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        return (socket_from_host(&host), Some("DOCKER_HOST".to_string()));
    }

    if let Some((name, host)) = context_endpoint() {
        if let Some(socket) = socket_from_host(&host) {
            if PathBuf::from(&socket).exists() {
                return (Some(socket), Some(name));
            }
        }
    }

    for candidate in well_known_sockets() {
        if candidate.exists() {
            return (Some(candidate.display().to_string()), None);
        }
    }

    (None, None)
}

/// Connect to the engine. Cheap enough to call per command — bollard holds a
/// connection pool internally and the socket is local.
pub fn connect() -> Result<Docker> {
    let (socket, _) = resolve_endpoint();
    let Some(socket) = socket else {
        return Err(
            Error::new(Code::EngineUnreachable, "No Docker socket found.")
                .with_hint("Start Docker Desktop, or set DOCKER_HOST if the engine is elsewhere."),
        );
    };

    #[cfg(target_os = "windows")]
    if crate::paths::is_named_pipe(&socket) {
        return Docker::connect_with_named_pipe(&socket, 8, bollard::API_DEFAULT_VERSION).map_err(
            |e| {
                Error::new(
                    Code::EngineUnreachable,
                    format!("Cannot reach the Docker engine: {e}"),
                )
                .with_hint("Start Docker Desktop and try again.")
            },
        );
    }

    Docker::connect_with_unix(&socket, 8, bollard::API_DEFAULT_VERSION).map_err(|e| {
        Error::new(
            Code::EngineUnreachable,
            format!("Cannot reach the Docker engine: {e}"),
        )
        .with_hint("Start Docker Desktop and try again.")
        .with_details(serde_json::json!({ "socket": socket }))
    })
}

/// Probe the engine. Never returns Err for an unreachable daemon — "Docker is
/// down" is a normal, displayable state for a desktop app, not a failure. That
/// distinction is the whole reason this command exists.
pub async fn status() -> EngineStatus {
    let (socket, context) = resolve_endpoint();

    let Some(socket_path) = socket else {
        return EngineStatus {
            reachable: false,
            version: None,
            api_version: None,
            context,
            platform: Platform::Unknown,
            socket_path: None,
            error: Some("No Docker socket found on this machine.".into()),
        };
    };

    let docker = match Docker::connect_with_unix(&socket_path, 8, bollard::API_DEFAULT_VERSION) {
        Ok(d) => d,
        Err(e) => return EngineStatus::unreachable(Some(socket_path), e),
    };

    match docker.version().await {
        Ok(v) => EngineStatus {
            reachable: true,
            version: v.version,
            api_version: v.api_version,
            context,
            platform: classify(&socket_path),
            socket_path: Some(socket_path),
            error: None,
        },
        Err(e) => EngineStatus::unreachable(Some(socket_path), e),
    }
}

/// Ask the OS to start the engine. Best-effort: it returns as soon as the
/// launch is issued, and the caller waits on the next `status()` poll.
pub fn start() -> Result<()> {
    #[cfg(target_os = "macos")]
    let attempt = std::process::Command::new("open")
        .args(["-a", "Docker"])
        .spawn();

    #[cfg(target_os = "windows")]
    let attempt = std::process::Command::new("cmd")
        .args(["/C", "start", "", "Docker Desktop.exe"])
        .spawn();

    #[cfg(target_os = "linux")]
    let attempt = std::process::Command::new("systemctl")
        .args(["--user", "start", "docker-desktop"])
        .spawn();

    attempt.map(|_| ()).map_err(|e| {
        Error::new(
            Code::EngineUnreachable,
            format!("Could not start Docker: {e}"),
        )
        .with_hint("Start Docker manually, then retry.")
    })
}

// ---------------------------------------------------------------- inventory

/// Every StackVo container is named `stackvo-<id>`; the CLI hardcodes the
/// prefix in `CONST_CONTAINER_PREFIX`.
pub const CONTAINER_PREFIX: &str = "stackvo-";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub container: u16,
    pub host: Option<u16>,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    pub name: String,
    pub image: Option<String>,
    pub state: String,
    pub running: bool,
    pub status: Option<String>,
    pub ports: Vec<Port>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCount {
    pub total: u32,
    pub in_use: u32,
    pub unused: u32,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemResources {
    pub images: ResourceCount,
    pub volumes: ResourceCount,
}

/// All StackVo containers, keyed by their `stackvo-`-stripped id.
///
/// Includes stopped containers: a project that exists but is not running is a
/// state the UI must show, not hide.
pub async fn stackvo_containers() -> Result<std::collections::HashMap<String, ContainerInfo>> {
    use bollard::query_parameters::ListContainersOptionsBuilder;

    let docker = connect()?;
    let options = ListContainersOptionsBuilder::new().all(true).build();

    let summaries = docker.list_containers(Some(options)).await.map_err(|e| {
        Error::new(
            Code::EngineUnreachable,
            format!("Cannot list containers: {e}"),
        )
    })?;

    let mut out = std::collections::HashMap::new();
    for c in summaries {
        // Docker returns names with a leading slash.
        let Some(name) = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
        else {
            continue;
        };
        let Some(id) = name.strip_prefix(CONTAINER_PREFIX) else {
            continue;
        };

        let state = c
            .state
            .as_ref()
            .map(|s| format!("{s:?}").to_lowercase())
            .unwrap_or_else(|| "unknown".into());

        let ports = c
            .ports
            .unwrap_or_default()
            .into_iter()
            .map(|p| Port {
                container: p.private_port,
                host: p.public_port,
                protocol: p
                    .typ
                    .map(|t| format!("{t:?}").to_lowercase())
                    .unwrap_or_else(|| "tcp".into()),
            })
            .collect();

        out.insert(
            id.to_string(),
            ContainerInfo {
                running: state == "running",
                name,
                image: c.image,
                state,
                status: c.status,
                ports,
            },
        );
    }

    Ok(out)
}

/// `host port → container name` for every *running* container, ours or not.
///
/// The doctor uses this to turn "com.docker.backend is listening on 3306" —
/// true and useless — into the name of the container that actually owns the
/// port, which is the difference between a conflict and the stack seeing
/// itself in the mirror.
pub async fn port_owners() -> Result<std::collections::HashMap<u16, String>> {
    use bollard::query_parameters::ListContainersOptionsBuilder;

    let docker = connect()?;
    // Running only: a stopped container publishes nothing.
    let options = ListContainersOptionsBuilder::new().all(false).build();

    let summaries = docker.list_containers(Some(options)).await.map_err(|e| {
        Error::new(
            Code::EngineUnreachable,
            format!("Cannot list containers: {e}"),
        )
    })?;

    let mut out = std::collections::HashMap::new();
    for c in summaries {
        let Some(name) = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
        else {
            continue;
        };
        for p in c.ports.unwrap_or_default() {
            if let Some(host) = p.public_port {
                out.entry(host).or_insert_with(|| name.clone());
            }
        }
    }
    Ok(out)
}

/// One stack member's share of the disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskOwner {
    /// `stackvo-`-stripped id: a project name, a service, or `tunnel-<x>`.
    /// Empty container fields with a `stackvo-*` image mean an orphaned build
    /// — an image this stack produced whose container is gone.
    pub id: String,
    pub image: Option<String>,
    pub image_size: u64,
    /// True when the image is one this stack built (`stackvo-<name>`), so its
    /// bytes belong to this entry alone and vanish with it. False for shared
    /// upstream images (`mysql:8.0`), which removing this member cannot free.
    pub image_dedicated: bool,
    /// The container's writable layer — what it has changed on top of the
    /// image. Zero for an orphaned image.
    pub container_rw: u64,
    pub running: bool,
}

/// Who holds the bytes: every StackVo container with its image size and
/// writable layer, plus stack-built images whose container no longer exists.
///
/// This is the per-member answer `docker system df` cannot give — its numbers
/// are totals, and the question a full disk raises is *which project*.
pub async fn disk_attribution() -> Result<Vec<DiskOwner>> {
    use bollard::query_parameters::{ListContainersOptionsBuilder, ListImagesOptionsBuilder};

    let docker = connect()?;

    let containers = docker
        .list_containers(Some(
            ListContainersOptionsBuilder::new()
                .all(true)
                .size(true)
                .build(),
        ))
        .await
        .map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("Cannot list containers: {e}"),
            )
        })?;

    let images = docker
        .list_images(Some(ListImagesOptionsBuilder::new().all(false).build()))
        .await
        .map_err(|e| Error::new(Code::EngineUnreachable, format!("Cannot list images: {e}")))?;

    let mut image_sizes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for img in &images {
        for tag in &img.repo_tags {
            image_sizes.insert(tag.clone(), img.size.max(0) as u64);
        }
    }

    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();

    for c in containers {
        let Some(id) = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/'))
            .and_then(|n| n.strip_prefix(CONTAINER_PREFIX))
        else {
            continue;
        };

        let image = c.image.clone();
        if let Some(tag) = &image {
            referenced.insert(tag.clone());
        }
        let image_size = image
            .as_deref()
            .and_then(|t| image_sizes.get(t))
            .copied()
            .unwrap_or(0);

        out.push(DiskOwner {
            id: id.to_string(),
            image_dedicated: image
                .as_deref()
                .is_some_and(|t| t.starts_with(CONTAINER_PREFIX)),
            image,
            image_size,
            container_rw: c.size_rw.unwrap_or(0).max(0) as u64,
            running: c
                .state
                .as_ref()
                .is_some_and(|s| format!("{s:?}").eq_ignore_ascii_case("running")),
        });
    }

    // Images this stack built whose container is gone: invisible in every
    // list the app shows, and exactly the bytes nobody remembers spending.
    for img in &images {
        for tag in &img.repo_tags {
            if tag.starts_with(CONTAINER_PREFIX) && !referenced.contains(tag) {
                out.push(DiskOwner {
                    id: tag
                        .strip_prefix(CONTAINER_PREFIX)
                        .unwrap_or(tag)
                        .split(':')
                        .next()
                        .unwrap_or(tag)
                        .to_string(),
                    image: Some(tag.clone()),
                    image_size: img.size.max(0) as u64,
                    image_dedicated: true,
                    container_rw: 0,
                    running: false,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        (b.image_size + b.container_rw)
            .cmp(&(a.image_size + a.container_rw))
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PruneReport {
    pub images_deleted: u64,
    pub volumes_deleted: u64,
    /// Build-cache records removed. Counted separately from images because one
    /// image's worth of cache is many records.
    pub caches_deleted: u64,
    pub space_reclaimed: u64,
}

/// How far a build-cache prune goes.
///
/// Three levels rather than a bool because the middle one is the only one that
/// is always safe, and it is the one a project deletion wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildCache {
    /// Leave it alone.
    #[default]
    Keep,
    /// Only records nothing references any more — what deleting an image
    /// orphans. Rebuilding anything still installed stays incremental.
    Dangling,
    /// Every record, including the layers the remaining projects would have
    /// reused. Reclaims the most and makes the next build of every project a
    /// full one.
    All,
}

/// Remove dangling images and, only when asked, unused volumes and build cache.
///
/// Separate flags because they are not the same risk. A dangling image is
/// rebuildable by definition. An unused volume can be the only copy of a
/// database that belongs to a stopped project — the engine's definition of
/// "unused" is "not currently mounted", not "not wanted". And the build cache
/// is *shared*: every StackVo project image starts from the same PHP base and
/// runs the same extension installs, so those layers are one cache serving all
/// of them. `BuildCache::All` on a machine with three projects reclaims the
/// most and costs all three a full rebuild, which is why it is a level rather
/// than the default.
pub async fn prune(images: bool, volumes: bool, build_cache: BuildCache) -> Result<PruneReport> {
    let docker = connect()?;
    let mut report = PruneReport {
        images_deleted: 0,
        volumes_deleted: 0,
        caches_deleted: 0,
        space_reclaimed: 0,
    };

    if images {
        // No filter: the API default prunes dangling images only, which is
        // exactly the safe set.
        let r = docker
            .prune_images(None::<bollard::query_parameters::PruneImagesOptions>)
            .await
            .map_err(|e| {
                Error::new(Code::EngineUnreachable, format!("Cannot prune images: {e}"))
            })?;
        report.images_deleted = r.images_deleted.map(|v| v.len() as u64).unwrap_or(0);
        report.space_reclaimed += r.space_reclaimed.unwrap_or(0).max(0) as u64;
    }

    if volumes {
        let r = docker
            .prune_volumes(None::<bollard::query_parameters::PruneVolumesOptions>)
            .await
            .map_err(|e| {
                Error::new(
                    Code::EngineUnreachable,
                    format!("Cannot prune volumes: {e}"),
                )
            })?;
        report.volumes_deleted = r.volumes_deleted.map(|v| v.len() as u64).unwrap_or(0);
        report.space_reclaimed += r.space_reclaimed.unwrap_or(0).max(0) as u64;
    }

    if build_cache != BuildCache::Keep {
        use bollard::query_parameters::PruneBuildOptionsBuilder;

        // `all(false)` is the API default and means dangling only. Spelled out
        // because the difference between the two is the whole point of the
        // level, and a reader should not have to know the default to see it.
        let options = PruneBuildOptionsBuilder::new()
            .all(build_cache == BuildCache::All)
            .build();

        let r = docker.prune_build(Some(options)).await.map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("Cannot prune the build cache: {e}"),
            )
        })?;
        report.caches_deleted = r.caches_deleted.map(|v| v.len() as u64).unwrap_or(0);
        report.space_reclaimed += r.space_reclaimed.unwrap_or(0).max(0) as u64;
    }

    Ok(report)
}

/// Image and volume inventory from `/system/df`.
pub async fn system_resources() -> Result<SystemResources> {
    let docker = connect()?;
    let df = docker.df(None).await.map_err(|e| {
        Error::new(
            Code::EngineUnreachable,
            format!("Cannot read disk usage: {e}"),
        )
    })?;

    // The engine already aggregates these, so we take its counts rather than
    // re-deriving them from the object lists — fewer places to disagree with
    // `docker system df`.
    let images = df.image_usage.unwrap_or_default();
    let volumes = df.volume_usage.unwrap_or_default();

    let count = |total: Option<i64>, active: Option<i64>, size: Option<i64>| {
        let total = total.unwrap_or(0).max(0) as u32;
        let in_use = (active.unwrap_or(0).max(0) as u32).min(total);
        ResourceCount {
            total,
            in_use,
            unused: total - in_use,
            size: size.unwrap_or(0).max(0) as u64,
        }
    };

    Ok(SystemResources {
        images: count(images.total_count, images.active_count, images.total_size),
        volumes: count(
            volumes.total_count,
            volumes.active_count,
            volumes.total_size,
        ),
    })
}

// ---------------------------------------------------------------- lifecycle

/// Prefix a bare id with `stackvo-` unless it already carries it, so callers
/// can pass either form without the ambiguity that produced
/// `stackvo-stackvo-mysql` bugs in the shell-string era.
pub fn container_name(id: &str) -> String {
    if id.starts_with(CONTAINER_PREFIX) {
        id.to_string()
    } else {
        format!("{CONTAINER_PREFIX}{id}")
    }
}

/// Turn a bollard error into a typed one. A 404 from the daemon is NOT_FOUND,
/// not "engine unreachable" — the old code lumped both into a 500.
fn lifecycle_error(action: &str, name: &str, err: bollard::errors::Error) -> Error {
    if let bollard::errors::Error::DockerResponseServerError {
        status_code: 404, ..
    } = err
    {
        return Error::not_found(format!("container {name}"))
            .with_hint("The project may not be built yet.");
    }
    Error::new(
        Code::EngineUnreachable,
        format!("Cannot {action} {name}: {err}"),
    )
}

pub async fn start_container(id: &str) -> Result<()> {
    use bollard::query_parameters::StartContainerOptions;
    let name = container_name(id);
    let docker = connect()?;

    match docker
        .start_container(&name, None::<StartContainerOptions>)
        .await
    {
        Ok(()) => Ok(()),
        // 304 = already started. Idempotent by contract, so this is success.
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 304, ..
        }) => Ok(()),
        Err(e) => Err(lifecycle_error("start", &name, e)),
    }
}

pub async fn stop_container(id: &str) -> Result<()> {
    use bollard::query_parameters::StopContainerOptions;
    let name = container_name(id);
    let docker = connect()?;

    match docker
        .stop_container(&name, None::<StopContainerOptions>)
        .await
    {
        Ok(()) => Ok(()),
        // 304 = already stopped.
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 304, ..
        }) => Ok(()),
        Err(e) => Err(lifecycle_error("stop", &name, e)),
    }
}

/// Delete a container, running or not.
///
/// `force` because stopping first and removing second is two round trips and a
/// race: a restart policy can bring the container back between them. Removing
/// a container that is not there is success, not a 404 to report — the caller
/// is asking for it to be gone, and it is.
///
/// Anonymous volumes go with it (`v`). A project's compose service declares
/// only bind mounts, so in practice this cleans up whatever an image's own
/// `VOLUME` directive created and nothing the user named.
pub async fn remove_container(id: &str) -> Result<()> {
    use bollard::query_parameters::RemoveContainerOptionsBuilder;
    let name = container_name(id);
    let docker = connect()?;

    let options = RemoveContainerOptionsBuilder::default()
        .force(true)
        .v(true)
        .build();

    match docker.remove_container(&name, Some(options)).await {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(lifecycle_error("remove", &name, e)),
    }
}

/// Is this `repo:tag` an image built for `repository`?
///
/// Split at the LAST colon and compare the repository whole. Two ways to get
/// this wrong, both of which delete something that is not yours: a
/// `starts_with` makes deleting `lara` take `stackvo-laravel` with it, and
/// splitting at the first colon mangles a registry port
/// (`localhost:5000/img:tag`).
fn is_project_image(tag: &str, repository: &str) -> bool {
    tag.rsplit_once(':')
        .is_some_and(|(repo, _)| repo == repository)
}

/// Delete every image built for one project, whatever it is tagged.
///
/// The tag is not fixed — Apache tags with the PHP version, everything else
/// with `latest` — so the repository is matched instead, and matched *exactly*:
/// a `starts_with` would let deleting `lara` take `stackvo-laravel` with it.
///
/// Returns what it removed, for the log line. A 409 (some other container
/// still uses the image) is not an error to raise: it means the image is not
/// this project's alone, and leaving it is the correct outcome.
pub async fn remove_project_images(project: &str) -> Result<Vec<String>> {
    use bollard::query_parameters::{ListImagesOptionsBuilder, RemoveImageOptionsBuilder};

    let docker = connect()?;
    let repository = container_name(project);

    let images = docker
        .list_images(Some(ListImagesOptionsBuilder::new().all(false).build()))
        .await
        .map_err(|e| Error::new(Code::EngineUnreachable, format!("Cannot list images: {e}")))?;

    let tags: Vec<String> = images
        .into_iter()
        .flat_map(|image| image.repo_tags)
        .filter(|tag| is_project_image(tag, &repository))
        .collect();

    let mut removed = Vec::new();
    for tag in tags {
        let options = RemoveImageOptionsBuilder::default().build();
        match docker.remove_image(&tag, Some(options), None).await {
            Ok(_) => removed.push(tag),
            // 404: already gone. 409: another container holds it.
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404 | 409,
                ..
            }) => {}
            Err(e) => return Err(lifecycle_error("remove image", &tag, e)),
        }
    }

    Ok(removed)
}

pub async fn restart_container(id: &str) -> Result<()> {
    use bollard::query_parameters::RestartContainerOptions;
    let name = container_name(id);
    let docker = connect()?;

    docker
        .restart_container(&name, None::<RestartContainerOptions>)
        .await
        .map_err(|e| lifecycle_error("restart", &name, e))
}

/// Does the shared network exist?
///
/// Asked by name rather than listed: the generator writes whatever
/// `DOCKER_DEFAULT_NETWORK` says into every compose file, and that is the name
/// that has to be there.
pub async fn network_exists(name: &str) -> bool {
    use bollard::query_parameters::InspectNetworkOptions;

    let Ok(docker) = connect() else {
        return false;
    };

    docker
        .inspect_network(name, None::<InspectNetworkOptions>)
        .await
        .is_ok()
}

/// Create it, the way `install.sh` does: a plain user-defined bridge.
pub async fn network_create(name: &str) -> Result<()> {
    use bollard::models::NetworkCreateRequest;

    let docker = connect()?;

    docker
        .create_network(NetworkCreateRequest {
            name: name.to_string(),
            ..Default::default()
        })
        .await
        .map(|_| ())
        .map_err(|e| lifecycle_error("create network", name, e))
}

// ---------------------------------------------------------------- inspect

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    /// Host path, or a volume name when Docker manages the storage.
    pub source: Option<String>,
    pub destination: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerDetails {
    pub name: String,
    pub id: Option<String>,
    pub image: Option<String>,
    pub state: Option<String>,
    pub running: bool,
    pub started_at: Option<String>,
    pub created: Option<String>,
    pub restart_count: i64,
    pub restart_policy: Option<String>,
    pub health: Option<String>,
    pub exit_code: Option<i64>,
    pub ports: Vec<Port>,
    pub networks: Vec<String>,
    /// First non-loopback gateway; the detail page shows one.
    pub gateway: Option<String>,
    pub mounts: Vec<Mount>,
    /// The container's address on the StackVo network.
    pub ip_address: Option<String>,
    pub env: Vec<String>,
    /// Bytes on disk for the image this container runs.
    pub image_size: Option<u64>,
}

pub async fn inspect(id: &str) -> Result<ContainerDetails> {
    use bollard::query_parameters::InspectContainerOptions;
    let name = container_name(id);
    let docker = connect()?;

    let info = docker
        .inspect_container(&name, None::<InspectContainerOptions>)
        .await
        .map_err(|e| lifecycle_error("inspect", &name, e))?;

    let state = info.state.as_ref();
    let ports = info
        .network_settings
        .as_ref()
        .and_then(|n| n.ports.clone())
        .map(|map| {
            map.into_iter()
                .filter_map(|(spec, bindings)| {
                    // Keys look like "80/tcp".
                    let (port, proto) = spec.split_once('/')?;
                    let container: u16 = port.parse().ok()?;
                    let host = bindings
                        .and_then(|b| b.first().and_then(|x| x.host_port.clone()))
                        .and_then(|p| p.parse().ok());
                    Some(Port {
                        container,
                        host,
                        protocol: proto.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // The image size lives on the image, not the container.
    let image_size = match info.image.as_deref() {
        Some(image_id) => docker
            .inspect_image(image_id)
            .await
            .ok()
            .and_then(|img| img.size)
            .map(|s| s.max(0) as u64),
        None => None,
    };

    let networks = info
        .network_settings
        .as_ref()
        .and_then(|n| n.networks.as_ref());

    Ok(ContainerDetails {
        id: info.id.clone(),
        created: info.created.as_ref().map(|d| d.to_string()),
        restart_policy: info
            .host_config
            .as_ref()
            .and_then(|h| h.restart_policy.as_ref())
            .and_then(|p| p.name)
            .map(|n| format!("{n:?}").to_lowercase().replace('_', "-")),
        ip_address: networks
            .and_then(|n| n.values().find_map(|e| e.ip_address.clone()))
            .filter(|ip| !ip.is_empty()),
        gateway: networks
            .and_then(|n| n.values().find_map(|e| e.gateway.clone()))
            .filter(|g| !g.is_empty()),
        image_size,
        image: info.config.as_ref().and_then(|c| c.image.clone()),
        state: state.and_then(|s| s.status.map(|st| format!("{st:?}").to_lowercase())),
        running: state.and_then(|s| s.running).unwrap_or(false),
        started_at: state.and_then(|s| s.started_at.clone()),
        restart_count: info.restart_count.unwrap_or(0),
        health: state
            .and_then(|s| s.health.as_ref())
            .and_then(|h| h.status.map(|st| format!("{st:?}").to_lowercase())),
        exit_code: state.and_then(|s| s.exit_code),
        networks: networks
            .map(|n| n.keys().cloned().collect())
            .unwrap_or_default(),
        mounts: info
            .mounts
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| {
                Some(Mount {
                    source: m.source.filter(|s| !s.is_empty()),
                    destination: m.destination?,
                    kind: m.typ.map(|t| format!("{t:?}").to_lowercase()),
                })
            })
            .collect(),
        // Values are redacted: container env routinely carries database
        // passwords, and this crosses the IPC boundary into a webview.
        env: info
            .config
            .as_ref()
            .and_then(|c| c.env.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|entry| match entry.split_once('=') {
                Some((k, v)) if crate::config::Env::is_secret(k) && !v.is_empty() => {
                    format!("{k}=••••••••")
                }
                _ => entry,
            })
            .collect(),
        ports,
        name,
    })
}

// ---------------------------------------------------------------- live stats

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStats {
    pub cpu_percent: f64,
    pub memory_used: u64,
    pub memory_limit: u64,
    pub memory_percent: f64,
    pub net_rx: u64,
    pub net_tx: u64,
    /// Processes inside the container.
    pub pids: u64,
    /// Cumulative block I/O since the container started.
    pub block_read: u64,
    pub block_write: u64,
    pub online_cpus: u64,
}

/// One-shot stats sample.
///
/// Docker's stats endpoint reports CPU as cumulative counters, so a percentage
/// needs two readings. `stream: false` with `one_shot: false` makes the daemon
/// send a pre-read followed by the real sample, which is what the CPU delta
/// below is computed from — a single `one_shot` read would always yield 0%.
pub async fn container_stats(id: &str) -> Result<ContainerStats> {
    use bollard::query_parameters::StatsOptionsBuilder;
    use futures_util::StreamExt;

    let name = container_name(id);
    let docker = connect()?;
    let options = StatsOptionsBuilder::new()
        .stream(false)
        .one_shot(false)
        .build();

    let sample = docker
        .stats(&name, Some(options))
        .next()
        .await
        .ok_or_else(|| Error::not_found(format!("stats for {name}")))?
        .map_err(|e| lifecycle_error("read stats for", &name, e))?;

    let cpu = sample.cpu_stats.as_ref();
    let pre = sample.precpu_stats.as_ref();

    let cpu_delta = cpu
        .and_then(|c| c.cpu_usage.as_ref())
        .and_then(|u| u.total_usage)
        .unwrap_or(0)
        .saturating_sub(
            pre.and_then(|c| c.cpu_usage.as_ref())
                .and_then(|u| u.total_usage)
                .unwrap_or(0),
        ) as f64;

    let system_delta =
        cpu.and_then(|c| c.system_cpu_usage)
            .unwrap_or(0)
            .saturating_sub(pre.and_then(|c| c.system_cpu_usage).unwrap_or(0)) as f64;

    let cores = cpu.and_then(|c| c.online_cpus).unwrap_or(1).max(1) as f64;

    let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
        (cpu_delta / system_delta) * cores * 100.0
    } else {
        0.0
    };

    let memory_used = sample
        .memory_stats
        .as_ref()
        .and_then(|m| m.usage)
        .unwrap_or(0);
    let memory_limit = sample
        .memory_stats
        .as_ref()
        .and_then(|m| m.limit)
        .unwrap_or(0);

    let (net_rx, net_tx) = sample
        .networks
        .as_ref()
        .map(|nets| {
            nets.values().fold((0u64, 0u64), |(rx, tx), n| {
                (rx + n.rx_bytes.unwrap_or(0), tx + n.tx_bytes.unwrap_or(0))
            })
        })
        .unwrap_or((0, 0));

    // Block I/O arrives as per-device entries; the detail view wants the total.
    let (block_read, block_write) = sample
        .blkio_stats
        .as_ref()
        .and_then(|b| b.io_service_bytes_recursive.as_ref())
        .map(|entries| {
            entries.iter().fold((0u64, 0u64), |(r, w), e| {
                match e.op.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    Some("read") => (r + e.value.unwrap_or(0), w),
                    Some("write") => (r, w + e.value.unwrap_or(0)),
                    _ => (r, w),
                }
            })
        })
        .unwrap_or((0, 0));

    Ok(ContainerStats {
        pids: sample
            .pids_stats
            .as_ref()
            .and_then(|p| p.current)
            .unwrap_or(0),
        block_read,
        block_write,
        online_cpus: cores as u64,
        cpu_percent,
        memory_used,
        memory_limit,
        memory_percent: if memory_limit > 0 {
            memory_used as f64 / memory_limit as f64 * 100.0
        } else {
            0.0
        },
        net_rx,
        net_tx,
    })
}

// ---------------------------------------------------------------- logs

/// One decoded log line and which stream it came from.
pub struct LogLine {
    pub text: String,
    pub stream: &'static str,
}

/// A live log stream for a container.
///
/// The web UI had no equivalent: following logs existed only as `stackvo logs`
/// in the CLI, because a container-hosted dashboard streaming its own siblings'
/// output over Socket.io was more plumbing than it was worth.
pub fn logs_stream(
    id: &str,
    tail: u32,
    follow: bool,
) -> Result<impl futures_util::Stream<Item = LogLine>> {
    use bollard::container::LogOutput;
    use bollard::query_parameters::LogsOptionsBuilder;
    use futures_util::StreamExt;

    let name = container_name(id);
    let docker = connect()?;

    let options = LogsOptionsBuilder::new()
        .follow(follow)
        .stdout(true)
        .stderr(true)
        .timestamps(false)
        .tail(&tail.to_string())
        .build();

    let stream = docker
        .logs(&name, Some(options))
        .filter_map(|item| async move {
            let out = item.ok()?;
            let (bytes, stream) = match out {
                LogOutput::StdOut { message } => (message, "stdout"),
                LogOutput::StdErr { message } => (message, "stderr"),
                LogOutput::Console { message } => (message, "stdout"),
                LogOutput::StdIn { .. } => return None,
            };

            // Docker frames are chunks, not lines, and may split mid-UTF-8.
            // Lossy decoding keeps a partial multi-byte character from killing the
            // whole stream.
            let text = String::from_utf8_lossy(&bytes)
                .trim_end_matches('\n')
                .to_string();
            (!text.is_empty()).then_some(LogLine { text, stream })
        });

    Ok(stream)
}

// ---------------------------------------------------------------- event stream

/// Follow the Docker event stream and report StackVo container transitions.
///
/// This replaces polling. The web UI refetched the whole container list on a
/// visibility-gated timer (`useVisiblePolling`), so a container that died
/// between ticks looked healthy until the next one. The daemon already
/// broadcasts these transitions; a host process can just listen.
///
/// Runs until the connection drops, which is normal when Docker restarts — the
/// caller reconnects.
pub async fn watch_container_events<F>(mut on_change: F) -> Result<()>
where
    F: FnMut(String, String, bool) + Send,
{
    use bollard::query_parameters::EventsOptionsBuilder;
    use futures_util::StreamExt;
    use std::collections::HashMap;

    let docker = connect()?;

    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    filters.insert("type".into(), vec!["container".into()]);

    let mut stream = docker.events(Some(EventsOptionsBuilder::new().filters(&filters).build()));

    while let Some(event) = stream.next().await {
        let Ok(event) = event else { break };

        let Some(action) = event.action.as_deref() else {
            continue;
        };
        // `exec_start: …` and friends are noise; only real transitions matter.
        let running = match action {
            "start" | "unpause" | "restart" => true,
            "die" | "stop" | "kill" | "pause" | "destroy" => false,
            _ => continue,
        };

        let name = event
            .actor
            .as_ref()
            .and_then(|a| a.attributes.as_ref())
            .and_then(|attrs| attrs.get("name"))
            .cloned();

        let Some(name) = name else { continue };
        if !name.starts_with(CONTAINER_PREFIX) {
            continue;
        }

        on_change(name, action.to_string(), running);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_name_is_idempotent() {
        assert_eq!(container_name("mysql"), "stackvo-mysql");
        // Passing an already-prefixed name must not double it.
        assert_eq!(container_name("stackvo-mysql"), "stackvo-mysql");
    }

    #[test]
    fn classifies_runtimes_by_socket_path() {
        assert_eq!(
            classify("/Users/x/.docker/run/docker.sock"),
            Platform::DockerDesktop
        );
        assert_eq!(
            classify("/Users/x/.colima/default/docker.sock"),
            Platform::Colima
        );
        assert_eq!(
            classify("/Users/x/.orbstack/run/docker.sock"),
            Platform::Orbstack
        );
        assert_eq!(classify("/var/run/docker.sock"), Platform::Engine);
        assert_eq!(classify("/tmp/something-else"), Platform::Unknown);
    }

    /// Deleting a project deletes its image, and only its image.
    #[test]
    fn a_project_image_is_matched_by_whole_repository() {
        assert!(is_project_image("stackvo-lara:latest", "stackvo-lara"));
        // Apache tags with the PHP version rather than `latest`.
        assert!(is_project_image("stackvo-lara:8.3", "stackvo-lara"));

        // The reason this is not a prefix test: deleting `lara` must not take
        // the neighbouring project's image with it.
        assert!(!is_project_image("stackvo-laravel:latest", "stackvo-lara"));
        assert!(!is_project_image("stackvo-lara-api:latest", "stackvo-lara"));
        // Someone else's image that merely ends the same way.
        assert!(!is_project_image(
            "ghcr.io/acme/stackvo-lara:latest",
            "stackvo-lara"
        ));
        // A registry port is a colon too, which is why the split is from the
        // right — this one is a genuine match despite having two.
        assert!(is_project_image(
            "localhost:5000/img:tag",
            "localhost:5000/img"
        ));
        // `<none>` images carry no tag at all.
        assert!(!is_project_image("stackvo-lara", "stackvo-lara"));
    }

    #[test]
    fn strips_the_unix_scheme() {
        assert_eq!(
            socket_from_host("unix:///var/run/docker.sock").as_deref(),
            Some("/var/run/docker.sock")
        );
        // A TCP endpoint yields no socket path — callers fall through.
        assert_eq!(socket_from_host("tcp://127.0.0.1:2375"), None);
    }
}
