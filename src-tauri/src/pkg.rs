//! Reading a service package, and deciding whether to believe it.
//!
//! The client half of `contracts/package-version.schema.json`. A package is
//! data somebody else wrote — in Faz 1 that somebody is this project's own
//! converter, and from Faz 5 it is whatever a registry served — so every field
//! that reaches a compose file passes through [`Manifest::check`] first, and
//! every byte on disk passes through [`verify`].
//!
//! ## Why the checks are here and not only in the schema
//!
//! JSON Schema says a port is an integer between 1 and 65535. It cannot say
//! that `connection.port` names a port this manifest actually declares, that at
//! most one port is `primary`, or that `files[].template` stays inside the
//! package directory. Those are the ones that matter: a manifest that passes
//! the schema and fails these is a manifest that renders a compose file naming
//! a port that does not exist, or writes a file outside the workspace.
//!
//! The path rules are the security boundary. `files/../../../../etc/cron.d/x`
//! is a valid JSON string and a valid relative path, and a package that could
//! carry one would be arbitrary file write dressed as a database.
//!
//! ## What this module does not do
//!
//! It does not fetch, and it does not check signatures. Fetching is `market`
//! and trust is `trust`; both are Faz 4 and both are layered *above* this. The
//! split is deliberate: this module is pure and has no network, which is what
//! lets its tests be the ones that describe attacks.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// The manifest contract this build speaks.
///
/// A package declaring anything else is refused whole rather than read for the
/// fields that happen to line up — a v2 manifest whose `volumes` mean something
/// new would otherwise be half-understood by a v1 client, and the half it got
/// wrong decides what gets deleted.
pub const API_VERSION: &str = "stackvo.dev/package/v1";

/// Tags that name a moving target rather than a version.
///
/// ADR 0014. These cannot be package versions: an image that changes under a
/// fixed manifest has no digest the manifest can pin, so it has no place in the
/// chain of trust — and a user whose `instances.json` says `latest` cannot be
/// told which version they are actually running. The registry expresses "the
/// newest one" as a flag on a concrete version instead.
pub const MOVING_TAGS: [&str; 5] = ["latest", "stable", "edge", "main", "master"];

pub fn is_moving_tag(tag: &str) -> bool {
    MOVING_TAGS.contains(&tag)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    #[serde(default)]
    pub registry: Option<String>,
    pub repository: String,
    pub tag: String,
    #[serde(default)]
    pub digest: Option<String>,
}

