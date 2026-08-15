//! What an administrator decided, for a machine this app does not own.
//!
//! A fleet of laptops running StackVo has settings somebody other than the
//! person at the keyboard cares about: the domain suffix every project is
//! addressed under, the web server the company standardises on, and — the one
//! that stops working entirely without it — the registry images are pulled
//! from, on a network where Docker Hub is not reachable.
//!
//! ## This is not a security boundary, and saying so is the point
//!
//! The app reads a file the user's own account may well be able to write, and
//! `STACKVO_POLICY_FILE` lets any process point it somewhere else. Both are
//! true and neither is a defect: the layer tells a **co-operating** app what
//! the organisation intends. It does not defend against the person holding the
//! machine, and an app that implied it did would be selling a lock with the key
//! taped to it.
//!
//! What it does buy is real. A managed default arrives without anybody typing
//! it, a locked key stops a well-meaning user from breaking their own stack,
//! and the Settings pane can say *why* a field cannot be edited instead of
//! looking broken.
//!
//! ## One JSON file, three paths, one parser
//!
//! | Platform | Path |
//! | --- | --- |
//! | macOS | `/Library/Managed Preferences/com.stackvo.desktop.json` |
//! | Windows | `%ProgramData%\StackVo\policy.json` |
//! | Linux | `/etc/stackvo/policy.json` |
//!
//! macOS MDM writes a `.plist` and Windows Group Policy writes registry keys,
//! and reading those would mean two parsers this crate does not have, for two
//! mechanisms that can both deliver a file as easily as a key. One format,
//! three paths, one parser. Native readers are an obvious next step and are
//! deliberately not guessed at now.
//!
//! ```json
//! {
//!   "schemaVersion": 1,
//!   "settings": { "DEFAULT_TLD_SUFFIX": "corp.test", "SERVER_TYPE": "nginx" },
//!   "locked": ["DEFAULT_TLD_SUFFIX"],
//!   "registryPrefix": "registry.corp.example/proxy"
//! }
//! ```
//!
//! ## Three rules that fell out of writing it
//!
//! **Precedence is embedded default < `.env` < policy.** Policy wins because a
//! setting an administrator pushed that a stale `.env` silently overrides is
//! not a policy, it is a suggestion.
//!
//! **It cannot lock a key it does not set.** "Do not change this" without
//! saying *to what* leaves the machine on whatever it happened to have, which
//! is the opposite of a fleet being consistent. Such an entry is ignored and
//! named in [`Policy::error`].
//!
//! **A broken policy must not make the app unstartable.** A typo in a file
//! pushed to every machine cannot mean a fleet that will not open. Parsing
//! failure yields an empty policy — but the failure is *reported*, never
//! swallowed: a policy that quietly does nothing is one the administrator who
//! deployed it believes is in force.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The only schema version this build understands.
const SCHEMA_VERSION: u64 = 1;

/// The environment variable that redirects every path below.
///
/// Named here rather than inlined because it appears in the docs, in the
/// Settings pane's explanation and in the tests, and a string in four places is
/// a string that comes to disagree with itself.
pub const OVERRIDE_VAR: &str = "STACKVO_POLICY_FILE";

/// What an administrator decided, after parsing.
///
/// Always constructible — [`Policy::none`] is the answer for "no file", and a
/// broken file yields an empty policy carrying [`Policy::error`]. Nothing in
/// the app has to handle a policy that failed to load, because there is no such
/// state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// `.env` keys and the values that override whatever the workspace holds.
    settings: BTreeMap<String, String>,
    /// The subset of `settings` a user may not write over.
    ///
    /// A subset by construction: [`Policy::parse`] drops any locked key the
    /// file does not also set, so nothing downstream has to re-check it.
    locked: BTreeSet<String>,
    /// Prepended to image references that do not already name a registry.
    registry_prefix: Option<String>,
    /// What an administrator says about where packages come from.
    market: Market,
    /// What an administrator says about a project's lifecycle hooks.
    hooks: Hooks,
    /// Where this came from, for an error message that can be acted on. `None`
    /// when no policy file was found, which is the ordinary case.
    source: Option<PathBuf>,
    /// Why the file did not fully apply. Surfaced by `policy_status` and shown
    /// in Settings — see the module comment on why this is not swallowed.
    error: Option<String>,
}

