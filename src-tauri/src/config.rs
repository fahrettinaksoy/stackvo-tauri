//! Reading the StackVo `.env`, exactly as the Bash loader and the Node parser
//! read it — see `contracts/env.schema.json` → `parsing.rules`.
//!
//! Naive on purpose: first `=` wins, no unquoting, no interpolation. Being
//! cleverer than the Bash loader would mean the two tools disagree about what a
//! line means, which is the drift this contract exists to prevent.

use crate::error::{Error, Result};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: BTreeMap<String, String>,
}

impl Env {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".env");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;
        Ok(Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let mut vars = BTreeMap::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                vars.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Self { vars }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.as_str())
    }

    /// The whole map, unredacted.
    ///
    /// For the template renderer, which is reproducing what Bash does after
    /// exporting `.env` — it needs the real values, including the secrets that
    /// legitimately end up inside a generated service definition. Everything
    /// user-facing uses [`Self::redacted`] instead; this is deliberately not
    /// something a command returns.
    pub fn raw(&self) -> &BTreeMap<String, String> {
        &self.vars
    }

    /// First key that is present, in the order given. Encodes the precedence
    /// chains in `contracts/project.schema.json` → `x-stackvo-read-rules`.
    pub fn first_of(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|k| self.get(k))
    }

    /// Only lowercase `true` counts, matching the Bash `[ "$value" = "true" ]`
    /// comparisons. `TRUE` and `1` are falsy here because they are falsy there.
    pub fn bool(&self, key: &str) -> bool {
        self.get(key) == Some("true")
    }

    /// Comma-separated list, empty entries dropped.
    pub fn list(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `.env` key family for a service id: `mongo-express` → `SERVICE_MONGO_EXPRESS_`.
    ///
    /// Note the direction. Going the other way — deriving a compose profile by
    /// lowercasing the env key — is exactly the bug in CONFLICTS.md C-09, where
    /// `SERVICE_MONGO_EXPRESS_ENABLE` yields the profile `mongo_express` while
    /// the template declares `mongo-express`. Service ids come from the
    /// contract catalog, never from reversing this transform.
    pub fn service_prefix(service_id: &str) -> String {
        format!("SERVICE_{}_", service_id.to_uppercase().replace('-', "_"))
    }

    pub fn service_enabled(&self, service_id: &str) -> bool {
        self.bool(&format!("{}ENABLE", Self::service_prefix(service_id)))
    }

    pub fn service_version(&self, service_id: &str) -> Option<&str> {
        self.get(&format!("{}VERSION", Self::service_prefix(service_id)))
    }

    pub fn service_url(&self, service_id: &str) -> Option<&str> {
        self.get(&format!("{}URL", Self::service_prefix(service_id)))
    }

    pub fn service_host_port(&self, service_id: &str) -> Option<u16> {
        self.get(&format!("{}HOST_PORT", Self::service_prefix(service_id)))
            .and_then(|v| v.parse().ok())
    }

    /// Every `SERVICE_<ID>_*` value a user might need to connect with, with the
    /// secrets already masked.
    ///
    /// `ENABLE`, `VERSION` and `URL` are dropped: they are the service's own
    /// wiring and are shown elsewhere in the row, so repeating them here would
    /// pad the list with the three entries nobody came for.
    ///
    /// Masked rather than raw for the reason `redacted()` exists — a password
    /// crossing into the webview by default puts it in every screenshot of this
    /// page. `env_reveal` hands over a single value when asked for it.
    pub fn service_credentials(&self, service_id: &str) -> Vec<(String, String, bool)> {
        let prefix = Self::service_prefix(service_id);

        self.vars
            .iter()
            .filter_map(|(key, value)| {
                let field = key.strip_prefix(&prefix)?;
                if matches!(field, "ENABLE" | "VERSION" | "URL") || value.is_empty() {
                    return None;
                }

                let secret = Self::is_secret(key);
                let shown = if secret {
                    "••••••••".to_string()
                } else {
                    value.clone()
                };
                Some((field.to_string(), shown, secret))
            })
            .collect()
    }

    /// Keys whose values must never reach a log, an event or an error message.
    /// Mirrors `contracts/env.schema.json` → `secrets.policy`.
    pub fn is_secret(key: &str) -> bool {
        ["PASSWORD", "PASS", "TOKEN", "SECRET", "SERVER_ID"]
            .iter()
            .any(|suffix| key.ends_with(suffix))
    }

    /// The whole map with secret values replaced. This is what `env_get`
    /// returns — the raw values never cross the IPC boundary by default.
    pub fn redacted(&self) -> BTreeMap<String, String> {
        self.vars
            .iter()
            .map(|(k, v)| {
                let value = if Self::is_secret(k) && !v.is_empty() {
                    "••••••••".to_string()
                } else {
                    v.clone()
                };
                (k.clone(), value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# comment line
DEFAULT_PHP_VERSION=8.2

SERVICE_MYSQL_ENABLE=true
SERVICE_MYSQL_ROOT_PASSWORD=hunter2
SERVICE_MONGO_EXPRESS_ENABLE=true
SUPPORTED_SERVERS=nginx,apache, caddy
LOOKS_LIKE_URL=postgres://user:pw@host:5432/db?a=1
STACKVO_UI_ENABLE=TRUE
"#;

    #[test]
    fn credentials_mask_secrets_and_drop_the_wiring() {
        let env = Env::parse(
            "SERVICE_MYSQL_ENABLE=true\n\
             SERVICE_MYSQL_VERSION=8.0\n\
             SERVICE_MYSQL_URL=db.stackvo.loc\n\
             SERVICE_MYSQL_ROOT_PASSWORD=hunter2\n\
             SERVICE_MYSQL_DATABASE=stackvo\n\
             SERVICE_MYSQL_EMPTY=\n\
             SERVICE_MONGO_DATABASE=other\n",
        );

        let creds = env.service_credentials("mysql");
        let keys: Vec<&str> = creds.iter().map(|(k, _, _)| k.as_str()).collect();

        // ENABLE/VERSION/URL are the service's wiring, shown elsewhere in the
        // row; an empty value is not a credential; another service's keys are
        // not this service's.
        assert_eq!(keys, vec!["DATABASE", "ROOT_PASSWORD"]);

        let password = creds.iter().find(|(k, _, _)| k == "ROOT_PASSWORD").unwrap();
        assert_eq!(password.1, "••••••••", "the raw secret must not cross");
        assert!(password.2, "and it must be flagged as one");

        let database = creds.iter().find(|(k, _, _)| k == "DATABASE").unwrap();
        assert_eq!(database.1, "stackvo", "a non-secret is shown as it is");
        assert!(!database.2);
    }

    #[test]
    fn splits_on_the_first_equals_only() {
        let env = Env::parse(SAMPLE);
        assert_eq!(
            env.get("LOOKS_LIKE_URL"),
            Some("postgres://user:pw@host:5432/db?a=1")
        );
    }

    #[test]
    fn only_lowercase_true_is_truthy() {
        let env = Env::parse(SAMPLE);
        assert!(env.bool("SERVICE_MYSQL_ENABLE"));
        // Matches Bash's `[ "$value" = "true" ]` — uppercase is NOT true there.
        assert!(!env.bool("STACKVO_UI_ENABLE"));
    }

    #[test]
    fn list_trims_entries() {
        let env = Env::parse(SAMPLE);
        assert_eq!(
            env.list("SUPPORTED_SERVERS"),
            vec!["nginx", "apache", "caddy"]
        );
    }

    #[test]
    fn service_prefix_maps_dash_to_underscore() {
        assert_eq!(
            Env::service_prefix("mongo-express"),
            "SERVICE_MONGO_EXPRESS_"
        );
        let env = Env::parse(SAMPLE);
        assert!(env.service_enabled("mongo-express"));
    }

    #[test]
    fn secrets_are_redacted_but_keys_survive() {
        let env = Env::parse(SAMPLE);
        let out = env.redacted();
        assert_eq!(out["SERVICE_MYSQL_ROOT_PASSWORD"], "••••••••");
        assert_eq!(out["DEFAULT_PHP_VERSION"], "8.2");
    }
}
