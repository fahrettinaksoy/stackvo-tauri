//! Stack presets — the shareable half of a StackVo configuration.
//!
//! ## What is actually missing
//!
//! The roadmap item said "turn the commit-friendly `stackvo.json` into a flow".
//! Read against the code, that framing is wrong in a way worth recording:
//! `stackvo.json` needs no flow at all. It already sits in the project
//! directory, it is already schema-validated, and a teammate who clones the
//! repository already has it. Exporting it would be a button that copies a file
//! out of a repository into the same repository.
//!
//! What a teammate does **not** get is the *stack*: which of the twenty
//! services are enabled and at which versions. That lives in `<root>/.env`,
//! which is the one file nobody commits, because it is also where every
//! password is. So the clone succeeds, the manifest is perfect, and the project
//! still will not start until somebody says out loud "you need MySQL 8.0,
//! Redis and Elasticsearch turned on". That sentence is the preset.
//!
//! ## A preset can never carry a secret
//!
//! Enforced by construction, not by filtering.
//!
//! * [`ServicePreset`] holds `enabled` and `version` — two named fields, not a
//!   passthrough map. There is no code path by which
//!   `SERVICE_MYSQL_ROOT_PASSWORD` reaches a preset: it is not that a filter
//!   drops it, it is that there is nowhere to put it.
//! * [`SHAREABLE`] is an **allow-list** of global keys. A deny-list would be
//!   wrong here even though [`crate::config::Env::is_secret`] exists, because
//!   that matches on suffix (`PASSWORD`, `TOKEN`, `SECRET`…) — a key added
//!   upstream tomorrow called `SERVICE_FOO_APIKEY` would sail straight through
//!   it. An allow-list fails closed against a file this app does not own.
//!
//! Checked twice: `no_secret_can_reach_a_preset` here runs a fixture full of
//! passwords through an export, and `no_real_secret_survives_a_preset_export`
//! in `tests/real_checkout.rs` does the same against whatever is genuinely in
//! `.env` on this machine — 12 real secrets on the checkout this was written
//! against.
//!
//! That second test earned its place immediately by failing for the *wrong*
//! reason: `SERVICE_GRAFANA_ADMIN_PASSWORD=admin`, and `admin` is a substring
//! of the service ids `phpmyadmin`, `pgadmin` and `phpcacheadmin`, which a
//! preset legitimately contains. The repair was to compare exactly against the
//! strings actually in the document rather than to raise a length threshold —
//! the threshold would have quietened a coincidence by letting a real
//! five-character secret through.
//!
//! ## Import is reviewed, then applied
//!
//! The same shape as `hosts_plan`/`hosts_apply` and `cert_plan`/`cert_apply`:
//! [`plan`] says exactly what would change and [`apply`] does it, so importing
//! a colleague's file is never a blind write over your own stack. Writing goes
//! through [`crate::env_writer`], which already backs the file up, preserves
//! comments and ordering, and serialises against the other `.env` writers.
//!
//! Rejections are **named**, never silently dropped — an unknown service id or
//! a key outside the allow-list appears in the plan. A preset that quietly
//! ignores half of what it was given is how somebody concludes the feature
//! works and then spends an afternoon on the service it skipped.

use crate::config::Env;
use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Marks the file as ours, so a JSON that is merely valid is not treated as a
/// preset. Checked on load: pointing the importer at a `package.json` should
/// say "this is not a preset", not silently plan zero changes.
pub const KIND: &str = "stackvo.preset";

/// Bumped when the shape changes incompatibly. A future version is refused
/// rather than half-read.
pub const VERSION: u32 = 1;

