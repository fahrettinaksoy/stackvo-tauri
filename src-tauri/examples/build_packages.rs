//! Turning this repository's twenty-five compiled-in service templates into a
//! package tree for `stackvo/stackvo-service-packages`.
//!
//!   cargo run --example build_packages -- --out ../../stackvo-service-packages
//!
//! Faz 1 of `docs/servis-market-mimarisi.md`. The packages have to come from
//! somewhere, and the two options were *write 109 manifests by hand* and
//! *derive them from what the app already knows*. This is the second, for a
//! reason the report states and this file has to keep honest: the templates
//! carry decisions that took measurement to get right — that Mongo
//! authenticates against `admin`, that Valkey publishes 6381 so it can sit
//! beside Redis, that RabbitMQ is pulled as `4.3-management` because the plain
//! tag exists for series the management one does not. Hand-copying them is how
//! each of those quietly becomes wrong in one of two places.
//!
//! So every value written here is **read from the source of truth**, not
//! restated: the catalog from `contracts/env.schema.json`, versions and
//! defaults from `config::EMBEDDED`, connection shapes from `connect::shapes()`
//! — which exists for this caller — and the rest by parsing the templates
//! `skeleton` already compiles in.
//!
//! ## It refuses rather than guesses
//!
//! Every line of every template is either recognised and transformed, or it is
//! an error naming the file, the line number and the text. A converter that
//! passes through what it does not understand produces a package that looks
//! finished and renders a container nobody meant — and nothing downstream would
//! catch it, because the compose file would still be valid YAML. The exit code
//! is the review gate: it is meant to fail on the first run against a template
//! nobody has looked at yet.
//!
//! ## What it deliberately does not do
//!
//! No `registry.json` — that is `tools/build-registry.mjs` in the packages
//! repository, because the index is a fact about a published tree and this is
//! a fact about this checkout. No hashes for `manifest.json` itself, for the
//! same reason. No image digests: resolving one needs the network and
//! `tools/probe-tags.mjs` is where network-shaped truth lives.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use stackvo_desktop_lib::{config::Env, connect, contracts::env_schema, pkg, skeleton};

// ---------------------------------------------------------------- errors

/// A refusal, with enough in it to fix the template or the converter.
#[derive(Debug)]
struct Refusal {
    service: String,
    line: usize,
    text: String,
    why: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} — {}\n      {}",
            self.service,
            self.line,
            self.why,
            self.text.trim()
        )
    }
}

// ---------------------------------------------------------------- model

#[derive(Debug, Clone)]
struct Port {
    name: String,
    container: u16,
    preferred: u16,
    /// The `.env` key the old template read it from, kept only so the summary
    /// can report which of the two spellings this service used.
    legacy_key: Option<String>,
}

#[derive(Debug, Clone)]
struct Volume {
    name: String,
    container: String,
}

#[derive(Debug, Clone)]
struct FileMount {
    name: String,
    /// Path inside the package: `files/<basename>`.
    template: String,
    /// The bytes, read out of the skeleton.
    contents: String,
    target: String,
}

#[derive(Debug, Clone)]
struct Setting {
    key: String,
    is_secret: bool,
    default: Option<String>,
}

#[derive(Debug, Clone)]
struct Companion {
    name: String,
    image_registry: Option<String>,
    image_repository: String,
    image_tag: String,
    fragment: String,
    ports: Vec<Port>,
    volumes: Vec<Volume>,
}

#[derive(Debug)]
struct Package {
    id: String,
    category: String,
    versions: Vec<String>,
    default_version: String,
    image_registry: Option<String>,
    image_repository: String,
    /// What the template appends after the version — `-management`, usually
    /// empty. The tag is this composed with each version.
    image_tag_suffix: String,
    ports: Vec<Port>,
    volumes: Vec<Volume>,
    files: Vec<FileMount>,
    settings: Vec<Setting>,
    subdomain: Option<String>,
    /// Which port entry the Traefik router forwards to.
    url_port: Option<String>,
    companions: Vec<Companion>,
    fragment: String,
}

// ---------------------------------------------------------------- helpers

/// `mongo-express` → `SERVICE_MONGO_EXPRESS_`. The app's own function, not a
/// second implementation: C-09 is what happens when two places derive this.
fn prefix_of(id: &str) -> String {
    Env::service_prefix(id)
}

/// The handle a port is known by, derived from the key the template read.
///
/// Mechanical on purpose. `HOST_PORT_MAILPIT_SMTP` → `smtp`,
/// `SERVICE_MINIO_CONSOLE_HOST_PORT` → `console`, `HOST_PORT_MYSQL` → `main`.
/// Naming them by hand would be twenty-five more rows to keep in step with a
/// tree that already has too many.
fn port_name_from_key(service: &str, key: &str) -> String {
    let upper = service.to_uppercase().replace('-', "_");
    let rest = key
        .trim_start_matches("SERVICE_")
        .trim_start_matches("HOST_PORT_")
        .replace(&format!("{upper}_"), "")
        .replace(&upper, "")
        .replace("HOST_PORT", "")
        .replace("_HOST_PORT", "");
    let cleaned = rest.trim_matches('_').to_lowercase().replace('_', "-");
    if cleaned.is_empty() {
        "main".to_string()
    } else {
        cleaned
    }
}

/// Every `{{ NAME | default('x') }}` or `{{ NAME }}` in a line.
fn placeholders(line: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else {
            break;
        };
        let inner = rest[start + 2..start + end].trim();
        rest = &rest[start + end + 2..];

        let (name, default) = match inner.split_once('|') {
            Some((n, d)) => {
                let d = d.trim();
                let value = d
                    .strip_prefix("default(")
                    .and_then(|v| v.strip_suffix(')'))
                    .map(|v| v.trim().trim_matches('\'').trim_matches('"').to_string());
                (n.trim().to_string(), value)
            }
            None => (inner.to_string(), None),
        };
        out.push((name, default));
    }
    out
}

// ---------------------------------------------------------------- parsing

