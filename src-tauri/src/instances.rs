//! What is installed, how many times, and under which names.
//!
//! Faz 2 of `docs/servis-market-mimarisi.md`. Until now a service's whole state
//! was a `SERVICE_<ID>_*` family in `.env`: `SERVICE_MYSQL_ENABLE=true` and
//! `SERVICE_MYSQL_VERSION=8.0`. That works exactly as long as a service has one
//! version, and it cannot be stretched — `.env` is flat by contract (first `=`
//! wins, no quoting, no nesting, and two other parsers read the same file), so
//! there is no spelling of `SERVICE_MYSQL_VERSION` that means "8.0 and 9.4".
//!
//! So the state moves here, and the shape of the move is the point: this is
//! **application state**, not configuration. `.env` keeps the stack-shaping
//! choices somebody makes on purpose — the domain suffix, whether TLS is on —
//! and stops carrying a record of what the app installed. A user editing
//! `.env` is expressing an intention; a user editing this file is fighting the
//! app.
//!
//! ## Names are derived, never stored twice
//!
//! Everything an instance is called comes from one pair, `(service, version)`,
//! through [`slug`]. That is not tidiness — it is the fix for the thing that
//! made multiple versions impossible. Twenty-five templates hardcode
//! `container_name: "stackvo-mysql"`, and eighteen hardcode a volume like
//! `stackvo-mysql-data`. The container name merely refuses to start twice; the
//! **volume silently succeeds**. Two MySQL versions sharing `stackvo-mysql-data`
//! is not an error Docker reports — 9.4 opens 8.0's datadir and upgrades it, and
//! the first anyone hears is that 8.0 will no longer start.
//!
//! ## The legacy alias, and why exactly one instance may hold it
//!
//! Every project's own `.env` says `DB_HOST=stackvo-mysql`, in the user's
//! source tree, where this app has no business writing. Renaming the container
//! to `stackvo-mysql-8-0` breaks all of them at once.
//!
//! So the instance marked [`Instance::primary`] carries the old name as a
//! network alias alongside its own. Docker resolves an alias exactly as it
//! resolves a container name, so nothing downstream can tell the difference.
//! What it does NOT do is complain when two containers claim one alias: DNS
//! answers with whichever it likes, and the symptom is "it connects to the
//! wrong database sometimes", which is the most expensive class of bug to
//! diagnose. [`Table::check`] refuses that arrangement before a compose file
//! can contain it.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Bumped when a field changes meaning, not when one is added.
///
/// A reader that meets a higher number stops rather than guessing: this file
/// decides which containers exist and which volumes are safe to delete, and
/// half-understanding it is worse than refusing it.
pub const SCHEMA_VERSION: u32 = 1;

/// `<root>/services/instances.json`.
pub fn path(root: &Path) -> PathBuf {
    root.join("services").join("instances.json")
}

/// Where a package came from, so an install can be re-verified later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRef {
    /// `official`, or the name of a source a policy allowed.
    pub source: String,
    /// Of the version manifest, as the registry stated it at install time.
    pub sha256: String,
    /// RFC 3339. Recorded rather than derived from mtime, which a backup
    /// restore rewrites.
    pub installed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// [`slug`] of `(service, version)`. Every other name derives from it.
    pub id: String,
    pub service: String,
    /// Concrete. `latest` never reaches this file — the registry resolves it at
    /// install time and the answer is written here, which is what stops a
    /// re-pull from moving somebody's version underneath them (ADR 0014).
    pub version: String,
    pub package: PackageRef,
    /// Whether it should be running. Distinct from whether it *is* — that is
    /// the engine's answer and is never cached here.
    pub enabled: bool,
    /// Holds the pre-package name as a network alias. At most one per service.
    pub primary: bool,
    /// Port handle from the manifest → the host port allocated to it.
    #[serde(default)]
    pub ports: BTreeMap<String, u16>,
    /// Volume handle → the Docker volume name. Stored rather than derived,
    /// because migration deliberately leaves an adopted instance pointing at
    /// the volume it already has: `stackvo-mysql-data` keeps its data, and only
    /// instances created afterwards get the derived name.
    #[serde(default)]
    pub volumes: BTreeMap<String, String>,
    /// Non-secret settings. Secrets are references, below.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// Setting key → keystore entry. The value is never in this file (ADR 0010).
    #[serde(default)]
    pub secret_refs: BTreeMap<String, String>,
}