/// Global `.env` keys a preset may carry.
///
/// An allow-list, deliberately — see the module note. These are the settings
/// that describe *what kind of stack this is* rather than who is running it:
/// the domain suffix, whether TLS is on, and the defaults new projects inherit.
/// Ports and paths are excluded on purpose; they are properties of one
/// developer's machine, and importing somebody else's is how two people end up
/// fighting over 3306.
pub const SHAREABLE: [&str; 5] = [
    "DEFAULT_TLD_SUFFIX",
    "SSL_ENABLE",
    "DEFAULT_PHP_VERSION",
    "DEFAULT_NODE_VERSION",
    "DEFAULT_WEBSERVER",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePreset {
    pub enabled: bool,
    /// Absent means "whatever this machine already has" — enabling a service
    /// without pinning its version is a legitimate thing to share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub kind: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Service id → what it should be. Ids come from the contract catalog.
    pub services: BTreeMap<String, ServicePreset>,
    /// Global settings, restricted to [`SHAREABLE`].
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// One line of the reviewed diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// The `.env` key that would be written.
    pub key: String,
    /// `service` or `setting`, so the UI can group without parsing the key.
    pub kind: String,
    /// The service id or the setting name, for display.
    pub subject: String,
    /// What the file says now. None when the key is absent entirely.
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub changes: Vec<Change>,
    /// Entries the preset asked for that this stack will not act on, each with
    /// its reason. Named rather than dropped.
    pub rejected: Vec<String>,
    /// How many entries already matched. Says "nothing to do" without making
    /// an empty change list ambiguous with a preset that was entirely rejected.
    pub unchanged: usize,
    /// True when anything a running stack would notice changed, so the UI can
    /// say what has to happen next instead of leaving the user to guess.
    pub needs_regenerate: bool,
}

// -------------------------------------------------------------- pure logic

/// `true`/`false` as the contract's parser reads them: lowercase, nothing else.
fn bool_str(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

/// What this stack looks like right now.
///
/// Every catalog service is included, disabled ones too. A preset that listed
/// only what happens to be on could not express "and turn Elasticsearch off",
/// which is half of what makes two machines match.
pub fn export(env: &Env, catalog: &[(String, String)], name: Option<String>) -> Preset {
    let mut services = BTreeMap::new();

    for (id, _category) in catalog {
        services.insert(
            id.clone(),
            ServicePreset {
                enabled: env.service_enabled(id),
                // Only the version this machine actually pins. An empty string
                // in `.env` means "unset", and copying it across as `Some("")`
                // would import a blank version key onto the other machine.
                version: env
                    .service_version(id)
                    .filter(|v| !v.trim().is_empty())
                    .map(str::to_string),
            },
        );
    }

    let settings = SHAREABLE
        .iter()
        .filter_map(|key| {
            env.get(key)
                .filter(|v| !v.trim().is_empty())
                .map(|v| (key.to_string(), v.to_string()))
        })
        .collect();

    Preset {
        kind: KIND.to_string(),
        version: VERSION,
        name,
        description: None,
        services,
        settings,
    }
}

/// Read a preset from JSON, refusing anything that is not one.
pub fn parse(text: &str) -> Result<Preset> {
    let preset: Preset = serde_json::from_str(text).map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("this is not a valid preset: {e}"),
        )
        .with_hint("A preset is the JSON that Settings → Presets exports.")
    })?;

    if preset.kind != KIND {
        return Err(Error::new(
            Code::InvalidInput,
            format!("expected a `{KIND}` file, found `{}`", preset.kind),
        )
        .with_hint("Pointing the importer at another JSON file is the usual cause."));
    }

    // Refused rather than half-read. A newer preset may mean something
    // different by a field this version already has, and applying the half we
    // recognise writes a stack that matches neither machine.
    if preset.version > VERSION {
        return Err(Error::new(
            Code::Unsupported,
            format!(
                "this preset is version {} and this app understands up to {VERSION}",
                preset.version
            ),
        )
        .with_hint("Update StackVo Desktop, or ask for a preset exported by an older version."));
    }

    Ok(preset)
}