/// The `market` block: where packages come from and which of them may be run.
///
/// `docs/servis-market-mimarisi.md` §9. Every field is optional and the default
/// for all of them is "no opinion", which is what an unmanaged machine has.
///
/// ## One of these is a lock and the rest are not
///
/// ADR 0009's sentence holds for this block as it holds for the rest of the
/// file: **it is not a security boundary.** A user who can write the policy can
/// widen it, and `STACKVO_POLICY_FILE` points it anywhere.
///
/// [`Market::require_signature`] is the exception, and the asymmetry is
/// deliberate rather than accidental: verification lives in the app's own code,
/// and this field can only turn it **on**. There is no value of it that turns a
/// check off. That is the difference between a lock and a note, and it is worth
/// stating because the inverse — a policy key that could disable a check —
/// would mean the check was never one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Market {
    /// A file server the organisation runs. Faz 5 reads it; today it is
    /// recorded and reported so an admin can deploy the file before the
    /// version that uses it, which is the order these things actually ship in.
    pub registry_url: Option<String>,
    /// A directory or bundle to install from with no network at all.
    ///
    /// ADR 0011 makes this the **only** way an air-gapped machine gets a
    /// catalogue, because nothing is embedded. Not an enterprise extra.
    pub offline_bundle: Option<PathBuf>,
    /// Refuse an unsigned index. Off by default only because no key is pinned
    /// yet (ADR 0015); a machine that sets it gets a refusal rather than a
    /// downgrade, which is `market::Trust::Signed`'s existing behaviour.
    pub require_signature: bool,
    /// Services that may be installed. Empty means no opinion — **not** "none".
    pub allowed_packages: BTreeSet<String>,
    /// Registries an image may come from. Empty means no opinion.
    pub allowed_registries: BTreeSet<String>,
    /// Catalogue sources this machine may fetch a package index from.
    ///
    /// `docs/servis-market-mimarisi.md` §4.6's recommendation, which is that
    /// third-party packages are not a v1 feature and the architecture should be
    /// *ready* for them: the source field, the signature verifier and the
    /// compose policy exist, and opening the gate is a separate decision. This
    /// is the enterprise half of that gate — an organisation that runs its own
    /// mirror names it here and the machine will fetch from nothing else.
    ///
    /// An `https://` entry is matched on its **host**, so listing a mirror does
    /// not mean listing every path on it separately. A local path is matched as
    /// a directory prefix, because a source is a directory tree and naming its
    /// root is the only spelling that survives somebody picking a subdirectory.
    ///
    /// Empty means no opinion, like every other list here — not "none".
    pub allowed_sources: BTreeSet<String>,
    /// Whether the app may replace an installed package on its own.
    pub auto_update: Option<bool>,
    /// Extra ed25519 public keys, for an organisation signing its own mirror.
    pub additional_keys: Vec<String>,
}

impl Market {
    /// Is this service one the organisation allows?
    ///
    /// An empty list is silence, not a refusal. Reading it the other way would
    /// mean any administrator who wrote a `market` block for one unrelated
    /// reason had accidentally forbidden every package.
    pub fn allows_package(&self, service: &str) -> bool {
        self.allowed_packages.is_empty() || self.allowed_packages.contains(service)
    }

    /// Is this image reference from a registry the organisation allows?
    ///
    /// The reference is matched on its host part, and a reference with no host
    /// is Docker Hub — which is the case that has to be got right, because
    /// `mysql:8.0` naming no registry is what most of the catalogue says.
    pub fn allows_registry(&self, reference: &str) -> bool {
        if self.allowed_registries.is_empty() {
            return true;
        }
        // A reference with no `/` at all has no host part — every colon in it
        // belongs to the tag. `mysql:8.0` is Docker Hub, and reading its `:8.0`
        // as a port would have let a bare image through a list that names one
        // registry, which is the opposite of what the list is for. Only past a
        // slash is a head with a dot, a port or the name `localhost` a registry.
        let host = match reference.split_once('/') {
            Some((head, _)) if head.contains('.') || head.contains(':') || head == "localhost" => {
                head
            }
            _ => "docker.io",
        };
        self.allowed_registries.contains(host)
    }

    /// Is this catalogue source one the organisation allows?
    ///
    /// Two spellings, because a source is one of two very different things.
    ///
    /// * `https://…` is matched on the **host**. A policy naming
    ///   `https://packages.corp.example` allows every path under that host and
    ///   nothing on any other, which is what an administrator writing one line
    ///   means — and matching the whole string instead would refuse the same
    ///   mirror the moment a path or a trailing slash differed.
    /// * A local path is matched as a **directory prefix**, on a boundary. A
    ///   policy naming `/opt/stackvo` allows `/opt/stackvo/packages` and
    ///   refuses `/opt/stackvo-evil`, which a bare `starts_with` would let
    ///   through — the same class of bug as reading `mysql:8.0`'s tag as a
    ///   port, and worth avoiding the same way.
    ///
    /// Not a security boundary — ADR 0009's sentence holds here as everywhere
    /// in this file. It stops a well-meaning user pointing the app at the wrong
    /// mirror; it does not stop the person holding the machine.
    pub fn allows_source(&self, location: &str) -> bool {
        if self.allowed_sources.is_empty() {
            return true;
        }
        // The scheme must actually be a scheme. Splitting on the first `://`
        // alone reads `/tmp/https://packages.example` as the host
        // `packages.example`, so a directory anybody can create under /tmp
        // would satisfy a policy naming a mirror.
        let host_of = |value: &str| {
            let (scheme, rest) = value.split_once("://")?;
            if scheme.is_empty() || scheme.contains('/') {
                return None;
            }
            Some(rest.split('/').next().unwrap_or("").to_ascii_lowercase())
        };

        match host_of(location) {
            Some(host) if !host.is_empty() => self
                .allowed_sources
                .iter()
                .filter_map(|allowed| host_of(allowed))
                .any(|allowed| allowed == host),
            // A path, or a URL this build cannot read a host out of. Either way
            // it is compared as a path, and an entry that *is* a URL will not
            // match one — which is correct: `https://x` does not authorise
            // a directory called `https://x`.
            _ => self.allowed_sources.iter().any(|allowed| {
                if host_of(allowed).is_some() {
                    return false;
                }
                let base = allowed.trim_end_matches('/');
                location == base
                    || location
                        .strip_prefix(base)
                        .is_some_and(|rest| rest.starts_with('/'))
            }),
        }
    }

    fn is_set(&self) -> bool {
        self.registry_url.is_some()
            || self.offline_bundle.is_some()
            || self.require_signature
            || !self.allowed_packages.is_empty()
            || !self.allowed_registries.is_empty()
            || !self.allowed_sources.is_empty()
            || self.auto_update.is_some()
            || !self.additional_keys.is_empty()
    }
}

