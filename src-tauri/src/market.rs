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
//!   a pinned key          →  registry.json          (crate::signing)
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
//! ## The first link exists now, and what is still missing is a key
//!
//! [`crate::signing`] verifies a minisign signature over `registry.json`
//! against keys this machine already trusts, and [`refresh`] runs it before
//! the index is parsed — the same ordering this module applies to a manifest,
//! for the same reason.
//!
//! What is **not** here is the official key. ADR 0015 gives the registry its
//! own ed25519 pair and `docs/durum.md` §5 still holds the ceremony that would
//! produce one as an open decision, so `signing::PINNED` is empty and a signed
//! refresh on a stock build is refused **naming that** as the missing half.
//! Shipping a placeholder would be worse than the gap: every later reader
//! would believe the chain was closed.
//!
//! An organisation running its own mirror is not waiting on any of that. It
//! signs its own index and names its own key in
//! `policy.market.additionalKeys`, and gets the whole chain today — which is
//! what makes third-party distribution an operational decision rather than a
//! missing feature.
//!
//! Faz 5 has since landed [`HttpSource`], and that changes who this costs. A
//! directory the user picked is trusted on the strength of where it came from,
//! and that is honest. A URL is not: over HTTPS the transport is now the *whole*
//! of what stands between an index and whoever is on the path, which is why
//! `HttpSource` refuses `http://` outright rather than leaving it to the
//! server. An organisation that will not take that trade sets
//! `policy.market.requireSignature`, and gets a refusal instead of a
//! downgrade — the one policy key that can only tighten.
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
    /// Withdrawn by the publisher: the client-side half of a takedown (C).
    ///
    /// A **marking**, never a deletion, and ADR 0014 is why: a version that
    /// disappeared from the index would leave every machine that installed it
    /// holding an `instances.json` entry pointing at nothing, with no way to
    /// find out what happened. Marked, the machine can say it.
    #[serde(default)]
    pub revoked: bool,
    /// Why, in the publisher's own words. Shown verbatim — a takedown nobody
    /// can read the reason for is one people work around.
    #[serde(default)]
    pub revoked_reason: Option<String>,
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
/// A trait, and three answers behind it: a directory ([`LocalSource`]), HTTPS
/// ([`HttpSource`]), and an offline bundle — which is a directory, and is why
/// air-gapped installation needed no third implementation. ADR 0011 leaves that
/// bundle as the **only** way a machine with no network gets a catalogue.
///
/// Synchronous on purpose. [`crate::pkg`] and [`crate::render`] read this trait
/// and neither has any business knowing what an async runtime is; the cost is
/// pushed to the one implementation that needs one, and to its callers — see
/// [`HttpSource`].
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

/// The most any single file from a source may be.
///
/// T-8 in `docs/servis-market-mimarisi.md` §4.1. A manifest is a few kilobytes
/// and the index is a few hundred; a body that keeps arriving is a disk that
/// keeps filling, and the check has to be on the bytes read rather than on
/// `Content-Length`, which the sender chooses.
const MOST_BYTES: u64 = 8 * 1024 * 1024;

/// A catalogue served over HTTPS. Faz 5.
///
/// ## `https` only, and it is checked here
///
/// Not left to the server, and not a matter of what somebody types. The chain
/// in §4.2 starts at a signature that does not exist yet (ADR 0015), so
/// transport is the only thing standing between an index and whoever is on the
/// path — and `http://` would remove even that. A URL that does not start
/// `https://` is refused before a request is made.
///
/// ## The system proxy is used, and that is the opposite of `mail.rs`
///
/// `mail.rs` builds its client with `no_proxy()` because it only ever talks to
/// 127.0.0.1 and a company proxy has no business in that. This is the exact
/// inverse: a managed machine reaches the outside world *through* the proxy,
/// and that is the machine `market.registryUrl` exists for. The `system-proxy`
/// feature is process-wide; here it is wanted.
///
/// ## ETag, so the second refresh is cheap and honest
///
/// The index is the file that is fetched again and again and changes rarely. A
/// `304` is not a failure and not an empty answer — it means the cached copy is
/// current, and the caller keeps it. Recorded next to the cache rather than in
/// it, because a validator is about a transfer and the index is about a
/// catalogue.
#[derive(Debug)]
pub struct HttpSource {
    base: String,
    etags: PathBuf,
}

/// The URL somebody pastes, turned into the URL files are actually served from.
///
/// `https://github.com/stackvo/stackvo-service-packages` is the address of a
/// *page*. Joining `registry.json` onto it asks GitHub for a file in its web UI
/// and gets an HTML 404 — a correct refusal to a question nobody meant to ask,
/// and it is the first thing anybody pastes, because it is the address in the
/// browser bar and the one written in the docs.
///
/// So a repository URL is translated to the raw base rather than refused:
///
/// ```text
///   github.com/<owner>/<repo>                → raw.githubusercontent.com/<owner>/<repo>/HEAD
///   github.com/<owner>/<repo>/tree/<ref>     → raw.githubusercontent.com/<owner>/<repo>/<ref>
/// ```
///
/// **`HEAD`, not `main`.** GitHub's raw host resolves `HEAD` to whatever the
/// repository's own default branch is, so this is a lookup rather than a guess —
/// and guessing was the alternative: `main` then `master` on a 404, which gets
/// the common cases and silently mis-reports the third one as "not found" when
/// the truth is "wrong branch". A `/tree/<ref>` in the pasted URL is an explicit
/// choice and is honoured.
///
/// Nothing else is rewritten. A CDN, a Pages site, a corporate file server —
/// ADR 0013's "any static host" — is taken exactly as given, because there is no
/// pattern to recognise and inventing one would be this function guessing at
/// somebody's infrastructure.
pub fn resolve_location(location: &str) -> String {
    let trimmed = location.trim().trim_end_matches('/');
    // `www.` is stripped first so the host appears once. Two literals for one
    // host is two things to keep in step, and `privacy_claims.rs` reads them as
    // two places this app can reach.
    let without_scheme = trimmed.strip_prefix("https://").unwrap_or(trimmed);
    let host_and_path = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);
    let Some(path) = host_and_path.strip_prefix("github.com/") else {
        return trimmed.to_string();
    };
    if !trimmed.starts_with("https://") {
        return trimmed.to_string();
    }

    // `.git` is on the clone URL, which is the other thing a person copies.
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        return trimmed.to_string();
    };
    if owner.is_empty() || repo.is_empty() {
        return trimmed.to_string();
    }

    let reference = match (parts.next(), parts.next()) {
        (Some("tree" | "blob"), Some(git_ref)) if !git_ref.is_empty() => git_ref,
        _ => "HEAD",
    };
    format!("https://raw.githubusercontent.com/{owner}/{repo}/{reference}")
}

