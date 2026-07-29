//! The frozen v1 contracts, compiled into the binary.
//!
//! Embedding rather than reading them from disk means the app cannot drift from
//! the contract it was built against, and it works before a workspace is even
//! selected. `cargo build` fails if a contract file is malformed — the check
//! happens at compile time for the shape, and once at startup for the parse.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const PHP_EXTENSIONS: &str = include_str!("../../contracts/php-extensions.json");
const ENV_SCHEMA: &str = include_str!("../../contracts/env.schema.json");

/// The IPC contract itself, compiled in.
///
/// Read by `mcp.rs` to check its tool table against the command surface, for
/// the reason suites E and F exist: nothing enforces the agreement at compile
/// time, so it has to be enforced by a test that runs.
const IPC: &str = include_str!("../../contracts/ipc.json");

/// The IPC contract as JSON. Parsed on first use, like the others.
pub fn ipc() -> &'static serde_json::Value {
    static CACHE: OnceLock<serde_json::Value> = OnceLock::new();
    CACHE.get_or_init(|| serde_json::from_str(IPC).expect("ipc.json is compiled in and valid"))
}

// ---------------------------------------------------------------- php extensions

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionSpec {
    /// `builtin` | `core` | `pecl` | `composer` | `special`
    pub install: String,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub configure: Option<String>,
    /// PHP `major.minor` → PECL version, plus a `default` key.
    #[serde(default, rename = "peclVersions")]
    pub pecl_versions: BTreeMap<String, String>,
    #[serde(default, rename = "peclDependencies")]
    pub pecl_dependencies: Vec<String>,
    #[serde(default, rename = "minPhp")]
    pub min_php: Option<String>,
    #[serde(default, rename = "removedIn")]
    pub removed_in: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Absent means "in the .env catalog"; `false` marks matrix-only entries.
    #[serde(default, rename = "inCatalog")]
    pub in_catalog: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PhpExtensions {
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    pub extensions: BTreeMap<String, ExtensionSpec>,
}

// ---------------------------------------------------------------- env schema

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceDependency {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    #[serde(default)]
    pub internal: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnvSchema {
    #[serde(rename = "contractVersion")]
    pub contract_version: String,
    /// category → service ids. `_note` is skipped by `service_catalog()`.
    pub services: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "serviceDependencies")]
    pub service_dependencies: BTreeMap<String, serde_json::Value>,
}

impl EnvSchema {
    /// Flat `(service_id, category)` list, `_note` entries dropped.
    pub fn service_catalog(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (category, value) in &self.services {
            if category == "_note" {
                continue;
            }
            if let Some(ids) = value.as_array() {
                for id in ids.iter().filter_map(|v| v.as_str()) {
                    out.push((id.to_string(), category.clone()));
                }
            }
        }
        out.sort();
        out
    }

    /// Is this a service the contract knows about?
    ///
    /// The catalog is the whole set — there are twenty of them and they are
    /// fixed by the schema. An unknown id is not a service that happens to be
    /// missing; it is a typo or a stale caller, and acting on it writes a
    /// `SERVICE_<JUNK>_ENABLE` key into the user's .env and brings up a compose
    /// profile that matches nothing. Silently doing nothing is the failure mode
    /// this project keeps finding in the shell version (CONFLICTS.md C-09).
    pub fn knows_service(&self, service: &str) -> bool {
        self.service_catalog().iter().any(|(id, _)| id == service)
    }

    pub fn dependencies_for(&self, service: &str) -> ServiceDependency {
        self.service_dependencies
            .get(service)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(ServiceDependency {
                required: Vec::new(),
                optional: Vec::new(),
                internal: Vec::new(),
                note: None,
            })
    }
}

// ---------------------------------------------------------------- accessors

static PHP: OnceLock<PhpExtensions> = OnceLock::new();
static ENV: OnceLock<EnvSchema> = OnceLock::new();

pub fn php_extensions() -> &'static PhpExtensions {
    PHP.get_or_init(|| {
        serde_json::from_str(PHP_EXTENSIONS).expect("bundled php-extensions.json is malformed")
    })
}

pub fn env_schema() -> &'static EnvSchema {
    ENV.get_or_init(|| {
        serde_json::from_str(ENV_SCHEMA).expect("bundled env.schema.json is malformed")
    })
}

/// Compare PHP `major.minor` strings. `8.10` sorts above `8.9`, which a plain
/// string comparison would get backwards.
pub fn cmp_php_version(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let ord = a
            .get(i)
            .copied()
            .unwrap_or(0)
            .cmp(&b.get(i).copied().unwrap_or(0));
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_catalog_is_the_whole_set_of_manageable_services() {
        let schema = env_schema();

        // Sampled from the categories in env.schema.json.
        for known in ["redis", "mongo-express", "mailhog", "postgres"] {
            assert!(schema.knows_service(known), "{known} is in the catalog");
        }

        // Near-misses are the realistic failure — each of these is the name a
        // caller would plausibly use, and each would write a
        // SERVICE_<JUNK>_ENABLE key into the user's .env that nothing reads.
        for unknown in ["postgresql", "mailpit", "mongodb", "", "_note", "redis "] {
            assert!(
                !schema.knows_service(unknown),
                "{unknown:?} must not be treated as a service"
            );
        }
    }

    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn bundled_contracts_parse() {
        assert_eq!(php_extensions().contract_version, "1.0.0");
        assert_eq!(env_schema().contract_version, "1.0.0");
    }

    #[test]
    fn imap_is_marked_removed_in_82() {
        // Guards CONFLICTS.md C-06: the .env default set ships imap while the
        // default PHP is 8.4, so this must stay detectable.
        let imap = &php_extensions().extensions["imap"];
        assert_eq!(imap.removed_in.as_deref(), Some("8.2"));
    }

    #[test]
    fn redis_pulls_igbinary_and_pins_per_version() {
        let redis = &php_extensions().extensions["redis"];
        assert_eq!(redis.install, "pecl");
        assert_eq!(redis.pecl_dependencies, vec!["igbinary"]);
        assert_eq!(redis.pecl_versions["8.4"], "6.3.0");
        assert_eq!(redis.pecl_versions["8.1"], "6.0.2");
    }

    #[test]
    fn service_catalog_has_twenty_entries() {
        // 20 templates on disk; README's "40+" and "14" are both wrong (C-17).
        assert_eq!(env_schema().service_catalog().len(), 20);
    }

    #[test]
    fn kibana_requires_elasticsearch() {
        assert_eq!(
            env_schema().dependencies_for("kibana").required,
            vec!["elasticsearch"]
        );
    }

    #[test]
    fn php_version_compare_is_numeric_not_lexical() {
        assert_eq!(cmp_php_version("8.10", "8.9"), Ordering::Greater);
        assert_eq!(cmp_php_version("8.4", "8.4"), Ordering::Equal);
        assert_eq!(cmp_php_version("7.4", "8.0"), Ordering::Less);
    }
}