/// The `hooks` block: whether a project may run commands when it starts.
///
/// B-3. Both fields default to on, which is the same "no opinion" default the
/// rest of this file has — an unmanaged machine behaves as though no policy
/// existed.
///
/// ## Both of these can only tighten
///
/// The same asymmetry as [`Market::require_signature`], and worth stating for
/// the same reason. Neither key can turn a check *off*: `allowHost` false stops
/// host commands, and `allowHost` true is the default, so there is no value of
/// either that grants something the machine did not already grant. In
/// particular **neither of them replaces consent** — an administrator can
/// forbid host steps fleet-wide, and cannot approve them on the user's behalf.
/// Approval is a thing a person does after reading a list of commands, and a
/// file pushed to three hundred laptops has not read anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hooks {
    /// Whether hooks run at all.
    pub enabled: bool,
    /// Whether a hook may run a command on the machine rather than in the
    /// project's container.
    pub allow_host: bool,
}

impl Default for Hooks {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_host: true,
        }
    }
}

impl Hooks {
    fn is_set(&self) -> bool {
        *self != Self::default()
    }
}

impl Policy {
    /// No administrator has said anything. The ordinary case.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn market(&self) -> &Market {
        &self.market
    }

    pub fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    /// Is there anything here at all?
    ///
    /// A policy file that exists but sets nothing still counts, because the
    /// answer is shown to a user asking why a field is greyed out — and "a
    /// policy is in force and it is empty" is a different thing to explain
    /// from "there is no policy".
    pub fn is_active(&self) -> bool {
        self.source.is_some()
    }

    pub fn settings(&self) -> &BTreeMap<String, String> {
        &self.settings
    }

    pub fn locked(&self) -> &BTreeSet<String> {
        &self.locked
    }

    pub fn is_locked(&self, key: &str) -> bool {
        self.locked.contains(key)
    }

    pub fn registry_prefix(&self) -> Option<&str> {
        self.registry_prefix.as_deref()
    }

    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Human-facing origin, for an error a user has to do something about.
    ///
    /// "the policy file" alone tells somebody nothing: the whole action they
    /// can take is to show the path to whoever administers the machine.
    pub fn origin(&self) -> String {
        match &self.source {
            Some(path) => path.display().to_string(),
            None => "the policy file".to_string(),
        }
    }

    /// Read and parse, reporting rather than failing.
    ///
    /// A missing file is not an error and not a policy — the overwhelmingly
    /// common case is an unmanaged machine, and treating that as a fault would
    /// put a warning on every desktop install.
    pub fn load_from(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::none(),
            Err(e) => {
                return Self {
                    source: Some(path.to_path_buf()),
                    error: Some(format!("could not be read: {e}")),
                    ..Self::none()
                }
            }
        };
        Self::parse(&text, path)
    }

    /// The parser. Pure, so every rule below is testable without a filesystem.
    pub fn parse(text: &str, from: &Path) -> Self {
        let source = Some(from.to_path_buf());
        let broken = |why: String| Self {
            source: source.clone(),
            error: Some(why),
            ..Self::none()
        };

        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(e) => return broken(format!("is not valid JSON: {e}")),
        };
        let Some(object) = value.as_object() else {
            return broken("must be a JSON object".to_string());
        };

        // Checked before anything is read out of it. A future file describing
        // settings in a shape this build does not know is not a file to apply
        // the recognisable half of — that is how half a policy takes effect.
        match object
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
        {
            Some(SCHEMA_VERSION) => {}
            Some(other) => {
                return broken(format!(
                    "declares schemaVersion {other}; this build understands {SCHEMA_VERSION}"
                ))
            }
            None => return broken("has no schemaVersion".to_string()),
        }

        let mut complaints: Vec<String> = Vec::new();

        let mut settings = BTreeMap::new();
        if let Some(given) = object.get("settings") {
            match given.as_object() {
                Some(map) => {
                    for (key, value) in map {
                        // Not `to_string()` on the Value: that would write a
                        // JSON number or a quoted string into `.env`, and
                        // `"8080"` with the quotes is not a port.
                        match value.as_str() {
                            Some(text) => {
                                settings.insert(key.clone(), text.to_string());
                            }
                            None => complaints
                                .push(format!("settings.{key} is not a string and was ignored")),
                        }
                    }
                }
                None => complaints.push("settings is not an object and was ignored".to_string()),
            }
        }

        let mut locked = BTreeSet::new();
        if let Some(given) = object.get("locked") {
            match given.as_array() {
                Some(list) => {
                    for entry in list {
                        let Some(key) = entry.as_str() else {
                            complaints.push("locked contains a non-string entry".to_string());
                            continue;
                        };
                        // The rule from the module comment, enforced here so
                        // `locked` is a subset of `settings` by construction.
                        if settings.contains_key(key) {
                            locked.insert(key.to_string());
                        } else {
                            complaints.push(format!(
                                "{key} is locked but not set, so there is nothing to hold it to; \
                                 the entry was ignored"
                            ));
                        }
                    }
                }
                None => complaints.push("locked is not an array and was ignored".to_string()),
            }
        }

        let mut registry_prefix = None;
        if let Some(given) = object.get("registryPrefix") {
            match given.as_str().map(str::trim) {
                Some("") | None => {
                    complaints.push("registryPrefix is not a non-empty string".to_string())
                }
                Some(prefix) => registry_prefix = Some(prefix.trim_end_matches('/').to_string()),
            }
        }

        let market = parse_market(object.get("market"), &mut complaints);
        let hooks = parse_hooks(object.get("hooks"), &mut complaints);

        Self {
            settings,
            locked,
            registry_prefix,
            market,
            hooks,
            source,
            error: (!complaints.is_empty()).then(|| complaints.join("; ")),
        }
    }

    /// Does the policy say anything about the market at all?
    pub fn constrains_market(&self) -> bool {
        self.market.is_set()
    }

    /// Does the policy say anything about hooks at all?
    pub fn constrains_hooks(&self) -> bool {
        self.hooks.is_set()
    }
}