/// What importing would change, without changing anything.
///
/// Takes the catalog so an unknown service id is rejected here rather than
/// becoming a `SERVICE_<JUNK>_ENABLE` key in somebody's `.env` — the failure
/// mode CONFLICTS.md C-09 records, where a key nobody reads brings up a compose
/// profile that matches nothing.
pub fn plan(env: &Env, catalog: &[(String, String)], preset: &Preset) -> Plan {
    let mut changes = Vec::new();
    let mut rejected = Vec::new();
    let mut unchanged = 0usize;

    for (id, wanted) in &preset.services {
        if !catalog.iter().any(|(known, _)| known == id) {
            rejected.push(format!(
                "{id}: not a service this version of StackVo knows about"
            ));
            continue;
        }

        let prefix = Env::service_prefix(id);

        let enable_key = format!("{prefix}ENABLE");
        let current = env.get(&enable_key);
        let target = bool_str(wanted.enabled);
        if current != Some(target) {
            changes.push(Change {
                key: enable_key,
                kind: "service".to_string(),
                subject: id.clone(),
                from: current.map(str::to_string),
                to: target.to_string(),
            });
        } else {
            unchanged += 1;
        }

        if let Some(version) = wanted
            .version
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let version_key = format!("{prefix}VERSION");
            let current = env.get(&version_key);
            if current != Some(version) {
                changes.push(Change {
                    key: version_key,
                    kind: "service".to_string(),
                    subject: id.clone(),
                    from: current.map(str::to_string),
                    to: version.to_string(),
                });
            } else {
                unchanged += 1;
            }
        }
    }

    for (key, value) in &preset.settings {
        if !SHAREABLE.contains(&key.as_str()) {
            // Rejected on import as well as omitted on export. Trusting the
            // export side alone would mean a hand-edited preset could write
            // any key it liked into somebody else's .env.
            rejected.push(format!("{key}: not a setting a preset may carry"));
            continue;
        }
        let current = env.get(key);
        if current != Some(value.as_str()) {
            changes.push(Change {
                key: key.clone(),
                kind: "setting".to_string(),
                subject: key.clone(),
                from: current.map(str::to_string),
                to: value.clone(),
            });
        } else {
            unchanged += 1;
        }
    }

    // Ordered so the reviewer reads services together and settings together,
    // rather than in the map's alphabetical interleaving of the two.
    changes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.key.cmp(&b.key))
    });

    Plan {
        name: preset.name.clone(),
        description: preset.description.clone(),
        needs_regenerate: !changes.is_empty(),
        changes,
        rejected,
        unchanged,
    }
}

/// The `.env` patch a plan describes.
pub fn patch(plan: &Plan) -> BTreeMap<String, String> {
    plan.changes
        .iter()
        .map(|c| (c.key.clone(), c.to.clone()))
        .collect()
}

// ------------------------------------------------------------------- I/O

fn catalog() -> Vec<(String, String)> {
    crate::contracts::env_schema().service_catalog()
}

/// This stack, as a preset.
pub fn export_current(root: &Path, name: Option<String>) -> Result<Preset> {
    let env = Env::load(root)?;
    Ok(export(&env, &catalog(), name))
}

/// Write it out. Pretty-printed, because the point of the file is being read
/// in a pull request.
pub fn save(root: &Path, path: &Path, name: Option<String>) -> Result<()> {
    let preset = export_current(root, name)?;
    let text = serde_json::to_string_pretty(&preset)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the preset: {e}")))?;
    crate::atomic::write(path, &format!("{text}\n"))
}

pub fn load(path: &Path) -> Result<Preset> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;
    parse(&text)
}

pub fn plan_file(root: &Path, path: &Path) -> Result<Plan> {
    let env = Env::load(root)?;
    let preset = load(path)?;
    Ok(plan(&env, &catalog(), &preset))
}

