//! What a downloaded compose fragment is allowed to say.
//!
//! `contracts/compose-policy.json`, enforced. The packages repository runs the
//! same file over its tree before publishing; this is the other half, and the
//! two are not redundant. A review at publish time asks "should we ship this";
//! this asks "should this machine run it", and only the second one is still
//! there when a repository has been taken over, a mirror is lying, or somebody
//! has pointed `market.registryUrl` somewhere they should not have.
//!
//! ## Why an allowlist
//!
//! The set of compose keys worth forbidding is only ever known in retrospect.
//! `privileged` is obvious; `userns_mode: host` is not, and neither was
//! `volumes_from` until somebody noticed it inherits mounts this policy had
//! already refused. A key nobody has considered is refused, and adding one is a
//! review.
//!
//! ## Why after rendering
//!
//! What a fragment puts inside `{{ settings.X }}` is only visible once X has a
//! value, and X is a field a user types into. A check that ran on the template
//! would be checking a string with a hole in it.
//!
//! That has a consequence worth stating: after rendering there are no handles
//! left, so "is this mount a volume the manifest declared" cannot be asked of
//! the text. The renderer knows — it put the values there — so it passes the
//! exact set it substituted, and anything else in a `volumes:` line is a path
//! the package chose. `/var/run/docker.sock` is the one that matters: it is
//! root on the host with an extra step, and it is four words.
//!
//! ## The policy travels in the binary
//!
//! `include_str!`, not a file beside the app. A security policy that can be
//! edited by whatever it is defending against is a lock with the key taped to
//! it — the same argument `policy.rs` makes about what its own layer is *not*.

use crate::error::{Code, Error, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// The contract, compiled in. Authored in `contracts/`, copied into the
/// packages repository by `check-schema-sync.mjs`, and read here at build time
/// so the two can never be a version apart on the same machine.
const CONTRACT: &str = include_str!("../../contracts/compose-policy.json");

#[derive(Debug, Deserialize)]
struct Contract {
    allowed: BTreeMap<String, String>,
    refused: BTreeMap<String, String>,
    rules: Rules,
}

#[derive(Debug, Deserialize)]
struct Rules {
    cap_add: ListRule,
    sysctls: ListRule,
    deploy: ListRule,
    labels: LabelRule,
}

#[derive(Debug, Deserialize)]
struct ListRule {
    allowed: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LabelRule {
    #[serde(rename = "requiredPrefix")]
    required_prefix: String,
}

fn contract() -> &'static Contract {
    use std::sync::OnceLock;
    static PARSED: OnceLock<Contract> = OnceLock::new();
    PARSED.get_or_init(|| {
        serde_json::from_str(CONTRACT)
            .expect("contracts/compose-policy.json is compiled in and is this crate's own file")
    })
}

/// The values the renderer substituted, so the check can tell them from
/// anything a package wrote itself.
#[derive(Debug, Default)]
pub struct Allowed {
    /// The image reference the manifest asked for, assembled by the app.
    pub image: String,
    /// Every mount source the renderer produced: derived volume names, rendered
    /// config paths, the instance's log directory.
    pub mounts: BTreeSet<String>,
}

/// Refuse a rendered fragment that says something it may not.
///
/// `who` names the package, because these run in a loop over a table and
/// "refused" on its own is a message somebody has to bisect.
pub fn check(who: &str, rendered: &str, allowed: &Allowed) -> Result<()> {
    let contract = contract();
    let refuse = |what: &str, why: &str| -> Result<()> {
        Err(
            Error::new(Code::Forbidden, format!("{who}: {what} — {why}"))
                .with_hint(crate::hints::PACKAGE_REFUSED_BY_POLICY),
        )
    };

    let mut section = String::new();
    for raw in rendered.lines() {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let indented = raw.starts_with(' ') || raw.starts_with('\t');
        let line = raw.trim();

        // ---- a key at column zero is a key of the service itself ----------
        if !indented {
            let key = line.split(':').next().unwrap_or("").trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                // A list item or a continuation, not a key.
                if line.starts_with("- ") {
                    check_item(who, &section, line, allowed, contract)?;
                }
                continue;
            }
            section = key.to_string();

            if let Some(why) = contract.refused.get(key) {
                return refuse(key, why);
            }
            if !contract.allowed.contains_key(key) {
                return refuse(
                    key,
                    "not a key this policy knows. Adding one is a review, not an omission",
                );
            }
            // `image:` is assembled by the app from the manifest, which is where
            // the registry allowlist and the digest pin are applied. A literal
            // one is a package choosing its own bytes.
            if key == "image" {
                let value = line
                    .split_once(':')
                    .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\''))
                    .unwrap_or_default();
                if value != allowed.image {
                    return refuse(
                        "image",
                        &format!(
                            "renders as {value:?} and the manifest asked for {:?}",
                            allowed.image
                        ),
                    );
                }
            }
            continue;
        }

        check_item(who, &section, line, allowed, contract)?;
    }

    Ok(())
}

