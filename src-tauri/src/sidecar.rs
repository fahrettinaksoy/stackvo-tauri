//! Containers a repository brings with it, scoped to the project that declared
//! them.
//!
//! §5.1. The *command* half of that question was answered by ADR 0020: yes, a
//! workspace may declare one, on the condition that it stays inside its own
//! container. This is the other half, and it needed its own answer because the
//! argument does not carry over.
//!
//! ## Why ADR 0020's reasoning stops here
//!
//! That decision rests on one sentence: the project's container already runs
//! the repository's code, so a repository able to name a command in it has
//! gained nothing it did not already have. Every word of that is about a
//! container that is already running somebody else's code by design.
//!
//! A sidecar is a **different image**. Nothing about `stackvo.json` already
//! running `typesense/typesense:27.1` was true before the file said so. So the
//! containment has to be built rather than inherited, and this module is that
//! containment:
//!
//! * **No host port.** A declared container is reachable from the project and
//!   from nothing else. Two clones of one repository therefore cannot fight
//!   over 8108, which is the failure a stack-wide definition would have made
//!   routine rather than rare.
//! * **No host path.** Volumes are Docker volumes, named from the project, so
//!   a repository cannot mount the directory above itself.
//! * **Named from the project, never declared.** The container is
//!   `stackvo-<project>-<id>` and a volume is `stackvo-<project>-<id>-<handle>`.
//!   Nothing in the file chooses a global name, so nothing in the file can
//!   collide with somebody else's.
//! * **Lives and dies with the project.** It is rendered into the project's own
//!   compose block with the project's profile, so `--profile project-shop`
//!   brings it up and stopping shop stops it.
//!
//! ## Not a service, and the distinction is the whole point
//!
//! `services: ["mysql"]` names a **catalogue id** — a need, which this machine
//! satisfies from a package, as one instance shared by every project that asked
//! for it. That is stack-wide by design: one MySQL, one datadir, one port.
//!
//! A sidecar is the opposite shape. It is not in `instances.json`, it has no
//! version to resolve, no package to install, no entry in the market, and it is
//! not shared. Putting the two in one list would have made "how many of these
//! exist" a question with two answers.
//!
//! ## What is deliberately refused, and where the rest of it went
//!
//! A sidecar that wants a host port or a host directory is refused **by name**
//! rather than ignored, because somebody who wrote one has a model to correct.
//! That case is not "never": it is the second half of §5.1's answer — a consent
//! gate in `hooks`' shape, digest-bound, asked once per repository and again
//! whenever the declaration changes. Until that exists, refusing is the honest
//! state, and it is refused at *parse* time so the message arrives on the
//! manifest rather than out of a compose file.

use serde::Serialize;
use std::collections::BTreeMap;

/// Everything one project declared, in the order the file had them.
///
/// A list of ids beside the map, on `quickcmd::Declared`'s reasoning: a
/// `BTreeMap` alone would alphabetise, so a pane would list somebody's
/// containers in an order they did not choose and a manifest saved from the
/// form would come back reordered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Declared {
    #[serde(serialize_with = "in_file_order")]
    inner: Inner,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Inner {
    by_id: BTreeMap<String, Sidecar>,
    order: Vec<String>,
}

fn in_file_order<S: serde::Serializer>(inner: &Inner, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = s.serialize_map(Some(inner.by_id.len()))?;
    for id in &inner.order {
        if let Some(value) = inner.by_id.get(id) {
            map.serialize_entry(id, value)?;
        }
    }
    map.end()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidecar {
    /// With a tag, always. ADR 0014's argument about `latest` applies here for
    /// the same reason: an untagged image moves under somebody who pulled it
    /// last month.
    pub image: String,
    pub about: String,
    /// argv, never a command string.
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    /// A handle. The Docker volume's real name is derived — see [`volume_name`].
    pub name: String,
    /// Where it is mounted inside the container.
    pub path: String,
}

impl Declared {
    pub fn is_empty(&self) -> bool {
        self.inner.by_id.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.by_id.len()
    }

    pub fn get(&self, id: &str) -> Option<&Sidecar> {
        self.inner.by_id.get(id)
    }

    /// In the order the manifest declared them.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Sidecar)> {
        self.inner
            .order
            .iter()
            .filter_map(|id| self.inner.by_id.get_key_value(id.as_str()))
    }
}