impl HttpSource {
    /// `base` is the directory the registry lives in, trailing slash optional.
    pub fn new(root: &Path, base: &str) -> Result<Self> {
        let base = resolve_location(base);
        if !base.starts_with("https://") {
            return Err(Error::new(
                Code::InvalidInput,
                format!(
                    "{base:?} is not an https:// address. Nothing verifies a signature yet \
                     (ADR 0015), so the transport is the whole of what stands between this \
                     catalogue and whoever is on the path"
                ),
            )
            .with_hint(crate::hints::REGISTRY_MUST_BE_HTTPS));
        }
        Ok(Self {
            base,
            etags: dir(root).join("etags.json"),
        })
    }

    fn cached_etag(&self, relative: &str) -> Option<String> {
        let text = std::fs::read_to_string(&self.etags).ok()?;
        let map: std::collections::BTreeMap<String, String> = serde_json::from_str(&text).ok()?;
        map.get(relative).cloned()
    }

    fn remember_etag(&self, relative: &str, etag: &str) {
        let mut map: std::collections::BTreeMap<String, String> =
            std::fs::read_to_string(&self.etags)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
        map.insert(relative.to_string(), etag.to_string());
        if let Some(parent) = self.etags.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Best effort on purpose. A validator that could not be written means
        // the next refresh transfers a file it did not have to; failing the
        // refresh over it would trade a wasted request for a broken feature.
        if let Ok(text) = serde_json::to_string_pretty(&map) {
            let _ = crate::atomic::write(&self.etags, &format!("{text}\n"));
        }
    }

    /// Where a `304` sends the caller: to the copy already on disk.
    ///
    /// Not `from_cache`. `from_*` is the constructor convention — a reader
    /// meeting it expects a `Self` built out of something, not a method that
    /// reads a file off an existing one, and clippy holds that convention.
    fn cached_copy(&self, root_relative: &str) -> Option<Vec<u8>> {
        let path = self.etags.parent()?.join(root_relative);
        std::fs::read(path).ok()
    }
}

/// One HTTPS GET, run on the caller's thread.
///
/// `Source::fetch` is synchronous because everything above it is, and making
/// the trait async would push a runtime into `pkg` and `render`, neither of
/// which has any business knowing where bytes came from. The command layer runs
/// the whole refresh inside `spawn_blocking`, so this is a blocking thread and
/// `Handle::block_on` is allowed on it — calling it from a runtime thread would
/// panic, which is a real constraint on callers and is why it is stated here.
fn get(url: &str, etag: Option<&str>) -> Result<Option<(Vec<u8>, Option<String>)>> {
    use futures_util::StreamExt as _;

    let handle = tokio::runtime::Handle::try_current().map_err(|_| {
        Error::new(
            Code::NetworkError,
            "the network source needs a Tokio runtime and was called outside one",
        )
    })?;

    let url = url.to_string();
    let etag = etag.map(str::to_string);
    handle.block_on(async move {
        let client = reqwest::Client::builder()
            .user_agent(concat!("stackvo/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| Error::new(Code::NetworkError, format!("building an HTTP client: {e}")))?;

        let mut request = client.get(&url);
        if let Some(etag) = &etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }

        let response = request.send().await.map_err(|e| {
            Error::new(
                Code::NetworkError,
                // The URL, always. "the catalogue could not be reached" sends
                // somebody to support; the address they can paste into a
                // browser is the whole of what they can act on.
                format!("{url} could not be reached: {e}"),
            )
            .with_hint(crate::hints::REGISTRY_UNREACHABLE)
        })?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        if !response.status().is_success() {
            // 404 gets its own sentence. It is the answer a *working* server
            // gives to an address that is one level off — the repository's web
            // page rather than the directory its files are served from — and
            // "could not be reached" sends somebody to look at their network
            // for a problem that is in the address bar.
            let hint = if response.status() == reqwest::StatusCode::NOT_FOUND {
                crate::hints::REGISTRY_ADDRESS_IS_A_DIRECTORY
            } else {
                crate::hints::REGISTRY_UNREACHABLE
            };
            return Err(Error::new(
                Code::NetworkError,
                format!("{url} answered {}", response.status()),
            )
            .with_hint(hint));
        }

        let tag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        // Streamed and counted rather than `bytes()`. `Content-Length` is
        // something the sender writes; this is the number of bytes that
        // actually arrived, and it is the only one a limit can be about.
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                Error::new(Code::NetworkError, format!("{url} stopped sending: {e}"))
            })?;
            if body.len() as u64 + chunk.len() as u64 > MOST_BYTES {
                return Err(Error::new(
                    Code::Forbidden,
                    format!("{url} is larger than {MOST_BYTES} bytes and was abandoned"),
                )
                .with_hint(crate::hints::PACKAGE_REFUSED_BY_POLICY));
            }
            body.extend_from_slice(&chunk);
        }

        Ok(Some((body, tag)))
    })
}