fn check_item(
    who: &str,
    section: &str,
    line: &str,
    allowed: &Allowed,
    contract: &Contract,
) -> Result<()> {
    let refuse = |what: &str, why: &str| -> Result<()> {
        Err(
            Error::new(Code::Forbidden, format!("{who}: {what} — {why}"))
                .with_hint(crate::hints::PACKAGE_REFUSED_BY_POLICY),
        )
    };
    let item = line.strip_prefix("- ").map(str::trim);

    match section {
        "volumes" => {
            let Some(item) = item else { return Ok(()) };
            let source = item
                .trim_matches('"')
                .trim_matches('\'')
                .split(':')
                .next()
                .unwrap_or_default();
            if !allowed.mounts.contains(source) {
                return refuse(
                    source,
                    "is not a volume or a file this package declared. A literal host path is a \
                     bind mount the package chose, and /var/run/docker.sock is root on the host",
                );
            }
        }
        "cap_add" => {
            let Some(cap) = item else { return Ok(()) };
            let cap = cap.trim_matches('"');
            if !contract.rules.cap_add.allowed.iter().any(|a| a == cap) {
                return refuse(
                    cap,
                    &format!(
                        "is not one of {}",
                        contract.rules.cap_add.allowed.join(", ")
                    ),
                );
            }
        }
        "labels" => {
            let Some(label) = item else { return Ok(()) };
            let name = label
                .trim_matches('"')
                .split('=')
                .next()
                .unwrap_or_default();
            if !name.starts_with(&contract.rules.labels.required_prefix) {
                return refuse(
                    name,
                    "is not a routing label, and routing is the only thing a package has \
                     business labelling a container as",
                );
            }
        }
        "sysctls" => {
            let name = item
                .unwrap_or(line)
                .trim_matches('"')
                .split(['=', ':'])
                .next()
                .unwrap_or_default()
                .trim();
            if !name.is_empty() && !contract.rules.sysctls.allowed.iter().any(|a| a == name) {
                return refuse(
                    name,
                    "is a kernel parameter this policy does not let a package choose",
                );
            }
        }
        // Only the first level under `deploy:` is checked; `resources` may
        // contain whatever Compose allows beneath it.
        "deploy" if !line.starts_with(' ') && !line.starts_with('-') => {
            let key = line.split(':').next().unwrap_or("").trim();
            if !key.is_empty() && !contract.rules.deploy.allowed.iter().any(|a| a == key) {
                return refuse(key, "is a Swarm setting, and this app does not run Swarm");
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> Allowed {
        Allowed {
            image: "mysql:8.0".into(),
            mounts: [
                "stackvo-mysql-8-0-data",
                "/root/generated/configs/mysql-8-0/my.cnf",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }

    const GOOD: &str = "\
image: \"mysql:8.0\"
container_name: \"stackvo-mysql-8-0\"
restart: unless-stopped
environment:
  MYSQL_DATABASE: \"shop\"
volumes:
  - \"stackvo-mysql-8-0-data:/var/lib/mysql\"
  - \"/root/generated/configs/mysql-8-0/my.cnf:/etc/my.cnf:ro\"
ports:
  - \"3306:3306\"
networks:
  stackvo-net:
    aliases: [\"stackvo-mysql-8-0\"]
";

    fn refused(fragment: &str) -> String {
        check("mysql@8.0", fragment, &allowed())
            .expect_err("this fragment should have been refused")
            .message
    }

    #[test]
    fn an_ordinary_fragment_passes() {
        check("mysql@8.0", GOOD, &allowed()).unwrap();
    }

    /// The contract compiles in and parses. A policy that failed to load would
    /// otherwise fail at the first render, on somebody else's machine.
    #[test]
    fn the_contract_is_readable_and_has_both_halves() {
        let c = contract();
        assert!(c.allowed.contains_key("image"));
        assert!(c.refused.contains_key("privileged"));
        assert!(c.rules.cap_add.allowed.contains(&"IPC_LOCK".to_string()));
        // A key on both lists would be a contract that contradicts itself, and
        // the allowlist branch would silently win.
        for key in c.refused.keys() {
            assert!(
                !c.allowed.contains_key(key),
                "{key} is both allowed and refused"
            );
        }
    }

    /// A key that carries a rule is a key that may appear.
    ///
    /// `cap_add` was written under `refused` while `rules.cap_add` listed six
    /// capabilities it permitted — so the exception the rule described could
    /// never be reached, and Elasticsearch's `IPC_LOCK` was refused by a
    /// contract that said it was allowed. The general form is this: a rule
    /// narrows a key, and narrowing something already forbidden is a sentence
    /// with no effect.
    #[test]
    fn every_key_with_a_rule_is_a_key_that_may_appear() {
        let c = contract();
        let raw: serde_json::Value = serde_json::from_str(CONTRACT).unwrap();
        let rules = raw["rules"].as_object().expect("the contract has rules");

        for key in rules.keys() {
            assert!(
                !c.refused.contains_key(key),
                "{key} carries a rule and is refused outright, so the rule is unreachable"
            );
            assert!(
                c.allowed.contains_key(key),
                "{key} carries a rule and is not an allowed key"
            );
        }
    }

    // ---- the attacks ------------------------------------------------------

    #[test]
    fn privileged_is_refused() {
        assert!(refused(&format!("{GOOD}privileged: true\n")).contains("privileged"));
    }

    #[test]
    fn the_host_namespaces_are_refused() {
        for key in [
            "network_mode: host",
            "pid: host",
            "ipc: host",
            "userns_mode: host",
        ] {
            let message = refused(&format!("{GOOD}{key}\n"));
            assert!(
                message.contains(key.split(':').next().unwrap()),
                "{key}: {message}"
            );
        }
    }

    /// The one that is root on the host with an extra step.
    #[test]
    fn the_docker_socket_cannot_be_mounted() {
        let with_socket = GOOD.replace(
            "volumes:\n",
            "volumes:\n  - \"/var/run/docker.sock:/var/run/docker.sock\"\n",
        );
        let message = refused(&with_socket);
        assert!(message.contains("/var/run/docker.sock"), "{message}");
        assert!(message.contains("root on the host"), "{message}");
    }

    /// Any host path, not only the famous one.
    #[test]
    fn a_bind_mount_the_renderer_did_not_produce_is_refused() {
        let with_bind = GOOD.replace(
            "volumes:\n",
            "volumes:\n  - \"/Users/me/.ssh:/root/.ssh:ro\"\n",
        );
        assert!(refused(&with_bind).contains("/Users/me/.ssh"));
    }

    #[test]
    fn a_build_context_is_refused() {
        assert!(refused(&format!("{GOOD}build: .\n")).contains("build"));
    }

    /// `env_file` and `extends` reach into files this check has not seen.
    #[test]
    fn the_keys_that_reach_outside_this_file_are_refused() {
        for key in [
            "env_file: /etc/passwd",
            "extends: other.yml",
            "volumes_from: other",
        ] {
            assert!(!refused(&format!("{GOOD}{key}\n")).is_empty());
        }
    }

    #[test]
    fn a_capability_outside_the_short_list_is_refused() {
        let with_cap = format!("{GOOD}cap_add:\n  - SYS_ADMIN\n");
        assert!(refused(&with_cap).contains("SYS_ADMIN"));

        // And the ones an image genuinely needs still pass.
        let allowed_cap = format!("{GOOD}cap_add:\n  - IPC_LOCK\n");
        check("mysql@8.0", &allowed_cap, &allowed()).unwrap();
    }

    #[test]
    fn a_label_that_is_not_routing_is_refused() {
        let with_label = format!("{GOOD}labels:\n  - \"com.example.trusted=true\"\n");
        assert!(refused(&with_label).contains("com.example.trusted"));

        let routing = format!("{GOOD}labels:\n  - \"traefik.enable=true\"\n");
        check("mysql@8.0", &routing, &allowed()).unwrap();
    }

    #[test]
    fn a_kernel_parameter_outside_the_list_is_refused() {
        let bad = format!("{GOOD}sysctls:\n  - kernel.shmmax=1\n");
        assert!(refused(&bad).contains("kernel.shmmax"));

        let good = format!("{GOOD}sysctls:\n  - vm.max_map_count=262144\n");
        check("mysql@8.0", &good, &allowed()).unwrap();
    }

    /// The image is the app's to assemble: it is where the registry allowlist
    /// and the digest pin are applied, and a literal bypasses both.
    #[test]
    fn an_image_other_than_the_one_the_manifest_named_is_refused() {
        let swapped = GOOD.replace("mysql:8.0", "attacker/backdoor:latest");
        let message = refused(&swapped);
        assert!(message.contains("attacker/backdoor"), "{message}");
    }

    /// A key nobody has thought about is refused rather than allowed by
    /// omission. This is the whole argument for an allowlist.
    #[test]
    fn a_key_this_policy_has_never_heard_of_is_refused() {
        let message = refused(&format!("{GOOD}some_future_compose_key: danger\n"));
        assert!(message.contains("some_future_compose_key"), "{message}");
        assert!(message.contains("review"), "{message}");
    }
}