impl Instance {
    /// `stackvo-mysql-8-0`.
    pub fn container(&self) -> String {
        format!("stackvo-{}", self.id)
    }

    /// `stackvo-mysql-8-0-data` — the name a volume gets when it is created
    /// fresh. An adopted one keeps whatever [`Self::volumes`] already says.
    pub fn volume(&self, handle: &str) -> String {
        self.volumes
            .get(handle)
            .cloned()
            .unwrap_or_else(|| format!("stackvo-{}-{handle}", self.id))
    }

    /// The names this instance answers to on the Docker network.
    ///
    /// Its own always; the service's bare name only when it is primary. The
    /// order is the one a compose file should carry — own name first, so a
    /// reader of the generated file sees which instance it is looking at before
    /// the compatibility name.
    pub fn aliases(&self) -> Vec<String> {
        let mut out = vec![self.container()];
        if self.primary {
            out.push(format!("stackvo-{}", self.service));
        }
        out
    }

    /// `<root>/logs/services/mysql-8-0`.
    pub fn logs(&self, root: &Path) -> PathBuf {
        root.join("logs").join("services").join(&self.id)
    }

    /// The name this instance is reached by in a browser.
    ///
    /// The primary keeps the bare one — `phpmyadmin.stackvo.loc` is in
    /// somebody's bookmarks and their password manager — and every other
    /// instance gets its version appended: `phpmyadmin-5-2.stackvo.loc`.
    ///
    /// This is what stops twelve of the twenty-five packages from being
    /// single-instance. They were not single-instance because anything about
    /// them is: they were single-instance because two of them would have asked
    /// Traefik for the same `Host()` rule, and Traefik does not report that as a
    /// conflict — it picks one, and the other silently never answers.
    ///
    /// The router *name* was already per instance (`{{ instance.slug }}` in the
    /// fragments), which is why only this needed deriving. One derivation, and
    /// three places read it: the router rule, the certificate SAN, and the hosts
    /// file — the same three that E-2's aliases go through.
    pub fn domain(&self, subdomain: &str, tld: &str) -> String {
        if self.primary {
            return format!("{subdomain}.{tld}");
        }
        // The version part of the slug, not the version itself: the slug is
        // already a DNS label and `8.0` is not.
        let suffix = self
            .id
            .strip_prefix(&format!("{}-", self.service))
            .unwrap_or(&self.id);
        format!("{subdomain}-{suffix}.{tld}")
    }
}

/// `("mysql", "8.0")` → `mysql-8-0`.
///
/// A DNS label, because it becomes one: a container name and a network alias.
/// Dots and plus signs become dashes, everything is lowercased, and anything
/// left that is not `[a-z0-9-]` is a refusal rather than a silent strip —
/// stripping is how two different versions arrive at one slug.
pub fn slug(service: &str, version: &str) -> Result<String> {
    let mapped: String = version
        .chars()
        .map(|c| match c {
            '.' | '+' | '_' => '-',
            c => c.to_ascii_lowercase(),
        })
        .collect();

    if mapped.is_empty()
        || !mapped
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(Error::new(
            Code::InvalidInput,
            format!("version {version:?} cannot be part of a container name"),
        ));
    }

    Ok(format!("{}-{}", service.to_ascii_lowercase(), mapped))
}

/// The whole file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub instances: Vec<Instance>,
}