/// The `market` block, field by field.
///
/// Every unreadable field is a complaint and a fallback to "no opinion" rather
/// than a refusal of the whole file — the module comment's rule, applied here:
/// a typo pushed to a fleet cannot be a fleet that will not start. The
/// complaints are what stop that from being silent.
fn parse_market(given: Option<&serde_json::Value>, complaints: &mut Vec<String>) -> Market {
    let mut market = Market::default();
    let Some(given) = given else {
        return market;
    };
    let Some(object) = given.as_object() else {
        complaints.push("market is not an object and was ignored".to_string());
        return market;
    };

    let text = |value: &serde_json::Value, key: &str, complaints: &mut Vec<String>| match value
        .as_str()
        .map(str::trim)
    {
        Some("") | None => {
            complaints.push(format!(
                "market.{key} is not a non-empty string and was ignored"
            ));
            None
        }
        Some(found) => Some(found.to_string()),
    };

    let list = |value: &serde_json::Value, key: &str, complaints: &mut Vec<String>| {
        let Some(array) = value.as_array() else {
            complaints.push(format!("market.{key} is not an array and was ignored"));
            return BTreeSet::new();
        };
        let mut out = BTreeSet::new();
        for entry in array {
            match entry.as_str().map(str::trim) {
                Some("") | None => {
                    complaints.push(format!("market.{key} contains a non-string entry"))
                }
                Some(found) => {
                    out.insert(found.to_string());
                }
            }
        }
        out
    };

    for (key, value) in object {
        match key.as_str() {
            "registryUrl" => market.registry_url = text(value, key, complaints),
            "offlineBundle" => {
                market.offline_bundle = text(value, key, complaints).map(PathBuf::from)
            }
            "requireSignature" => match value.as_bool() {
                Some(on) => market.require_signature = on,
                None => complaints
                    .push("market.requireSignature is not a boolean and was ignored".to_string()),
            },
            "autoUpdate" => match value.as_bool() {
                Some(on) => market.auto_update = Some(on),
                None => complaints
                    .push("market.autoUpdate is not a boolean and was ignored".to_string()),
            },
            "allowedPackages" => market.allowed_packages = list(value, key, complaints),
            "allowedRegistries" => market.allowed_registries = list(value, key, complaints),
            "allowedSources" => market.allowed_sources = list(value, key, complaints),
            "additionalKeys" => {
                market.additional_keys = list(value, key, complaints).into_iter().collect()
            }
            // Named rather than dropped. An administrator who typed
            // `allowedPackage` deployed a file that does nothing, and finding
            // that out from the app beats finding it out from a user.
            other => complaints.push(format!("market.{other} is not a key this build knows")),
        }
    }

    market
}

/// The `hooks` block, field by field. Same shape as [`parse_market`].
fn parse_hooks(given: Option<&serde_json::Value>, complaints: &mut Vec<String>) -> Hooks {
    let mut hooks = Hooks::default();
    let Some(value) = given else {
        return hooks;
    };
    let Some(object) = value.as_object() else {
        complaints.push("hooks is not an object and was ignored".to_string());
        return hooks;
    };

    for (key, value) in object {
        match key.as_str() {
            "enabled" => match value.as_bool() {
                Some(on) => hooks.enabled = on,
                None => {
                    complaints.push("hooks.enabled is not a boolean and was ignored".to_string())
                }
            },
            "allowHost" => match value.as_bool() {
                Some(on) => hooks.allow_host = on,
                None => {
                    complaints.push("hooks.allowHost is not a boolean and was ignored".to_string())
                }
            },
            // Named rather than dropped, for the reason `market` gives: a typo
            // deploys a file that does nothing, and the app is a better place
            // to find that out than a user is.
            other => complaints.push(format!("hooks.{other} is not a key this build knows")),
        }
    }

    hooks
}

/// The file this build will look at, override included.
///
/// Returns `None` on a platform with no defined location rather than guessing
/// one — a path invented for an unknown target is a file nobody will ever put
/// there and a support answer that is wrong.
pub fn path() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os(OVERRIDE_VAR) {
        if !override_path.is_empty() {
            return Some(PathBuf::from(override_path));
        }
    }
    default_path()
}