impl Source for HttpSource {
    fn describe(&self) -> String {
        self.base.clone()
    }

    fn fetch(&self, relative: &str) -> Result<Vec<u8>> {
        // The same rule a directory source lives under. A path is joined onto
        // a URL here rather than concatenated blindly: `..` in an index would
        // otherwise walk the *server's* tree, and an index is data.
        checked_relative(relative)?;
        let url = format!("{}/{relative}", self.base);

        match get(&url, self.cached_etag(relative).as_deref())? {
            Some((body, tag)) => {
                if let Some(tag) = tag {
                    self.remember_etag(relative, &tag);
                }
                Ok(body)
            }
            // 304. The server agrees with what is here, so what is here is the
            // answer — and if it is somehow gone, the validator was wrong and
            // asking again without it is the recovery.
            None => match self.cached_copy(relative) {
                Some(body) => Ok(body),
                None => {
                    let (body, tag) = get(&url, None)?.ok_or_else(|| {
                        Error::new(
                            Code::NetworkError,
                            format!("{url} answered 304 to a request with no validator"),
                        )
                    })?;
                    if let Some(tag) = tag {
                        self.remember_etag(relative, &tag);
                    }
                    Ok(body)
                }
            },
        }
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
    /// they were handed. Weaker for a network source, where it means the
    /// catalogue is trusted on the strength of TLS and the address — which is
    /// why `HttpSource` will not accept `http://`, and why
    /// `policy.market.requireSignature` exists for anybody who needs more.
    Unsigned,
    /// Require a signature from a key this machine already trusts.
    ///
    /// Implemented by [`crate::signing`]. On a build with no key pinned and no
    /// policy key, [`refresh`] refuses and says *that* is what is missing —
    /// rather than quietly downgrading, because a security check that silently
    /// does nothing is worse than one that is absent.
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
    /// `local` or `https`. A field rather than an enum because it is written to
    /// disk and read back by a build that may be older than the value.
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
///
/// The one place a source becomes something that can read bytes, which is why
/// `policy.market.allowedSources` is enforced here rather than at each call
/// site. A remembered reference goes through this too, deliberately: a policy
/// that arrived after somebody had already fetched from a mirror must take
/// effect on the next refresh, not on the next fresh install.
pub fn open(root: &Path, reference: &SourceRef) -> Result<Box<dyn Source>> {
    let market = crate::policy::current().market();
    if !market.allows_source(&reference.location) {
        return Err(Error::new(
            Code::Forbidden,
            format!(
                "{} is not a catalogue source this machine is allowed to use",
                reference.location
            ),
        ));
    }

    match reference.kind.as_str() {
        "local" => Ok(Box::new(LocalSource::new(&reference.location))),
        "https" => Ok(Box::new(HttpSource::new(root, &reference.location)?)),
        other => Err(Error::new(
            Code::Unsupported,
            format!("{other:?} is not a source this build can read"),
        )),
    }
}

/// Which kind a location is, without asking the caller to say.
///
/// A user pastes a URL or picks a directory; making them also choose a radio
/// button would be asking them to restate something the string already says.
pub fn kind_of(location: &str) -> &'static str {
    if location.trim().starts_with("https://") || location.trim().starts_with("http://") {
        "https"
    } else {
        "local"
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
    let bytes = source.fetch("registry.json")?;

    // The first link of the chain, and the order matters: the bytes are
    // checked before they are parsed, exactly as `manifestSha256` is checked
    // before a manifest is parsed. A document is parsed by code that trusts
    // its shape, and the cheapest way to keep that trust honest is to settle
    // where the bytes came from first.
    if trust == Trust::Signed {
        let keys = crate::signing::Keys::pinned()
            .with_policy(&crate::policy::current().market().additional_keys);

        // The keys are checked **before** the signature file is fetched, and
        // the order is not cosmetic. Fetching first meant a machine with no
        // pinned key was told `registry.json.minisig: No such file` — which
        // sends somebody to the publisher to ask for a signature that would
        // not have helped, when the missing half is on this side. Found by a
        // test written to assert the order rather than by reading.
        if keys.is_empty() {
            return Err(Error::new(
                Code::Unsupported,
                "a signed index was asked for and no registry key is pinned in this build",
            )
            .with_hint(crate::hints::NO_REGISTRY_KEY));
        }

        let signature =
            String::from_utf8_lossy(&source.fetch("registry.json.minisig")?).to_string();
        let by = keys.verify(&bytes, &signature)?;
        tracing::info!(source = %source.describe(), key = %by.id(), "index signature verified");
    }

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
    market: &crate::policy::Market,
) -> Result<Installed> {
    let row = registry.version(service, version).ok_or_else(|| {
        Error::not_found(format!("{service}@{version} in the index"))
            .with_hint(crate::hints::PACKAGE_NOT_IN_REGISTRY)
    })?;

    // The publisher's own withdrawal, before the organisation's list and before
    // anything is fetched — the client half of a takedown (C).
    //
    // Refused rather than merely marked on screen. ADR 0014 keeps a withdrawn
    // version *in* the index precisely so a machine can find out what happened
    // to something it already installed; that is a different question from
    // whether a new install may go ahead, and answering both with a warning
    // would make the withdrawal advisory.
    if row.revoked {
        return Err(Error::new(
            Code::Forbidden,
            match &row.revoked_reason {
                Some(reason) => format!("{service}@{version} was withdrawn: {reason}"),
                None => format!("{service}@{version} was withdrawn by its publisher"),
            },
        )
        .with_hint(crate::hints::PACKAGE_VERSION_REVOKED));
    }

    // The organisation's list, before anything is fetched. Passed in rather
    // than read from the global: this is the function that puts somebody else's
    // bytes on the disk, and a check it could be called without is a check.
    if !market.allows_package(service) {
        return Err(Error::new(
            Code::Forbidden,
            format!(
                "{service} is not on this machine's list of allowed packages ({})",
                crate::policy::current().origin()
            ),
        )
        .with_hint(crate::hints::PACKAGE_REFUSED_BY_POLICY));
    }

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

    // The image the manifest asks for, against the registries the organisation
    // allows — checked here rather than at run time because a package whose
    // image will be refused is a package that should never have been installed.
    // Only readable after the manifest has been parsed, which is why it is not
    // beside the package check above.
    let reference = manifest.image.reference();
    if !market.allows_registry(&reference) {
        return Err(Error::new(
            Code::Forbidden,
            format!(
                "{service}@{version} runs {reference}, which is not from a registry this \
                 machine allows ({})",
                crate::policy::current().origin()
            ),
        )
        .with_hint(crate::hints::PACKAGE_REFUSED_BY_POLICY));
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

/// What a bundle came out as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundled {
    pub packages: usize,
    pub versions: usize,
    pub files: usize,
    pub bytes: u64,
    /// Versions whose files were deliberately not carried, and why.
    ///
    /// Reported rather than silent. A bundle that is smaller than the index it
    /// carries is a fact somebody handing over a USB stick should be told, not
    /// something they discover on the machine that has no network.
    pub skipped: Vec<String>,
    /// Whether the index's signature travelled with it.
    ///
    /// A separate field rather than folded into the file count, because it is
    /// the one file whose absence changes what the far end can do: a machine
    /// whose policy sets `requireSignature` refuses a bundle without it, and
    /// finding that out here is the difference between a five-minute fix and a
    /// second trip.
    pub signed: bool,
}

/// Write everything an air-gapped machine needs into one directory.
///
/// §3 #31. The consuming half has been there since [`LocalSource`] — point
/// `market.offlineBundle` at a directory and the catalogue, the manifests and
/// the templates are read from it with the same verification as from the
/// network. What was missing was the **producing** half: nothing on a connected
/// machine could make that directory, so the only way to get one was to clone
/// the packages repository and hope its layout matched what the client reads.
///
/// This is that half, and it is deliberately the same walk [`install`] makes,
/// once per version, into a directory instead of into the workspace. The output
/// is a source: `LocalSource::new(dest)` and the far end cannot tell it from a
/// checkout.
///
/// ## A directory, not the tar `servis-market-mimarisi.md` §9 names
///
/// The design says `stackvo-packages.tar`. A tar is a *packaging* of this, not
/// a second mechanism — `tar -cf stackvo-packages.tar -C <dest> .` produces it,
/// and the far end unpacks it into a directory because that is what the reader
/// reads. Shipping the archive format first would have meant a second `Source`
/// implementation, on the argument that a USB stick prefers one file. It does
/// not: a directory copies, resumes and diffs, and an archive that must be
/// unpacked before it can be verified is a verification that happens after the
/// bytes are already on the disk.
///
/// ## The index is copied byte for byte
///
/// Not re-serialised from [`Registry`]. Two reasons and both are load-bearing:
/// the signature (ADR 0015) is over the bytes, so a round trip through serde
/// invalidates it even when every field survives; and `manifestSha256` chains
/// from those bytes to each manifest, which is the chain `refresh` and
/// [`install`] check on the far end. A bundle that had to be trusted differently
/// from a fetch would not be an offline install, it would be an exception.
///
/// ## Every manifest is verified here, on the machine with a person at it
///
/// `install` refuses a manifest whose bytes do not hash to what the index says.
/// Doing that check again while bundling costs one hash per version and moves
/// the failure to the only place it can be acted on. The alternative is a stick
/// that is carried across a building and refuses on arrival, with the reason on
/// the wrong side of the air gap.
///
/// ## Withdrawn versions travel as rows, not as files
///
/// ADR 0014 keeps a revoked version *in* the index so a machine can find out
/// what happened to something it installed, and [`install`] refuses to install
/// one — before it fetches anything. So its files would be bytes nobody can
/// ever ask for. They are skipped and named in [`Bundled::skipped`]; the row
/// stays, because the row is what answers the question the far end will ask.
pub fn bundle(source: &dyn Source, dest: &Path) -> Result<Bundled> {
    // The destination has to be ours. A bundle written over somebody's existing
    // directory is a directory whose contents nobody can account for — half a
    // catalogue from one refresh and half from another, with an index that
    // describes neither.
    if dest.exists() {
        let empty = std::fs::read_dir(dest)
            .map_err(|e| Error::new(Code::IoError, format!("reading {}: {e}", dest.display())))?
            .next()
            .is_none();
        if !empty {
            return Err(Error::new(
                Code::AlreadyExists,
                format!("{} is not empty", dest.display()),
            )
            .with_hint(crate::hints::BUNDLE_NEEDS_AN_EMPTY_DIRECTORY));
        }
    }
    std::fs::create_dir_all(dest)
        .map_err(|e| Error::new(Code::IoError, format!("creating {}: {e}", dest.display())))?;

    let outcome = (|| -> Result<Bundled> {
        let index = source.fetch("registry.json")?;
        let registry: Registry = serde_json::from_slice(&index).map_err(|e| {
            Error::new(
                Code::InvalidManifest,
                format!("{}: registry.json is unreadable: {e}", source.describe()),
            )
        })?;
        registry.check()?;

        let mut out = Bundled {
            packages: registry.packages.len(),
            versions: 0,
            files: 0,
            bytes: 0,
            skipped: Vec::new(),
            signed: false,
        };

        // `out` travels as an argument rather than being captured, so this
        // stays an `Fn` and the loops below can still touch the counters.
        let keep = |relative: &str, bytes: &[u8], out: &mut Bundled| -> Result<()> {
            write_into(dest, relative, bytes)?;
            out.files += 1;
            out.bytes += bytes.len() as u64;
            Ok(())
        };

        keep("registry.json", &index, &mut out)?;

        // Best effort, and the only best-effort fetch in this function. An
        // unsigned index is the state every build is in until the key ceremony
        // (ADR 0021), so treating a missing signature as a failure would make
        // this command unusable today; treating it as invisible would let
        // somebody carry an unsignable bundle to a machine that requires one.
        // Reported instead.
        if let Ok(signature) = source.fetch("registry.json.minisig") {
            keep("registry.json.minisig", &signature, &mut out)?;
            out.signed = true;
        }

        for package in &registry.packages {
            // The identity file, a level above the versions and shared by all
            // of them — the same one `install` fetches after the move.
            let identity = format!(
                "packages/{}/{}/package.json",
                package.category, package.service
            );
            let bytes = source.fetch(&identity)?;
            keep(&identity, &bytes, &mut out)?;

            for row in &package.versions {
                if row.revoked {
                    out.skipped.push(format!(
                        "{}@{} — withdrawn by its publisher{}",
                        package.service,
                        row.version,
                        match &row.revoked_reason {
                            Some(reason) => format!(": {reason}"),
                            None => String::new(),
                        }
                    ));
                    continue;
                }

                let manifest_bytes = source.fetch(&format!("{}/manifest.json", row.path))?;
                let actual = pkg::sha256_hex(&manifest_bytes);
                if actual != row.manifest_sha256 {
                    return Err(Error::new(
                        Code::InvalidManifest,
                        format!(
                            "{}@{}: the index says the manifest hashes to {} and it hashes to \
                             {actual} — refused here rather than on the machine with no network",
                            package.service, row.version, row.manifest_sha256
                        ),
                    )
                    .with_hint(crate::hints::PACKAGE_CONTENT_CHANGED));
                }

                let text = String::from_utf8(manifest_bytes.clone()).map_err(|_| {
                    Error::new(
                        Code::InvalidManifest,
                        format!(
                            "{}@{}: the manifest is not UTF-8",
                            package.service, row.version
                        ),
                    )
                })?;
                let manifest = pkg::parse(&text)?;

                keep(
                    &format!("{}/manifest.json", row.path),
                    &manifest_bytes,
                    &mut out,
                )?;

                // Exactly what `install` asks for, in the same order. A list
                // assembled differently here is a bundle that installs on this
                // build and not on the next one.
                let mut wanted: Vec<String> = vec![manifest.compose.file.clone()];
                wanted.extend(manifest.files.iter().map(|f| f.template.clone()));
                wanted.extend(manifest.companions.iter().map(|c| c.compose.file.clone()));

                for relative in wanted {
                    let bytes = source.fetch(&format!("{}/{relative}", row.path))?;
                    keep(&format!("{}/{relative}", row.path), &bytes, &mut out)?;
                }

                out.versions += 1;
            }
        }

        Ok(out)
    })();

    match outcome {
        Ok(out) => Ok(out),
        Err(e) => {
            // A half-written bundle is the one outcome worth cleaning up after:
            // it looks exactly like a whole one, and the machine it is carried
            // to has no way to ask.
            let _ = std::fs::remove_dir_all(dest);
            Err(e)
        }
    }
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

    /// A machine nobody administers, which is what almost every one of them is.
    fn unmanaged() -> crate::policy::Market {
        crate::policy::Market::default()
    }

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
    ///
    /// The refusal moved rather than went away (C): it used to be "not
    /// implemented", and it is now "this build pins no key". The verifier is
    /// real — `signing.rs` proves it against a signature it did not make — and
    /// what is missing is the ceremony that produces the official key, which
    /// `docs/durum.md` §5 still holds open.
    #[test]
    fn asking_for_a_signed_index_is_refused_rather_than_downgraded() {
        let root = scratch("signed");
        let source = LocalSource::new(publish(&root, 1));
        let err = refresh(&root, &source, Trust::Signed, None).unwrap_err();
        assert_eq!(err.code, Code::Unsupported);
        assert!(err.message.contains("no registry key"), "{}", err.message);
    }

    /// And it fails on the **key**, not on the signature file being absent —
    /// the check that a machine with a key would get as far as looking for one.
    #[test]
    fn a_signed_refresh_asks_for_the_key_before_the_signature_file() {
        let root = scratch("signed-order");
        let dir = publish(&root, 1);
        assert!(
            !dir.join("registry.json.minisig").exists(),
            "the fixture publishes no signature"
        );
        let err = refresh(&root, &LocalSource::new(&dir), Trust::Signed, None).unwrap_err();
        assert_eq!(
            err.code,
            Code::Unsupported,
            "a missing key is reported before a missing signature: {}",
            err.message
        );
    }

    /// The client half of a takedown (C): a withdrawn version does not install.
    ///
    /// Refused rather than warned about. ADR 0014 keeps a withdrawn version in
    /// the index so a machine can find out what happened to one it already
    /// has; whether a *new* install may proceed is a different question, and
    /// answering both with a warning would make the withdrawal advisory.
    #[test]
    fn a_withdrawn_version_is_refused_with_the_publishers_reason() {
        let root = scratch("revoked");
        let source = LocalSource::new(publish(&root, 1));
        let mut registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        // Installing it is fine until the publisher says otherwise.
        let market = crate::policy::Market::default();
        assert!(install(&root, &source, &registry, "mysql", "8.0", &market).is_ok());

        registry.packages[0].versions[0].revoked = true;
        registry.packages[0].versions[0].revoked_reason =
            Some("a bad image tag shipped in this build".into());

        let err = install(&root, &source, &registry, "mysql", "8.0", &market).unwrap_err();
        assert_eq!(err.code, Code::Forbidden);
        assert!(err.message.contains("bad image tag"), "{}", err.message);
        assert!(err.message.contains("withdrawn"), "{}", err.message);
    }

    /// Withdrawn with no reason given is still withdrawn.
    #[test]
    fn a_withdrawal_with_no_reason_still_refuses() {
        let root = scratch("revoked-bare");
        let source = LocalSource::new(publish(&root, 1));
        let mut registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();
        registry.packages[0].versions[0].revoked = true;

        let err = install(
            &root,
            &source,
            &registry,
            "mysql",
            "8.0",
            &crate::policy::Market::default(),
        )
        .unwrap_err();
        assert_eq!(err.code, Code::Forbidden);
    }

    /// A withdrawal is checked before the organisation's own list, and both
    /// before anything is fetched: the order is what keeps a refusal from
    /// depending on a network call.
    #[test]
    fn a_withdrawal_is_refused_even_where_the_source_is_gone() {
        let root = scratch("revoked-offline");
        let source = LocalSource::new(publish(&root, 1));
        let mut registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();
        registry.packages[0].versions[0].revoked = true;

        let gone = LocalSource::new(root.join("nowhere-at-all"));
        let err = install(
            &root,
            &gone,
            &registry,
            "mysql",
            "8.0",
            &crate::policy::Market::default(),
        )
        .unwrap_err();
        assert_eq!(
            err.code,
            Code::Forbidden,
            "not an I/O error: {}",
            err.message
        );
    }

    #[test]
    fn installing_puts_a_package_where_the_tree_finds_it() {
        let root = scratch("install");
        let source = LocalSource::new(publish(&root, 1));
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        let done = install(&root, &source, &registry, "mysql", "8.0", &unmanaged()).unwrap();
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

        let err = install(&root, &source, &registry, "mysql", "8.0", &unmanaged()).unwrap_err();
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

        assert!(install(&root, &source, &registry, "mysql", "8.0", &unmanaged()).is_err());
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
        install(&root, &source, &registry, "mysql", "8.0", &unmanaged()).unwrap();

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

    /// The organisation's list is checked before a single byte is fetched. A
    /// package that will be refused should not be downloaded first — and the
    /// error names where the list came from, because the only action the person
    /// at the keyboard can take is to show that path to whoever wrote it.
    #[test]
    fn a_package_outside_the_allow_list_is_refused_before_it_is_fetched() {
        let root = scratch("policy-package");
        let source = LocalSource::new(publish(&root, 1));
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        let market = crate::policy::Market {
            allowed_packages: ["postgres".to_string()].into_iter().collect(),
            ..Default::default()
        };
        let err = install(&root, &source, &registry, "mysql", "8.0", &market).unwrap_err();

        assert_eq!(err.code, Code::Forbidden);
        assert!(err.message.contains("mysql"), "{}", err.message);
        assert!(
            !packages_dir(&root)
                .join("databases/mysql/versions/8.0")
                .exists(),
            "a refused package left files behind"
        );
    }

    /// And the registry list is checked after the manifest is parsed, because
    /// that is the first moment the image reference exists.
    #[test]
    fn an_image_from_an_unlisted_registry_is_refused() {
        let root = scratch("policy-registry");
        let source = LocalSource::new(publish(&root, 1));
        let registry = refresh(&root, &source, Trust::Unsigned, None).unwrap();

        let market = crate::policy::Market {
            allowed_registries: ["registry.corp.example".to_string()].into_iter().collect(),
            ..Default::default()
        };
        let err = install(&root, &source, &registry, "mysql", "8.0", &market).unwrap_err();

        assert_eq!(err.code, Code::Forbidden);
        assert!(
            !packages_dir(&root)
                .join("databases/mysql/versions/8.0")
                .exists(),
            "a refused package left files behind"
        );
    }

    // ------------------------------------------------------ the network source

    /// `http://` is refused where a person can still do something about it,
    /// not at the moment a request would have gone out. Nothing verifies a
    /// signature yet (ADR 0015), so the transport is the whole of what stands
    /// between an index and whoever is on the path.
    #[test]
    fn a_plain_http_catalogue_is_refused_before_anything_is_requested() {
        let root = scratch("http-refused");
        let err = HttpSource::new(&root, "http://packages.example/stackvo").unwrap_err();
        assert_eq!(err.code, Code::InvalidInput);
        assert_eq!(err.hint_key, Some("registryMustBeHttps"));

        assert!(HttpSource::new(&root, "https://packages.example/stackvo").is_ok());
    }

    /// A trailing slash is a thing people paste, and two of them in a URL is a
    /// 404 from a server that is working perfectly.
    #[test]
    fn a_trailing_slash_does_not_become_a_double_one() {
        let root = scratch("http-slash");
        let source = HttpSource::new(&root, "https://packages.example/stackvo/").unwrap();
        assert_eq!(source.describe(), "https://packages.example/stackvo");
    }

    /// The same rule a directory source lives under, for the same reason: an
    /// index is data, and a path in it must not walk the *server's* tree either.
    #[test]
    fn a_network_source_refuses_a_path_that_walks_out() {
        let root = scratch("http-traversal");
        let source = HttpSource::new(&root, "https://packages.example/stackvo").unwrap();
        let err = source.fetch("../../etc/passwd").unwrap_err();
        assert_eq!(err.code, Code::InvalidInput);
    }

    /// Which kind a location is comes from the string, not from a radio button
    /// the user would have to agree with what they just typed.
    #[test]
    fn a_location_says_what_kind_it_is() {
        assert_eq!(kind_of("https://packages.stackvo.dev"), "https");
        assert_eq!(kind_of("  https://packages.stackvo.dev "), "https");
        assert_eq!(kind_of("/opt/stackvo/packages"), "local");
        assert_eq!(kind_of("C:\\packages"), "local");
    }

    /// A validator is remembered per file and survives a reopen — the whole
    /// point of it is the *second* refresh.
    #[test]
    fn an_etag_is_remembered_per_file() {
        let root = scratch("http-etag");
        let source = HttpSource::new(&root, "https://packages.example/stackvo").unwrap();
        assert_eq!(source.cached_etag("registry.json"), None);

        source.remember_etag("registry.json", "\"abc\"");
        source.remember_etag(
            "packages/databases/mysql/versions/8.0/manifest.json",
            "\"def\"",
        );

        let reopened = HttpSource::new(&root, "https://packages.example/stackvo").unwrap();
        assert_eq!(
            reopened.cached_etag("registry.json").as_deref(),
            Some("\"abc\"")
        );
        assert_eq!(
            reopened
                .cached_etag("packages/databases/mysql/versions/8.0/manifest.json")
                .as_deref(),
            Some("\"def\"")
        );
        assert_eq!(reopened.cached_etag("nothing.json"), None);
    }

    /// Blocking on a runtime handle from a runtime thread panics, so the
    /// command layer runs the whole refresh in `spawn_blocking`. Called from
    /// nowhere at all it has no handle to block on, and it says so instead of
    /// unwrapping — this test is the one that would have caught it.
    #[test]
    fn a_network_fetch_outside_a_runtime_reports_rather_than_panics() {
        let root = scratch("http-noruntime");
        let source = HttpSource::new(&root, "https://packages.invalid/stackvo").unwrap();
        let err = source.fetch("registry.json").unwrap_err();
        assert_eq!(err.code, Code::NetworkError);
    }

    // ------------------------------------------ the address people paste

    /// The repository's web page is the address in the browser bar and the one
    /// in the docs, so it is the one that gets pasted — and joining
    /// `registry.json` onto it asks GitHub for a file in its web UI.
    #[test]
    fn a_github_repository_url_becomes_the_raw_base() {
        assert_eq!(
            resolve_location("https://github.com/stackvo/stackvo-service-packages"),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/HEAD"
        );
        // The three other shapes of the same copy: a trailing slash, the clone
        // URL, and the `www` host.
        assert_eq!(
            resolve_location("https://github.com/stackvo/stackvo-service-packages/"),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/HEAD"
        );
        assert_eq!(
            resolve_location("https://github.com/stackvo/stackvo-service-packages.git"),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/HEAD"
        );
        assert_eq!(
            resolve_location("https://www.github.com/stackvo/stackvo-service-packages"),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/HEAD"
        );
    }

    /// `HEAD` is a lookup, not a guess — GitHub's raw host resolves it to the
    /// repository's own default branch. A branch named in the URL is an
    /// explicit choice and wins.
    #[test]
    fn a_branch_in_the_url_is_honoured_and_otherwise_head_decides() {
        assert_eq!(
            resolve_location("https://github.com/stackvo/stackvo-service-packages/tree/next"),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/next"
        );
        assert_eq!(
            resolve_location("https://github.com/o/r/blob/v2"),
            "https://raw.githubusercontent.com/o/r/v2"
        );
        assert!(resolve_location("https://github.com/o/r").ends_with("/HEAD"));
    }

    /// Everything else is taken as given. ADR 0013 says any static host, and a
    /// second pattern here would be this function guessing at somebody's
    /// infrastructure.
    #[test]
    fn every_other_address_is_left_alone() {
        for address in [
            "https://packages.stackvo.dev",
            "https://stackvo.github.io/stackvo-service-packages",
            "https://files.corp.example/mirrors/stackvo",
            "/opt/stackvo/packages",
        ] {
            assert_eq!(resolve_location(address), address);
        }
        // A trailing slash still goes, because it is joined onto.
        assert_eq!(
            resolve_location("https://packages.stackvo.dev/"),
            "https://packages.stackvo.dev"
        );
        // Not a repository: no owner, or no name.
        assert_eq!(
            resolve_location("https://github.com/stackvo"),
            "https://github.com/stackvo"
        );
    }

    /// And the translation is what the source actually uses, not advice printed
    /// beside it.
    #[test]
    fn the_source_fetches_from_the_translated_base() {
        let root = scratch("http-github");
        let source =
            HttpSource::new(&root, "https://github.com/stackvo/stackvo-service-packages").unwrap();
        assert_eq!(
            source.describe(),
            "https://raw.githubusercontent.com/stackvo/stackvo-service-packages/HEAD"
        );
    }

    // ------------------------------------------------------- the offline bundle

    /// The claim, and the only one that matters: what `bundle` writes is a
    /// source.
    ///
    /// Not "the files are there" — a directory holding the right filenames
    /// proves nothing about whether the far end can use it. This refreshes and
    /// installs **from the bundle**, into a second workspace that has never
    /// seen the original, and reads the package back out of the tree. That is
    /// the round trip an air-gapped machine makes, minus the walk down the
    /// corridor.
    #[test]
    fn a_bundle_is_a_source_the_far_end_can_install_from() {
        let here = scratch("bundle-source");
        let source = LocalSource::new(publish(&here, 4));

        let out = bundle(&source, &here.join("carry")).unwrap();
        assert_eq!(out.packages, 1);
        assert_eq!(out.versions, 1);
        // index + package.json + manifest + fragment + config.
        assert_eq!(out.files, 5, "{out:?}");
        assert!(out.bytes > 0);
        assert!(out.skipped.is_empty());

        // A machine that has never seen the original.
        let far = scratch("bundle-far");
        let carried = LocalSource::new(here.join("carry"));

        let registry = refresh(&far, &carried, Trust::Unsigned, None).unwrap();
        assert_eq!(registry.sequence, 4);
        assert_eq!(registry.recommended("mysql").unwrap().version, "8.0");

        let done = install(&far, &carried, &registry, "mysql", "8.0", &unmanaged()).unwrap();
        assert_eq!(done.files, 3, "manifest, fragment, config");

        // Read back through the tree, which re-checks every hash the manifest
        // states. A bundle that copied a file wrong fails here rather than at
        // render time.
        let tree = pkg::Tree::open(&dir(&far)).unwrap();
        assert!(pkg::Catalogue::services(&tree).contains(&"mysql".to_string()));
    }

    /// The index travels as bytes, not as a re-serialised `Registry`.
    ///
    /// The signature is over the bytes (ADR 0015) and `manifestSha256` chains
    /// from them, so a round trip through serde would break both while every
    /// field still looked right — the class of failure that only shows up on
    /// the machine that cannot be debugged.
    #[test]
    fn the_index_is_copied_byte_for_byte() {
        let here = scratch("bundle-bytes");
        let from = publish(&here, 9);

        bundle(&LocalSource::new(&from), &here.join("carry")).unwrap();

        assert_eq!(
            std::fs::read(from.join("registry.json")).unwrap(),
            std::fs::read(here.join("carry/registry.json")).unwrap(),
        );
    }

    /// A signature travels when there is one, and its absence is reported
    /// rather than hidden.
    #[test]
    fn the_signature_travels_and_says_when_it_did_not() {
        let here = scratch("bundle-sig");
        let from = publish(&here, 1);

        let without = bundle(&LocalSource::new(&from), &here.join("plain")).unwrap();
        assert!(!without.signed, "there is no signature to carry");

        std::fs::write(
            from.join("registry.json.minisig"),
            b"untrusted comment: x\nsig\n",
        )
        .unwrap();
        let with = bundle(&LocalSource::new(&from), &here.join("signed")).unwrap();
        assert!(with.signed);
        assert!(here.join("signed/registry.json.minisig").is_file());
        assert_eq!(with.files, without.files + 1);
    }

    /// A manifest that does not match the index is caught **here**.
    ///
    /// The point of the check is where it happens. `install` would refuse this
    /// too — on the machine with no network, with the reason on the wrong side
    /// of the air gap.
    #[test]
    fn a_manifest_that_lost_its_hash_is_refused_before_it_is_carried() {
        let here = scratch("bundle-tamper");
        let from = publish(&here, 1);
        let manifest = from.join("packages/databases/mysql/versions/8.0/manifest.json");

        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("\"8.0\"", "\"8.0\" ")).unwrap();

        let dest = here.join("carry");
        let err = bundle(&LocalSource::new(&from), &dest).unwrap_err();
        assert_eq!(err.code, Code::InvalidManifest);
        assert!(err.message.contains("no network"), "{}", err.message);

        // And nothing is left behind. A half-written bundle looks exactly like
        // a whole one to the machine it is carried to.
        assert!(!dest.exists(), "the failed bundle was not cleaned up");
    }

    /// A withdrawn version keeps its row and loses its files.
    ///
    /// ADR 0014 keeps the row so the far end can find out what happened to
    /// something it installed; `install` refuses to install one before it
    /// fetches anything, so its files would be bytes nobody can ask for.
    #[test]
    fn a_withdrawn_version_travels_as_a_row_and_not_as_files() {
        let here = scratch("bundle-revoked");
        let from = publish(&here, 1);

        // Mark it in the index the source serves, which is where a publisher
        // marks it.
        let index = std::fs::read_to_string(from.join("registry.json")).unwrap();
        std::fs::write(
            from.join("registry.json"),
            index.replace(
                r#""support": "supported""#,
                r#""support": "supported", "revoked": true, "revokedReason": "a bad tag""#,
            ),
        )
        .unwrap();

        let dest = here.join("carry");
        let out = bundle(&LocalSource::new(&from), &dest).unwrap();

        assert_eq!(out.versions, 0);
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].contains("mysql@8.0"), "{:?}", out.skipped);
        assert!(out.skipped[0].contains("a bad tag"), "{:?}", out.skipped);

        // The row is still there — that is what answers the question.
        assert!(dest.join("registry.json").is_file());
        assert!(dest.join("packages/databases/mysql/package.json").is_file());
        assert!(!dest
            .join("packages/databases/mysql/versions/8.0/manifest.json")
            .exists());
    }

    /// A destination with something in it is refused.
    ///
    /// Half a catalogue from one refresh beside half from another, under one
    /// index that describes neither, is a directory nobody can account for —
    /// and it is exactly what a second `bundle` into the same folder produces.
    #[test]
    fn a_bundle_will_not_be_written_over_somebody_elses_files() {
        let here = scratch("bundle-occupied");
        let from = publish(&here, 1);
        let dest = here.join("carry");

        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("notes.txt"), "somebody's").unwrap();

        let err = bundle(&LocalSource::new(&from), &dest).unwrap_err();
        assert_eq!(err.code, Code::AlreadyExists);
        assert!(dest.join("notes.txt").is_file(), "it deleted the files");

        // An empty directory is fine: that is what a person makes before
        // choosing it in a file picker.
        let empty = here.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(bundle(&LocalSource::new(&from), &empty).is_ok());
    }
}
