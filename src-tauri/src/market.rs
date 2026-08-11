//! Getting a package onto this machine, and being able to say why you believe
//! it.
//!
//! Faz 4 of `docs/servis-market-mimarisi.md`. [`crate::pkg`] reads a package
//! that is already here and [`crate::render`] turns it into a compose file;
//! this is the step in front of both — where bytes somebody else wrote first
//! arrive.
//!
//! ## The chain, and which link this module owns
//!
//! ```text
//!   a pinned key          →  registry.json          (trust, not yet written)
//!   registry.json         →  manifest.json          (here)
//!   manifest.json         →  every file it ships    (pkg::verify)
//! ```
//!
//! The middle link is this module's: the index states a `manifestSha256`, and a
//! manifest that does not hash to it is never parsed — refused as bytes, before
//! any field of it has been read. That ordering is the point. A manifest is
//! parsed by code that trusts its shape, and the cheapest way to keep that
//! trust honest is to compare the bytes first.
//!
//! ## What is missing, and it is the first link
//!
//! Nothing verifies a **signature** yet, because there is no key to verify
//! against: ADR 0015 says the registry gets its own ed25519 key, and
//! `docs/durum.md` §5 records the ceremony that would produce one as an open
//! decision. Writing a placeholder key here would be worse than the gap — it
//! would make every later reader believe the chain was closed.
//!
//! So [`Trust`] is the shape of that link, `Trust::Unsigned` is the only value
//! it can take today, and [`refresh`] **refuses** when a caller asks for a
//! signed index. A machine using a local source is unaffected; a machine
//! pointed at a network source will be, and that is the correct order — the
//! network source is Faz 5.
//!
//! ## Install is atomic or it did not happen
//!
//! A package is unpacked into a scratch directory beside its destination,
//! verified whole, and only then moved into place. A half-written package is
//! the one failure mode a client cannot recover from on its own: `pkg::verify`
//! would refuse it forever, and the user would have a service that is installed
//! and cannot start with no way to tell which file is short.

use crate::error::{Code, Error, Result};
use crate::pkg;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `<root>/market`.
pub fn dir(root: &Path) -> PathBuf {
    root.join("market")
}

/// Where the index is cached once it has been fetched.
///
/// Absent before the first refresh, and that absence is a **state** rather than
/// an error (ADR 0011): a machine that has never fetched has no catalogue, and
/// the app says so rather than showing an empty one.
pub fn registry_path(root: &Path) -> PathBuf {
    dir(root).join("registry.json")
}

/// Where verified packages live.
pub fn packages_dir(root: &Path) -> PathBuf {
    dir(root).join("packages")
}