impl Table {
    /// Read it, or an empty table when there is none.
    ///
    /// Absent is not an error: a workspace that has never installed anything
    /// has no instances, and that is the state a fresh install is in.
    pub fn load(root: &Path) -> Result<Self> {
        let file = path(root);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    schema_version: SCHEMA_VERSION,
                    instances: Vec::new(),
                })
            }
            Err(e) => {
                return Err(Error::new(
                    Code::IoError,
                    format!("reading {}: {e}", file.display()),
                ))
            }
        };

        let table: Self = serde_json::from_str(&text).map_err(|e| {
            Error::new(
                Code::InvalidManifest,
                format!("{} is not readable: {e}", file.display()),
            )
        })?;

        if table.schema_version > SCHEMA_VERSION {
            return Err(Error::new(
                Code::Unsupported,
                format!(
                    "{} is version {} and this app understands {SCHEMA_VERSION} — \
                     it decides which containers exist and which volumes may be deleted, \
                     so a newer file is refused rather than half-read",
                    file.display(),
                    table.schema_version
                ),
            ));
        }

        table.check()?;
        Ok(table)
    }

    /// Write it, atomically, after checking it.
    ///
    /// The check is on the way out and not only on the way in. A caller that
    /// has just built an invalid table is the caller that can still be told;
    /// once it is on disk the next reader inherits the problem, and the reader
    /// is a generate that is about to hand a compose file to Docker.
    pub fn save(&self, root: &Path) -> Result<()> {
        self.check()?;
        let file = path(root);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::new(Code::IoError, format!("creating {}: {e}", parent.display()))
            })?;
        }
        let mut text = serde_json::to_string_pretty(&Self {
            schema_version: SCHEMA_VERSION,
            instances: self.instances.clone(),
        })
        .map_err(|e| Error::new(Code::IoError, format!("serialising instances: {e}")))?;
        text.push('\n');
        crate::atomic::write(&file, &text)
    }

    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    /// Every instance of one service, in table order.
    pub fn of_service<'a>(&'a self, service: &'a str) -> impl Iterator<Item = &'a Instance> {
        self.instances.iter().filter(move |i| i.service == service)
    }

    pub fn primary_of(&self, service: &str) -> Option<&Instance> {
        self.instances
            .iter()
            .find(|i| i.service == service && i.primary)
    }

    /// Every host port this table has spoken for, enabled or not.
    ///
    /// Disabled instances count. Handing a disabled instance's port to a new
    /// one turns "switch this back on" into a bind failure at the least
    /// convenient moment, and the user has no way to see why.
    pub fn reserved_ports(&self) -> BTreeSet<u16> {
        self.instances
            .iter()
            .flat_map(|i| i.ports.values().copied())
            .collect()
    }

    /// Add an instance, refusing a duplicate rather than replacing one.
    pub fn insert(&mut self, instance: Instance) -> Result<()> {
        if self.get(&instance.id).is_some() {
            return Err(Error::new(
                Code::AlreadyExists,
                format!("instance {} is already installed", instance.id),
            ));
        }
        self.instances.push(instance);
        self.check()
    }

    /// Move the legacy alias to `id`, taking it from whoever holds it.
    ///
    /// One call rather than "clear that one, set this one", because the state
    /// between those two writes is a table with no primary — and if the process
    /// dies there, every project pointing at `stackvo-mysql` resolves nothing.
    pub fn promote(&mut self, id: &str) -> Result<()> {
        let Some(service) = self.get(id).map(|i| i.service.clone()) else {
            return Err(Error::not_found(format!("instance {id}")));
        };
        for instance in &mut self.instances {
            if instance.service == service {
                instance.primary = instance.id == id;
            }
        }
        self.check()
    }

    pub fn remove(&mut self, id: &str) -> Result<Instance> {
        let Some(at) = self.instances.iter().position(|i| i.id == id) else {
            return Err(Error::not_found(format!("instance {id}")));
        };
        Ok(self.instances.remove(at))
    }

    /// Everything that must be true before this table can produce a compose
    /// file.
    ///
    /// Each of these is a failure Docker does not report. A duplicate id is two
    /// compose keys where the second silently merges over the first; a second
    /// primary is two containers answering one DNS name, at random, per query;
    /// a shared port is a bind error naming a number and not a service; a shared
    /// volume is the data loss this whole module exists to prevent.
    pub fn check(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut primaries: BTreeMap<&str, &str> = BTreeMap::new();
        let mut ports: BTreeMap<u16, &str> = BTreeMap::new();
        let mut volumes: BTreeMap<String, &str> = BTreeMap::new();
        let mut aliases: BTreeMap<String, &str> = BTreeMap::new();

        for instance in &self.instances {
            let expected = slug(&instance.service, &instance.version)?;
            if instance.id != expected {
                return Err(Error::new(
                    Code::InvalidManifest,
                    format!(
                        "instance {} names {}@{} but that pair is {expected} — \
                         an id that is not derived from its own pair is a name \
                         nothing else in the app will agree with",
                        instance.id, instance.service, instance.version
                    ),
                ));
            }
            if !ids.insert(instance.id.as_str()) {
                return Err(Error::new(
                    Code::Conflict,
                    format!("instance {} appears twice", instance.id),
                ));
            }
            if instance.primary {
                if let Some(other) = primaries.insert(&instance.service, &instance.id) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!(
                            "{} and {} are both primary for {} — two containers on one \
                             network alias resolve at random, which reads as \"it connects \
                             to the wrong database sometimes\"",
                            other, instance.id, instance.service
                        ),
                    ));
                }
            }
            for port in instance.ports.values() {
                if let Some(other) = ports.insert(*port, &instance.id) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!("{other} and {} both publish host port {port}", instance.id),
                    ));
                }
            }
            for handle in instance.volumes.keys() {
                let name = instance.volume(handle);
                if let Some(other) = volumes.insert(name.clone(), &instance.id) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!(
                            "{other} and {} would share the volume {name} — the newer \
                             engine opens the older one's data directory and upgrades it, \
                             and Docker reports nothing",
                            instance.id
                        ),
                    ));
                }
            }
            for alias in instance.aliases() {
                if let Some(other) = aliases.insert(alias.clone(), &instance.id) {
                    return Err(Error::new(
                        Code::Conflict,
                        format!("{other} and {} both answer to {alias}", instance.id),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> PackageRef {
        PackageRef {
            source: "official".into(),
            sha256: "0".repeat(64),
            installed_at: "2026-08-11T09:00:00Z".into(),
        }
    }

    fn instance(service: &str, version: &str, primary: bool) -> Instance {
        Instance {
            id: slug(service, version).unwrap(),
            service: service.into(),
            version: version.into(),
            package: package(),
            enabled: true,
            primary,
            ports: BTreeMap::new(),
            volumes: BTreeMap::new(),
            settings: BTreeMap::new(),
            secret_refs: BTreeMap::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stackvo-instances-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating the scratch directory");
        dir
    }

    #[test]
    fn a_slug_is_a_dns_label() {
        assert_eq!(slug("mysql", "8.0").unwrap(), "mysql-8-0");
        assert_eq!(
            slug("mongo-express", "1.0.2").unwrap(),
            "mongo-express-1-0-2"
        );
        // MinIO's date tags, which is why this had to cope with more than dots.
        assert_eq!(
            slug("minio", "RELEASE.2025-09-07T16-13-09Z").unwrap(),
            "minio-release-2025-09-07t16-13-09z"
        );
    }

    /// A version this cannot spell is refused rather than stripped.
    ///
    /// Stripping is the tempting fix and it is how `1.0/beta` and `1.0-beta`
    /// arrive at one slug — two versions, one container name, and the second
    /// install silently adopts the first one's data.
    #[test]
    fn a_version_that_is_not_a_label_is_refused_rather_than_cleaned() {
        assert!(slug("mysql", "8.0/beta").is_err());
        assert!(slug("mysql", "").is_err());
        assert!(slug("mysql", "sürüm").is_err());
    }

    #[test]
    fn two_versions_of_one_service_get_separate_names() {
        let a = instance("mysql", "8.0", true);
        let b = instance("mysql", "9.4", false);

        assert_eq!(a.container(), "stackvo-mysql-8-0");
        assert_eq!(b.container(), "stackvo-mysql-9-4");
        assert_eq!(a.volume("data"), "stackvo-mysql-8-0-data");
        assert_eq!(b.volume("data"), "stackvo-mysql-9-4-data");
        assert_ne!(a.volume("data"), b.volume("data"));
    }

    /// The compatibility name rides on exactly one of them.
    #[test]
    fn only_the_primary_answers_to_the_old_name() {
        let a = instance("mysql", "8.0", true);
        let b = instance("mysql", "9.4", false);

        assert_eq!(a.aliases(), ["stackvo-mysql-8-0", "stackvo-mysql"]);
        assert_eq!(b.aliases(), ["stackvo-mysql-9-4"]);
    }

    /// An adopted volume keeps its old name, which is the whole of the
    /// migration's safety: the data is where it is.
    #[test]
    fn a_migrated_instance_keeps_the_volume_it_already_has() {
        let mut a = instance("mysql", "8.0", true);
        a.volumes.insert("data".into(), "stackvo-mysql-data".into());

        assert_eq!(a.volume("data"), "stackvo-mysql-data");
        // And a second version created afterwards does not collide with it.
        assert_eq!(
            instance("mysql", "9.4", false).volume("data"),
            "stackvo-mysql-9-4-data"
        );
    }

    #[test]
    fn two_primaries_for_one_service_are_refused() {
        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![
                instance("mysql", "8.0", true),
                instance("mysql", "9.4", true),
            ],
        };
        let message = table.check().unwrap_err().message;
        assert!(message.contains("both primary"), "{message}");
    }

    /// Two primaries for *different* services is the ordinary arrangement.
    #[test]
    fn one_primary_per_service_is_not_one_primary_overall() {
        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![
                instance("mysql", "8.0", true),
                instance("postgres", "16", true),
            ],
        };
        assert!(table.check().is_ok());
    }

    #[test]
    fn a_shared_host_port_is_refused() {
        let mut a = instance("mysql", "8.0", true);
        let mut b = instance("mysql", "9.4", false);
        a.ports.insert("main".into(), 3306);
        b.ports.insert("main".into(), 3306);

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a, b],
        };
        assert!(table.check().unwrap_err().message.contains("3306"));
    }

    /// The one Docker would not have complained about.
    #[test]
    fn a_shared_volume_is_refused() {
        let mut a = instance("mysql", "8.0", true);
        let mut b = instance("mysql", "9.4", false);
        a.volumes.insert("data".into(), "stackvo-mysql-data".into());
        b.volumes.insert("data".into(), "stackvo-mysql-data".into());

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a, b],
        };
        let message = table.check().unwrap_err().message;
        assert!(message.contains("stackvo-mysql-data"), "{message}");
    }

    #[test]
    fn an_id_that_does_not_match_its_pair_is_refused() {
        let mut a = instance("mysql", "8.0", true);
        a.id = "mysql-latest".into();

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a],
        };
        assert!(table.check().is_err());
    }

    /// Promotion is one write, because the state between two writes has no
    /// primary at all.
    #[test]
    fn promotion_takes_the_alias_rather_than_adding_one() {
        let mut table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![
                instance("mysql", "8.0", true),
                instance("mysql", "9.4", false),
            ],
        };
        table.promote("mysql-9-4").unwrap();

        assert!(!table.get("mysql-8-0").unwrap().primary);
        assert!(table.get("mysql-9-4").unwrap().primary);
        assert!(table.check().is_ok());
    }

    #[test]
    fn a_disabled_instance_still_holds_its_port() {
        let mut a = instance("mysql", "8.0", true);
        a.enabled = false;
        a.ports.insert("main".into(), 3306);

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a],
        };
        assert!(table.reserved_ports().contains(&3306));
    }

    #[test]
    fn a_missing_file_is_an_empty_table_and_not_an_error() {
        let root = scratch("missing");
        let table =
            Table::load(&root).expect("an absent file is a workspace with nothing installed");
        assert!(table.instances.is_empty());
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let root = scratch("roundtrip");
        let mut a = instance("mysql", "8.0", true);
        a.ports.insert("main".into(), 3306);
        a.volumes.insert("data".into(), "stackvo-mysql-data".into());
        a.settings.insert("DATABASE".into(), "stackvo".into());
        a.secret_refs.insert(
            "ROOT_PASSWORD".into(),
            "keychain:stackvo/mysql-8-0/ROOT_PASSWORD".into(),
        );

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a.clone(), instance("mysql", "9.4", false)],
        };
        table.save(&root).unwrap();

        let back = Table::load(&root).unwrap();
        assert_eq!(back.schema_version, SCHEMA_VERSION);
        assert_eq!(back.instances.len(), 2);
        assert_eq!(back.get("mysql-8-0"), Some(&a));
    }

    /// No password reaches the file, only the reference (ADR 0010).
    #[test]
    fn the_written_file_carries_references_and_not_secrets() {
        let root = scratch("secrets");
        let mut a = instance("mysql", "8.0", true);
        a.secret_refs.insert(
            "ROOT_PASSWORD".into(),
            "keychain:stackvo/mysql-8-0/ROOT_PASSWORD".into(),
        );
        Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a],
        }
        .save(&root)
        .unwrap();

        let text = std::fs::read_to_string(path(&root)).unwrap();
        assert!(text.contains("keychain:stackvo/mysql-8-0/ROOT_PASSWORD"));
        assert!(!text.contains("\"password\""));
    }

    /// A file from a newer app is refused, not half-read.
    #[test]
    fn a_newer_schema_is_refused() {
        let root = scratch("newer");
        std::fs::create_dir_all(root.join("services")).unwrap();
        std::fs::write(
            path(&root),
            format!(
                "{{\"schemaVersion\": {}, \"instances\": []}}",
                SCHEMA_VERSION + 1
            ),
        )
        .unwrap();

        let err = Table::load(&root).unwrap_err();
        assert_eq!(err.code, Code::Unsupported);
    }

    /// An invalid table cannot be written, so the next reader cannot inherit it.
    #[test]
    fn saving_checks_before_it_writes() {
        let root = scratch("refuse");
        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![
                instance("mysql", "8.0", true),
                instance("mysql", "8.0", false),
            ],
        };
        assert!(table.save(&root).is_err());
        assert!(!path(&root).exists());
    }

    /// S-18. The primary keeps the bare name — it is in a bookmark and a
    /// password manager — and every other instance carries its version.
    #[test]
    fn only_the_primary_answers_on_the_bare_subdomain() {
        let primary = instance("phpmyadmin", "5.2", true);
        let second = instance("phpmyadmin", "5.1", false);

        assert_eq!(
            primary.domain("phpmyadmin", "stackvo.loc"),
            "phpmyadmin.stackvo.loc"
        );
        assert_eq!(
            second.domain("phpmyadmin", "stackvo.loc"),
            "phpmyadmin-5-1.stackvo.loc"
        );
    }

    /// Two instances, two names. Traefik does not report two routers claiming
    /// one `Host()` as a conflict — it picks one and the other never answers —
    /// so this being different is the whole of what makes the packages
    /// multi-instance.
    #[test]
    fn two_instances_of_one_service_do_not_ask_for_the_same_name() {
        let a = instance("pgadmin", "9.17", true);
        let b = instance("pgadmin", "9.16", false);
        let c = instance("pgadmin", "8.14", false);

        let names = [
            a.domain("pgadmin", "stackvo.loc"),
            b.domain("pgadmin", "stackvo.loc"),
            c.domain("pgadmin", "stackvo.loc"),
        ];
        let unique: std::collections::BTreeSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "{names:?}");
    }

    /// The derived name is a hostname, so it has to be a run of DNS labels —
    /// the version part comes from the slug rather than from the version,
    /// because `8.0` is not one.
    #[test]
    fn a_derived_domain_is_made_of_dns_labels() {
        let second = instance("mongo-express", "1.0.2", false);
        let domain = second.domain("mongo-express", "stackvo.loc");
        assert_eq!(domain, "mongo-express-1-0-2.stackvo.loc");
        for label in domain.split('.') {
            assert!(
                !label.is_empty()
                    && label
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{label:?} in {domain:?}"
            );
        }
    }
}