/// The container's name on this machine.
///
/// Derived from the project rather than declared, which is what makes two
/// clones of one repository two containers instead of one collision.
pub fn container_name(project: &str, id: &str) -> String {
    format!("stackvo-{}-{id}", project.to_ascii_lowercase())
}

/// The Docker volume's name on this machine.
///
/// Same derivation, and it matters more here: a shared container is an
/// annoyance, a shared datadir is two projects writing the same files.
pub fn volume_name(project: &str, id: &str, handle: &str) -> String {
    format!("stackvo-{}-{id}-{handle}", project.to_ascii_lowercase())
}

fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 40
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// An image reference that names a tag.
///
/// Checked here rather than left to Docker: an image without a tag is accepted
/// by the engine and silently means `latest`, so the failure would arrive
/// months later as a container that changed by itself.
fn is_tagged_image(text: &str) -> bool {
    let Some((repository, tag)) = text.rsplit_once(':') else {
        return false;
    };
    // A registry with a port — `registry:5000/thing` — puts a colon in the
    // repository half. Splitting from the right and then checking the tag has
    // no slash is what tells the two apart.
    if tag.is_empty() || tag.contains('/') || repository.is_empty() {
        return false;
    }
    tag.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Read the `sidecars` block, with every reason it could not be read.
///
/// Shaped exactly like [`crate::quickcmd::parse`] and reporting through the
/// same [`crate::hooks::Problem`], so a malformed declaration lands on the
/// manifest as a finding beside every other one rather than as a failure from
/// somewhere downstream.
pub fn parse(json: &serde_json::Value) -> (Declared, Vec<crate::hooks::Problem>) {
    use crate::hooks::Problem;

    let mut out = Declared::default();
    let mut problems = Vec::new();

    let Some(block) = json.get("sidecars") else {
        return (out, problems);
    };
    let Some(map) = block.as_object() else {
        problems.push(Problem {
            path: "sidecars".into(),
            message: "`sidecars` must be an object keyed by id".into(),
        });
        return (out, problems);
    };

    for (id, value) in map {
        let at = format!("sidecars.{id}");
        let bad = |message: String| Problem {
            path: at.clone(),
            message,
        };

        if !is_id(id) {
            problems.push(bad(format!(
                "\"{id}\" is not a usable id — lower-case letters, digits and \
                 dashes, up to 40 characters. It becomes part of a container name"
            )));
            continue;
        }

        let Some(object) = value.as_object() else {
            problems.push(bad("a sidecar is an object with `image`".into()));
            continue;
        };

        // Named in the error rather than ignored, on `quickcmd`'s reasoning:
        // somebody who wrote one has a mental model to correct, and the
        // correction is a real thing rather than "you cannot do that".
        if object.contains_key("ports") {
            problems.push(bad(
                "a sidecar has no host port: it is reachable from this project \
                 and from nothing else, which is what stops two clones of one \
                 repository fighting over the same number. Binding a host port \
                 is the half of §5.1 that waits on a consent gate"
                    .into(),
            ));
            continue;
        }

        let Some(image) = object.get("image").and_then(|v| v.as_str()) else {
            problems.push(bad("a sidecar needs `image`".into()));
            continue;
        };
        if !is_tagged_image(image) {
            problems.push(bad(format!(
                "\"{image}\" names no tag. An untagged image is `latest`, which \
                 moves under whoever pulled it last — pin it the way this file \
                 already pins a PHP version"
            )));
            continue;
        }

        let command = match object.get("command") {
            None => Vec::new(),
            Some(value) => match argv_of(value) {
                Ok(argv) => argv,
                Err(message) => {
                    problems.push(bad(message));
                    continue;
                }
            },
        };

        let mut env = BTreeMap::new();
        let mut env_ok = true;
        if let Some(value) = object.get("env") {
            let Some(pairs) = value.as_object() else {
                problems.push(bad("`env` is an object of name to value".into()));
                continue;
            };
            for (key, value) in pairs {
                let Some(text) = value.as_str() else {
                    problems.push(bad(format!("`env.{key}` must be a string")));
                    env_ok = false;
                    break;
                };
                env.insert(key.clone(), text.to_string());
            }
        }
        if !env_ok {
            continue;
        }

        let mut volumes = Vec::new();
        let mut volumes_ok = true;
        if let Some(value) = object.get("volumes") {
            let Some(list) = value.as_array() else {
                problems.push(bad("`volumes` is a list".into()));
                continue;
            };
            for entry in list {
                let handle = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let path = entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if !is_id(handle) {
                    problems.push(bad(format!(
                        "\"{handle}\" is not a usable volume handle — lower-case \
                         letters, digits and dashes"
                    )));
                    volumes_ok = false;
                    break;
                }
                // The refusal that matters. A relative path, or one that does
                // not start at the root, is how a bind mount would be smuggled
                // in — and a repository that can mount a host directory can
                // mount the one above itself.
                if !path.starts_with('/') {
                    problems.push(bad(format!(
                        "\"{path}\" is not a path inside the container. A sidecar \
                         mounts Docker volumes only; a host directory is the half \
                         of §5.1 that waits on a consent gate"
                    )));
                    volumes_ok = false;
                    break;
                }
                if volumes.iter().any(|v: &Volume| v.name == handle) {
                    problems.push(bad(format!("\"{handle}\" is declared twice")));
                    volumes_ok = false;
                    break;
                }
                volumes.push(Volume {
                    name: handle.to_string(),
                    path: path.to_string(),
                });
            }
        }
        if !volumes_ok {
            continue;
        }

        out.inner.order.push(id.clone());
        out.inner.by_id.insert(
            id.clone(),
            Sidecar {
                image: image.to_string(),
                about: object
                    .get("about")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                command,
                env,
                volumes,
            },
        );
    }

    (out, problems)
}

/// A JSON array of non-empty strings, or why it is not one.
fn argv_of(value: &serde_json::Value) -> Result<Vec<String>, String> {
    let Some(list) = value.as_array() else {
        return Err(
            "`command` is an argv array, never a command string — nothing \
                    here splits a line back apart"
                .into(),
        );
    };
    if list.is_empty() {
        return Err("`command` is empty".into());
    }
    let mut argv = Vec::with_capacity(list.len());
    for item in list {
        match item.as_str() {
            Some(text) if !text.is_empty() => argv.push(text.to_string()),
            _ => return Err("every element of `command` is a non-empty string".into()),
        }
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(json: &str) -> (Declared, Vec<crate::hooks::Problem>) {
        parse(&serde_json::from_str(json).expect("the fixture is JSON"))
    }

    #[test]
    fn a_project_with_no_block_declares_nothing_and_complains_about_nothing() {
        let (declared, problems) = parsed(r#"{"name":"shop"}"#);
        assert!(declared.is_empty());
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn a_sidecar_keeps_the_order_the_file_had() {
        // A `BTreeMap` alone alphabetises, which would reorder somebody's file
        // the first time they saved the form.
        let (declared, problems) = parsed(
            r#"{"sidecars":{
                 "search":{"image":"typesense/typesense:27.1"},
                 "cache":{"image":"redis:7.4"},
                 "aaa":{"image":"busybox:1.36"}}}"#,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let ids: Vec<&String> = declared.iter().map(|(id, _)| id).collect();
        assert_eq!(ids, ["search", "cache", "aaa"]);
    }

    #[test]
    fn an_image_without_a_tag_is_refused_and_the_message_says_why() {
        let (declared, problems) = parsed(r#"{"sidecars":{"search":{"image":"typesense"}}}"#);
        assert!(declared.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("latest"), "{problems:?}");
    }

    #[test]
    fn a_registry_port_is_not_mistaken_for_a_missing_tag() {
        // `registry:5000/thing:1.2` has two colons and only the second is a
        // tag. Splitting from the left would refuse a perfectly good reference.
        let (declared, problems) =
            parsed(r#"{"sidecars":{"search":{"image":"registry.example:5000/team/search:1.2"}}}"#);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(declared.len(), 1);

        // And a registry port with no tag is still refused.
        let (_, problems) =
            parsed(r#"{"sidecars":{"search":{"image":"registry.example:5000/team/search"}}}"#);
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn a_host_port_is_refused_by_name_rather_than_dropped() {
        let (declared, problems) =
            parsed(r#"{"sidecars":{"search":{"image":"a/b:1","ports":["8108:8108"]}}}"#);
        assert!(declared.is_empty(), "it must not be half-accepted");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("host port"), "{problems:?}");
        // The message points at what would make it possible rather than
        // stopping at "no".
        assert!(problems[0].message.contains("consent"), "{problems:?}");
    }

    #[test]
    fn a_host_directory_cannot_be_smuggled_in_through_a_volume_path() {
        // The one that matters. `path` is where it is mounted *inside* the
        // container; a relative path is how a bind mount would arrive.
        for path in ["./", "../..", "~/code", "C:/", ""] {
            let (declared, problems) = parsed(&format!(
                r#"{{"sidecars":{{"x":{{"image":"a/b:1","volumes":[{{"name":"d","path":"{path}"}}]}}}}}}"#
            ));
            assert!(declared.is_empty(), "{path:?} was accepted");
            assert_eq!(problems.len(), 1, "{path:?}: {problems:?}");
        }
    }

    #[test]
    fn a_command_is_an_argv_array_and_never_a_line() {
        let (_, problems) =
            parsed(r#"{"sidecars":{"x":{"image":"a/b:1","command":"sh -c 'a && b'"}}}"#);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("argv"), "{problems:?}");

        let (declared, problems) =
            parsed(r#"{"sidecars":{"x":{"image":"a/b:1","command":["serve","--port","7700"]}}}"#);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            declared.get("x").unwrap().command,
            ["serve", "--port", "7700"]
        );
    }

    #[test]
    fn an_id_that_would_not_survive_a_container_name_is_refused() {
        for id in ["Search", "a_b", "x".repeat(41).as_str(), ""] {
            let (declared, problems) =
                parsed(&format!(r#"{{"sidecars":{{"{id}":{{"image":"a/b:1"}}}}}}"#));
            assert!(declared.is_empty(), "{id:?} was accepted");
            assert_eq!(problems.len(), 1, "{id:?}");
        }
    }

    #[test]
    fn one_bad_sidecar_does_not_take_the_good_ones_with_it() {
        let (declared, problems) = parsed(
            r#"{"sidecars":{
                 "good":{"image":"a/b:1"},
                 "bad":{"image":"no-tag"},
                 "alsogood":{"image":"c/d:2"}}}"#,
        );
        assert_eq!(problems.len(), 1);
        assert_eq!(declared.len(), 2, "the readable ones are kept");
        assert!(declared.get("good").is_some());
        assert!(declared.get("alsogood").is_some());
    }

    /// Every name a sidecar gets on this machine is derived, so two clones of
    /// one repository cannot collide.
    #[test]
    fn nothing_in_the_file_chooses_a_name_this_machine_shares() {
        assert_eq!(container_name("shop", "search"), "stackvo-shop-search");
        assert_eq!(
            container_name("shop-staging", "search"),
            "stackvo-shop-staging-search"
        );
        assert_ne!(
            container_name("shop", "search"),
            container_name("shop2", "search")
        );

        assert_eq!(
            volume_name("shop", "search", "data"),
            "stackvo-shop-search-data"
        );
        assert_ne!(
            volume_name("shop", "search", "data"),
            volume_name("shop2", "search", "data")
        );

        // Docker refuses a container name with capitals in it the same way it
        // refuses an image reference with them, and `generator.rs` already
        // lower-cases for that reason.
        assert_eq!(container_name("Aksoyca", "x"), "stackvo-aksoyca-x");
    }
}