impl Image {
    /// What `docker pull` is given. The digest wins when there is one: a tag is
    /// a name and a digest is the bytes.
    pub fn reference(&self) -> String {
        let host = self
            .registry
            .as_deref()
            .filter(|r| *r != "docker.io")
            .map(|r| format!("{r}/"))
            .unwrap_or_default();
        match &self.digest {
            Some(d) => format!("{host}{}@{d}", self.repository),
            None => format!("{host}{}:{}", self.repository, self.tag),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instancing {
    pub multiple: bool,
    #[serde(default = "identity_default")]
    pub identity: String,
}

fn identity_default() -> String {
    "version".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub name: String,
    pub container: u16,
    pub preferred: u16,
    #[serde(default = "tcp")]
    pub protocol: String,
    /// The `.env` key this port was published under before packages existed.
    ///
    /// Migration only, and it is carried rather than derived because the
    /// derivation does not run backwards: two key families reduce to the same
    /// handle, and reconstructing one from `main` would be a guess about which
    /// family a service used. A wrong guess here does not fail — it quietly
    /// hands the user a different port from the one their tooling has.
    #[serde(default)]
    pub legacy_key: Option<String>,
    #[serde(default)]
    pub primary: bool,
}

fn tcp() -> String {
    "tcp".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub container: String,
    #[serde(default = "yes")]
    pub purgeable: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub template: String,
    pub target: String,
    #[serde(default)]
    pub mode: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub capability: Option<String>,
    /// Locale → human label. Absent means the key is shown as it is.
    #[serde(default)]
    pub label: std::collections::BTreeMap<String, String>,
}

impl Setting {
    pub fn is_secret(&self) -> bool {
        self.kind == "secret"
    }

    /// The default as a string, for the one place it is written: the keystore
    /// entry or the settings map an install seeds.
    pub fn default_text(&self) -> Option<String> {
        match self.default.as_ref()? {
            serde_json::Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub scheme: String,
    pub port: String,
    #[serde(default)]
    pub user_setting: Option<String>,
    #[serde(default)]
    pub default_user: Option<String>,
    #[serde(default)]
    pub password_setting: Option<String>,
    #[serde(default)]
    pub database_setting: Option<String>,
    #[serde(default)]
    pub default_database: Option<String>,
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Url {
    pub subdomain: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub capability: String,
    #[serde(default)]
    pub service: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Companion {
    pub name: String,
    pub image: Image,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    pub compose: Blob,
}

/// Compose's healthcheck, as the manifest states it.
///
/// Optional, and the difference between "no healthcheck" and "a healthcheck
/// that always passes" is the reason it is `Option` rather than a struct with
/// empty defaults: `depends_on: condition: service_healthy` means nothing
/// against a service that never declared one, and rendering an empty test would
/// make every container report healthy the instant it started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub test: Vec<String>,
    #[serde(default)]
    pub interval: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub retries: Option<u32>,
    #[serde(default)]
    pub start_period: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Support {
    pub status: String,
    #[serde(default)]
    pub eol_date: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub api_version: String,
    pub service: String,
    pub version: String,
    pub image: Image,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub instancing: Instancing,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub settings: Vec<Setting>,
    #[serde(default)]
    pub connection: Option<Connection>,
    #[serde(default)]
    pub url: Option<Url>,
    #[serde(default)]
    pub health: Option<Health>,
    #[serde(default)]
    pub depends_on: Vec<Dependency>,
    #[serde(default)]
    pub companions: Vec<Companion>,
    pub compose: Blob,
    pub support: Support,
    #[serde(default)]
    pub notes: std::collections::BTreeMap<String, String>,
}

impl Manifest {
    pub fn port(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.name == name)
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.key == key)
    }

    /// Everything JSON Schema cannot say.
    ///
    /// Ordered so the cheapest refusals come first, and every message names the
    /// package: these are read in a loop over a tree, and "invalid manifest" on
    /// its own is a message somebody has to bisect.
    pub fn check(&self) -> Result<()> {
        let who = format!("{}@{}", self.service, self.version);
        let bad = |m: String| Err(Error::new(Code::InvalidManifest, format!("{who}: {m}")));

        if self.api_version != API_VERSION {
            return bad(format!(
                "declares {} and this build speaks {API_VERSION} — a manifest from \
                 another contract is refused rather than read for the fields that \
                 happen to line up",
                self.api_version
            ));
        }
        if !is_id(&self.service) {
            return bad(format!("service id {:?} is not a DNS label", self.service));
        }
        if is_moving_tag(&self.version) {
            return bad(format!(
                "version {:?} is a moving tag, which cannot be pinned or verified",
                self.version
            ));
        }
        if self.image.repository.is_empty() || self.image.tag.is_empty() {
            return bad("image has no repository or no tag".into());
        }
        if let Some(digest) = &self.image.digest {
            if !is_sha256(digest.strip_prefix("sha256:").unwrap_or("")) {
                return bad(format!("image digest {digest:?} is not a sha256"));
            }
        }

        unique(self.ports.iter().map(|p| p.name.as_str()), "port", &who)?;
        unique(self.volumes.iter().map(|v| v.name.as_str()), "volume", &who)?;
        unique(self.files.iter().map(|f| f.name.as_str()), "file", &who)?;
        unique(
            self.settings.iter().map(|s| s.key.as_str()),
            "setting",
            &who,
        )?;

        if self.ports.iter().filter(|p| p.primary).count() > 1 {
            return bad("more than one port is primary, so a connection string \
                        would be built from whichever was listed first"
                .into());
        }
        for port in &self.ports {
            if port.container == 0 || port.preferred == 0 {
                return bad(format!("port {:?} has a zero on it", port.name));
            }
        }

        if let Some(c) = &self.connection {
            if self.port(&c.port).is_none() {
                return bad(format!(
                    "connection is built from port {:?}, which this manifest does not declare",
                    c.port
                ));
            }
            for key in [&c.user_setting, &c.password_setting, &c.database_setting]
                .into_iter()
                .flatten()
            {
                if self.setting(key).is_none() {
                    return bad(format!(
                        "connection names setting {key:?}, which this manifest does not declare"
                    ));
                }
            }
        }
        if let Some(u) = &self.url {
            if self.port(&u.port).is_none() {
                return bad(format!(
                    "the router forwards to port {:?}, which this manifest does not declare",
                    u.port
                ));
            }
            if !is_id(&u.subdomain) {
                return bad(format!("subdomain {:?} is not a DNS label", u.subdomain));
            }
        }

        // The security boundary. Both of these are a valid JSON string and a
        // valid relative path, and a package carrying one is arbitrary file
        // write with a database's name on it.
        checked_relative(&self.compose.file, &who)?;
        if !is_sha256(&self.compose.sha256) {
            return bad("the compose fragment's sha256 is not a sha256".into());
        }
        for file in &self.files {
            let Some(rest) = file.template.strip_prefix("files/") else {
                return bad(format!(
                    "file {:?} points at {:?}, and a package may only ship files/ ",
                    file.name, file.template
                ));
            };
            checked_relative(rest, &who)?;
            if !file.target.starts_with('/') {
                return bad(format!(
                    "file {:?} mounts at {:?}, which is not an absolute path inside the container",
                    file.name, file.target
                ));
            }
            if !is_sha256(&file.sha256) {
                return bad(format!("file {:?} has no usable sha256", file.name));
            }
        }

        for setting in &self.settings {
            if !matches!(
                setting.kind.as_str(),
                "string" | "secret" | "int" | "bool" | "enum" | "instanceRef"
            ) {
                return bad(format!(
                    "setting {:?} is of type {:?}, which this build does not know how to \
                     render or store",
                    setting.key, setting.kind
                ));
            }
        }

        if let Some(health) = &self.health {
            if health.test.is_empty() {
                return bad(
                    "declares a healthcheck with no test, which reports healthy \
                            the instant the container starts"
                        .into(),
                );
            }
        }

        if !matches!(
            self.support.status.as_str(),
            "supported" | "deprecated" | "eol"
        ) {
            return bad(format!("support status {:?}", self.support.status));
        }

        for companion in &self.companions {
            if !is_id(&companion.name) {
                return bad(format!("companion {:?} is not a DNS label", companion.name));
            }
            checked_relative(&companion.compose.file, &who)?;
        }

        Ok(())
    }
}

/// A DNS label: what a container name and a subdomain both have to be.
fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 63
        && text.starts_with(|c: char| c.is_ascii_lowercase())
        && text.ends_with(|c: char| c.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}

/// A path a package may write: relative, no traversal, no separators it should
/// not have, nothing absolute.
fn checked_relative(path: &str, who: &str) -> Result<()> {
    let bad = |m: &str| {
        Err(
            Error::new(Code::InvalidManifest, format!("{who}: {path:?} {m}"))
                .with_hint(crate::hints::PACKAGE_PATHS_STAY_INSIDE),
        )
    };
    if path.is_empty() {
        return bad("is empty");
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return bad("is absolute");
    }
    // A Windows drive letter is not caught by the leading-slash rule.
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return bad("names a drive");
    }
    for part in path.split(['/', '\\']) {
        if part == ".." {
            return bad("walks out of the package");
        }
        if part.is_empty() || part == "." {
            return bad("has an empty path segment");
        }
    }
    Ok(())
}

fn unique<'a>(names: impl Iterator<Item = &'a str>, what: &str, who: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(Error::new(
                Code::InvalidManifest,
                format!("{who}: {what} {name:?} is declared twice"),
            ));
        }
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Where manifests come from.
///
/// A trait rather than a path, because the answer differs by phase and nothing
/// above it should know which: a fixture in a test, [`Tree`] over a directory
/// today, and the verified market cache once fetching exists. The one property
/// every implementation owes its callers is that a `Manifest` it hands back has
/// already been checked — a caller cannot tell where it came from, so it cannot
/// be expected to know whether to trust it.
pub trait Catalogue {
    /// Every service the catalogue knows, in any order.
    fn services(&self) -> Vec<String>;
    /// The concrete versions of one service, newest first.
    fn versions(&self, service: &str) -> Vec<String>;
    /// What `latest` means for this service today.
    fn recommended(&self, service: &str) -> Option<String>;
    fn manifest(&self, service: &str, version: &str) -> Option<Manifest>;
    /// A file the package ships, by the relative path its manifest names.
    ///
    /// The catalogue owns where the bytes are, so a renderer asks rather than
    /// joining paths itself — and an implementation with no directory at all
    /// (a fixture, a bundle held in memory) can answer without pretending to
    /// have one.
    fn file(&self, service: &str, version: &str, relative: &str) -> Option<String>;
}

/// A service's identity — `package.json`, the half that is true across versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub api_version: String,
    pub service: String,
    pub category: String,
    #[serde(default)]
    pub name: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub summary: std::collections::BTreeMap<String, String>,
    /// Which version `latest` resolves to (ADR 0014).
    #[serde(default)]
    pub recommended_version: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub maintainer: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// The `SERVICE_<ID>_` prefix this service used before packages existed.
    #[serde(default)]
    pub legacy_env_prefix: Option<String>,
}

/// A package tree on disk: `<root>/packages/<category>/<service>/versions/…`.
///
/// Scanned once at [`Tree::open`] so that listing a catalogue is not a
/// directory walk per question, and **verified on every read** rather than at
/// scan time. Those are different guarantees and only the second one is worth
/// having: a tree that was intact when the app started is not the same claim as
/// a package whose bytes match the manifest at the moment it is rendered.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    entries: std::collections::BTreeMap<String, TreeEntry>,
}

#[derive(Debug, Clone)]
struct TreeEntry {
    identity: Identity,
    /// version → the directory holding it.
    versions: std::collections::BTreeMap<String, std::path::PathBuf>,
}

impl Tree {
    /// Scan `<root>/packages`. An absent directory is an empty tree, not an
    /// error: a workspace that has installed nothing has no packages, and that
    /// is the state ADR 0011 leaves a fresh machine in.
    pub fn open(root: &Path) -> Result<Self> {
        let mut tree = Self::default();
        let packages = root.join("packages");
        if !packages.is_dir() {
            return Ok(tree);
        }

        for category in read_dirs(&packages)? {
            for service_dir in read_dirs(&category)? {
                let identity_path = service_dir.join("package.json");
                if !identity_path.is_file() {
                    continue;
                }
                let text = std::fs::read_to_string(&identity_path).map_err(|e| {
                    Error::new(
                        Code::IoError,
                        format!("reading {}: {e}", identity_path.display()),
                    )
                })?;
                let identity: Identity = serde_json::from_str(&text).map_err(|e| {
                    Error::new(
                        Code::InvalidManifest,
                        format!("{}: {e}", identity_path.display()),
                    )
                })?;

                // The directory is the identity, and a mismatch is not a
                // detail: every derived name — instance slug, container,
                // volume — comes from the id, so a package found under one
                // name and calling itself another is a package that installs
                // as something else.
                let dir_name = service_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if identity.service != dir_name {
                    return Err(Error::new(
                        Code::InvalidManifest,
                        format!(
                            "{} calls itself {:?}",
                            service_dir.display(),
                            identity.service
                        ),
                    ));
                }

                let mut versions = std::collections::BTreeMap::new();
                for version_dir in read_dirs(&service_dir.join("versions")).unwrap_or_default() {
                    if !version_dir.join("manifest.json").is_file() {
                        continue;
                    }
                    if let Some(name) = version_dir.file_name().and_then(|n| n.to_str()) {
                        versions.insert(name.to_string(), version_dir.clone());
                    }
                }
                if versions.is_empty() {
                    continue;
                }

                tree.entries
                    .insert(identity.service.clone(), TreeEntry { identity, versions });
            }
        }

        Ok(tree)
    }