/// Where the administrator's tooling is expected to write.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(
            "/Library/Managed Preferences/com.stackvo.desktop.json",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        // `%ProgramData%` rather than a hard-coded `C:\ProgramData`: the
        // directory is relocatable and a domain image is exactly where it will
        // have been relocated.
        std::env::var_os("ProgramData")
            .map(|dir| PathBuf::from(dir).join("StackVo").join("policy.json"))
    }
    #[cfg(target_os = "linux")]
    {
        Some(PathBuf::from("/etc/stackvo/policy.json"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// The policy for this run, read once.
///
/// Once per process, deliberately. A managed preference is not a setting that
/// changes while the app is open — the file is written by an administrator's
/// tooling, and re-reading it on every `.env` load would put a stat call in
/// every command for a value that has not moved. Restarting is the documented
/// way to pick up a change, which is how every managed-preferences reader on
/// every platform behaves.
pub fn current() -> &'static Policy {
    static CURRENT: std::sync::OnceLock<Policy> = std::sync::OnceLock::new();
    CURRENT.get_or_init(|| match path() {
        Some(path) => Policy::load_from(&path),
        None => Policy::none(),
    })
}

/// Point an image reference at the organisation's registry.
///
/// Three references are left exactly as they were, and each exception exists
/// because rewriting it would break something:
///
///   * **one that already names a registry** — `ghcr.io/x/y`,
///     `localhost:5000/z`. Docker's own rule: the first path component is a
///     host if it contains `.` or `:`, or is exactly `localhost`. Redirecting a
///     deliberate choice somewhere else is not what a mirror is for.
///   * **one that already starts with the prefix** — rendering twice must not
///     rewrite twice, and `proxy/proxy/mysql` is a 404.
///   * **an image starting with `stackvo-`** — these are built on this machine
///     by `docker compose build` and exist in no registry at all. A prefix
///     would make them simultaneously unpullable and unbuildable.
pub fn mirror(prefix: &str, reference: &str) -> String {
    let trimmed = reference.trim();
    if prefix.is_empty() || trimmed.is_empty() {
        return reference.to_string();
    }
    if names_a_registry(trimmed)
        || trimmed.starts_with(&format!("{prefix}/"))
        || trimmed.starts_with("stackvo-")
    {
        return reference.to_string();
    }
    format!("{prefix}/{trimmed}")
}

/// Docker's rule for "the first component is a registry host, not a namespace".
///
/// `mysql:8.0` has a colon and no host; `docker.io/library/mysql` has one. The
/// difference is entirely whether there is a `/` at all, which is why the split
/// comes first.
fn names_a_registry(reference: &str) -> bool {
    let Some((first, _)) = reference.split_once('/') else {
        return false;
    };
    first.contains('.') || first.contains(':') || first == "localhost"
}

/// Should this generated file have its image references rewritten?
///
/// Narrow on purpose. `image:` is a key that could plausibly appear in a
/// service's own configuration file one day, and a rewrite that silently
/// edited `elasticsearch.yml` would be a bug nobody could find. The two
/// syntaxes below only mean "pull this" in the two kinds of file named here.
pub fn rewrites(label: &str) -> bool {
    let name = label.rsplit('/').next().unwrap_or(label);
    name == "Dockerfile"
        || name.ends_with(".Dockerfile")
        || name.ends_with(".yml")
        || name.ends_with(".yaml")
}

/// Rewrite every image reference in already-rendered text.
///
/// **Rendered text, not the twenty `.tpl` files.** The templates are the
/// contract with the Bash generator, which knows nothing about a mirror;
/// editing them would drop every differential comparison for a reason that has
/// nothing to do with the port, and would leave the workspace's own copies of
/// those templates unrewritten anyway.
pub fn rewrite(text: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }

    // Stage names declared by `FROM x AS base`. A later `FROM base` refers to
    // that stage and is not an image at all — prefixing it turns a working
    // multi-stage build into a pull of something that does not exist. Docker
    // resolves it this way and so must this.
    let mut stages: BTreeSet<String> = BTreeSet::new();

    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        out.push_str(&rewrite_line(line, prefix, &mut stages));
    }
    out
}

fn rewrite_line(line: &str, prefix: &str, stages: &mut BTreeSet<String>) -> String {
    let (body, newline) = match line.strip_suffix('\n') {
        Some(body) => (body.strip_suffix('\r').unwrap_or(body), "\n"),
        None => (line, ""),
    };
    let carriage = if body.len() + 1 < line.len() {
        "\r"
    } else {
        ""
    };

    let replaced = rewrite_from(body, prefix, stages).or_else(|| rewrite_image(body, prefix));

    match replaced {
        Some(text) => format!("{text}{carriage}{newline}"),
        None => line.to_string(),
    }
}

/// `FROM [--flag=…] <image> [AS <stage>]`.
fn rewrite_from(body: &str, prefix: &str, stages: &mut BTreeSet<String>) -> Option<String> {
    let indent_len = body.len() - body.trim_start().len();
    let (indent, rest) = body.split_at(indent_len);

    let mut words = rest.split_whitespace();
    // Case-insensitive because the Dockerfile spec is, even though every
    // generator in this repo writes it upper case.
    if !words.next()?.eq_ignore_ascii_case("FROM") {
        return None;
    }

    // `--platform=linux/amd64` and friends sit between the keyword and the
    // image; they are not the image.
    let image = words.by_ref().find(|word| !word.starts_with("--"))?;

    // `AS <name>` declares a stage. Recorded whether or not the image itself
    // gets rewritten, because a later `FROM <name>` has to be recognisable.
    let mut tail = words;
    if let (Some(keyword), Some(name)) = (tail.next(), tail.next()) {
        if keyword.eq_ignore_ascii_case("AS") {
            stages.insert(name.to_string());
        }
    }

    if stages.contains(image) {
        return None;
    }

    let mirrored = mirror(prefix, image);
    if mirrored == image {
        return None;
    }
    // `replacen` over the original text rather than rejoining the words: the
    // spacing in the line is somebody's formatting and rebuilding it from
    // `split_whitespace` would quietly reflow every FROM in the file.
    Some(format!("{indent}{}", rest.replacen(image, &mirrored, 1)))
}

