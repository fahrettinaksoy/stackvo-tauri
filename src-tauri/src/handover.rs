//! Handing the service state in `.env` over to the instance table.
//!
//! Faz 2 of `docs/servis-market-mimarisi.md`, and the one piece of it that
//! touches a workspace somebody is already using. Everything else in this
//! sprint is new code with no users; this reads a file that has seven services
//! switched on in it and decides what happens to their data.
//!
//! So it is a **plan first and a write second**. [`plan`] is pure: it reads,
//! decides, and returns what it would do together with every note and every
//! reason it cannot. Nothing on disk changes until a caller looks at the plan
//! and applies it. That split is the difference between "the migration failed
//! halfway" and "the migration did not start".
//!
//! ## The three rules that keep data where it is
//!
//! **The volume is adopted, never renamed.** A fresh `mysql@8.0` would get
//! `stackvo-mysql-8-0-data`; a migrated one keeps `stackvo-mysql-data`, because
//! that is where the rows are. Renaming a Docker volume is a copy, and a copy
//! of a database somebody is using is the slowest possible way to lose it.
//!
//! **The port is kept unless the machine has taken it.** It is the number in
//! somebody's TablePlus, their `.env`, their notes. It is also almost always
//! free — it is the port this very service was using — so the honest failure
//! case is narrow and gets a note rather than a stop.
//!
//! **The old name is kept as an alias.** The migrated instance is `primary`, so
//! `stackvo-mysql` still resolves and every project's `DB_HOST` keeps working.
//! One per service, which is all a single-version workspace has.
//!
//! ## What stops it
//!
//! A version that is enabled in `.env` and absent from the catalogue is a
//! **blocker**, not a nudge to the nearest neighbour. Silently migrating
//! `mysql@5.7` to 8.0 because 5.7 was withdrawn is an upgrade nobody asked for,
//! performed on a datadir, without a backup. ADR 0014's rule that a published
//! version is never removed exists so this stays a case that only a mistake can
//! produce — and when it is produced, this says so.

use crate::config::Env;
use crate::instances::{Instance, PackageRef, Table};
use crate::pkg::{self, Catalogue};
use crate::ports::{self, Claims};
use std::collections::{BTreeMap, BTreeSet};

/// Something the plan did that the user should be told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// `SERVICE_<ID>_VERSION=latest` was resolved to a concrete version.
    ///
    /// Eleven of the twenty-five shipped defaults were `latest`, so this is the
    /// common case rather than the odd one — and it is the note that matters
    /// most, because it is where a user's version stops being able to move
    /// under them.
    ResolvedMovingTag {
        service: String,
        from: String,
        to: String,
    },
    /// The port in `.env` was taken on this machine, so the instance moved.
    PortMoved {
        instance: String,
        port: String,
        from: u16,
        to: u16,
    },
    /// The instance kept a volume created before packages existed.
    AdoptedVolume { instance: String, volume: String },
    /// A `SERVICE_<ID>_*` key had no home in the manifest.
    ///
    /// Reported rather than dropped in silence. Some are genuinely dead keys
    /// (`contracts/CONFLICTS.md` C-11 counts nineteen); others mean the package
    /// is missing a setting the template used to read, which is a packaging bug
    /// and is invisible unless somebody says so.
    SettingHasNoHome { service: String, key: String },
}

/// Why the plan cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// `.env` enables a service the catalogue has never heard of.
    UnknownService { service: String },
    /// The exact version in `.env` has no package on this machine.
    ///
    /// **Not "unpublished".** The catalogue this reads is the local package
    /// tree, so the honest sentence is "not installed here" — and the two ask
    /// for different things: one is a click in the Market, the other is a
    /// version that was never in the index. Saying the second when the first is
    /// true sends somebody to look for a withdrawal that never happened, and
    /// ADR 0014 makes withdrawals impossible anyway.
    ///
    /// `available` is what *is* installed, which is the list somebody needs to
    /// see next to the one they asked for.
    VersionNotInstalled {
        service: String,
        version: String,
        available: Vec<String>,
    },
    /// A service is enabled and the catalogue has no concrete version at all.
    NothingToInstall { service: String },
    /// No port could be found for one of the instance's ports.
    NoFreePort { instance: String, port: String },
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub instances: Vec<Instance>,
    pub notes: Vec<Note>,
    pub blockers: Vec<Blocker>,
}