/// One service template, read and taken apart.
///
/// Line-based rather than through a YAML parser, and the justification is the
/// same one `migrate.rs` gives for the opposite choice: that module parses
/// *arbitrary* compose files written by other people, so it shells out to
/// Docker. These twenty-five are files this repository owns, whose shape is
/// fixed and whose every deviation should be reported rather than absorbed.
fn parse(
    id: &str,
    category: &str,
    text: &str,
    embedded: &BTreeMap<&str, &str>,
) -> Result<Package, Vec<Refusal>> {
    let mut refusals = Vec::new();
    let prefix = prefix_of(id);

    // ---- versions, from the app's own defaults --------------------------
    //
    // ADR 0014: a moving tag cannot be a version directory, because a manifest
    // whose image can change under it has no fixed digest and so no place in
    // the chain of trust. The catalog offers `latest` for eleven services and
    // twelve list it, so this is not an edge case — it is most of the tree, and
    // dropping the entry is the whole of the resolution: the newest concrete
    // version becomes `recommendedVersion`, which is what `latest` will mean
    // from the registry's side.
    let offered: Vec<String> = embedded
        .get(format!("{prefix}VERSIONS").as_str())
        .map(|v| v.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let moving = pkg::MOVING_TAGS;
    let versions: Vec<String> = offered
        .iter()
        .filter(|v| !moving.contains(&v.as_str()))
        .cloned()
        .collect();

    let declared_default = embedded
        .get(format!("{prefix}VERSION").as_str())
        .map(|v| v.to_string())
        .unwrap_or_default();
    // The first concrete entry, because the lists are newest-first. When the
    // declared default is already concrete it wins — a user's current version
    // must stay the recommended one, or migration would quietly propose an
    // upgrade nobody asked for.
    let default_version = if moving.contains(&declared_default.as_str()) {
        versions.first().cloned().unwrap_or_default()
    } else {
        declared_default
    };

    if versions.is_empty() {
        refusals.push(Refusal {
            service: id.into(),
            line: 0,
            text: format!("{prefix}VERSIONS = {}", offered.join(",")),
            why: "every offered version is a moving tag — the catalog has no concrete \
                  version to package (ADR 0014)"
                .into(),
        });
    } else if !versions.contains(&default_version) {
        refusals.push(Refusal {
            service: id.into(),
            line: 0,
            text: format!("{prefix}VERSION = {default_version}"),
            why: "the default version is not among the offered ones".into(),
        });
    }

    // ---- walk the template ----------------------------------------------
    let mut blocks = split_services(id, text, &mut refusals);
    if blocks.is_empty() {
        refusals.push(Refusal {
            service: id.into(),
            line: 0,
            text: "services:".into(),
            why: "no service block found".into(),
        });
        return Err(refusals);
    }

    // The block whose key is the service id is the service; anything else is a
    // companion. Kafka is the only one in the current catalog and its extra
    // block is Zookeeper, which env.schema.json already calls `internal`.
    let main_index = blocks
        .iter()
        .position(|b| b.key == id)
        .unwrap_or(blocks.len() - 1);
    let main = blocks.remove(main_index);

    let mut pkg = Package {
        id: id.into(),
        category: category.into(),
        versions,
        default_version,
        image_registry: None,
        image_repository: String::new(),
        image_tag_suffix: String::new(),
        ports: Vec::new(),
        volumes: Vec::new(),
        files: Vec::new(),
        settings: Vec::new(),
        subdomain: None,
        url_port: None,
        companions: Vec::new(),
        fragment: String::new(),
    };

    let body = transform(id, &prefix, &main, embedded, &mut pkg, &mut refusals, true);
    pkg.fragment = body;
    adopt_orphaned_config(id, &mut pkg, &mut refusals);

    for block in blocks {
        let mut side = Package {
            id: block.key.clone(),
            category: category.into(),
            versions: Vec::new(),
            default_version: String::new(),
            image_registry: None,
            image_repository: String::new(),
            image_tag_suffix: String::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
            files: Vec::new(),
            settings: Vec::new(),
            subdomain: None,
            url_port: None,
            companions: Vec::new(),
            fragment: String::new(),
        };
        let fragment = transform(
            id,
            &prefix,
            &block,
            embedded,
            &mut side,
            &mut refusals,
            false,
        );
        pkg.companions.push(Companion {
            name: block.key.clone(),
            image_registry: side.image_registry.clone(),
            image_repository: side.image_repository.clone(),
            // A companion's tag is written in full in the template — Zookeeper
            // is `cp-zookeeper:latest` — so there is no version to compose it
            // with, and nothing here may invent one.
            image_tag: side.image_tag_suffix.clone(),
            fragment,
            ports: side.ports.clone(),
            volumes: side.volumes.clone(),
        });
    }

    if refusals.is_empty() {
        Ok(pkg)
    } else {
        Err(refusals)
    }
}

struct Block {
    key: String,
    /// (line number in the original file, text with the block's own indent
    /// removed)
    lines: Vec<(usize, String)>,
}

/// Split `services:` into one block per service key, dropping the file's
/// comment banner and its trailing top-level `volumes:` section.
fn split_services(id: &str, text: &str, refusals: &mut Vec<Refusal>) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut in_services = false;
    let mut done = false;

    for (n, raw) in text.lines().enumerate() {
        let line_no = n + 1;
        let trimmed = raw.trim_end();

        if trimmed.trim().is_empty() {
            if let Some(b) = blocks.last_mut() {
                if in_services {
                    b.lines.push((line_no, String::new()));
                }
            }
            continue;
        }
        if trimmed.trim_start().starts_with('#') && !in_services {
            continue;
        }
        if trimmed == "services:" {
            in_services = true;
            continue;
        }
        // A key at column zero. Three cases, and the awkward one is the middle.
        //
        // `template.rs::service_body` documents it: several templates write
        // `ports:` and `networks:` at column zero even though they belong to the
        // service above, and that filter's whole job is lifting them back to
        // four spaces. They are the service's, not the file's — and the package
        // format is where that stops being true, because a fragment is written
        // at one indent by construction.
        //
        // `volumes:` at column zero is the real top-level key and ends the
        // useful part of the file; its contents are re-derived from the mounts.
        if !trimmed.starts_with(' ') && trimmed.ends_with(':') && in_services {
            if trimmed == "volumes:" {
                done = true;
                in_services = false;
                continue;
            }
            if trimmed == "ports:" || trimmed == "networks:" {
                if let Some(b) = blocks.last_mut() {
                    b.lines.push((line_no, trimmed.to_string()));
                    continue;
                }
            }
            refusals.push(Refusal {
                service: id.into(),
                line: line_no,
                text: trimmed.into(),
                why: "unexpected top-level key inside services:".into(),
            });
            continue;
        }
        if !trimmed.starts_with(' ') && trimmed.ends_with(':') {
            continue;
        }
        // A list item at column zero belongs to whichever column-zero key was
        // just lifted. Same provenance, same fix.
        if in_services && trimmed.starts_with("- ") {
            if let Some(b) = blocks.last_mut() {
                b.lines.push((line_no, format!("  {trimmed}")));
                continue;
            }
        }
        if done || !in_services {
            continue;
        }

        // `  <key>:` at exactly two spaces starts a service.
        let indent = trimmed.len() - trimmed.trim_start().len();
        if indent == 2 && trimmed.trim_end().ends_with(':') && !trimmed.trim().contains(' ') {
            blocks.push(Block {
                key: trimmed.trim().trim_end_matches(':').to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        match blocks.last_mut() {
            Some(b) => {
                let stripped = trimmed.strip_prefix("    ").unwrap_or(trimmed).to_string();
                b.lines.push((line_no, stripped));
            }
            None => refusals.push(Refusal {
                service: id.into(),
                line: line_no,
                text: trimmed.into(),
                why: "content before any service key".into(),
            }),
        }
    }

    blocks
}

/// Rewrite one service block into a package fragment, harvesting everything the
/// manifest needs on the way through.
#[allow(clippy::too_many_arguments)]
fn transform(
    id: &str,
    prefix: &str,
    block: &Block,
    embedded: &BTreeMap<&str, &str>,
    pkg: &mut Package,
    refusals: &mut Vec<Refusal>,
    is_main: bool,
) -> String {
    let mut out = String::new();
    let mut seen_settings: BTreeSet<String> = BTreeSet::new();
    let mut section = String::new();

    // A companion renders in a namespace of its own. The first version of this
    // wrote `{{ image }}` into Zookeeper's fragment, which resolves to Kafka's
    // image — one substitution away from a compose file that starts two Kafka
    // brokers and calls one of them a coordinator. Every handle a companion
    // uses is its own: its image, its container name, its ports, its volumes.
    let ns = if is_main { "" } else { "companion." };

    for (line_no, line) in &block.lines {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // Track which key we are under, so `ports:`/`volumes:` list items can
        // be read as ports and volumes rather than as arbitrary strings.
        let indent = line.len() - line.trim_start().len();
        if indent == 0 && trimmed.ends_with(':') {
            section = trimmed.trim_end_matches(':').to_string();
        } else if indent == 0 && trimmed.contains(':') {
            section = trimmed.split(':').next().unwrap_or("").to_string();
        }

        // ---- the lines that are dropped entirely -------------------------
        //
        // `profiles:` is the app's to decide now: the profile is the instance
        // slug, and a template that named its own would name the same one for
        // both versions of a service.
        if section == "profiles" || trimmed.starts_with("profiles:") {
            continue;
        }

        // `healthcheck:` moves to the manifest, where `health_of` writes it and
        // the schema constrains it. Two of the twenty-five templates carried
        // one, both in the string-through-a-shell form, and leaving them here
        // would mean the same field had two authors — which is a duplicate YAML
        // key once the app started writing its own.
        if section == "healthcheck" || trimmed.starts_with("healthcheck:") {
            continue;
        }

        // ---- image ------------------------------------------------------
        if let Some(rest) = trimmed.strip_prefix("image:") {
            let reference = rest.trim().trim_matches('"').trim_matches('\'');
            let version_placeholder = format!("{{{{ {prefix}VERSION }}}}");

            let (registry, repository, suffix) = match reference.split_once(&version_placeholder) {
                Some((head, tail)) => {
                    let head = head.trim_end_matches(':');
                    let (reg, repo) = split_registry(head);
                    (reg, repo, tail.to_string())
                }
                None => {
                    // A companion writes its tag in full. For the main service
                    // that is a template this converter must not guess at.
                    if is_main {
                        refusals.push(Refusal {
                            service: id.into(),
                            line: *line_no,
                            text: line.clone(),
                            why: format!("image does not interpolate {prefix}VERSION"),
                        });
                    }
                    match reference.rsplit_once(':') {
                        Some((head, tag)) => {
                            let (reg, repo) = split_registry(head);
                            (reg, repo, tag.to_string())
                        }
                        None => {
                            let (reg, repo) = split_registry(reference);
                            (reg, repo, "latest".to_string())
                        }
                    }
                }
            };

            pkg.image_registry = registry;
            pkg.image_repository = repository;
            pkg.image_tag_suffix = suffix;
            let _ = writeln!(out, "image: \"{{{{ {ns}image }}}}\"");
            continue;
        }

        // ---- container name ---------------------------------------------
        if trimmed.starts_with("container_name:") {
            let _ = writeln!(out, "container_name: \"{{{{ {ns}instance.container }}}}\"");
            continue;
        }

        // ---- ports -------------------------------------------------------
        if section == "ports" && trimmed.starts_with("- ") {
            let spec = trimmed[2..].trim().trim_matches('"').trim_matches('\'');
            let Some((host, container)) = spec.rsplit_once(':') else {
                refusals.push(Refusal {
                    service: id.into(),
                    line: *line_no,
                    text: line.clone(),
                    why: "port mapping has no host:container form".into(),
                });
                continue;
            };
            let Ok(container_port) = container.trim().parse::<u16>() else {
                refusals.push(Refusal {
                    service: id.into(),
                    line: *line_no,
                    text: line.clone(),
                    why: "container port is not a number".into(),
                });
                continue;
            };

            let found = placeholders(host);
            let (name, preferred, legacy_key) = match found.first() {
                Some((key, default)) => {
                    let preferred = default
                        .as_deref()
                        .or_else(|| embedded.get(key.as_str()).copied())
                        .and_then(|v| v.parse::<u16>().ok())
                        .unwrap_or(container_port);
                    (port_name_from_key(id, key), preferred, Some(key.clone()))
                }
                // Zookeeper's `2181:2181` — a literal, and the only one.
                None => match host.trim().parse::<u16>() {
                    Ok(p) => ("main".to_string(), p, None),
                    Err(_) => {
                        refusals.push(Refusal {
                            service: id.into(),
                            line: *line_no,
                            text: line.clone(),
                            why: "host port is neither a placeholder nor a number".into(),
                        });
                        continue;
                    }
                },
            };

            // The embedded default wins over the template's when they disagree,
            // because `config::EMBEDDED` is what the running app actually reads
            // — and they DO disagree: mongo-express's template says 8081, which
            // is phpMyAdmin's port, and only EMBEDDED's 8083 keeps them apart.
            let preferred = legacy_key
                .as_deref()
                .and_then(|k| embedded.get(k).copied())
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(preferred);

            let _ = writeln!(out, "  - \"{{{{ {ns}port.{name} }}}}:{container_port}\"");
            pkg.ports.push(Port {
                name,
                container: container_port,
                preferred,
                legacy_key,
            });
            continue;
        }

        // ---- volumes and file mounts -------------------------------------
        if section == "volumes" && trimmed.starts_with("- ") {
            let spec = trimmed[2..].trim().trim_matches('"').trim_matches('\'');
            let parts: Vec<&str> = spec.split(':').collect();
            if parts.len() < 2 {
                refusals.push(Refusal {
                    service: id.into(),
                    line: *line_no,
                    text: line.clone(),
                    why: "volume line has no source:target form".into(),
                });
                continue;
            }
            let source = parts[0];
            let target = parts[1];
            let mode = parts.get(2).map(|m| format!(":{m}")).unwrap_or_default();

            if let Some(rest) = source.strip_prefix("${HOST_STACKVO_ROOT}/generated/configs/") {
                let name = rest.replace(['.', '-'], "_");
                let _ = writeln!(out, "  - \"{{{{ {ns}file.{name} }}}}:{target}{mode}\"");
                pkg.files.push(FileMount {
                    name,
                    template: String::new(),
                    contents: String::new(),
                    target: target.into(),
                });
                continue;
            }
            if source.starts_with("${HOST_STACKVO_ROOT}/logs/services/") {
                // Kafka does not get one, and this is not a preference.
                //
                // Docker creates a bind target that does not exist as root, and
                // `cp-kafka` runs as `appuser`. Its entrypoint passes
                // `-Xlog:gc*:file=/var/log/kafka/kafkaServer-gc.log`, so the
                // *JVM* fails to start — not the broker, the JVM — and the
                // container restarts forever:
                //
                //   Error opening log file '…/kafkaServer-gc.log': Permission denied
                //   Error: Could not create the Java Virtual Machine.
                //
                // The same mount is in the pre-package template, so this is not
                // a regression: Kafka has never started, and nothing said so
                // because nothing asked whether it was up. `health_probe.rs`
                // asked. Without the mount the image's own `/var/log/kafka` is
                // there and is `appuser`'s, so the write succeeds — and no user
                // loses a file, because none was ever written.
                // Scoped to the broker. Zookeeper shares the image family and
                // the uid, and it starts with its mount — measured, in the same
                // run. Dropping it too would be tidier and would be a change
                // nothing observed, which is the kind this catalogue already
                // has too many of.
                if id == "kafka" && is_main {
                    continue;
                }
                let _ = writeln!(out, "  - \"{{{{ {ns}instance.logs }}}}:{target}{mode}\"");
                continue;
            }
            if let Some(rest) = source.strip_prefix("stackvo-") {
                // `stackvo-mysql-data` → `data`. Matched against the block's own
                // key rather than the package id, so a companion's volume is
                // named against the companion.
                let owner = format!("{}-", block.key);
                let Some(name) = rest.strip_prefix(&owner) else {
                    refusals.push(Refusal {
                        service: id.into(),
                        line: *line_no,
                        text: line.clone(),
                        why: format!("named volume is not prefixed stackvo-{}-", block.key),
                    });
                    continue;
                };
                let _ = writeln!(out, "  - \"{{{{ {ns}volume.{name} }}}}:{target}{mode}\"");
                pkg.volumes.push(Volume {
                    name: name.into(),
                    container: target.into(),
                });
                continue;
            }

            refusals.push(Refusal {
                service: id.into(),
                line: *line_no,
                text: line.clone(),
                why: "unrecognised volume source — a bind outside the workspace is refused".into(),
            });
            continue;
        }

        // ---- networks ----------------------------------------------------
        //
        // The one structural rewrite. A list of network names becomes a mapping
        // with aliases, because that is where the legacy name survives: the
        // primary instance carries `stackvo-mysql` so that every project whose
        // .env says DB_HOST=stackvo-mysql keeps working.
        if section == "networks" {
            if trimmed == "networks:" {
                let _ = writeln!(
                    out,
                    "networks:\n  {{{{ network }}}}:\n    aliases: {{{{ {ns}instance.aliases }}}}"
                );
            }
            continue;
        }

        // ---- Traefik labels ----------------------------------------------
        if section == "labels" && trimmed.starts_with("- ") {
            let label = rewrite_label(id, prefix, &trimmed[2..], pkg);
            let _ = writeln!(out, "  - {label}");
            continue;
        }

        // ---- everything else, with its placeholders rewritten -------------
        let mut rewritten = line.clone();
        for (name, default) in placeholders(line) {
            let whole_with_default = default
                .as_ref()
                .map(|d| format!("{{{{ {name} | default('{d}') }}}}"))
                .unwrap_or_default();
            let whole_plain = format!("{{{{ {name} }}}}");

            let setting_key = name.strip_prefix(prefix).unwrap_or(&name).to_string();
            if setting_key == "VERSION" || setting_key == "VERSIONS" {
                refusals.push(Refusal {
                    service: id.into(),
                    line: *line_no,
                    text: line.clone(),
                    why: "the version reaches a fragment only through {{ image }}".into(),
                });
                continue;
            }
            if name == "DOCKER_DEFAULT_NETWORK" {
                rewritten = rewritten.replace(&whole_plain, "{{ network }}");
                continue;
            }

            let replacement = format!("{{{{ settings.{setting_key} }}}}");
            if !whole_with_default.is_empty() {
                rewritten = rewritten.replace(&whole_with_default, &replacement);
            }
            rewritten = rewritten.replace(&whole_plain, &replacement);

            if seen_settings.insert(setting_key.clone()) && is_main {
                let embedded_default = embedded.get(name.as_str()).map(|v| v.to_string());
                pkg.settings.push(Setting {
                    key: setting_key,
                    is_secret: Env::is_secret(&name),
                    default: default.or(embedded_default),
                });
            }
        }
        out.push_str(&rewritten);
        out.push('\n');
    }

    // Trailing blank lines are noise in a fragment that gets concatenated.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Rewrite a config template's placeholders and harvest what they name.
///
/// The first version of this copied config files **verbatim**, and the
/// equivalence test found it: `redis.conf.tpl` reads `{{ REDIS_PASSWORD }}` and
/// `elasticsearch.yml.tpl` reads `{{ ELASTIC_SECURITY }}`, neither of which any
/// manifest declared — so the renderer refused the package at the last possible
/// moment, correctly.
///
/// Worth recording what those two were doing before. Neither name matches
/// `template::PREFIXES`, so the old renderer left them alone as
/// `${REDIS_PASSWORD}` and `${ELASTIC_SECURITY:-false}` for Compose to
/// interpolate — and these are **config files**, not compose files, so nothing
/// ever did. Redis is harmless because the line is commented out, which
/// `connect.rs` already documents as the reason it publishes no password;
/// Elasticsearch's was not read at all, because no template mounted the file.
/// Packaging them is the first time either becomes a setting.
fn absorb_config(
    prefix: &str,
    contents: &str,
    pkg: &mut Package,
    embedded: &BTreeMap<&str, &str>,
) -> String {
    let mut out = contents.to_string();
    for (name, default) in contents.lines().flat_map(placeholders) {
        let key = name.strip_prefix(prefix).unwrap_or(&name).to_string();
        let with_default = default
            .as_ref()
            .map(|d| format!("{{{{ {name} | default('{d}') }}}}"))
            .unwrap_or_default();
        let plain = format!("{{{{ {name} }}}}");
        let replacement = format!("{{{{ settings.{key} }}}}");

        if !with_default.is_empty() {
            out = out.replace(&with_default, &replacement);
        }
        out = out.replace(&plain, &replacement);

        if !pkg.settings.iter().any(|s| s.key == key) {
            pkg.settings.push(Setting {
                key,
                is_secret: Env::is_secret(&name),
                default: default.or_else(|| embedded.get(name.as_str()).map(|v| v.to_string())),
            });
        }
    }
    out
}

/// Config files the app renders on every generate and no template ever mounts.
///
/// Measured, not assumed: `commands.rs`'s `RENDERED` table writes
/// `generated/configs/postgres.conf` and `generated/configs/elasticsearch.yml`
/// each time, and grepping both compose templates for `configs/` returns
/// nothing. So those two services have started against image defaults for as
/// long as the templates have existed, and every line in those two files has
/// been dead — which is precisely the failure `RENDERED`'s own comment says the
/// list exists to prevent, arriving from the other side.
///
/// Fixed here rather than in `skeleton/`, on purpose. Faz 1 must not change
/// what the running app renders; the packages are where this becomes true, and
/// the templates are deleted in Faz 6 anyway.
///
/// Returns the file handle, where it belongs inside the container, and — for
/// Postgres — the command that makes it read the thing. That third field is the
/// part guessing would have missed: Postgres reads its configuration out of
/// PGDATA unless told otherwise, so a bare mount would have left the file
/// ignored a second time, in a new place, looking fixed.
///
/// Both were verified against running containers rather than reasoned about.
/// Postgres reports `max_connections=200` against a default of 100 and
/// `autovacuum_max_workers=5` against 3; Elasticsearch reports
/// `node.name=stackvo-es-node-1` and `thread_pool.write.queue_size=1000`,
/// neither of which exists anywhere but that file — and it starts cleanly with
/// the mount alongside the `environment:` keys that overlap it, which was the
/// live question.
fn orphaned_config(id: &str) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    match id {
        "postgres" => Some((
            "postgres_conf",
            "/etc/postgresql/postgresql.conf",
            Some("postgres -c config_file=/etc/postgresql/postgresql.conf"),
        )),
        "elasticsearch" => Some((
            "elasticsearch_yml",
            "/usr/share/elasticsearch/config/elasticsearch.yml",
            None,
        )),
        _ => None,
    }
}

/// Splice the mount — and where needed the command — into a fragment.
fn adopt_orphaned_config(id: &str, pkg: &mut Package, refusals: &mut Vec<Refusal>) {
    let Some((name, target, command)) = orphaned_config(id) else {
        return;
    };
    if pkg.files.iter().any(|f| f.name == name) {
        // The template grew a mount of its own. Then this table is stale and
        // saying so is better than adding a second mount of the same file.
        refusals.push(Refusal {
            service: id.into(),
            line: 0,
            text: name.into(),
            why: "template now mounts this config — drop it from orphaned_config".into(),
        });
        return;
    }

    let mut lines: Vec<String> = pkg.fragment.lines().map(str::to_string).collect();

    // After the last item of the `volumes:` block, so it sits with its kind.
    let Some(start) = lines.iter().position(|l| l.trim_start() == "volumes:") else {
        refusals.push(Refusal {
            service: id.into(),
            line: 0,
            text: "volumes:".into(),
            why: "fragment has no volumes block to mount the config into".into(),
        });
        return;
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| !l.trim_start().starts_with("- "))
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    lines.insert(end, format!("  - \"{{{{ file.{name} }}}}:{target}:ro\""));

    if let Some(cmd) = command {
        let anchor = lines
            .iter()
            .position(|l| l.trim_start().starts_with("container_name:"))
            .map(|i| i + 1)
            .unwrap_or(0);
        lines.insert(anchor, format!("command: {cmd}"));
    }

    pkg.fragment = lines.join("\n");
    pkg.fragment.push('\n');
    pkg.files.push(FileMount {
        name: name.into(),
        template: String::new(),
        contents: String::new(),
        target: target.into(),
    });
}

/// `docker.elastic.co/elasticsearch/elasticsearch` → (registry, repository).
///
/// A leading segment is a registry only if it looks like a host — it has a dot
/// or a colon. `minio/minio` is a Hub user, not a host, and treating it as one
/// would produce an image reference that resolves nowhere.
fn split_registry(reference: &str) -> (Option<String>, String) {
    match reference.split_once('/') {
        Some((head, rest)) if head.contains('.') || head.contains(':') => {
            (Some(head.to_string()), rest.to_string())
        }
        _ => (None, reference.to_string()),
    }
}

/// Traefik labels, made per-instance.
///
/// Router and service names carry the service id today, which is exactly the
/// collision two instances would hit: both would declare
/// `traefik.http.routers.phpmyadmin`, and Traefik resolves a duplicate by
/// keeping one — silently.
fn rewrite_label(id: &str, prefix: &str, label: &str, pkg: &mut Package) -> String {
    let host_placeholder = format!("{{{{ {prefix}URL }}}}.{{{{ DEFAULT_TLD_SUFFIX }}}}");
    let mut out = label.trim().to_string();

    if out.contains(&host_placeholder) {
        out = out.replace(&host_placeholder, "{{ instance.domain }}");
    }
    out = out
        .replace(&format!("routers.{id}."), "routers.{{ instance.slug }}.")
        .replace(&format!("services.{id}."), "services.{{ instance.slug }}.");

    // `…loadbalancer.server.port=9001` says which port the browser reaches.
    if let Some(port) = out
        .rsplit_once("loadbalancer.server.port=")
        .and_then(|(_, p)| p.trim_matches('"').parse::<u16>().ok())
    {
        pkg.url_port = pkg
            .ports
            .iter()
            .find(|p| p.container == port)
            .map(|p| p.name.clone());
    }
    out
}

// ---------------------------------------------------------------- emitting

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

/// `admin-uis`, not `adminUis` — a directory name should not be camelCase.
fn category_dir(category: &str) -> String {
    let mut out = String::new();
    for (i, ch) in category.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn emit(
    package: &Package,
    out_root: &Path,
    shape: Option<&connect::Shape>,
    preserved: &mut usize,
) -> std::io::Result<usize> {
    let dir = out_root
        .join("packages")
        .join(category_dir(&package.category))
        .join(&package.id);

    // ---- package.json ---------------------------------------------------
    let mut p = String::new();
    p.push_str("{\n  \"apiVersion\": \"stackvo.dev/package/v1\",\n");
    let _ = writeln!(p, "  \"service\": \"{}\",", package.id);
    let _ = writeln!(
        p,
        "  \"category\": \"{}\",",
        category_dir(&package.category)
    );
    let _ = writeln!(
        p,
        "  \"name\": {{ \"en\": \"{}\" }},",
        json_escape(&display_name(&package.id))
    );
    let _ = writeln!(p, "  \"maintainer\": \"stackvo\",");
    let _ = writeln!(
        p,
        "  \"recommendedVersion\": \"{}\",",
        package.default_version
    );
    let _ = write!(
        p,
        "  \"legacyEnvPrefix\": \"{}\"\n}}\n",
        prefix_of(&package.id)
    );
    write_file(&dir.join("package.json"), &p)?;

    // ---- one directory per version --------------------------------------
    let mut written = 0;
    for version in &package.versions {
        let vdir = dir.join("versions").join(version);

        let compose_name = "compose.yml.tpl";
        let compose_path = vdir.join(compose_name);
        let fragment = match std::fs::read_to_string(&compose_path) {
            Ok(existing) => {
                if existing != package.fragment {
                    *preserved += 1;
                }
                existing
            }
            Err(_) => {
                write_file(&compose_path, &package.fragment)?;
                package.fragment.clone()
            }
        };
        let compose_sha = sha256_hex(fragment.as_bytes());

        // Files and the fragment are written only where none exists.
        //
        // The converter derives one config and one fragment per service and
        // copies them into every version directory, and versions genuinely
        // differ: `innodb_log_file_size` and `skip-character-set-client-handshake`
        // are both gone in MySQL 9, and either one makes mysqld exit on first
        // boot. Those two were fixed in the packages repository against a
        // running container; overwriting them on the next run would put the
        // catalogue back to a state where picking 9.4 gives you a service that
        // never starts.
        //
        // So the tree wins for content, exactly as it does for `support`. What
        // this program owns is seeding a version directory that is not there
        // yet — and `preserved` says how much it left alone, because silence
        // here would read as "regenerated".
        let mut file_shas = Vec::new();
        for f in &package.files {
            let path = vdir.join(&f.template);
            let contents = match std::fs::read_to_string(&path) {
                Ok(existing) => {
                    if existing != f.contents {
                        *preserved += 1;
                    }
                    existing
                }
                Err(_) => {
                    write_file(&path, &f.contents)?;
                    f.contents.clone()
                }
            };
            file_shas.push(sha256_hex(contents.as_bytes()));
        }

        for c in &package.companions {
            let path = vdir.join(format!("companion.{}.yml.tpl", c.name));
            if !path.exists() {
                write_file(&path, &c.fragment)?;
            }
        }

        let manifest = manifest_json(
            package,
            version,
            &compose_sha,
            &file_shas,
            shape,
            existing_support(&vdir),
        );

        // Read back through the client's own parser before it is written.
        //
        // The converter builds this JSON by hand, so "it looks right" is a
        // review and not a check. `pkg::parse` is the code that will refuse it
        // on somebody's machine — a cross-reference that names a port nobody
        // declared, a path that walks out of the package, a moving tag — and
        // running it here means a manifest this tree cannot install never
        // reaches the tree at all.
        if let Err(e) = pkg::parse(&manifest) {
            return Err(std::io::Error::other(format!(
                "the manifest this converter built for {}@{version} is one the client \
                 would refuse: {}",
                package.id, e.message
            )));
        }

        write_file(&vdir.join("manifest.json"), &manifest)?;
        written += 1;
    }

    Ok(written)
}

fn display_name(id: &str) -> String {
    match id {
        "mysql" => "MySQL",
        "mariadb" => "MariaDB",
        "postgres" => "PostgreSQL",
        "mongo" => "MongoDB",
        "mongo-express" => "Mongo Express",
        "cassandra" => "Apache Cassandra",
        "redis" => "Redis",
        "valkey" => "Valkey",
        "memcached" => "Memcached",
        "rabbitmq" => "RabbitMQ",
        "kafka" => "Apache Kafka",
        "kafbat" => "Kafbat UI",
        "elasticsearch" => "Elasticsearch",
        "kibana" => "Kibana",
        "meilisearch" => "Meilisearch",
        "typesense" => "Typesense",
        "minio" => "MinIO",
        "grafana" => "Grafana",
        "mailhog" => "MailHog",
        "mailpit" => "Mailpit",
        "blackfire" => "Blackfire",
        "phpmyadmin" => "phpMyAdmin",
        "adminer" => "Adminer",
        "pgadmin" => "pgAdmin",
        "phpcacheadmin" => "phpCacheAdmin",
        other => other,
    }
    .to_string()
}

/// Which services may run more than one version at once.
///
/// Not derived, and it should not be: a package that is bound to one subdomain
/// cannot have two instances until subdomains are derived per instance too, and
/// that is a decision about the product rather than a fact about the template.
/// The report's §12 states this list and this is the same one.
fn allows_multiple(id: &str, subdomain: Option<&String>) -> bool {
    let _ = subdomain;
    // A subdomain used to be the disqualifier, and it is not any more.
    //
    // Twelve packages were single-instance because two of them would have asked
    // Traefik for the same `Host()` rule — and Traefik answers that by picking
    // one, silently, so the second instance would have looked installed and
    // never responded. `instances::Instance::domain` now derives the name per
    // instance (`phpmyadmin-5-2.stackvo.loc` beside `phpmyadmin.stackvo.loc`),
    // and the router name was already per instance, so the collision is gone
    // and with it the reason.
    //
    // `blackfire` stays single, and for a different reason that has not moved:
    // it is one probe with one server-side credential pair, and a second copy
    // profiling the same account is not a second environment.
    id != "blackfire"
}

/// The `support` block of a manifest already at this path, if there is one.
///
/// This converter reads templates; whether a series is still maintained is a
/// fact about the world and it has no way to know. `tools/eol.mjs` in the
/// packages repository measures it against endoflife.date and writes the answer
/// back — the first run of that corrected twenty of a hundred and one versions,
/// three of which were this app's own shipped defaults.
///
/// So a regenerate must not overturn it. Preserving what is there makes the two
/// tools cooperative rather than a loop: this one owns everything derived from a
/// template, that one owns the one field neither a template nor a checkout can
/// answer.
fn existing_support(vdir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(vdir.join("manifest.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let support = value.get("support")?;
    serde_json::to_string(support).ok()
}

fn manifest_json(
    pkg: &Package,
    version: &str,
    compose_sha: &str,
    file_shas: &[String],
    shape: Option<&connect::Shape>,
    support: Option<String>,
) -> String {
    let mut m = String::new();
    m.push_str("{\n  \"apiVersion\": \"stackvo.dev/package/v1\",\n");
    let _ = writeln!(m, "  \"service\": \"{}\",", pkg.id);
    let _ = writeln!(m, "  \"version\": \"{version}\",");

    // image
    m.push_str("  \"image\": {\n");
    if let Some(reg) = &pkg.image_registry {
        let _ = writeln!(m, "    \"registry\": \"{reg}\",");
    }
    let _ = writeln!(m, "    \"repository\": \"{}\",", pkg.image_repository);
    let _ = writeln!(
        m,
        "    \"tag\": \"{}{}\"",
        json_escape(version),
        json_escape(&pkg.image_tag_suffix)
    );
    m.push_str("  },\n");

    // capabilities
    let caps = capabilities_of(&pkg.id);
    let _ = writeln!(
        m,
        "  \"capabilities\": [{}],",
        caps.iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let _ = writeln!(
        m,
        "  \"instancing\": {{ \"multiple\": {}, \"identity\": \"version\" }},",
        allows_multiple(&pkg.id, pkg.subdomain.as_ref())
    );

    // ports
    m.push_str("  \"ports\": [\n");
    let primary_port = shape.and_then(|s| {
        pkg.ports
            .iter()
            .find(|p| p.container == s.container_port)
            .map(|p| p.name.clone())
    });
    for (i, port) in pkg.ports.iter().enumerate() {
        let comma = if i + 1 == pkg.ports.len() { "" } else { "," };
        let primary = primary_port.as_deref() == Some(port.name.as_str());
        let _ = writeln!(
            m,
            "    {{ \"name\": \"{}\", \"container\": {}, \"preferred\": {}, \"protocol\": \"tcp\"{}, \"primary\": {} }}{}",
            port.name,
            port.container,
            port.preferred,
            port.legacy_key
                .as_ref()
                .map(|k| format!(", \"legacyKey\": \"{k}\""))
                .unwrap_or_default(),
            primary,
            comma
        );
    }
    m.push_str("  ],\n");

    // volumes
    m.push_str("  \"volumes\": [\n");
    for (i, v) in pkg.volumes.iter().enumerate() {
        let comma = if i + 1 == pkg.volumes.len() { "" } else { "," };
        let _ = writeln!(
            m,
            "    {{ \"name\": \"{}\", \"container\": \"{}\", \"purgeable\": true }}{}",
            v.name,
            json_escape(&v.container),
            comma
        );
    }
    m.push_str("  ],\n");

    // files
    m.push_str("  \"files\": [\n");
    for (i, f) in pkg.files.iter().enumerate() {
        let comma = if i + 1 == pkg.files.len() { "" } else { "," };
        let sha = file_shas.get(i).cloned().unwrap_or_default();
        let _ = writeln!(
            m,
            "    {{ \"name\": \"{}\", \"template\": \"{}\", \"target\": \"{}\", \"mode\": \"0444\", \"sha256\": \"{}\" }}{}",
            f.name,
            f.template,
            json_escape(&f.target),
            sha,
            comma
        );
    }
    m.push_str("  ],\n");

    // settings
    m.push_str("  \"settings\": [\n");
    for (i, s) in pkg.settings.iter().enumerate() {
        let comma = if i + 1 == pkg.settings.len() { "" } else { "," };
        let kind = if s.is_secret { "secret" } else { "string" };
        let default = s
            .default
            .as_ref()
            .map(|d| format!(", \"default\": \"{}\"", json_escape(d)))
            .unwrap_or_default();
        let _ = writeln!(
            m,
            "    {{ \"key\": \"{}\", \"type\": \"{kind}\"{default} }}{comma}",
            s.key
        );
    }
    m.push_str("  ],\n");

    // connection
    match shape {
        Some(s) => {
            let port_name = primary_port.clone().unwrap_or_else(|| "main".into());
            m.push_str("  \"connection\": {\n");
            let _ = writeln!(m, "    \"scheme\": \"{}\",", s.scheme);
            let _ = write!(m, "    \"port\": \"{port_name}\"");
            let strip = |key: &str| key.trim_start_matches(&prefix_of(&pkg.id)).to_string();
            if let Some(k) = s.user_key {
                let _ = write!(m, ",\n    \"userSetting\": \"{}\"", strip(k));
            }
            if let Some(u) = s.default_user {
                let _ = write!(m, ",\n    \"defaultUser\": \"{u}\"");
            }
            if let Some(k) = s.password_key {
                let _ = write!(m, ",\n    \"passwordSetting\": \"{}\"", strip(k));
            }
            if let Some(k) = s.database_key {
                let _ = write!(m, ",\n    \"databaseSetting\": \"{}\"", strip(k));
            }
            if let Some(d) = s.default_database {
                let _ = write!(m, ",\n    \"defaultDatabase\": \"{d}\"");
            }
            if s.scheme == "mongodb" {
                m.push_str(",\n    \"options\": { \"authSource\": \"admin\" }");
            }
            m.push_str("\n  },\n");
        }
        None => m.push_str("  \"connection\": null,\n"),
    }

    // url
    match (&pkg.subdomain, &pkg.url_port) {
        (Some(sub), Some(port)) => {
            let _ = writeln!(
                m,
                "  \"url\": {{ \"subdomain\": \"{sub}\", \"port\": \"{port}\" }},"
            );
        }
        _ => m.push_str("  \"url\": null,\n"),
    }

    // dependsOn
    let deps = dependencies_of(&pkg.id);
    if deps.is_empty() {
        m.push_str("  \"dependsOn\": [],\n");
    } else {
        m.push_str("  \"dependsOn\": [\n");
        for (i, (cap, service, required)) in deps.iter().enumerate() {
            let comma = if i + 1 == deps.len() { "" } else { "," };
            let narrow = service
                .map(|s| format!(", \"service\": \"{s}\""))
                .unwrap_or_default();
            let _ = writeln!(
                m,
                "    {{ \"capability\": \"{cap}\"{narrow}, \"required\": {required} }}{comma}"
            );
        }
        m.push_str("  ],\n");
    }

    // health
    if let Some(health) = health_json(health_of(&pkg.id)) {
        let _ = writeln!(m, "  \"health\": {health},");
    }

    // companions
    if !pkg.companions.is_empty() {
        m.push_str("  \"companions\": [\n");
        for (i, c) in pkg.companions.iter().enumerate() {
            let comma = if i + 1 == pkg.companions.len() {
                ""
            } else {
                ","
            };
            let reg = c
                .image_registry
                .as_ref()
                .map(|r| format!("\"registry\": \"{r}\", "))
                .unwrap_or_default();
            let ports = c
                .ports
                .iter()
                .map(|p| {
                    format!(
                        "{{ \"name\": \"{}\", \"container\": {}, \"preferred\": {} }}",
                        p.name, p.container, p.preferred
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let volumes = c
                .volumes
                .iter()
                .map(|v| {
                    format!(
                        "{{ \"name\": \"{}\", \"container\": \"{}\" }}",
                        v.name,
                        json_escape(&v.container)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let health = health_json(companion_health_of(&c.name))
                .map(|h| format!("\"health\": {h}, "))
                .unwrap_or_default();
            let _ = writeln!(
                m,
                "    {{ \"name\": \"{}\", \"image\": {{ {reg}\"repository\": \"{}\", \"tag\": \"{}\" }}, \"ports\": [{ports}], \"volumes\": [{volumes}], {health}\"compose\": {{ \"file\": \"companion.{}.yml.tpl\", \"sha256\": \"{}\" }} }}{comma}",
                c.name,
                c.image_repository,
                json_escape(&c.image_tag),
                c.name,
                sha256_hex(c.fragment.as_bytes())
            );
        }
        m.push_str("  ],\n");
    }

    let _ = writeln!(
        m,
        "  \"compose\": {{ \"file\": \"compose.yml.tpl\", \"sha256\": \"{compose_sha}\" }},"
    );
    // `supported` only when nothing has measured it yet — see `existing_support`.
    let _ = writeln!(
        m,
        "  \"support\": {}\n}}",
        support.unwrap_or_else(|| "{ \"status\": \"supported\" }".into())
    );
    m
}

/// What a service can be used AS. Matched by `dependsOn`, so an admin UI can
/// ask for "something that speaks sql" instead of naming mysql and being wrong
/// on a machine running mariadb.
fn capabilities_of(id: &str) -> Vec<&'static str> {
    match id {
        "mysql" | "mariadb" => vec!["sql", "mysql-protocol"],
        "postgres" => vec!["sql", "postgres-protocol"],
        "mongo" => vec!["document-store"],
        "cassandra" => vec!["wide-column"],
        "redis" | "valkey" => vec!["cache", "redis-protocol"],
        "memcached" => vec!["cache"],
        "rabbitmq" => vec!["queue", "amqp"],
        "kafka" => vec!["queue", "kafka-protocol"],
        "elasticsearch" => vec!["search", "elasticsearch-protocol"],
        "meilisearch" | "typesense" => vec!["search"],
        "minio" => vec!["object-store", "s3"],
        "mailhog" | "mailpit" => vec!["smtp", "mail-catcher"],
        "grafana" | "kibana" => vec!["dashboard"],
        _ => vec!["admin-ui"],
    }
}

/// The dependency graph, completed.
///
/// `env.schema.json`'s own note says its version is 3 entries out of 20 and
/// names a `prometheus` that does not exist (C-16). Stated by capability so a
/// stack running MariaDB satisfies phpMyAdmin without a second row.
fn dependencies_of(id: &str) -> Vec<(&'static str, Option<&'static str>, bool)> {
    match id {
        "kibana" => vec![("search", Some("elasticsearch"), true)],
        "kafbat" => vec![("queue", Some("kafka"), true)],
        "mongo-express" => vec![("document-store", Some("mongo"), true)],
        "phpmyadmin" => vec![("mysql-protocol", None, false)],
        "pgadmin" => vec![("postgres-protocol", None, false)],
        "adminer" => vec![("sql", None, false)],
        "phpcacheadmin" => vec![("cache", None, false)],
        _ => vec![],
    }
}

/// What "up" means, per service, as a command that exists inside that image.
///
/// Nothing derives this: the templates carried a healthcheck for two services
/// out of twenty-five, so for the other twenty-three there was nothing to
/// convert. It is a table for the same reason `capabilities_of` is one.
///
/// **Every command here was measured, not chosen.** `command -v` was run inside
/// each catalogue image on this machine before a line was written, because the
/// failure mode is silent and expensive: a test naming a binary the image does
/// not ship never succeeds, and a container that is permanently unhealthy is
/// strictly worse than one that never claimed to be checkable — `depends_on:
/// condition: service_healthy` waits for it forever. The measurement is what
/// `examples/health_probe.rs` re-runs.
///
/// Four results from that sweep decided four lines:
///
/// - `typesense`, `memcached` — **no** `curl`, `wget` or `nc`. Both have `bash`,
///   so the check is bash's own `/dev/tcp`, which is a socket and not a program.
/// - `mailhog`, `mailpit`, `pgadmin`, `kafbat`, `mongo-express` — `wget` but no
///   `curl`. BusyBox `wget` has no `-f`, so the URL check is `-O /dev/null` and
///   the exit code.
/// - `mongo` — `mongosh` is present in **5.0 as well**, so the shell rename is
///   not the version split it looks like and one line covers six versions.
/// - `mariadb` — ships `healthcheck.sh`, which knows about `innodb_initialized`.
///   `mysqladmin ping` is also there and would answer sooner than the server is
///   actually usable.
///
/// A service with no entry declares no healthcheck, and that is a statement:
/// see `blackfire`, which is a probe with no readiness surface to ask.
fn health_of(id: &str) -> Option<(Vec<&'static str>, &'static str, u32, Option<&'static str>)> {
    // (test, interval, retries, startPeriod)
    let row: (Vec<&'static str>, &'static str, u32, Option<&'static str>) = match id {
        // `-h 127.0.0.1` is not decoration. Both engines run a temporary,
        // socket-only server while they build the datadir, and a check that
        // reaches it over the socket reports healthy through the whole of
        // first boot — which is the exact window `--wait` exists to cover.
        "mysql" => (
            vec!["CMD", "mysqladmin", "ping", "-h", "127.0.0.1"],
            "10s",
            12,
            Some("30s"),
        ),
        "mariadb" => (
            vec!["CMD", "healthcheck.sh", "--connect", "--innodb_initialized"],
            "10s",
            12,
            Some("30s"),
        ),
        "postgres" => (
            vec!["CMD", "pg_isready", "-h", "127.0.0.1"],
            "10s",
            12,
            Some("30s"),
        ),
        "mongo" => (
            vec![
                "CMD",
                "mongosh",
                "--quiet",
                "--eval",
                "db.adminCommand('ping')",
            ],
            "10s",
            12,
            Some("20s"),
        ),
        "cassandra" => (
            vec!["CMD-SHELL", "cqlsh -e 'describe cluster'"],
            "15s",
            20,
            Some("90s"),
        ),
        "redis" => (vec!["CMD", "redis-cli", "ping"], "10s", 10, Some("5s")),
        "valkey" => (vec!["CMD", "valkey-cli", "ping"], "10s", 10, Some("5s")),
        "memcached" => (
            vec![
                "CMD-SHELL",
                "timeout 2 bash -c '</dev/tcp/127.0.0.1/11211'",
            ],
            "10s",
            10,
            Some("5s"),
        ),
        "rabbitmq" => (
            vec!["CMD", "rabbitmq-diagnostics", "-q", "ping"],
            "15s",
            12,
            Some("40s"),
        ),
        "kafka" => (
            vec![
                "CMD-SHELL",
                "kafka-broker-api-versions --bootstrap-server 127.0.0.1:9092",
            ],
            "15s",
            15,
            Some("45s"),
        ),
        "elasticsearch" => (
            vec![
                "CMD-SHELL",
                "curl -fsS -o /dev/null http://127.0.0.1:9200/_cluster/health",
            ],
            "15s",
            15,
            Some("45s"),
        ),
        "kibana" => (
            vec![
                "CMD-SHELL",
                "curl -fsS -o /dev/null http://127.0.0.1:5601/api/status",
            ],
            "15s",
            20,
            Some("60s"),
        ),
        "meilisearch" => (
            vec!["CMD-SHELL", "curl -fsS -o /dev/null http://127.0.0.1:7700/health"],
            "10s",
            10,
            Some("10s"),
        ),
        "typesense" => (
            vec!["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/127.0.0.1/8108'"],
            "10s",
            10,
            Some("10s"),
        ),
        // MinIO's own answer. `mc ready local` is in the image and reports the
        // cluster rather than the process, which is the difference that matters
        // on a first boot that is formatting a disk.
        "minio" => (vec!["CMD", "mc", "ready", "local"], "10s", 12, Some("10s")),
        "grafana" => (
            vec![
                "CMD-SHELL",
                "curl -fsS -o /dev/null http://127.0.0.1:3000/api/health",
            ],
            "10s",
            12,
            Some("20s"),
        ),
        "mailhog" => (
            vec![
                "CMD-SHELL",
                "wget -q -O /dev/null http://127.0.0.1:8025/api/v2/messages",
            ],
            "10s",
            10,
            Some("5s"),
        ),
        "mailpit" => (vec!["CMD", "/mailpit", "readyz"], "10s", 10, Some("5s")),
        "phpmyadmin" => (
            vec!["CMD-SHELL", "curl -fsS -o /dev/null http://127.0.0.1:80/"],
            "15s",
            8,
            Some("15s"),
        ),
        "phpcacheadmin" => (
            vec!["CMD-SHELL", "curl -fsS -o /dev/null http://127.0.0.1:80/"],
            "15s",
            8,
            Some("15s"),
        ),
        "adminer" => (
            vec!["CMD-SHELL", "curl -fsS -o /dev/null http://127.0.0.1:8080/"],
            "15s",
            8,
            Some("10s"),
        ),
        "pgadmin" => (
            vec![
                "CMD-SHELL",
                "wget -q -O /dev/null http://127.0.0.1:80/misc/ping",
            ],
            "15s",
            12,
            Some("30s"),
        ),
        "kafbat" => (
            vec![
                "CMD-SHELL",
                "wget -q -O /dev/null http://127.0.0.1:8080/actuator/health",
            ],
            "15s",
            15,
            Some("40s"),
        ),
        // Not an HTTP check, and the probe is why. mongo-express turns basic
        // auth on by default, so every unauthenticated request is a 401 and
        // `wget` exits 1 — measured: it sat unhealthy for the full 420s budget
        // with `"1: wget: server returned error: HTTP/1.1 401 Unauthorized"`.
        // The credentials are settings, and a manifest's `health` block cannot
        // read settings (it is written by the app, not substituted like a
        // fragment). So the question this asks is the weaker one it can answer
        // honestly: the server is listening.
        "mongo-express" => (
            vec!["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/127.0.0.1/8081'"],
            "15s",
            10,
            Some("20s"),
        ),
        // `blackfire` is deliberately absent: the agent is an outbound probe
        // with no readiness endpoint, and inventing one would be a check that
        // passes for a reason unrelated to whether profiling works.
        _ => return None,
    };
    Some(row)
}

/// The same table, for the one companion in the catalogue.
fn companion_health_of(name: &str) -> Option<(Vec<&'static str>, &'static str, u32, Option<&'static str>)> {
    match name {
        // Not `ruok`, and the probe is why.
        //
        // The four-letter words are the obvious check and ZooKeeper 3.5 turned
        // them off by default; `cp-zookeeper` answers "ruok is not executed
        // because it is not in the whitelist" and `nc` exits 1 forever. Setting
        // `ZOOKEEPER_4LW_COMMANDS_WHITELIST=srvr,ruok` did **not** re-enable it
        // — measured on this machine, both ways — so the fix would have been an
        // env var that did nothing next to a check that never passed.
        //
        // `zookeeper-shell` is in the image and would answer properly, at the
        // cost of a JVM every ten seconds. What this companion has to be, for
        // the broker that waits on it, is reachable on its client port — so
        // that is what is asked. The broker's own check is the one that proves
        // the pair actually works, and it needs ZooKeeper-backed metadata to
        // pass.
        "zookeeper" => Some((
            vec!["CMD-SHELL", "timeout 2 bash -c '</dev/tcp/127.0.0.1/2181'"],
            "10s",
            12,
            Some("15s"),
        )),
        _ => None,
    }
}

/// A `health` block, as JSON, or nothing.
fn health_json(
    row: Option<(Vec<&'static str>, &'static str, u32, Option<&'static str>)>,
) -> Option<String> {
    let (test, interval, retries, start) = row?;
    let args = test
        .iter()
        .map(|a| format!("\"{}\"", json_escape(a)))
        .collect::<Vec<_>>()
        .join(", ");
    let start = start
        .map(|s| format!(", \"startPeriod\": \"{s}\""))
        .unwrap_or_default();
    Some(format!(
        "{{ \"test\": [{args}], \"interval\": \"{interval}\", \"retries\": {retries}{start} }}"
    ))
}

// ---------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("usage: cargo run --example build_packages -- --out <packages repo>");
            std::process::exit(2);
        });

    let embedded: BTreeMap<&str, &str> = stackvo_desktop_lib::config::EMBEDDED
        .iter()
        .copied()
        .collect();
    let schema = env_schema();
    let shapes = connect::shapes();
    // No workspace: `read_template` falls back to the copy compiled in, which
    // is exactly the set being converted.
    let nowhere = Path::new("/nonexistent");

    let mut refusals: Vec<Refusal> = Vec::new();
    let mut services = 0;
    let mut versions = 0;
    // Which services used the older of the two port-key spellings. Reported
    // rather than merely resolved: `HOST_PORT_<ID>` has no default anywhere in
    // `EMBEDDED`, so those ports could not be changed from the UI at all, and
    // the count is the size of a bug the package format closes.
    let mut legacy_host_port: Vec<String> = Vec::new();
    // Files the tree already had, differing from what this run would write.
    let mut preserved = 0usize;

    for (id, category) in schema.service_catalog() {
        // One service's refusals must not silence the next one's. The first
        // version of this loop checked `refusals.is_empty()` and so reported
        // MinIO and stopped — twelve services never ran, and the summary read
        // like a complete conversion of thirteen.
        let before = refusals.len();
        let relative = format!("core/templates/services/{id}/docker-compose.{id}.tpl");
        let Some(text) = skeleton::read_template(nowhere, &relative) else {
            refusals.push(Refusal {
                service: id.clone(),
                line: 0,
                text: relative,
                why: "catalog entry has no template".into(),
            });
            continue;
        };

        let mut pkg = match parse(&id, &category, &text, &embedded) {
            Ok(p) => p,
            Err(mut e) => {
                refusals.append(&mut e);
                continue;
            }
        };

        // The subdomain is a value, not a placeholder — the template reads
        // SERVICE_<ID>_URL and EMBEDDED is where that lives.
        pkg.subdomain = embedded
            .get(format!("{}URL", prefix_of(&id)).as_str())
            .map(|v| v.to_string());

        // Config templates: one non-compose file per service directory, which
        // is how the tree is laid out. Reading the directory rather than
        // carrying commands.rs's RENDERED table means the percona/my.cnf that
        // table names — and which does not exist — cannot be copied forward.
        // Read them first, then rewrite: `absorb_config` needs `&mut pkg` to
        // record the settings it finds, and the loop above is already borrowing
        // `pkg.files`.
        let mut raw: Vec<(usize, String, String)> = Vec::new();
        for (index, file) in pkg.files.iter().enumerate() {
            let dir = format!("core/templates/services/{id}/");
            let found = skeleton::overridable()
                .into_iter()
                .find(|p| p.starts_with(&dir) && !p[dir.len()..].starts_with("docker-compose."));
            match found.and_then(|p| {
                let name = p[dir.len()..].to_string();
                skeleton::read_template(nowhere, &p).map(|t| (name, t))
            }) {
                Some((name, contents)) => raw.push((index, name, contents)),
                None => refusals.push(Refusal {
                    service: id.clone(),
                    line: 0,
                    text: file.name.clone(),
                    why: "compose mounts a config file with no template in the service directory"
                        .into(),
                }),
            }
        }
        let prefix = prefix_of(&id);
        for (index, name, contents) in raw {
            let rewritten = absorb_config(&prefix, &contents, &mut pkg, &embedded);
            pkg.files[index].template = format!("files/{name}");
            pkg.files[index].contents = rewritten;
        }

        if refusals.len() != before {
            continue;
        }

        if pkg.ports.iter().any(|p| {
            p.legacy_key
                .as_deref()
                .is_some_and(|k| k.starts_with("HOST_PORT_"))
        }) {
            legacy_host_port.push(id.clone());
        }

        let shape = shapes.iter().find(|s| s.service == id);
        match emit(&pkg, &out, shape, &mut preserved) {
            Ok(n) => {
                services += 1;
                versions += n;
                let mult = if allows_multiple(&pkg.id, pkg.subdomain.as_ref()) {
                    "multi"
                } else {
                    "single"
                };
                println!(
                    "  {:<14} {:>3} versions  {:>2} ports  {:>2} volumes  {:>2} files  {:>2} settings  {mult}",
                    pkg.id,
                    n,
                    pkg.ports.len(),
                    pkg.volumes.len(),
                    pkg.files.len(),
                    pkg.settings.len()
                );
            }
            Err(e) => {
                eprintln!("  {id}: {e}");
                std::process::exit(1);
            }
        }
    }

    if !refusals.is_empty() {
        eprintln!("\n{} refusal(s):\n", refusals.len());
        for r in &refusals {
            eprintln!("  {r}");
        }
        eprintln!(
            "\nNothing was written for the services above. Each line is either a template this \
             converter has not been taught, or a template that should change."
        );
        std::process::exit(1);
    }

    println!(
        "\n{services} services, {versions} version directories → {}",
        out.display()
    );
    if preserved > 0 {
        println!(
            "\n{preserved} file(s) in the tree differ from what this run derives and were left \
             alone. Versions differ for real reasons — MySQL 9 drops two directives 8.0 needs — \
             and the tree is where those live."
        );
    }
    if !legacy_host_port.is_empty() {
        println!(
            "\n{} service(s) read their host port from the HOST_PORT_<ID> family, which has no \
             default in config::EMBEDDED — so the Services sheet offered no port row and the \
             number could not be changed from the app at all. `ports[].preferred` is now the \
             single place it lives:\n  {}",
            legacy_host_port.len(),
            legacy_host_port.join(", ")
        );
    }
}