// ---------------------------------------------------------------- the index

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRow {
    pub version: String,
    pub path: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub recommended: bool,
    pub support: String,
    #[serde(default)]
    pub eol_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRow {
    pub service: String,
    pub category: String,
    #[serde(default)]
    pub name: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub summary: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub instancing: Option<Instancing>,
    #[serde(default)]
    pub legacy_env_prefix: Option<String>,
    pub versions: Vec<VersionRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instancing {
    pub multiple: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub schema_version: u32,
    pub sequence: u64,
    pub generated_at: String,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub packages: Vec<PackageRow>,
}

impl Registry {
    pub fn package(&self, service: &str) -> Option<&PackageRow> {
        self.packages.iter().find(|p| p.service == service)
    }

    pub fn version(&self, service: &str, version: &str) -> Option<&VersionRow> {
        self.package(service)?
            .versions
            .iter()
            .find(|v| v.version == version)
    }

    /// What `latest` means, per ADR 0014.
    pub fn recommended(&self, service: &str) -> Option<&VersionRow> {
        self.package(service)?
            .versions
            .iter()
            .find(|v| v.recommended)
    }

    /// Everything JSON Schema cannot say about an index.
    fn check(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(Error::new(
                Code::Unsupported,
                format!(
                    "the index is schema version {} and this build reads 1",
                    self.schema_version
                ),
            ));
        }
        for package in &self.packages {
            let recommended = package.versions.iter().filter(|v| v.recommended).count();
            if recommended != 1 {
                return Err(Error::new(
                    Code::InvalidManifest,
                    format!(
                        "{} has {recommended} recommended version(s) — `latest` resolves to \
                         exactly one, and an index that cannot say which is one a migration \
                         cannot read",
                        package.service
                    ),
                ));
            }
            for version in &package.versions {
                // Anchored so a crafted index cannot walk out of the package
                // tree when a path is joined onto a local directory.
                let expected = format!(
                    "packages/{}/{}/versions/{}",
                    package.category, package.service, version.version
                );
                if version.path != expected {
                    return Err(Error::new(
                        Code::InvalidManifest,
                        format!(
                            "{}@{} is at {:?} and its own fields say {expected:?}",
                            package.service, version.version, version.path
                        ),
                    ));
                }
                if pkg::is_moving_tag(&version.version) {
                    return Err(Error::new(
                        Code::InvalidManifest,
                        format!(
                            "{} offers {:?} as a version",
                            package.service, version.version
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- the source

/// Where bytes come from.
///
/// A trait because the answer changes in Faz 5 and nothing above it should
/// notice: a directory today, HTTPS then, and an offline bundle for a machine
/// with no network at all — which ADR 0011 leaves as the **only** way such a
/// machine gets a catalogue.
pub trait Source {
    /// A name for messages: a path, a URL.
    fn describe(&self) -> String;
    /// One file, by its path relative to the source's root.
    fn fetch(&self, relative: &str) -> Result<Vec<u8>>;
}

/// A directory. Used by the offline bundle and by every test in this module.
#[derive(Debug, Clone)]
pub struct LocalSource {
    root: PathBuf,
}

impl LocalSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Source for LocalSource {
    fn describe(&self) -> String {
        self.root.display().to_string()
    }

    fn fetch(&self, relative: &str) -> Result<Vec<u8>> {
        // The same rule the manifest's own paths live under. A source is not
        // trusted to say where in the filesystem its files are, even when it is
        // a directory on this machine: an index is data, and an offline bundle
        // is a file somebody was sent.
        checked_relative(relative)?;
        let path = self.root.join(relative);
        std::fs::read(&path).map_err(|e| {
            Error::new(
                Code::NotFound,
                format!("{}: reading {relative}: {e}", self.describe()),
            )
        })
    }
}

fn checked_relative(path: &str) -> Result<()> {
    let bad = |why: &str| {
        Err(Error::new(Code::InvalidInput, format!("{path:?} {why}"))
            .with_hint(crate::hints::PACKAGE_PATHS_STAY_INSIDE))
    };
    if path.is_empty() {
        return bad("is empty");
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return bad("is absolute");
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return bad("names a drive");
    }
    for part in path.split(['/', '\\']) {
        if part == ".." || part.is_empty() || part == "." {
            return bad("walks out of the source");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- trust

/// How much of the chain of trust a refresh is asked to check.
///
/// One variant today, and the enum exists so the call sites that will need the
/// other one are already written against something. See the module comment:
/// there is no published key, and ADR 0015's ceremony is an open decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Accept an index on the strength of where it came from.
    ///
    /// Honest for a local directory the user chose, and for the offline bundle
    /// they were handed. Not honest for a network source, which is why Faz 5
    /// cannot ship before the other variant exists.
    Unsigned,
    /// Require a signature from a pinned key. **Not implemented**, and
    /// [`refresh`] says so rather than quietly downgrading — a security check
    /// that silently does nothing is worse than one that is absent, because
    /// the absent one is visible.
    Signed,
}

/// Which source this workspace last fetched from.
///
/// Remembered because installing happens after refreshing, often much later,
/// and asking the user twice for the same directory is asking them to get it
/// right twice. Stored beside the cached index rather than in `.env`: it is
/// application state, not a decision somebody wants to keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// `local` today. The field exists so Faz 5's `https` is a value rather
    /// than a schema change.
    pub kind: String,
    pub location: String,
}

fn source_ref_path(root: &Path) -> PathBuf {
    dir(root).join("source.json")
}

pub fn remember(root: &Path, reference: &SourceRef) -> Result<()> {
    let path = source_ref_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::new(Code::IoError, format!("creating {}: {e}", parent.display()))
        })?;
    }
    let text = serde_json::to_string_pretty(reference)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the source: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))
}

pub fn remembered(root: &Path) -> Result<Option<SourceRef>> {
    match std::fs::read_to_string(source_ref_path(root)) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| Error::new(Code::InvalidManifest, format!("market/source.json: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::new(
            Code::IoError,
            format!("reading the source: {e}"),
        )),
    }
}

/// Turn a remembered reference back into something that can fetch.
pub fn open(reference: &SourceRef) -> Result<Box<dyn Source>> {
    match reference.kind.as_str() {
        "local" => Ok(Box::new(LocalSource::new(&reference.location))),
        other => Err(Error::new(
            Code::Unsupported,
            format!(
                "{other:?} is not a source this build can read. Only `local` exists today: \
                 the network source is Faz 5 and waits on the key ceremony ADR 0015 names"
            ),
        )),
    }
}

// ---------------------------------------------------------------- refreshing

/// Fetch the index, check it, and cache it.
///
/// `previous` is what this machine already has, or `None` on a first refresh.
/// An index that goes backwards is refused: withdrawing a version has to mean
/// something, and replaying yesterday's index is how it stops meaning anything.
pub fn refresh(
    root: &Path,
    source: &dyn Source,
    trust: Trust,
    previous: Option<&Registry>,
) -> Result<Registry> {
    if trust == Trust::Signed {
        return Err(Error::new(
            Code::Unsupported,
            "signature verification is not implemented: no registry key is pinned, and \
             ADR 0015's key ceremony is still an open decision. Refusing rather than \
             accepting an unsigned index under a name that promises otherwise",
        ));
    }

    let bytes = source.fetch("registry.json")?;
    let registry: Registry = serde_json::from_slice(&bytes).map_err(|e| {
        Error::new(
            Code::InvalidManifest,
            format!("{}: registry.json is unreadable: {e}", source.describe()),
        )
    })?;
    registry.check()?;

    if let Some(previous) = previous {
        if registry.sequence < previous.sequence {
            return Err(Error::new(
                Code::Conflict,
                format!(
                    "{} served index {} and this machine already has {} — an index that goes \
                     backwards is how a withdrawn version comes back",
                    source.describe(),
                    registry.sequence,
                    previous.sequence
                ),
            )
            .with_hint(crate::hints::REGISTRY_WENT_BACKWARDS));
        }
    }

    let path = registry_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::new(Code::IoError, format!("creating {}: {e}", parent.display()))
        })?;
    }
    crate::atomic::write(
        &path,
        &String::from_utf8_lossy(
            &serde_json::to_vec_pretty(&registry)
                .map_err(|e| Error::new(Code::IoError, format!("serialising the index: {e}")))?,
        ),
    )?;

    Ok(registry)
}

/// The cached index, or `None` when nothing has been fetched.
pub fn cached(root: &Path) -> Result<Option<Registry>> {
    let path = registry_path(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error::new(
                Code::IoError,
                format!("reading {}: {e}", path.display()),
            ))
        }
    };
    let registry: Registry = serde_json::from_str(&text)
        .map_err(|e| Error::new(Code::InvalidManifest, format!("{}: {e}", path.display())))?;
    registry.check()?;
    Ok(Some(registry))
}