    pub fn identity(&self, service: &str) -> Option<&Identity> {
        self.entries.get(service).map(|e| &e.identity)
    }

    pub fn dir(&self, service: &str, version: &str) -> Option<&Path> {
        self.entries
            .get(service)?
            .versions
            .get(version)
            .map(|p| p.as_path())
    }

    /// Read a manifest and check that the package's bytes are the ones it
    /// describes. The error says which file and both hashes.
    pub fn load(&self, service: &str, version: &str) -> Result<Manifest> {
        let dir = self
            .dir(service, version)
            .ok_or_else(|| Error::not_found(format!("package {service}@{version}")))?;
        let manifest = load(dir)?;
        verify(dir, &manifest)?;
        Ok(manifest)
    }
}

/// Sorted, so a scan produces the same order on every filesystem.
fn read_dirs(path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(path)
        .map_err(|e| Error::new(Code::IoError, format!("reading {}: {e}", path.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if path.is_dir() && !hidden {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

impl Catalogue for Tree {
    fn services(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Newest first, and "newest" is the order the app already uses everywhere
    /// else: `contracts::cmp_php_version`, which compares dotted numbers as
    /// numbers rather than as strings — so 10 sorts above 9 and `RELEASE.2025-…`
    /// falls back to a string compare that happens to be chronological.
    fn versions(&self, service: &str) -> Vec<String> {
        let Some(entry) = self.entries.get(service) else {
            return Vec::new();
        };
        let mut out: Vec<String> = entry.versions.keys().cloned().collect();
        out.sort_by(|a, b| crate::contracts::cmp_php_version(b, a));
        out
    }

    fn recommended(&self, service: &str) -> Option<String> {
        self.entries
            .get(service)?
            .identity
            .recommended_version
            .clone()
    }

    fn manifest(&self, service: &str, version: &str) -> Option<Manifest> {
        self.load(service, version).ok()
    }

    /// Checked again on the way out, even though every path in a manifest has
    /// already been through `checked_relative`. This is the call that turns a
    /// string into a filesystem read, and a check at the point of the read is
    /// the one that is still there after somebody refactors the parser.
    fn file(&self, service: &str, version: &str, relative: &str) -> Option<String> {
        checked_relative(relative, "package file").ok()?;
        let dir = self.dir(service, version)?;
        std::fs::read_to_string(dir.join(relative)).ok()
    }
}

/// Parse and check, in that order, with the second failure naming the package.
pub fn parse(text: &str) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("manifest is unreadable: {e}"),
        )
    })?;
    manifest.check()?;
    Ok(manifest)
}

/// Read `<dir>/manifest.json`.
pub fn load(dir: &Path) -> Result<Manifest> {
    let file = dir.join("manifest.json");
    let text = std::fs::read_to_string(&file)
        .map_err(|e| Error::new(Code::NotFound, format!("reading {}: {e}", file.display())))?;
    parse(&text)
}

/// Do the bytes on disk match what the manifest says they are?
///
/// The last link of the chain: a signature vouches for the registry, the
/// registry states the manifest's hash, and the manifest states these. Checked
/// on **every read** rather than only at install, because the point of writing
/// a hash down is to catch the change nobody announced — a half-finished
/// download, a disk that lied, an editor opened in the wrong window.
pub fn verify(dir: &Path, manifest: &Manifest) -> Result<()> {
    let check = |relative: &str, expected: &str, what: &str| -> Result<()> {
        let path = dir.join(relative);
        let bytes = std::fs::read(&path).map_err(|e| {
            Error::new(
                Code::NotFound,
                format!(
                    "{}@{}: reading {what}: {e}",
                    manifest.service, manifest.version
                ),
            )
        })?;
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(Error::new(
                Code::InvalidManifest,
                format!(
                    "{}@{}: {what} does not match the manifest — expected {expected}, \
                     found {actual}",
                    manifest.service, manifest.version
                ),
            )
            .with_hint(crate::hints::PACKAGE_CONTENT_CHANGED));
        }
        Ok(())
    };

    check(
        &manifest.compose.file,
        &manifest.compose.sha256,
        "the compose fragment",
    )?;
    for file in &manifest.files {
        check(&file.template, &file.sha256, &file.template)?;
    }
    for companion in &manifest.companions {
        check(
            &companion.compose.file,
            &companion.compose.sha256,
            &companion.compose.file,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal manifest, as JSON, so the tests exercise the parser rather
    /// than a struct literal that skips it.
    fn json(patch: &[(&str, &str)]) -> String {
        let mut fields: Vec<(String, String)> = [
            ("apiVersion", format!("\"{API_VERSION}\"")),
            ("service", "\"mysql\"".into()),
            ("version", "\"8.0\"".into()),
            (
                "image",
                "{\"repository\": \"mysql\", \"tag\": \"8.0\"}".into(),
            ),
            ("instancing", "{\"multiple\": true}".into()),
            (
                "ports",
                "[{\"name\": \"main\", \"container\": 3306, \"preferred\": 3306, \"primary\": true}]"
                    .into(),
            ),
            (
                "settings",
                "[{\"key\": \"ROOT_PASSWORD\", \"type\": \"secret\", \"default\": \"root\"}]".into(),
            ),
            (
                "compose",
                format!("{{\"file\": \"compose.yml.tpl\", \"sha256\": \"{}\"}}", "a".repeat(64)),
            ),
            ("support", "{\"status\": \"supported\"}".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();

        for (key, value) in patch {
            match fields.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value.to_string(),
                None => fields.push((key.to_string(), value.to_string())),
            }
        }

        let body: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("\"{k}\": {v}"))
            .collect();
        format!("{{{}}}", body.join(", "))
    }

    #[test]
    fn a_well_formed_manifest_parses_and_keeps_its_fields() {
        let m = parse(&json(&[])).unwrap();
        assert_eq!(m.service, "mysql");
        assert_eq!(m.version, "8.0");
        assert_eq!(m.image.reference(), "mysql:8.0");
        assert!(m.instancing.multiple);
        assert_eq!(m.instancing.identity, "version");
        assert!(m.setting("ROOT_PASSWORD").unwrap().is_secret());
        assert_eq!(m.port("main").unwrap().container, 3306);
        // Defaults that are not written are still the documented ones.
        assert_eq!(m.ports[0].protocol, "tcp");
    }

    /// A digest names bytes; a tag names a name. When both are present the
    /// bytes win, because that is the whole reason to record one.
    #[test]
    fn a_digest_beats_the_tag_and_a_registry_is_carried() {
        let m = parse(&json(&[(
            "image",
            &format!(
                "{{\"registry\": \"docker.elastic.co\", \"repository\": \"elasticsearch/elasticsearch\", \
                  \"tag\": \"8.11.3\", \"digest\": \"sha256:{}\"}}",
                "b".repeat(64)
            ),
        )]))
        .unwrap();
        assert_eq!(
            m.image.reference(),
            format!(
                "docker.elastic.co/elasticsearch/elasticsearch@sha256:{}",
                "b".repeat(64)
            )
        );
    }

    /// `docker.io` is the default and naming it produces a longer reference
    /// that means the same thing — which is a diff nobody wants to review.
    #[test]
    fn the_default_registry_is_not_written_into_the_reference() {
        let m = parse(&json(&[(
            "image",
            "{\"registry\": \"docker.io\", \"repository\": \"mysql\", \"tag\": \"8.0\"}",
        )]))
        .unwrap();
        assert_eq!(m.image.reference(), "mysql:8.0");
    }

    #[test]
    fn a_manifest_from_another_contract_is_refused_whole() {
        let err = parse(&json(&[("apiVersion", "\"stackvo.dev/package/v2\"")])).unwrap_err();
        assert_eq!(err.code, Code::InvalidManifest);
        assert!(err.message.contains("v2"), "{}", err.message);
    }

    #[test]
    fn a_moving_tag_cannot_be_a_version() {
        for tag in MOVING_TAGS {
            let err = parse(&json(&[("version", &format!("\"{tag}\""))])).unwrap_err();
            assert!(err.message.contains("moving tag"), "{tag}: {}", err.message);
        }
    }

    // ---- the path rules, which are the security boundary ------------------

    #[test]
    fn a_file_that_walks_out_of_the_package_is_refused() {
        for template in [
            "files/../../../../etc/cron.d/x",
            "files/..%2Fx",
            "files/a/../../b",
        ] {
            let entry = format!(
                "[{{\"name\": \"x\", \"template\": \"{template}\", \"target\": \"/etc/x\", \
                  \"sha256\": \"{}\"}}]",
                "c".repeat(64)
            );
            let result = parse(&json(&[("files", &entry)]));
            // `..%2Fx` is not traversal once it is a path — it is a filename
            // with odd characters — so the two outcomes differ and both are
            // safe. What must never happen is a `..` segment surviving.
            if let Ok(m) = result {
                assert!(
                    !m.files[0].template.split('/').any(|p| p == ".."),
                    "{template} was accepted with a traversal segment"
                );
            }
        }
    }

    #[test]
    fn a_file_outside_the_files_directory_is_refused() {
        let entry = format!(
            "[{{\"name\": \"x\", \"template\": \"compose.yml.tpl\", \"target\": \"/etc/x\", \
              \"sha256\": \"{}\"}}]",
            "c".repeat(64)
        );
        assert!(parse(&json(&[("files", &entry)])).is_err());
    }

    #[test]
    fn an_absolute_compose_path_is_refused() {
        for file in ["/etc/passwd", "C:\\windows\\x", "..\\..\\x"] {
            let compose = format!(
                "{{\"file\": \"{}\", \"sha256\": \"{}\"}}",
                file.replace('\\', "\\\\"),
                "a".repeat(64)
            );
            assert!(
                parse(&json(&[("compose", &compose)])).is_err(),
                "{file} was accepted"
            );
        }
    }

    #[test]
    fn a_mount_target_that_is_not_absolute_is_refused() {
        let entry = format!(
            "[{{\"name\": \"x\", \"template\": \"files/x.tpl\", \"target\": \"etc/x\", \
              \"sha256\": \"{}\"}}]",
            "c".repeat(64)
        );
        assert!(parse(&json(&[("files", &entry)])).is_err());
    }

    // ---- the cross-references JSON Schema cannot express ------------------

    #[test]
    fn a_connection_built_from_a_port_that_does_not_exist_is_refused() {
        let err = parse(&json(&[(
            "connection",
            "{\"scheme\": \"mysql\", \"port\": \"sql\"}",
        )]))
        .unwrap_err();
        assert!(err.message.contains("does not declare"), "{}", err.message);
    }

    #[test]
    fn a_connection_naming_a_setting_that_does_not_exist_is_refused() {
        let err = parse(&json(&[(
            "connection",
            "{\"scheme\": \"mysql\", \"port\": \"main\", \"passwordSetting\": \"NOPE\"}",
        )]))
        .unwrap_err();
        assert!(err.message.contains("NOPE"), "{}", err.message);
    }

    #[test]
    fn a_router_forwarding_to_a_port_that_does_not_exist_is_refused() {
        assert!(parse(&json(&[(
            "url",
            "{\"subdomain\": \"phpmyadmin\", \"port\": \"web\"}"
        )]))
        .is_err());
    }

    #[test]
    fn two_primary_ports_are_refused() {
        let ports = "[{\"name\": \"a\", \"container\": 1, \"preferred\": 1, \"primary\": true}, \
                     {\"name\": \"b\", \"container\": 2, \"preferred\": 2, \"primary\": true}]";
        assert!(parse(&json(&[("ports", ports)])).is_err());
    }

    #[test]
    fn a_duplicate_handle_is_refused() {
        let ports = "[{\"name\": \"main\", \"container\": 1, \"preferred\": 1}, \
                     {\"name\": \"main\", \"container\": 2, \"preferred\": 2}]";
        let err = parse(&json(&[("ports", ports)])).unwrap_err();
        assert!(err.message.contains("twice"), "{}", err.message);
    }

    #[test]
    fn a_setting_type_this_build_cannot_render_is_refused() {
        let settings = "[{\"key\": \"X\", \"type\": \"lua\"}]";
        assert!(parse(&json(&[("settings", settings)])).is_err());
    }

    // ---- verify -----------------------------------------------------------

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-pkg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn verify_accepts_the_bytes_the_manifest_describes() {
        let dir = scratch("ok");
        let body = "image: \"{{ image }}\"\n";
        std::fs::write(dir.join("compose.yml.tpl"), body).unwrap();

        let compose = format!(
            "{{\"file\": \"compose.yml.tpl\", \"sha256\": \"{}\"}}",
            sha256_hex(body.as_bytes())
        );
        let manifest = parse(&json(&[("compose", &compose)])).unwrap();
        verify(&dir, &manifest).unwrap();
    }

    /// One byte changed, and the message says which file and both hashes —
    /// because the next question is always "changed how".
    #[test]
    fn verify_refuses_a_fragment_that_has_been_edited() {
        let dir = scratch("edited");
        let body = "image: \"{{ image }}\"\n";
        let compose = format!(
            "{{\"file\": \"compose.yml.tpl\", \"sha256\": \"{}\"}}",
            sha256_hex(body.as_bytes())
        );
        let manifest = parse(&json(&[("compose", &compose)])).unwrap();

        std::fs::write(dir.join("compose.yml.tpl"), "image: \"evil\"\n").unwrap();
        let err = verify(&dir, &manifest).unwrap_err();
        assert!(err.message.contains("compose fragment"), "{}", err.message);
        assert!(err.message.contains("expected"), "{}", err.message);
    }

    #[test]
    fn verify_refuses_a_package_with_a_file_missing() {
        let dir = scratch("missing");
        let manifest = parse(&json(&[])).unwrap();
        assert_eq!(verify(&dir, &manifest).unwrap_err().code, Code::NotFound);
    }

    // ---- the tree ---------------------------------------------------------

    /// Write the smallest package a tree can hold, hashes and all.
    fn plant(root: &std::path::Path, service: &str, version: &str, recommended: &str) {
        let dir = root
            .join("packages/databases")
            .join(service)
            .join("versions")
            .join(version);
        std::fs::create_dir_all(&dir).unwrap();

        let fragment = format!("image: \"{{{{ image }}}}\"\n# {service} {version}\n");
        std::fs::write(dir.join("compose.yml.tpl"), &fragment).unwrap();

        let manifest = format!(
            r#"{{"apiVersion": "{}", "service": "{service}", "version": "{version}",
                "image": {{"repository": "{service}", "tag": "{version}"}},
                "instancing": {{"multiple": true}},
                "compose": {{"file": "compose.yml.tpl", "sha256": "{}"}},
                "support": {{"status": "supported"}}}}"#,
            API_VERSION,
            sha256_hex(fragment.as_bytes())
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();

        let identity = format!(
            r#"{{"apiVersion": "{API_VERSION}", "service": "{service}",
                "category": "databases", "name": {{"en": "{service}"}},
                "recommendedVersion": "{recommended}"}}"#
        );
        std::fs::write(
            root.join("packages/databases")
                .join(service)
                .join("package.json"),
            identity,
        )
        .unwrap();
    }

    #[test]
    fn a_tree_lists_what_it_holds_and_reads_it_back() {
        let root = scratch("tree");
        plant(&root, "mysql", "8.0", "8.0");
        plant(&root, "mysql", "9.4", "8.0");

        let tree = Tree::open(&root).unwrap();
        assert_eq!(tree.services(), ["mysql"]);
        assert_eq!(tree.recommended("mysql").as_deref(), Some("8.0"));

        let manifest = tree.manifest("mysql", "9.4").expect("a package it listed");
        assert_eq!(manifest.image.reference(), "mysql:9.4");
    }

    /// Newest first, and numerically — a string sort puts 10 before 9.
    #[test]
    fn versions_come_back_newest_first() {
        let root = scratch("order");
        for v in ["8.0", "9.4", "10.11", "5.7"] {
            plant(&root, "mariadb", v, "10.11");
        }
        let tree = Tree::open(&root).unwrap();
        assert_eq!(tree.versions("mariadb"), ["10.11", "9.4", "8.0", "5.7"]);
    }

    /// A package whose bytes have changed is not handed out.
    ///
    /// This is the one that matters: the tree is scanned once and read many
    /// times, and verifying at scan time would mean an edit after startup went
    /// straight into a compose file.
    #[test]
    fn an_edited_fragment_is_refused_at_read_time_not_at_scan_time() {
        let root = scratch("tree-edited");
        plant(&root, "mysql", "8.0", "8.0");
        let tree = Tree::open(&root).expect("the tree is intact when it is scanned");

        std::fs::write(
            root.join("packages/databases/mysql/versions/8.0/compose.yml.tpl"),
            "image: \"evil\"\n",
        )
        .unwrap();

        let err = tree.load("mysql", "8.0").unwrap_err();
        assert_eq!(err.code, Code::InvalidManifest);
        assert!(err.message.contains("compose fragment"), "{}", err.message);
        // And the trait's answer is the same refusal, expressed as absence.
        assert!(tree.manifest("mysql", "8.0").is_none());
    }

    /// A directory and the identity inside it must agree, because every derived
    /// name comes from the id.
    #[test]
    fn a_package_that_calls_itself_something_else_is_refused() {
        let root = scratch("misnamed");
        plant(&root, "mysql", "8.0", "8.0");
        let identity = root.join("packages/databases/mysql/package.json");
        let text = std::fs::read_to_string(&identity)
            .unwrap()
            .replace("\"service\": \"mysql\"", "\"service\": \"mariadb\"");
        std::fs::write(&identity, text).unwrap();

        assert!(Tree::open(&root).is_err());
    }

    /// A machine that has installed nothing has no packages, and that is a
    /// state rather than a failure (ADR 0011).
    #[test]
    fn a_root_with_no_packages_is_an_empty_tree() {
        let root = scratch("empty");
        let tree = Tree::open(&root).expect("nothing installed is not an error");
        assert!(tree.services().is_empty());
    }

    #[test]
    fn the_known_hash_of_a_known_string() {
        // The published SHA-256 of "abc", so a broken hash shows up here rather
        // than as "every package is corrupt".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