impl Plan {
    pub fn is_applicable(&self) -> bool {
        self.blockers.is_empty() && !self.instances.is_empty()
    }
}

/// Is there anything to hand over?
///
/// True when `.env` has service state and the table does not exist yet. Both
/// halves matter: a workspace that has already migrated must not be migrated
/// again — the second run would adopt the same volumes into a table that
/// already claims them — and a workspace that never enabled anything has
/// nothing to carry across.
pub fn is_pending(root: &std::path::Path, env: &Env, catalogue: &dyn Catalogue) -> bool {
    if crate::instances::path(root).exists() {
        return false;
    }
    catalogue
        .services()
        .iter()
        .any(|service| env.service_enabled(service))
}

/// Decide what the handover would do. Reads nothing but `.env` and the
/// catalogue; writes nothing at all.
///
/// `probe` answers "is this host port free" — [`ports::is_free`] in production.
/// `now` is the timestamp to stamp installs with, passed in rather than read so
/// the result is a function of its inputs and a test can assert on all of it.
pub fn plan(
    root: &std::path::Path,
    env: &Env,
    catalogue: &dyn Catalogue,
    probe: &dyn Fn(u16) -> bool,
    now: &str,
) -> Plan {
    let mut plan = Plan::default();
    let mut claims = Claims::default();
    // Nothing is installed yet, so the only reservations are the ones this plan
    // makes as it goes.
    let reserved: BTreeSet<u16> = BTreeSet::new();

    let mut enabled: Vec<String> = catalogue
        .services()
        .into_iter()
        .filter(|s| env.service_enabled(s))
        .collect();
    enabled.sort();

    for service in enabled {
        let prefix = Env::service_prefix(&service);

        // ---- which version -------------------------------------------------
        let declared = env
            .service_version(&service)
            .unwrap_or_default()
            .to_string();
        let available = catalogue.versions(&service);
        if available.is_empty() {
            plan.blockers.push(Blocker::NothingToInstall { service });
            continue;
        }

        let version = if declared.is_empty() || pkg::is_moving_tag(&declared) {
            let Some(resolved) = catalogue.recommended(&service) else {
                plan.blockers.push(Blocker::NothingToInstall { service });
                continue;
            };
            if !declared.is_empty() {
                plan.notes.push(Note::ResolvedMovingTag {
                    service: service.clone(),
                    from: declared.clone(),
                    to: resolved.clone(),
                });
            }
            resolved
        } else {
            declared
        };

        if !available.contains(&version) {
            plan.blockers.push(Blocker::VersionNotInstalled {
                service,
                version,
                available,
            });
            continue;
        }

        let Some(manifest) = catalogue.manifest(&service, &version) else {
            plan.blockers.push(Blocker::UnknownService { service });
            continue;
        };

        let Ok(id) = crate::instances::slug(&service, &version) else {
            plan.blockers.push(Blocker::VersionNotInstalled {
                service,
                version,
                available,
            });
            continue;
        };

        // ---- ports ----------------------------------------------------------
        let mut ports = BTreeMap::new();
        let mut failed = false;
        for port in &manifest.ports {
            // The number the workspace is publishing today, if `.env` says. The
            // manifest carries the key because the two families cannot be told
            // apart from the handle alone.
            let current = port
                .legacy_key
                .as_deref()
                .and_then(|key| env.get(key))
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(0);

            match ports::keep_or_move(current, port.preferred, &reserved, &mut claims, probe) {
                Ok(chosen) => {
                    if current != 0 && chosen != current {
                        plan.notes.push(Note::PortMoved {
                            instance: id.clone(),
                            port: port.name.clone(),
                            from: current,
                            to: chosen,
                        });
                    }
                    ports.insert(port.name.clone(), chosen);
                }
                Err(_) => {
                    plan.blockers.push(Blocker::NoFreePort {
                        instance: id.clone(),
                        port: port.name.clone(),
                    });
                    failed = true;
                }
            }
        }
        if failed {
            continue;
        }

        // ---- volumes: adopted, never renamed --------------------------------
        let mut volumes = BTreeMap::new();
        for volume in &manifest.volumes {
            let legacy = format!("stackvo-{service}-{}", volume.name);
            volumes.insert(volume.name.clone(), legacy.clone());
            plan.notes.push(Note::AdoptedVolume {
                instance: id.clone(),
                volume: legacy,
            });
        }

        // ---- settings and secrets ------------------------------------------
        let mut settings = BTreeMap::new();
        let mut secret_refs = BTreeMap::new();
        for setting in &manifest.settings {
            let key = format!("{prefix}{}", setting.key);
            let value = env.get(&key).map(str::to_string);
            if setting.is_secret() {
                // The reference, never the value (ADR 0010). A secret that has
                // never been moved is still in `.env`; naming the entry here is
                // what lets `secrets` move it on the first read.
                //
                // Built by `secrets::reference_for` rather than formatted here,
                // and the difference is not cosmetic: that function appends a
                // digest of the workspace path, so two workspaces on one machine
                // do not share a keychain entry. The first version of this line
                // wrote the name itself and dropped the digest — two checkouts
                // would have quietly shared one password.
                secret_refs.insert(
                    setting.key.clone(),
                    crate::secrets::reference_for(&format!("{id}/{}", setting.key), root),
                );
                continue;
            }
            if let Some(value) = value.or_else(|| setting.default_text()) {
                settings.insert(setting.key.clone(), value);
            }
        }

        // Anything `.env` carried for this service that the package has no slot
        // for. Named, because silence here hides a packaging gap.
        let owned: Vec<&String> = env
            .raw()
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .collect();
        for key in owned {
            let bare = key.trim_start_matches(&prefix);
            if matches!(bare, "ENABLE" | "VERSION" | "VERSIONS" | "URL") {
                continue;
            }
            if manifest.setting(bare).is_some() {
                continue;
            }
            if manifest
                .ports
                .iter()
                .any(|p| p.legacy_key.as_deref() == Some(key.as_str()))
            {
                continue;
            }
            plan.notes.push(Note::SettingHasNoHome {
                service: service.clone(),
                key: key.clone(),
            });
        }

        plan.instances.push(Instance {
            id,
            service,
            version,
            package: PackageRef {
                source: "official".into(),
                sha256: pkg::sha256_hex(
                    serde_json::to_string(&manifest)
                        .unwrap_or_default()
                        .as_bytes(),
                ),
                installed_at: now.to_string(),
            },
            enabled: true,
            // A single-version workspace has exactly one instance per service,
            // and it must be the one that answers to the old name.
            primary: true,
            ports,
            volumes,
            settings,
            secret_refs,
        });
    }

    plan
}