/// `image: mysql:8.0`, quoted or not.
fn rewrite_image(body: &str, prefix: &str) -> Option<String> {
    let indent_len = body.len() - body.trim_start().len();
    let (indent, rest) = body.split_at(indent_len);

    // `- image: …` never appears in a compose file, and a YAML list item that
    // happens to be called `image` is not one either. Anchoring on the key
    // keeps this from matching prose in a comment.
    let value = rest.strip_prefix("image:")?.trim();
    if value.is_empty() || value.starts_with('#') {
        return None;
    }

    // Something the renderer left behind — a compose-time `${VAR}` or an
    // unsubstituted `{{ … }}`. Prefixing a variable reference is a guess about
    // what it will expand to, and the guess is wrong as often as not.
    if value.contains("${") || value.contains("{{") {
        return None;
    }

    let (quote, bare) = match value.chars().next() {
        Some(q @ ('"' | '\'')) => (q.to_string(), value.trim_matches(q)),
        _ => (String::new(), value),
    };

    // An inline comment after the value: `image: mysql:8.0  # pinned`.
    let (bare, comment) = match bare.split_once(" #") {
        Some((image, note)) => (image.trim_end(), format!(" #{note}")),
        None => (bare, String::new()),
    };

    let mirrored = mirror(prefix, bare);
    if mirrored == bare {
        return None;
    }
    Some(format!("{indent}image: {quote}{mirrored}{quote}{comment}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Policy {
        Policy::parse(text, Path::new("/etc/stackvo/policy.json"))
    }

    const GOOD: &str = r#"{
        "schemaVersion": 1,
        "settings": { "DEFAULT_TLD_SUFFIX": "corp.test", "SERVER_TYPE": "nginx" },
        "locked": ["DEFAULT_TLD_SUFFIX"],
        "registryPrefix": "registry.corp.example/proxy"
    }"#;

    #[test]
    fn a_well_formed_policy_reads_back_exactly() {
        let policy = at(GOOD);
        assert_eq!(policy.error(), None, "nothing to complain about");
        assert!(policy.is_active());
        assert_eq!(
            policy.settings().get("SERVER_TYPE").map(String::as_str),
            Some("nginx")
        );
        assert!(policy.is_locked("DEFAULT_TLD_SUFFIX"));
        assert!(!policy.is_locked("SERVER_TYPE"), "set is not locked");
        assert_eq!(
            policy.registry_prefix(),
            Some("registry.corp.example/proxy")
        );
    }

    /// The rule that stops a policy from freezing a machine on whatever it had.
    #[test]
    fn a_key_that_is_locked_without_being_set_is_ignored_and_reported() {
        let policy = at(r#"{
            "schemaVersion": 1,
            "settings": { "SERVER_TYPE": "nginx" },
            "locked": ["SERVER_TYPE", "SSL_ENABLE"]
        }"#);

        assert!(policy.is_locked("SERVER_TYPE"));
        assert!(
            !policy.is_locked("SSL_ENABLE"),
            "locking a key the policy does not set holds the machine to whatever \
             it happened to have, which is the opposite of a managed fleet"
        );
        let error = policy
            .error()
            .expect("the administrator has to hear about this");
        assert!(
            error.contains("SSL_ENABLE"),
            "and it has to name the key: {error}"
        );
    }

    /// A typo in a file pushed to every machine must not close every machine.
    #[test]
    fn broken_json_yields_an_empty_policy_rather_than_a_failure() {
        let policy = at("{ this is not json");

        assert!(policy.settings().is_empty());
        assert!(policy.locked().is_empty());
        assert_eq!(policy.registry_prefix(), None);
        assert!(
            policy.error().is_some_and(|e| e.contains("not valid JSON")),
            "silence would leave the administrator believing it is in force"
        );
        assert!(
            policy.is_active(),
            "the file is there and did not apply — that is a different thing to \
             report from no policy at all"
        );
    }

    #[test]
    fn a_schema_version_this_build_does_not_know_applies_nothing() {
        let policy = at(r#"{ "schemaVersion": 7, "settings": { "SERVER_TYPE": "apache" } }"#);

        assert!(
            policy.settings().is_empty(),
            "applying the half of a future file this build recognises is how half \
             a policy takes effect"
        );
        assert!(policy.error().is_some_and(|e| e.contains("7")));
    }

    #[test]
    fn a_file_with_no_schema_version_is_not_guessed_at() {
        assert!(at(r#"{ "settings": {} }"#).error().is_some());
    }

    /// A number in `settings` would be written into `.env` as one, and the
    /// quoting is exactly what makes `PORT="8080"` not a port.
    #[test]
    fn a_non_string_setting_is_dropped_and_named() {
        let policy = at(r#"{
            "schemaVersion": 1,
            "settings": { "SERVER_TYPE": "nginx", "SERVER_KEEPALIVE_TIMEOUT": 75 }
        }"#);

        assert_eq!(policy.settings().len(), 1);
        assert!(policy
            .error()
            .is_some_and(|e| e.contains("SERVER_KEEPALIVE_TIMEOUT")));
    }

    #[test]
    fn no_file_is_not_a_policy_and_not_a_complaint() {
        let policy = Policy::load_from(Path::new("/nonexistent/stackvo/policy.json"));

        assert!(!policy.is_active());
        assert_eq!(
            policy.error(),
            None,
            "an unmanaged machine is the ordinary case"
        );
        assert_eq!(policy.origin(), "the policy file");
    }

    #[test]
    fn a_trailing_slash_on_the_prefix_does_not_become_a_double_slash() {
        let policy = at(r#"{ "schemaVersion": 1, "registryPrefix": "reg.example/proxy/" }"#);
        assert_eq!(policy.registry_prefix(), Some("reg.example/proxy"));
    }

    // ------------------------------------------------------------ the mirror

    const PREFIX: &str = "registry.corp.example/proxy";

    #[test]
    fn a_bare_reference_is_mirrored() {
        assert_eq!(
            mirror(PREFIX, "mysql:8.0"),
            "registry.corp.example/proxy/mysql:8.0"
        );
        assert_eq!(
            mirror(PREFIX, "library/redis:7"),
            "registry.corp.example/proxy/library/redis:7"
        );
    }

    #[test]
    fn a_reference_that_already_names_a_registry_is_left_alone() {
        for reference in [
            "ghcr.io/foo/bar:1",
            "localhost:5000/thing",
            "localhost/thing",
            "192.168.1.5:5000/thing",
            "quay.io/prometheus/node-exporter",
        ] {
            assert_eq!(
                mirror(PREFIX, reference),
                reference,
                "{reference} is a deliberate choice of registry"
            );
        }
    }

    #[test]
    fn rendering_twice_does_not_rewrite_twice() {
        let once = mirror(PREFIX, "mysql:8.0");
        assert_eq!(mirror(PREFIX, &once), once);
    }

    /// The exception that is not obvious until a build fails.
    #[test]
    fn an_image_built_on_this_machine_is_never_mirrored() {
        assert_eq!(
            mirror(PREFIX, "stackvo-php-8.4:latest"),
            "stackvo-php-8.4:latest"
        );
        assert_eq!(mirror(PREFIX, "stackvo-shop"), "stackvo-shop");
    }

    #[test]
    fn no_prefix_means_no_rewriting() {
        assert_eq!(mirror("", "mysql:8.0"), "mysql:8.0");
        assert_eq!(rewrite("image: mysql:8.0\n", ""), "image: mysql:8.0\n");
    }

    // ----------------------------------------------------------- the rewrite

    #[test]
    fn compose_image_lines_are_rewritten_in_place() {
        let text = "services:\n  db:\n    image: mysql:8.0\n    restart: always\n";
        assert_eq!(
            rewrite(text, PREFIX),
            "services:\n  db:\n    image: registry.corp.example/proxy/mysql:8.0\n    restart: always\n"
        );
    }

    #[test]
    fn a_quoted_or_commented_image_keeps_its_quoting_and_its_comment() {
        assert_eq!(
            rewrite("    image: \"redis:7\"\n", PREFIX),
            "    image: \"registry.corp.example/proxy/redis:7\"\n"
        );
        assert_eq!(
            rewrite("    image: redis:7  # pinned\n", PREFIX),
            "    image: registry.corp.example/proxy/redis:7 # pinned\n"
        );
    }

    #[test]
    fn an_unsubstituted_variable_is_not_guessed_at() {
        for line in [
            "    image: ${MYSQL_IMAGE}\n",
            "    image: {{ SERVICE_X }}\n",
        ] {
            assert_eq!(rewrite(line, PREFIX), line);
        }
    }

    #[test]
    fn dockerfile_from_lines_are_rewritten_including_behind_a_flag() {
        assert_eq!(
            rewrite("FROM php:8.4-fpm\n", PREFIX),
            "FROM registry.corp.example/proxy/php:8.4-fpm\n"
        );
        assert_eq!(
            rewrite("FROM --platform=linux/amd64 node:22 AS build\n", PREFIX),
            "FROM --platform=linux/amd64 registry.corp.example/proxy/node:22 AS build\n"
        );
    }

    /// The one that turns a working build into a pull of something that has
    /// never existed anywhere.
    #[test]
    fn a_from_that_names_an_earlier_stage_is_not_an_image() {
        let dockerfile = "FROM php:8.4-fpm AS base\nRUN true\nFROM base\nCOPY . .\n";
        let out = rewrite(dockerfile, PREFIX);

        assert!(out.contains("FROM registry.corp.example/proxy/php:8.4-fpm AS base"));
        assert!(
            out.contains("\nFROM base\n"),
            "`base` is a stage in this same file, not something to pull: {out}"
        );
    }

    #[test]
    fn nothing_else_in_the_file_is_touched() {
        let text =
            "# image: mysql:8.0 in a comment\nCOPY --from=build /app /app\nRUN echo image: x\n";
        assert_eq!(rewrite(text, PREFIX), text);
    }

    #[test]
    fn a_file_with_no_trailing_newline_does_not_gain_one() {
        assert_eq!(
            rewrite("image: mysql:8.0", PREFIX),
            "image: registry.corp.example/proxy/mysql:8.0"
        );
    }

    #[test]
    fn crlf_survives_the_rewrite() {
        assert_eq!(
            rewrite("image: mysql:8.0\r\n", PREFIX),
            "image: registry.corp.example/proxy/mysql:8.0\r\n"
        );
    }

    /// Which files the rewrite is allowed near.
    #[test]
    fn only_dockerfiles_and_compose_files_are_rewritten() {
        assert!(rewrites("shop/Dockerfile"));
        assert!(rewrites("docker-compose.projects.yml"));
        assert!(rewrites("stackvo.yml"));
        assert!(!rewrites("configs/mysql.cnf"));
        assert!(!rewrites("shop/.dockerignore"));
        assert!(!rewrites("shop/nginx.conf"));
    }

    // ------------------------------------------------------------- market

    fn market_of(json: &str) -> Policy {
        Policy::parse(json, Path::new("/policy.json"))
    }

    /// The default is silence, and silence is not refusal. An administrator who
    /// wrote a `market` block for one unrelated reason has not thereby
    /// forbidden every package on the machine.
    #[test]
    fn an_empty_market_block_forbids_nothing() {
        let policy = market_of(r#"{"schemaVersion": 1, "market": {}}"#);
        assert!(policy.market().allows_package("mysql"));
        assert!(policy.market().allows_registry("mysql:8.0"));
        assert!(!policy.constrains_market());
        assert_eq!(policy.error(), None);
    }

    #[test]
    fn an_allow_list_admits_what_it_names_and_nothing_else() {
        let policy =
            market_of(r#"{"schemaVersion": 1, "market": {"allowedPackages": ["mysql", "redis"]}}"#);
        assert!(policy.market().allows_package("mysql"));
        assert!(!policy.market().allows_package("cassandra"));
        assert!(policy.constrains_market());
    }

    /// A reference with no host is Docker Hub, and it is the case that has to be
    /// right: most of the catalogue writes `mysql:8.0` and names no registry at
    /// all. Reading that as "no registry, so allowed" would make the list
    /// enforce nothing.
    #[test]
    fn a_bare_image_reference_counts_as_docker_hub() {
        let policy = market_of(
            r#"{"schemaVersion": 1, "market": {"allowedRegistries": ["registry.corp.example"]}}"#,
        );
        assert!(!policy.market().allows_registry("mysql:8.0"));
        assert!(!policy.market().allows_registry("valkey/valkey:8"));
        assert!(policy
            .market()
            .allows_registry("registry.corp.example/mysql:8.0"));
        assert!(!policy
            .market()
            .allows_registry("docker.elastic.co/elasticsearch/elasticsearch:8.11.3"));
    }

    #[test]
    fn docker_hub_can_be_named_explicitly() {
        let policy =
            market_of(r#"{"schemaVersion": 1, "market": {"allowedRegistries": ["docker.io"]}}"#);
        assert!(policy.market().allows_registry("mysql:8.0"));
        assert!(!policy
            .market()
            .allows_registry("docker.elastic.co/elasticsearch/elasticsearch:8.11.3"));
    }

    /// The one key in the block that can only tighten. There is no value of it
    /// that turns verification off — a policy key that could would mean the
    /// check was never a check (ADR 0009).
    #[test]
    fn require_signature_is_off_until_it_is_asked_for() {
        assert!(
            !market_of(r#"{"schemaVersion": 1}"#)
                .market()
                .require_signature
        );
        assert!(
            market_of(r#"{"schemaVersion": 1, "market": {"requireSignature": true}}"#)
                .market()
                .require_signature
        );
    }

    /// A typo cannot make the app unstartable, and it cannot be silent either.
    /// A key nobody knows is a file that does nothing, and the administrator
    /// who deployed it believes it is in force.
    #[test]
    fn an_unknown_market_key_is_named_rather_than_dropped() {
        let policy = market_of(r#"{"schemaVersion": 1, "market": {"allowedPackage": ["mysql"]}}"#);
        let error = policy.error().unwrap_or_default();
        assert!(error.contains("allowedPackage"), "{error}");
        assert!(policy.market().allows_package("cassandra"));
    }

    #[test]
    fn a_market_field_of_the_wrong_type_is_reported_and_ignored() {
        let policy = market_of(
            r#"{"schemaVersion": 1, "market": {"requireSignature": "yes", "allowedPackages": "mysql"}}"#,
        );
        let error = policy.error().unwrap_or_default();
        assert!(error.contains("requireSignature"), "{error}");
        assert!(error.contains("allowedPackages"), "{error}");
        assert!(!policy.market().require_signature);
        assert!(policy.market().allows_package("anything"));
    }

    /// ADR 0011: with nothing embedded, this is the whole of how a machine with
    /// no network gets a catalogue.
    #[test]
    fn an_offline_bundle_is_a_path() {
        let policy = market_of(
            r#"{"schemaVersion": 1, "market": {"offlineBundle": "/opt/stackvo/packages"}}"#,
        );
        assert_eq!(
            policy.market().offline_bundle.as_deref(),
            Some(Path::new("/opt/stackvo/packages"))
        );
        assert!(policy.constrains_market());
    }

    // ---- market.allowedSources (C-2) --------------------------------------

    fn sources(list: &[&str]) -> Market {
        Market {
            allowed_sources: list.iter().map(|s| s.to_string()).collect(),
            ..Market::default()
        }
    }

    /// Silence is not a refusal — the same reading every other list here gets.
    #[test]
    fn an_empty_source_list_allows_everything() {
        let market = Market::default();
        assert!(market.allows_source("https://packages.example/x"));
        assert!(market.allows_source("/opt/anything"));
    }

    /// One line naming a mirror has to mean the mirror, not one path on it.
    #[test]
    fn an_https_source_is_matched_on_its_host() {
        let market = sources(&["https://packages.corp.example"]);
        assert!(market.allows_source("https://packages.corp.example"));
        assert!(market.allows_source("https://packages.corp.example/catalogue/v2"));
        assert!(market.allows_source("https://PACKAGES.CORP.EXAMPLE/x"));
        assert!(!market.allows_source("https://evil.example/packages.corp.example"));
    }

    /// The `mysql:8.0`-read-as-a-port class of bug, in its path form: a bare
    /// `starts_with` would let a sibling directory through.
    #[test]
    fn a_local_source_matches_on_a_directory_boundary() {
        let market = sources(&["/opt/stackvo"]);
        assert!(market.allows_source("/opt/stackvo"));
        assert!(market.allows_source("/opt/stackvo/packages"));
        assert!(!market.allows_source("/opt/stackvo-evil"));
        assert!(!market.allows_source("/opt"));
    }

    /// A trailing slash in the policy is a typo, not a different rule.
    #[test]
    fn a_trailing_slash_in_the_policy_changes_nothing() {
        assert!(sources(&["/opt/stackvo/"]).allows_source("/opt/stackvo/packages"));
    }

    /// The two spellings do not authorise each other.
    #[test]
    fn a_url_entry_does_not_authorise_a_path_that_looks_like_it() {
        assert!(
            !sources(&["https://packages.example"]).allows_source("/tmp/https://packages.example")
        );
        assert!(!sources(&["/opt/stackvo"]).allows_source("https://opt/stackvo"));
    }

    /// The block counts as "an administrator said something", so Settings can
    /// explain why a field is refusing rather than looking broken.
    #[test]
    fn naming_a_source_makes_the_market_block_active() {
        assert!(sources(&["/opt/stackvo"]).is_set());
    }
}