/// Apply it, then report the plan that was applied.
///
/// Re-planned rather than trusting a plan the frontend hands back: between the
/// review and the click, `.env` may have been changed by a service toggle in
/// another pane, and writing a stale diff would silently undo it.
pub fn apply_file(root: &Path, path: &Path) -> Result<Plan> {
    let plan = plan_file(root, path)?;
    let patch = patch(&plan);
    if !patch.is_empty() {
        crate::env_writer::apply(root, &patch)?;
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: [&str; 3] = ["mysql", "redis", "elasticsearch"];

    fn catalog() -> Vec<(String, String)> {
        CATALOG
            .iter()
            .map(|id| (id.to_string(), "database".to_string()))
            .collect()
    }

    fn env(text: &str) -> Env {
        Env::parse(text)
    }

    const REAL_ISH: &str = "\
DEFAULT_TLD_SUFFIX=stackvo.loc
SSL_ENABLE=true
DEFAULT_PHP_VERSION=8.2
SERVICE_MYSQL_ENABLE=true
SERVICE_MYSQL_VERSION=8.0
SERVICE_MYSQL_ROOT_PASSWORD=hunter2
SERVICE_MYSQL_DATABASE=stackvo
SERVICE_REDIS_ENABLE=true
SERVICE_REDIS_VERSION=7.0
SERVICE_REDIS_PASSWORD=s3cret
SERVICE_ELASTICSEARCH_ENABLE=false
SERVICE_ELASTICSEARCH_VERSION=8.11.3
GRAFANA_ADMIN_TOKEN=tok_live_abcdef
";

    /// The claim the whole design rests on. Run against an `.env` full of
    /// passwords: not one of them may appear anywhere in the serialised file,
    /// and it must hold because there is nowhere to put them — not because a
    /// filter happened to catch these particular names.
    #[test]
    fn no_secret_can_reach_a_preset() {
        let preset = export(&env(REAL_ISH), &catalog(), Some("team".into()));
        let text = serde_json::to_string(&preset).unwrap();

        for secret in ["hunter2", "s3cret", "tok_live_abcdef"] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
        // And nothing that merely looks like a credential key, either.
        for key in ["PASSWORD", "TOKEN", "SECRET", "DATABASE"] {
            assert!(!text.contains(key), "{key} leaked into {text}");
        }
    }

    /// A disabled service has to be *in* the preset. Listing only what is on
    /// cannot express "and turn Elasticsearch off", which is half of what makes
    /// two machines match.
    #[test]
    fn a_preset_carries_the_services_that_are_off_too() {
        let preset = export(&env(REAL_ISH), &catalog(), None);

        assert_eq!(preset.services.len(), 3);
        assert!(preset.services["mysql"].enabled);
        assert!(!preset.services["elasticsearch"].enabled);
        // Its version travels regardless, so enabling it later lands on the
        // version the team agreed on rather than on a default.
        assert_eq!(
            preset.services["elasticsearch"].version.as_deref(),
            Some("8.11.3")
        );
    }

    #[test]
    fn only_allow_listed_settings_are_exported() {
        let preset = export(&env(REAL_ISH), &catalog(), None);
        let keys: Vec<&str> = preset.settings.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["DEFAULT_PHP_VERSION", "DEFAULT_TLD_SUFFIX", "SSL_ENABLE"]
        );
    }

    #[test]
    fn a_plan_names_every_change_and_counts_the_rest() {
        let preset = Preset {
            kind: KIND.into(),
            version: VERSION,
            name: Some("team".into()),
            description: None,
            services: [
                (
                    "mysql".to_string(),
                    ServicePreset {
                        enabled: true,
                        version: Some("8.4".into()),
                    },
                ),
                (
                    "elasticsearch".to_string(),
                    ServicePreset {
                        enabled: true,
                        version: None,
                    },
                ),
            ]
            .into(),
            settings: BTreeMap::new(),
        };

        let plan = plan(&env(REAL_ISH), &catalog(), &preset);

        let described: Vec<(&str, Option<&str>, &str)> = plan
            .changes
            .iter()
            .map(|c| (c.key.as_str(), c.from.as_deref(), c.to.as_str()))
            .collect();
        assert_eq!(
            described,
            [
                ("SERVICE_ELASTICSEARCH_ENABLE", Some("false"), "true"),
                ("SERVICE_MYSQL_VERSION", Some("8.0"), "8.4"),
            ]
        );
        // mysql was already enabled; that is not a change and not a silence.
        assert_eq!(plan.unchanged, 1);
        assert!(plan.needs_regenerate);
    }

    /// The C-09 failure mode: acting on an id nobody knows writes a key nothing
    /// reads and brings up a compose profile that matches nothing. Rejecting is
    /// right; rejecting *silently* is how the user concludes it worked.
    #[test]
    fn an_unknown_service_is_rejected_by_name() {
        let preset = Preset {
            kind: KIND.into(),
            version: VERSION,
            name: None,
            description: None,
            services: [(
                "cockroachdb".to_string(),
                ServicePreset {
                    enabled: true,
                    version: None,
                },
            )]
            .into(),
            settings: BTreeMap::new(),
        };

        let plan = plan(&env(REAL_ISH), &catalog(), &preset);
        assert!(plan.changes.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(
            plan.rejected[0].contains("cockroachdb"),
            "{:?}",
            plan.rejected
        );
    }

    /// The allow-list has to hold on import as well as on export. Trusting the
    /// export side alone means a hand-edited preset can write any key it likes
    /// into somebody else's .env — including one whose name this app has never
    /// heard of and therefore cannot recognise as a secret.
    #[test]
    fn a_hand_edited_preset_cannot_smuggle_a_key_past_the_allow_list() {
        let preset = Preset {
            kind: KIND.into(),
            version: VERSION,
            name: None,
            description: None,
            services: BTreeMap::new(),
            settings: [
                ("SSL_ENABLE".to_string(), "false".to_string()),
                (
                    "SERVICE_MYSQL_ROOT_PASSWORD".to_string(),
                    "pwned".to_string(),
                ),
            ]
            .into(),
        };

        let plan = plan(&env(REAL_ISH), &catalog(), &preset);

        assert_eq!(plan.changes.len(), 1, "{:?}", plan.changes);
        assert_eq!(plan.changes[0].key, "SSL_ENABLE");
        assert!(!patch(&plan).contains_key("SERVICE_MYSQL_ROOT_PASSWORD"));
        assert!(plan
            .rejected
            .iter()
            .any(|r| r.contains("SERVICE_MYSQL_ROOT_PASSWORD")));
    }

    #[test]
    fn a_file_that_is_not_a_preset_is_refused_rather_than_read_as_empty() {
        // Valid JSON, wrong thing entirely — pointing the importer at a
        // package.json must say so, not plan zero changes and look successful.
        assert!(parse(r#"{"name":"my-app","version":"1.0.0"}"#).is_err());
        assert!(parse("not json at all").is_err());

        let future = format!(
            r#"{{"kind":"{KIND}","version":{},"services":{{}}}}"#,
            VERSION + 1
        );
        assert!(
            parse(&future).is_err(),
            "a newer preset must not be half-read"
        );
    }

    #[test]
    fn a_round_trip_plans_no_changes() {
        let env = env(REAL_ISH);
        let preset = export(&env, &catalog(), None);
        let plan = plan(&env, &catalog(), &preset);

        assert!(plan.changes.is_empty(), "{:?}", plan.changes);
        assert!(plan.rejected.is_empty(), "{:?}", plan.rejected);
        assert!(!plan.needs_regenerate);
    }

    /// An empty `SERVICE_X_VERSION` means "unset". Copying it across as a value
    /// would write a blank version key onto the other machine.
    #[test]
    fn a_blank_version_is_not_exported_as_one() {
        let preset = export(
            &env("SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_VERSION=\n"),
            &catalog(),
            None,
        );
        assert_eq!(preset.services["mysql"].version, None);
    }
}