/// Where `.env` is copied before the handover writes anything.
pub fn backup_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".env.pre-market.bak")
}

/// The line written above the migrated block in `.env`.
///
/// A whole-line comment, and that is the only form it can take. `Env::parse`
/// takes everything after the first `=` as the value with no quoting and no
/// comment stripping (`contracts/env.schema.json` → `parsing`, deliberately
/// naive because a Bash loader and a Node parser read the same file). An
/// inline `SERVICE_MYSQL_ENABLE=true  # migrated` would therefore be the value
/// `true  # migrated`, and the revert this note describes would no longer work.
const MIGRATED_MARK: &str =
    "# Migrated to services/instances.json — these keys are read for migration only and are \
     no longer written. To go back, delete services/instances.json.";

/// Write the plan out, refusing an inapplicable one.
///
/// The table's own `check` runs inside `save`, so a plan that would produce two
/// primaries or a shared volume never reaches disk even if this module built it
/// by mistake. Two gates on the same property, deliberately: this one knows why
/// and that one knows what.
///
/// ## The order is the safety
///
/// The backup is written **first**, before the table and before `.env` is
/// touched. `docs/servis-market-mimarisi.md` §7 asks for a revert path, and a
/// revert whose only artefact is written after the risky step is a revert that
/// exists in the cases where nothing went wrong.
///
/// An existing backup is not overwritten. A second run means the first one
/// already happened; replacing `.env.pre-market.bak` with a post-migration
/// `.env` would turn the one file that remembers the previous state into a
/// second copy of the current one.
pub fn apply(root: &std::path::Path, plan: &Plan) -> crate::error::Result<Table> {
    use crate::error::{Code, Error};

    if !plan.blockers.is_empty() {
        return Err(Error::new(
            Code::Conflict,
            format!(
                "{} thing(s) stop this handover, and it is all-or-nothing on purpose",
                plan.blockers.len()
            ),
        ));
    }

    let env_path = root.join(".env");
    let before = std::fs::read_to_string(&env_path).ok();

    if let Some(text) = &before {
        let backup = backup_path(root);
        if !backup.exists() {
            crate::atomic::write(&backup, text)?;
        }
    }

    let table = Table {
        schema_version: crate::instances::SCHEMA_VERSION,
        instances: plan.instances.clone(),
    };
    table.save(root)?;

    // `.env`'s service lines are marked, not removed (§7, step 3). Removing
    // them would make the revert — delete the table, get the old workspace
    // back — a restore from backup instead of a deletion, and the two differ
    // by everything the user changed in between.
    //
    // After the table exists, so a failure to annotate leaves a migrated
    // workspace with an unannotated `.env` rather than the reverse. One of
    // those is cosmetic.
    if let Some(text) = before {
        if let Some(marked) = mark_migrated(&text) {
            crate::atomic::write(&env_path, &marked)?;
        }
    }

    Ok(table)
}