// ---------------------------------------------------------------- installing

/// What an install did, for the caller that has to tell somebody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub service: String,
    pub version: String,
    /// Of the manifest, as the index stated it — recorded so an instance can
    /// say which package it was created against.
    pub sha256: String,
    pub files: usize,
}

/// Fetch one package, verify it whole, and put it where `pkg::Tree` looks.
///
/// Verification happens **before** the package reaches its destination, and in
/// this order: the manifest's bytes against the index, then the manifest's
/// fields against the schema, then every file against the manifest. A failure
/// at any point leaves nothing behind but a scratch directory this removes.
pub fn install(
    root: &Path,
    source: &dyn Source,
    registry: &Registry,
    service: &str,
    version: &str,
) -> Result<Installed> {
    let row = registry.version(service, version).ok_or_else(|| {
        Error::not_found(format!("{service}@{version} in the index"))
            .with_hint(crate::hints::PACKAGE_NOT_IN_REGISTRY)
    })?;

    // ---- the manifest, as bytes first -----------------------------------
    let manifest_bytes = source.fetch(&format!("{}/manifest.json", row.path))?;
    let actual = pkg::sha256_hex(&manifest_bytes);
    if actual != row.manifest_sha256 {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "{service}@{version}: the index says the manifest hashes to {} and it hashes \
                 to {actual} — refused as bytes, before anything read a field of it",
                row.manifest_sha256
            ),
        )
        .with_hint(crate::hints::PACKAGE_CONTENT_CHANGED));
    }

    let text = String::from_utf8(manifest_bytes.clone()).map_err(|_| {
        Error::new(
            Code::InvalidManifest,
            format!("{service}@{version}: the manifest is not UTF-8"),
        )
    })?;
    let manifest = pkg::parse(&text)?;

    if manifest.service != service || manifest.version != version {
        return Err(Error::new(
            Code::InvalidManifest,
            format!(
                "the index lists {service}@{version} and the manifest calls itself {}@{}",
                manifest.service, manifest.version
            ),
        ));
    }

    // ---- into a scratch directory beside the destination -----------------
    let destination = packages_dir(root).join(row.path.trim_start_matches("packages/"));
    let scratch = destination.with_file_name(format!(
        ".{}.incoming",
        destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package")
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("creating {}: {e}", scratch.display()),
        )
    })?;

    let outcome = (|| -> Result<usize> {
        write_into(&scratch, "manifest.json", &manifest_bytes)?;
        let mut files = 1;

        let mut wanted: Vec<String> = vec![manifest.compose.file.clone()];
        wanted.extend(manifest.files.iter().map(|f| f.template.clone()));
        wanted.extend(manifest.companions.iter().map(|c| c.compose.file.clone()));

        for relative in wanted {
            let bytes = source.fetch(&format!("{}/{relative}", row.path))?;
            write_into(&scratch, &relative, &bytes)?;
            files += 1;
        }

        // Every hash the manifest states, against what is now on disk. The same
        // call the tree makes on every read, run once here so a package that
        // would never be readable is never installed.
        pkg::verify(&scratch, &manifest)?;

        // The identity file is NOT written here: it lives a level above the
        // versions and is shared by all of them, so it goes in after the move.
        // Writing it into the scratch directory was the first attempt and it is
        // a good example of why `checked_relative` guards `write_into` — the
        // path it needed was `../../package.json`, which that function refuses,
        // and the call quietly did nothing.
        Ok(files)
    })();

    let files = match outcome {
        Ok(files) => files,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(e);
        }
    };

    // ---- and only now into place ----------------------------------------
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::new(Code::IoError, format!("creating {}: {e}", parent.display()))
        })?;
    }
    let _ = std::fs::remove_dir_all(&destination);
    std::fs::rename(&scratch, &destination).map_err(|e| {
        let _ = std::fs::remove_dir_all(&scratch);
        Error::new(
            Code::IoError,
            format!("moving the package into {}: {e}", destination.display()),
        )
    })?;

    // The identity file sits a level above the versions, shared by all of them.
    let identity_path = destination
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("package.json"));
    if let Some(path) = identity_path {
        if !path.is_file() {
            let category = manifest_category(registry, service);
            let bytes = source.fetch(&format!("packages/{category}/{service}/package.json"))?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&path, bytes).map_err(|e| {
                Error::new(Code::IoError, format!("writing {}: {e}", path.display()))
            })?;
        }
    }

    Ok(Installed {
        service: service.to_string(),
        version: version.to_string(),
        sha256: row.manifest_sha256.clone(),
        files,
    })
}

fn manifest_category(registry: &Registry, service: &str) -> String {
    registry
        .package(service)
        .map(|p| p.category.clone())
        .unwrap_or_default()
}

fn write_into(base: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    checked_relative(relative)?;
    let path = base.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::new(Code::IoError, format!("creating {}: {e}", parent.display()))
        })?;
    }
    std::fs::write(&path, bytes)
        .map_err(|e| Error::new(Code::IoError, format!("writing {}: {e}", path.display())))
}

/// Remove one version's package directory.
///
/// Only the package — not the instance that used it, not its volumes, not its
/// data. ADR 0012 puts data deletion behind `purgeData` on the command above
/// this one, and a module that removed a directory of templates has no business
/// deciding about somebody's database.
pub fn uninstall(root: &Path, category: &str, service: &str, version: &str) -> Result<()> {
    let dir = packages_dir(root)
        .join(category)
        .join(service)
        .join("versions")
        .join(version);
    if !dir.is_dir() {
        return Err(Error::not_found(format!("package {service}@{version}")));
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| Error::new(Code::IoError, format!("removing {}: {e}", dir.display())))?;

    // A service with no versions left keeps no identity file: `pkg::Tree` skips
    // such a directory anyway, and leaving it makes "what is installed" answer
    // differently depending on who is asking.
    let versions = packages_dir(root)
        .join(category)
        .join(service)
        .join("versions");
    let empty = std::fs::read_dir(&versions)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false);
    if empty {
        let _ = std::fs::remove_dir_all(packages_dir(root).join(category).join(service));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::Catalogue;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stackvo-market-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A source directory holding one package and an index that describes it.
    fn publish(root: &Path, sequence: u64) -> PathBuf {
        let source = root.join("source");
        let dir = source.join("packages/databases/mysql/versions/8.0");
        std::fs::create_dir_all(dir.join("files")).unwrap();

        let fragment = "image: \"{{ image }}\"\n";
        let config = "port = {{ port.main }}\n";
        std::fs::write(dir.join("compose.yml.tpl"), fragment).unwrap();
        std::fs::write(dir.join("files/my.cnf.tpl"), config).unwrap();

        let manifest = format!(
            r#"{{"apiVersion": "{}", "service": "mysql", "version": "8.0",
                "image": {{"repository": "mysql", "tag": "8.0"}},
                "instancing": {{"multiple": true}},
                "ports": [{{"name": "main", "container": 3306, "preferred": 3306}}],
                "files": [{{"name": "my_cnf", "template": "files/my.cnf.tpl",
                            "target": "/etc/my.cnf", "sha256": "{}"}}],
                "compose": {{"file": "compose.yml.tpl", "sha256": "{}"}},
                "support": {{"status": "supported"}}}}"#,
            pkg::API_VERSION,
            pkg::sha256_hex(config.as_bytes()),
            pkg::sha256_hex(fragment.as_bytes())
        );
        std::fs::write(dir.join("manifest.json"), &manifest).unwrap();
        std::fs::write(
            source.join("packages/databases/mysql/package.json"),
            format!(
                r#"{{"apiVersion": "{}", "service": "mysql", "category": "databases",
                    "name": {{"en": "MySQL"}}, "recommendedVersion": "8.0"}}"#,
                pkg::API_VERSION
            ),
        )
        .unwrap();

        let registry = format!(
            r#"{{"schemaVersion": 1, "sequence": {sequence},
                "generatedAt": "2026-08-11T09:00:00Z",
                "packages": [{{"service": "mysql", "category": "databases",
                    "name": {{"en": "MySQL"}},
                    "versions": [{{"version": "8.0",
                        "path": "packages/databases/mysql/versions/8.0",
                        "manifestSha256": "{}",
                        "recommended": true, "support": "supported"}}]}}]}}"#,
            pkg::sha256_hex(manifest.as_bytes())
        );
        std::fs::write(source.join("registry.json"), registry).unwrap();
        source
    }

    #[test]
    fn a_refresh_caches_an_index_it_could_read() {
        let root = scratch("refresh");
        let source = LocalSource::new(publish(&root, 1));

        assert!(cached(&root).unwrap().is_none(), "nothing fetched yet");
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();
        assert_eq!(registry.sequence, 1);
        assert_eq!(registry.recommended("mysql").unwrap().version, "8.0");
        assert_eq!(cached(&root).unwrap().unwrap(), registry);
    }

    /// Withdrawing a version has to mean something.
    #[test]
    fn an_index_that_goes_backwards_is_refused() {
        let root = scratch("replay");
        let newer = refresh(
            &root,
            &LocalSource::new(publish(&root, 7)),
            Trust::Unsigned,
            None,
        )
        .unwrap();

        let older = scratch("replay-old");
        let source = LocalSource::new(publish(&older, 3));
        let err = refresh(&root, &source, Trust::Unsigned, Some(&newer)).unwrap_err();
        assert_eq!(err.code, Code::Conflict);
        assert!(err.message.contains("backwards"), "{}", err.message);
    }

    /// The same sequence is a re-fetch, not a replay.
    #[test]
    fn the_same_index_can_be_fetched_again() {
        let root = scratch("again");
        let source = LocalSource::new(publish(&root, 4));
        let first = refresh(&root, &source, Trust::Unsigned, None).unwrap();
        refresh(&root, &source, Trust::Unsigned, Some(&first)).unwrap();
    }

    /// A security check that silently does nothing is worse than one that is
    /// absent, because the absent one is visible.
    #[test]
    fn asking_for_a_signed_index_is_refused_rather_than_downgraded() {
        let root = scratch("signed");
        let source = LocalSource::new(publish(&root, 1));
        let err = refresh(&root, &source, Trust::Signed, None).unwrap_err();
        assert_eq!(err.code, Code::Unsupported);
        assert!(err.message.contains("ADR 0015"), "{}", err.message);
    }

    #[test]
    fn installing_puts_a_package_where_the_tree_finds_it() {
        let root = scratch("install");
        let source = LocalSource::new(publish(&root, 1));
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        let done = install(&root, &source, &registry, "mysql", "8.0").unwrap();
        assert_eq!(done.files, 3, "manifest, fragment, config");

        let tree = pkg::Tree::open(&dir(&root)).unwrap();
        assert_eq!(tree.services(), ["mysql"]);
        let manifest = tree.manifest("mysql", "8.0").expect("verified on read");
        assert_eq!(manifest.image.reference(), "mysql:8.0");
    }

    /// The middle link of the chain: refused as bytes, before any field is read.
    #[test]
    fn a_manifest_that_does_not_match_the_index_is_never_parsed() {
        let root = scratch("tampered");
        let source_dir = publish(&root, 1);
        let source = LocalSource::new(&source_dir);
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        // A change that leaves the manifest perfectly valid, and only the hash
        // disagrees — which is the shape of the attack this link is for.
        let path = source_dir.join("packages/databases/mysql/versions/8.0/manifest.json");
        let text = std::fs::read_to_string(&path).unwrap().replace(
            "\"repository\": \"mysql\"",
            "\"repository\": \"attacker/mysql\"",
        );
        std::fs::write(&path, text).unwrap();

        let err = install(&root, &source, &registry, "mysql", "8.0").unwrap_err();
        assert!(err.message.contains("hashes to"), "{}", err.message);
        assert!(
            !packages_dir(&root)
                .join("databases/mysql/versions/8.0")
                .exists(),
            "nothing was left behind"
        );
    }

    /// A file that does not match its manifest fails the same way, and leaves
    /// nothing half-installed.
    #[test]
    fn a_tampered_file_leaves_nothing_behind() {
        let root = scratch("halfway");
        let source_dir = publish(&root, 1);
        let source = LocalSource::new(&source_dir);
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        std::fs::write(
            source_dir.join("packages/databases/mysql/versions/8.0/compose.yml.tpl"),
            "image: \"evil\"\n",
        )
        .unwrap();

        assert!(install(&root, &source, &registry, "mysql", "8.0").is_err());
        assert!(!packages_dir(&root)
            .join("databases/mysql/versions/8.0")
            .exists());
        // And no scratch directory is left for somebody to find later.
        let versions = packages_dir(&root).join("databases/mysql/versions");
        let leftovers = std::fs::read_dir(&versions)
            .map(|d| d.flatten().count())
            .unwrap_or(0);
        assert_eq!(leftovers, 0, "a scratch directory survived");
    }

    /// A source is not trusted to say where in the filesystem its files are.
    #[test]
    fn an_index_naming_a_path_outside_the_tree_is_refused() {
        let root = scratch("traversal");
        let source_dir = publish(&root, 1);
        let path = source_dir.join("registry.json");
        let text = std::fs::read_to_string(&path).unwrap().replace(
            "\"path\": \"packages/databases/mysql/versions/8.0\"",
            "\"path\": \"../../../../etc\"",
        );
        std::fs::write(&path, text).unwrap();

        let err =
            refresh(&root, &LocalSource::new(&source_dir), Trust::Unsigned, None).unwrap_err();
        assert!(
            err.message.contains("its own fields say"),
            "{}",
            err.message
        );
    }

    /// An index that cannot say what `latest` means is one a migration cannot
    /// read.
    #[test]
    fn an_index_with_no_recommended_version_is_refused() {
        let root = scratch("norec");
        let source_dir = publish(&root, 1);
        let path = source_dir.join("registry.json");
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"recommended\": true, ", "");
        std::fs::write(&path, text).unwrap();

        let err =
            refresh(&root, &LocalSource::new(&source_dir), Trust::Unsigned, None).unwrap_err();
        assert!(err.message.contains("recommended"), "{}", err.message);
    }

    #[test]
    fn uninstalling_removes_the_package_and_nothing_else() {
        let root = scratch("uninstall");
        let source = LocalSource::new(publish(&root, 1));
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();
        install(&root, &source, &registry, "mysql", "8.0").unwrap();

        uninstall(&root, "databases", "mysql", "8.0").unwrap();
        assert!(pkg::Tree::open(&dir(&root)).unwrap().services().is_empty());
        // The index is untouched: what is published and what is installed are
        // different questions.
        assert!(cached(&root).unwrap().is_some());
    }

    #[test]
    fn uninstalling_something_that_is_not_there_says_so() {
        let root = scratch("absent");
        assert_eq!(
            uninstall(&root, "databases", "mysql", "8.0")
                .unwrap_err()
                .code,
            Code::NotFound
        );
    }
}