/// Put [`MIGRATED_MARK`] above the first legacy service key, or `None` when
/// there is nothing to mark or it is already there.
fn mark_migrated(text: &str) -> Option<String> {
    if text.contains(MIGRATED_MARK) {
        return None;
    }
    let is_legacy = |line: &str| {
        let key = line.trim().split('=').next().unwrap_or("").trim();
        (key.starts_with("SERVICE_") || key.starts_with("HOST_PORT_")) && line.contains('=')
    };
    let at = text.lines().position(is_legacy)?;

    let mut out: Vec<&str> = text.lines().collect();
    out.insert(at, MIGRATED_MARK);
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the fixture needs the type; the module itself passes manifests through.
    use crate::pkg::Manifest;

    /// A catalogue built from literals, so a test says what it means without a
    /// package tree on disk.
    struct Fixture {
        entries: Vec<(String, Vec<String>, String)>,
    }

    impl Fixture {
        fn new(entries: &[(&str, &[&str], &str)]) -> Self {
            Self {
                entries: entries
                    .iter()
                    .map(|(s, vs, r)| {
                        (
                            s.to_string(),
                            vs.iter().map(|v| v.to_string()).collect(),
                            r.to_string(),
                        )
                    })
                    .collect(),
            }
        }
    }

    fn manifest_json(service: &str, version: &str) -> String {
        let legacy = match service {
            // The two families, one of each, which is the point of the field.
            "mysql" => "\"legacyKey\": \"HOST_PORT_MYSQL\", ",
            "postgres" => "\"legacyKey\": \"SERVICE_POSTGRES_HOST_PORT\", ",
            _ => "",
        };
        let container = if service == "postgres" { 5432 } else { 3306 };
        format!(
            r#"{{
              "apiVersion": "stackvo.dev/package/v1",
              "service": "{service}",
              "version": "{version}",
              "image": {{"repository": "{service}", "tag": "{version}"}},
              "instancing": {{"multiple": true}},
              "ports": [{{"name": "main", "container": {container}, "preferred": {container}, {legacy}"primary": true}}],
              "volumes": [{{"name": "data", "container": "/var/lib/x"}}],
              "settings": [
                {{"key": "ROOT_PASSWORD", "type": "secret", "default": "root"}},
                {{"key": "DATABASE", "type": "string", "default": "stackvo"}}
              ],
              "compose": {{"file": "compose.yml.tpl", "sha256": "{}"}},
              "support": {{"status": "supported"}}
            }}"#,
            "a".repeat(64)
        )
    }

    impl Catalogue for Fixture {
        fn services(&self) -> Vec<String> {
            self.entries.iter().map(|(s, _, _)| s.clone()).collect()
        }
        fn versions(&self, service: &str) -> Vec<String> {
            self.entries
                .iter()
                .find(|(s, _, _)| s == service)
                .map(|(_, v, _)| v.clone())
                .unwrap_or_default()
        }
        fn recommended(&self, service: &str) -> Option<String> {
            self.entries
                .iter()
                .find(|(s, _, _)| s == service)
                .map(|(_, _, r)| r.clone())
        }
        fn manifest(&self, service: &str, version: &str) -> Option<Manifest> {
            if !self.versions(service).contains(&version.to_string()) {
                return None;
            }
            pkg::parse(&manifest_json(service, version)).ok()
        }

        /// A catalogue with no directory behind it, which is the case the trait
        /// exists to allow. Nothing in this module reads a package file — the
        /// handover decides what to install, not how to render it.
        fn file(&self, _service: &str, _version: &str, _relative: &str) -> Option<String> {
            None
        }
    }

    fn catalogue() -> Fixture {
        Fixture::new(&[
            ("mysql", &["9.4", "8.0", "5.7"], "8.0"),
            ("postgres", &["16", "14"], "16"),
            ("adminer", &["5.5.1", "4.8.1"], "5.5.1"),
        ])
    }

    fn free(_: u16) -> bool {
        true
    }

    const NOW: &str = "2026-08-11T09:00:00Z";

    /// A fixed workspace path. `plan` only uses it to build a keystore
    /// reference, whose digest makes two checkouts on one machine not share an
    /// entry — so a constant here keeps that string stable to assert on.
    const ROOT: &str = "/workspace";
    fn at() -> &'static std::path::Path {
        std::path::Path::new(ROOT)
    }

    #[test]
    fn a_switched_on_service_becomes_a_primary_instance() {
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
        assert_eq!(plan.instances.len(), 1);
        let mysql = &plan.instances[0];
        assert_eq!(mysql.id, "mysql-8-0");
        assert!(mysql.primary);
        assert!(mysql.enabled);
        assert_eq!(mysql.aliases(), ["stackvo-mysql-8-0", "stackvo-mysql"]);
    }

    #[test]
    fn a_service_that_is_off_is_not_carried_across() {
        let env = Env::parse("SERVICE_MYSQL_ENABLE=false\nSERVICE_POSTGRES_ENABLE=true\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);
        assert_eq!(plan.instances.len(), 1);
        assert_eq!(plan.instances[0].service, "postgres");
    }

    /// The volume is where the rows are, so it is adopted rather than derived.
    #[test]
    fn the_existing_volume_is_kept_under_its_old_name() {
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert_eq!(plan.instances[0].volume("data"), "stackvo-mysql-data");
        assert!(plan.notes.contains(&Note::AdoptedVolume {
            instance: "mysql-8-0".into(),
            volume: "stackvo-mysql-data".into(),
        }));
        // And a second version installed afterwards does not land on it.
        assert_ne!(
            crate::instances::Instance {
                volumes: BTreeMap::new(),
                ..plan.instances[0].clone()
            }
            .volume("data"),
            "stackvo-mysql-data"
        );
    }

    /// Both key families are read, which is why the manifest carries the key.
    #[test]
    fn the_port_in_env_is_kept_whichever_family_it_used() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\nHOST_PORT_MYSQL=3399\n\
             SERVICE_POSTGRES_ENABLE=true\nSERVICE_POSTGRES_VERSION=14\n\
             SERVICE_POSTGRES_HOST_PORT=5499\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        let by_id = |id: &str| {
            plan.instances
                .iter()
                .find(|i| i.id == id)
                .unwrap()
                .ports
                .get("main")
                .copied()
                .unwrap()
        };
        assert_eq!(by_id("mysql-8-0"), 3399);
        assert_eq!(by_id("postgres-14"), 5499);
    }

    /// …and moves, with a note, when the machine has taken it since.
    #[test]
    fn a_port_the_machine_has_taken_moves_and_says_so() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\nHOST_PORT_MYSQL=3399\n",
        );
        let busy = |p: u16| p != 3399;
        let plan = plan(at(), &env, &catalogue(), &busy, NOW);

        assert_eq!(plan.instances[0].ports["main"], 3306);
        assert!(plan.notes.contains(&Note::PortMoved {
            instance: "mysql-8-0".into(),
            port: "main".into(),
            from: 3399,
            to: 3306,
        }));
    }

    /// The common case: eleven of the shipped defaults were `latest`.
    #[test]
    fn a_moving_tag_is_resolved_to_a_concrete_version_and_recorded() {
        let env = Env::parse("SERVICE_ADMINER_ENABLE=true\nSERVICE_ADMINER_VERSION=latest\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert_eq!(plan.instances[0].version, "5.5.1");
        assert_eq!(plan.instances[0].id, "adminer-5-5-1");
        assert!(plan.notes.contains(&Note::ResolvedMovingTag {
            service: "adminer".into(),
            from: "latest".into(),
            to: "5.5.1".into(),
        }));
    }

    /// No version in `.env` at all is the same resolution, without the note —
    /// nothing was overridden, so there is nothing to tell anyone.
    #[test]
    fn an_absent_version_takes_the_recommended_one_quietly() {
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert_eq!(plan.instances[0].version, "8.0");
        assert!(!plan
            .notes
            .iter()
            .any(|n| matches!(n, Note::ResolvedMovingTag { .. })));
    }

    /// The rule that keeps a migration from being an upgrade nobody asked for.
    #[test]
    fn a_version_that_is_not_published_stops_the_handover() {
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=5.5\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert!(plan.instances.is_empty());
        assert!(!plan.is_applicable());
        match &plan.blockers[0] {
            Blocker::VersionNotInstalled {
                service, version, ..
            } => {
                assert_eq!((service.as_str(), version.as_str()), ("mysql", "5.5"));
            }
            other => panic!("expected a version blocker, got {other:?}"),
        }
    }

    /// One service's blocker does not quietly drop another's instance.
    #[test]
    fn a_blocker_stops_the_whole_plan_rather_than_one_service() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=5.5\n\
             SERVICE_POSTGRES_ENABLE=true\nSERVICE_POSTGRES_VERSION=16\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert_eq!(plan.instances.len(), 1, "postgres is still planned");
        assert_eq!(plan.blockers.len(), 1);
        assert!(!plan.is_applicable(), "but nothing may be written");
    }

    #[test]
    fn a_secret_becomes_a_reference_and_never_a_value() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n\
             SERVICE_MYSQL_ROOT_PASSWORD=hunter2\nSERVICE_MYSQL_DATABASE=shop\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);
        let mysql = &plan.instances[0];

        assert_eq!(
            mysql.secret_refs.get("ROOT_PASSWORD").map(String::as_str),
            Some(crate::secrets::reference_for("mysql-8-0/ROOT_PASSWORD", at()).as_str()),
            "the reference must carry the workspace digest, or two checkouts share one entry"
        );
        assert!(!mysql.settings.contains_key("ROOT_PASSWORD"));
        assert!(!format!("{mysql:?}").contains("hunter2"));
        // The ordinary setting travels as itself.
        assert_eq!(
            mysql.settings.get("DATABASE").map(String::as_str),
            Some("shop")
        );
    }

    /// A `.env` key with nowhere to go is named rather than dropped in silence.
    #[test]
    fn a_setting_the_package_has_no_slot_for_is_reported() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n\
             SERVICE_MYSQL_CHARACTER_SET=utf8mb4\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert!(plan.notes.contains(&Note::SettingHasNoHome {
            service: "mysql".into(),
            key: "SERVICE_MYSQL_CHARACTER_SET".into(),
        }));
    }

    /// The keys that describe the handover itself are not "homeless".
    #[test]
    fn the_bookkeeping_keys_are_not_reported_as_orphans() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n\
             SERVICE_MYSQL_VERSIONS=9.4,8.0\nHOST_PORT_MYSQL=3399\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert!(
            !plan
                .notes
                .iter()
                .any(|n| matches!(n, Note::SettingHasNoHome { .. })),
            "{:?}",
            plan.notes
        );
    }

    // ---- pending and apply ------------------------------------------------

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stackvo-handover-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_workspace_with_services_on_and_no_table_is_pending() {
        let root = scratch("pending");
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\n");
        assert!(is_pending(&root, &env, &catalogue()));
    }

    #[test]
    fn a_workspace_with_nothing_on_is_not_pending() {
        let root = scratch("nothing");
        let env = Env::parse("SERVICE_MYSQL_ENABLE=false\n");
        assert!(!is_pending(&root, &env, &catalogue()));
    }

    /// Running twice would adopt the same volumes into a table that already
    /// claims them.
    #[test]
    fn a_workspace_that_has_already_migrated_is_not_pending_again() {
        let root = scratch("again");
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);
        apply(&root, &plan).unwrap();

        assert!(!is_pending(&root, &env, &catalogue()));
    }

    #[test]
    fn applying_writes_a_table_that_reads_back() {
        let root = scratch("apply");
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=8.0\n\
             SERVICE_POSTGRES_ENABLE=true\nSERVICE_POSTGRES_VERSION=16\n",
        );
        let plan = plan(at(), &env, &catalogue(), &free, NOW);
        apply(&root, &plan).unwrap();

        let table = Table::load(&root).unwrap();
        assert_eq!(table.instances.len(), 2);
        assert_eq!(table.primary_of("mysql").unwrap().id, "mysql-8-0");
        assert_eq!(table.primary_of("postgres").unwrap().id, "postgres-16");
    }

    #[test]
    fn a_blocked_plan_writes_nothing() {
        let root = scratch("blocked");
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=5.5\n");
        let plan = plan(at(), &env, &catalogue(), &free, NOW);

        assert!(apply(&root, &plan).is_err());
        assert!(!crate::instances::path(&root).exists());
    }

    // ---- the backup, and what `.env` looks like afterwards ----------------

    const REAL_ENV: &str = "\
# StackVo
DEFAULT_TLD_SUFFIX=stackvo.loc
SERVICE_MYSQL_ENABLE=true
SERVICE_MYSQL_VERSION=8.0
SERVICE_MYSQL_ROOT_PASSWORD=hunter2
";

    /// Written before the table and before `.env` is touched. A revert whose
    /// only artefact appears *after* the risky step exists in exactly the cases
    /// where nothing went wrong.
    #[test]
    fn the_env_is_backed_up_before_anything_is_written() {
        let root = scratch("backup");
        std::fs::write(root.join(".env"), REAL_ENV).unwrap();

        let env = Env::parse(REAL_ENV);
        let plan = plan(&root, &env, &catalogue(), &free, NOW);
        apply(&root, &plan).unwrap();

        assert_eq!(std::fs::read_to_string(backup_path(&root)).unwrap(), REAL_ENV);
    }

    /// A second run does not replace it. The first migration already happened,
    /// so overwriting would turn the one file that remembers the previous state
    /// into a second copy of the current one.
    #[test]
    fn an_existing_backup_is_not_overwritten() {
        let root = scratch("backup-twice");
        std::fs::write(root.join(".env"), REAL_ENV).unwrap();
        std::fs::write(backup_path(&root), "# from the first run\n").unwrap();

        let env = Env::parse(REAL_ENV);
        let plan = plan(&root, &env, &catalogue(), &free, NOW);
        apply(&root, &plan).unwrap();

        assert_eq!(
            std::fs::read_to_string(backup_path(&root)).unwrap(),
            "# from the first run\n"
        );
    }

    /// Marked, not removed — and marked on a line of its own.
    ///
    /// `Env::parse` takes everything after the first `=` as the value, with no
    /// quoting and no comment stripping (`contracts/env.schema.json` →
    /// `parsing`). An inline note would therefore make the value
    /// `true # migrated`, and the revert this very note describes would stop
    /// working. This test is that rule.
    #[test]
    fn the_service_keys_are_marked_and_still_parse_to_what_they_did() {
        let root = scratch("marked");
        std::fs::write(root.join(".env"), REAL_ENV).unwrap();

        let env = Env::parse(REAL_ENV);
        let plan = plan(&root, &env, &catalogue(), &free, NOW);
        apply(&root, &plan).unwrap();

        let after = std::fs::read_to_string(root.join(".env")).unwrap();
        assert!(after.contains("instances.json"), "{after}");
        assert!(after.contains("SERVICE_MYSQL_ENABLE=true"), "{after}");

        let reparsed = Env::parse(&after);
        assert_eq!(reparsed.get("SERVICE_MYSQL_VERSION"), Some("8.0"));
        assert!(reparsed.service_enabled("mysql"));
        assert_eq!(reparsed.get("DEFAULT_TLD_SUFFIX"), Some("stackvo.loc"));
    }

    /// And marking is not cumulative: applying twice would otherwise stack a
    /// comment per run at the top of somebody's file.
    #[test]
    fn the_mark_is_written_once() {
        let marked = mark_migrated(REAL_ENV).unwrap();
        assert!(mark_migrated(&marked).is_none());
    }

    /// A blocked plan writes no backup either. Nothing happened, so nothing
    /// should be left behind claiming something did.
    #[test]
    fn a_blocked_plan_leaves_no_backup() {
        let root = scratch("blocked-backup");
        std::fs::write(root.join(".env"), "SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=5.5\n")
            .unwrap();
        let env = Env::parse("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=5.5\n");
        let plan = plan(&root, &env, &catalogue(), &free, NOW);

        assert!(apply(&root, &plan).is_err());
        assert!(!backup_path(&root).exists());
    }
}
