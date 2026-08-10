//! Reading somebody else's `docker-compose.yml` into a StackVo project.
//!
//! The other half of P2-12. The *folder* half shipped with adoption in Sprint 4
//! — point at a directory and `detect` infers runtime, server and document root
//! from `artisan`, `wp-config.php`, `composer.json` and `package.json`. What
//! that cannot see is everything the person who wrote the compose file already
//! decided: the PHP version, the domain, and — the part with no equivalent in
//! any marker file — **which backing services the project needs**.
//!
//! A `docker-compose.yml` with `mysql:8.0` and `redis:7.2` is a statement about
//! the stack. Adoption alone adopts the project and leaves the developer to
//! rediscover that by reading a stack trace about a refused connection.
//!
//! ## Docker parses it, not this module
//!
//! `docker compose config --format json` is the reference implementation of the
//! Compose specification. It resolves YAML anchors, `extends`, `.env`
//! interpolation and profile selection, and normalises every shorthand — a
//! label list becomes a map, a `"8080:80"` string becomes a structured port, a
//! relative bind becomes an absolute path.
//!
//! The alternative was a YAML dependency, and that was the wrong trade twice
//! over. `serde_yaml` is archived by its author, and `deny.toml` in this
//! repository says a *direct* dependency going unmaintained still fails the
//! build — waving that through for a convenience parser is exactly the habit
//! that file exists to prevent. And a hand-rolled parser would be wrong on real
//! files: `xdebug::generated_services` parses by indentation only because it
//! reads a file this project generated itself, whose shape is fixed. A user's
//! compose file is arbitrary YAML written by somebody else.
//!
//! Compose is already a hard preflight requirement here, so this adds no new
//! dependency at all. When it is missing the answer is an error saying so, not
//! a half-parse.
//!
//! ## What is mapped, and what is refused
//!
//! Three kinds of compose service, told apart because they become three
//! different things:
//!
//! * **The application** — the one with a `build:`, or a `php`/`node` image.
//!   Becomes the manifest.
//! * **A web server** — nginx, apache, caddy. Becomes the manifest's `server`
//!   field, *not* a StackVo service: StackVo runs the web server inside the
//!   project container, so importing it as a sidecar would produce two.
//! * **A backing service** — mysql, redis, and the rest of the catalog.
//!   Becomes a preset entry.
//!
//! Anything else is **named in `unmapped`**, never dropped. A migration that
//! silently ignores the one service the project actually needs is worse than
//! one that refuses, because it looks finished.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Image repository → StackVo catalog id.
///
/// Keyed on the repository with its registry and namespace stripped, so
/// `docker.elastic.co/elasticsearch/elasticsearch` and `elasticsearch` are the
/// same entry. Bitnami and friends publish under their own namespace with the
/// upstream name intact, which is why the last path segment is the key.
///
/// Deliberately not a fuzzy match. A project running `ghcr.io/acme/redis-shim`
/// is not running Redis, and guessing that it is would enable a service the
/// developer never asked for.
const IMAGE_TO_SERVICE: [(&str, &str); 24] = [
    ("mysql", "mysql"),
    ("mysql-server", "mysql"),
    ("mariadb", "mariadb"),
    ("postgres", "postgres"),
    ("postgresql", "postgres"),
    ("mongo", "mongo"),
    ("mongodb", "mongo"),
    ("cassandra", "cassandra"),
    ("redis", "redis"),
    ("valkey", "valkey"),
    ("memcached", "memcached"),
    ("rabbitmq", "rabbitmq"),
    ("kafka", "kafka"),
    ("elasticsearch", "elasticsearch"),
    ("meilisearch", "meilisearch"),
    ("typesense", "typesense"),
    // MinIO publishes the server as `minio/minio` and the client as
    // `minio/mc`. Only the first is a service; `mc` is a one-shot CLI a
    // compose file runs to create buckets, and adopting it as a service would
    // leave a container that exits immediately marked as failing to start.
    ("minio", "minio"),
    ("kibana", "kibana"),
    ("grafana", "grafana"),
    ("mailhog", "mailhog"),
    ("mailpit", "mailpit"),
    ("phpmyadmin", "phpmyadmin"),
    ("adminer", "adminer"),
    ("pgadmin4", "pgadmin"),
];

/// Images that mean "this project is served by X", not "add a service".
const IMAGE_TO_SERVER: [(&str, &str); 5] = [
    ("nginx", "nginx"),
    ("httpd", "apache"),
    ("apache", "apache"),
    ("caddy", "caddy"),
    ("frankenphp", "frankenphp"),
];

/// One backing service the compose file implies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedService {
    /// StackVo catalog id.
    pub id: String,
    /// The version read off the image tag, when it is one StackVo can pin.
    pub version: Option<String>,
    /// The compose service it came from, so the mapping can be checked.
    pub from: String,
    pub image: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    /// The compose file this was read from.
    pub source: String,
    /// The compose service taken to be the application, if one was found.
    pub app_service: Option<String>,
    pub runtime: Option<String>,
    pub server: Option<String>,
    pub php_version: Option<String>,
    pub node_version: Option<String>,
    pub document_root: Option<String>,
    pub domain: Option<String>,
    pub port: Option<u16>,
    /// PHP extensions read from the Dockerfile the compose file builds.
    pub extensions: Vec<String>,
    pub services: Vec<MappedService>,
    /// Compose services with no StackVo equivalent, each with its image.
    /// Named, never dropped.
    pub unmapped: Vec<String>,
    /// What each conclusion was read from.
    pub evidence: Vec<String>,
}

// -------------------------------------------------------------- pure logic

/// The repository and tag of an image reference.
///
/// Registry-aware: a host before the first `/` may carry a `:port`, which is
/// not a tag. `localhost:5000/mysql:8.0` has to yield `("mysql", "8.0")`, and
/// splitting on the last `:` without checking for a `/` after it gets that
/// right while splitting on the first does not.
pub fn split_image(image: &str) -> (String, Option<String>) {
    // Digest pins carry their own `@sha256:…`; the tag before it, if any, is
    // what names the version.
    let image = image.split('@').next().unwrap_or(image);

    let (repo, tag) = match image.rfind(':') {
        Some(i) if !image[i + 1..].contains('/') => (&image[..i], Some(&image[i + 1..])),
        _ => (image, None),
    };

    let last = repo.rsplit('/').next().unwrap_or(repo);
    (last.to_lowercase(), tag.map(str::to_string))
}

/// The version a tag states, if it states one.
///
/// `8.0` and `7.2-alpine` yield a version; `latest`, `alpine` and `stable` do
/// not. Returning None is the honest answer — importing `latest` as a pinned
/// version would invent a decision the source file did not make.
pub fn version_from_tag(tag: &str) -> Option<String> {
    let head = tag.split('-').next()?;
    if head.is_empty() || !head.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if !head.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some(head.to_string())
}

/// A domain out of a Traefik router rule.
///
/// Only `Host(...)`. A rule combining Host with PathPrefix still yields the
/// host; a rule with no Host at all yields nothing rather than a guess.
pub fn domain_from_rule(rule: &str) -> Option<String> {
    let start = rule.find("Host(")? + "Host(".len();
    let rest = &rule[start..];
    let end = rest.find(')')?;
    let inside = &rest[..end];

    inside
        .split(',')
        .next()?
        .trim()
        .trim_matches(|c| c == '`' || c == '\'' || c == '"')
        .trim()
        .to_string()
        .into()
}

/// `docker-php-ext-install` and `pecl install` lines in a Dockerfile.
///
/// Read as words rather than parsed: the two commands take a space-separated
/// list, and the flags they accept (`-j$(nproc)`) are recognisable enough to
/// skip. `pecl install redis-6.0.2` carries its version in the argument, which
/// is dropped — the manifest names extensions, and the pin comes from
/// `contracts/php-extensions.json` for the PHP version in play.
pub fn extensions_from_dockerfile(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // A RUN can continue over many lines with a trailing backslash, so the
    // whole file is flattened before looking for the commands.
    let flat = text.replace("\\\n", " ").replace("\\\r\n", " ");

    for line in flat.lines() {
        for marker in ["docker-php-ext-install", "pecl install"] {
            let Some(at) = line.find(marker) else {
                continue;
            };
            let rest = &line[at + marker.len()..];

            for word in rest.split_whitespace() {
                // The command list ends at the next shell operator.
                if word.starts_with("&&") || word.starts_with(';') || word.starts_with('|') {
                    break;
                }
                if word.starts_with('-') || word.starts_with('$') {
                    continue;
                }
                let name = word.split('-').next().unwrap_or(word);
                let clean: String = name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if clean.is_empty() || out.contains(&clean) {
                    continue;
                }
                out.push(clean);
            }
        }
    }

    out
}

/// The document root, from where the web server is told to look.
///
/// A container path under the application's `working_dir` is turned back into
/// the relative path a manifest wants: `/var/www/html/public` under a
/// `working_dir` of `/var/www/html` is `public`.
pub fn relative_root(container_path: &str, working_dir: &str) -> Option<String> {
    let trimmed = container_path.trim_end_matches('/');
    let base = working_dir.trim_end_matches('/');
    if base.is_empty() || trimmed == base {
        return None;
    }
    trimmed
        .strip_prefix(base)
        .map(|rest| rest.trim_start_matches('/').to_string())
        .filter(|s| !s.is_empty() && !s.contains('/'))
}

/// Turn `docker compose config --format json` into a migration.
///
/// Split from the process call so the mapping can be tested against captured
/// output rather than against a Docker daemon.
pub fn from_config(
    source: &str,
    config: &serde_json::Value,
    dockerfile: Option<&str>,
) -> Migration {
    let mut out = Migration {
        source: source.to_string(),
        ..Default::default()
    };

    let Some(services) = config.get("services").and_then(|v| v.as_object()) else {
        return out;
    };

    let mut app_working_dir: Option<String> = None;

    for (name, service) in services {
        let image = service
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let builds = service.get("build").is_some();
        let (repo, tag) = split_image(&image);

        // --- the application ------------------------------------------------
        // A `build:` is the strongest signal there is: nobody builds their
        // database. Failing that, an official php/node image.
        let is_app = builds || repo == "php" || repo == "node";
        if is_app && out.app_service.is_none() {
            out.app_service = Some(name.clone());
            out.evidence.push(if builds {
                format!("{name}: has a build context")
            } else {
                format!("{name}: image {image}")
            });

            if repo == "php" {
                out.runtime = Some("php".into());
                out.php_version = tag.as_deref().and_then(version_from_tag);
                // The variant names the server: php:8.2-apache is mod_php.
                if let Some(t) = tag.as_deref() {
                    if t.contains("apache") {
                        out.server = Some("apache".into());
                    }
                }
            } else if repo == "node" {
                out.runtime = Some("node".into());
                out.node_version = tag
                    .as_deref()
                    .and_then(version_from_tag)
                    .map(|v| v.split('.').next().unwrap_or(&v).to_string());
            }

            if let Some(dir) = service.get("working_dir").and_then(|v| v.as_str()) {
                app_working_dir = Some(dir.to_string());
            }

            // A port the application publishes, which for a node project is the
            // one the manifest has to name.
            if let Some(ports) = service.get("ports").and_then(|v| v.as_array()) {
                out.port = ports
                    .iter()
                    .find_map(|p| p.get("target").and_then(|t| t.as_u64()))
                    .and_then(|t| u16::try_from(t).ok());
            }

            out.domain = domain_from_service(service);
            continue;
        }

        // --- a web server ---------------------------------------------------
        // Becomes the manifest's `server`, not a sidecar: StackVo runs the web
        // server inside the project container, so importing it as a service
        // would give the project two.
        if let Some((_, server)) = IMAGE_TO_SERVER.iter().find(|(k, _)| *k == repo) {
            if out.server.is_none() {
                out.server = Some((*server).to_string());
                out.evidence
                    .push(format!("{name}: {image} → server {server}"));
            }
            if out.domain.is_none() {
                out.domain = domain_from_service(service);
            }
            if out.port.is_none() {
                out.port = service
                    .get("ports")
                    .and_then(|v| v.as_array())
                    .and_then(|ports| {
                        ports
                            .iter()
                            .find_map(|p| p.get("target").and_then(|t| t.as_u64()))
                    })
                    .and_then(|t| u16::try_from(t).ok());
            }
            continue;
        }

        // --- a backing service ----------------------------------------------
        if let Some((_, id)) = IMAGE_TO_SERVICE.iter().find(|(k, _)| *k == repo) {
            out.services.push(MappedService {
                id: (*id).to_string(),
                version: tag.as_deref().and_then(version_from_tag),
                from: name.clone(),
                image: image.clone(),
            });
            continue;
        }

        // --- everything else ------------------------------------------------
        // Named, not dropped. A migration that silently ignores the one service
        // the project actually needs is worse than one that refuses, because it
        // looks finished.
        out.unmapped.push(if image.is_empty() {
            name.clone()
        } else {
            format!("{name} ({image})")
        });
    }

    // Two compose services can map to the same catalog id — a project with
    // `mysql` and `mysql-test` needs MySQL once. Keeping both would plan the
    // same .env key twice with different versions.
    out.services
        .sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.from.cmp(&b.from)));
    out.services.dedup_by(|a, b| a.id == b.id);
    out.unmapped.sort();

    for service in &out.services {
        out.evidence.push(format!(
            "{}: {} → {}",
            service.from, service.image, service.id
        ));
    }

    if let Some(working_dir) = app_working_dir.as_deref() {
        out.document_root = document_root_from(services, working_dir);
        if out.document_root.is_some() {
            out.evidence
                .push(format!("document root, relative to {working_dir}"));
        }
    }

    if let Some(text) = dockerfile {
        out.extensions = extensions_from_dockerfile(text);
        if !out.extensions.is_empty() {
            out.evidence
                .push(format!("Dockerfile: {} extension(s)", out.extensions.len()));
        }
        // A `FROM php:8.2-fpm` names the version when compose only said `build:`.
        if out.php_version.is_none() {
            if let Some((runtime, version, server)) = from_line(text) {
                out.runtime.get_or_insert(runtime.to_string());
                match runtime {
                    "php" => out.php_version = version,
                    _ => out.node_version = version,
                }
                if let Some(server) = server {
                    out.server.get_or_insert(server.to_string());
                }
                out.evidence.push("Dockerfile: FROM line".to_string());
            }
        }
    }

    out
}

/// A domain from Traefik labels, or from nginx-proxy's `VIRTUAL_HOST`.
fn domain_from_service(service: &serde_json::Value) -> Option<String> {
    if let Some(labels) = service.get("labels").and_then(|v| v.as_object()) {
        for (key, value) in labels {
            if key.contains("routers") && key.ends_with(".rule") {
                if let Some(domain) = value.as_str().and_then(domain_from_rule) {
                    return Some(domain);
                }
            }
        }
    }

    service
        .get("environment")
        .and_then(|v| v.as_object())
        .and_then(|env| env.get("VIRTUAL_HOST"))
        .and_then(|v| v.as_str())
        .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The document root, from whichever service mounts one under the app's tree.
///
/// A web server container is routinely given `./public:/var/www/html/public`,
/// which names the root more precisely than the application service does.
fn document_root_from(
    services: &serde_json::Map<String, serde_json::Value>,
    working_dir: &str,
) -> Option<String> {
    for service in services.values() {
        let Some(volumes) = service.get("volumes").and_then(|v| v.as_array()) else {
            continue;
        };
        for volume in volumes {
            let Some(target) = volume.get("target").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(relative) = relative_root(target, working_dir) {
                return Some(relative);
            }
        }
    }
    None
}

/// `(runtime, version, server)` from a Dockerfile's first `FROM`.
fn from_line(text: &str) -> Option<(&'static str, Option<String>, Option<&'static str>)> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("FROM ")
            .or_else(|| line.strip_prefix("from "))
        else {
            continue;
        };
        let image = rest.split_whitespace().next()?;
        let (repo, tag) = split_image(image);

        if repo == "php" {
            let server = tag
                .as_deref()
                .filter(|t| t.contains("apache"))
                .map(|_| "apache");
            return Some(("php", tag.as_deref().and_then(version_from_tag), server));
        }
        if repo == "node" {
            let major = tag
                .as_deref()
                .and_then(version_from_tag)
                .map(|v| v.split('.').next().unwrap_or(&v).to_string());
            return Some(("node", major, None));
        }
        if repo == "frankenphp" {
            return Some(("php", None, Some("frankenphp")));
        }
    }
    None
}

/// The preset a migration implies: exactly the services it found, enabled.
///
/// Reuses the Sprint 6 format rather than inventing a second way to say "turn
/// these on" — and gets the same guarantee for free, that there is nowhere in
/// it to put a password read out of somebody else's compose file.
pub fn to_preset(migration: &Migration, name: Option<String>) -> crate::preset::Preset {
    let services = migration
        .services
        .iter()
        .map(|s| {
            (
                s.id.clone(),
                crate::preset::ServicePreset {
                    enabled: true,
                    version: s.version.clone(),
                },
            )
        })
        .collect();

    crate::preset::Preset {
        kind: crate::preset::KIND.to_string(),
        version: crate::preset::VERSION,
        name,
        description: Some(format!("Imported from {}", migration.source)),
        services,
        settings: BTreeMap::new(),
    }
}

// ------------------------------------------------------------------- I/O

/// Ask Docker to resolve the file, and read what comes back.
pub async fn read(path: &Path) -> Result<Migration> {
    if !path.is_file() {
        return Err(Error::not_found(format!("compose file {}", path.display())));
    }

    let output = tokio::process::Command::new("docker")
        .args(["compose", "-f"])
        .arg(path)
        .args(["config", "--format", "json"])
        .output()
        .await
        .map_err(|e| Error::io("running docker compose config", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Compose's own message is the useful one — it names the line. Passing
        // it through beats replacing it with "could not read the file".
        return Err(Error::new(
            Code::InvalidInput,
            format!("Docker could not read this compose file: {}", stderr.trim()),
        )
        .with_hint(crate::hints::COMPOSE_FILE_MUST_BE_VALID));
    }

    let config: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("docker compose config did not return JSON: {e}"),
        )
    })?;

    // A Dockerfile beside the compose file is where the PHP version and the
    // extension list usually are, and compose only reports that it exists.
    let dockerfile = path
        .parent()
        .map(|dir| dir.join("Dockerfile"))
        .and_then(|p| std::fs::read_to_string(p).ok());

    Ok(from_config(
        &path.display().to_string(),
        &config,
        dockerfile.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `docker compose config --format json` on a real file, not
    /// hand-written: the point of going through Docker is that its normalised
    /// shape is what this reads, and a fixture invented here would only prove
    /// the fixture.
    const RESOLVED: &str = r#"{
      "name": "shop",
      "services": {
        "app": {
          "build": { "context": "/w/shop", "dockerfile": "Dockerfile" },
          "working_dir": "/var/www/html",
          "labels": { "traefik.http.routers.shop.rule": "Host(`shop.test`)" },
          "volumes": [
            { "type": "bind", "source": "/w/shop", "target": "/var/www/html", "bind": {} }
          ]
        },
        "web": {
          "image": "nginx:1.25-alpine",
          "ports": [ { "mode": "ingress", "target": 80, "published": "8080", "protocol": "tcp" } ],
          "volumes": [
            { "type": "bind", "source": "/w/shop/public", "target": "/var/www/html/public", "bind": {} }
          ]
        },
        "db":    { "image": "mysql:8.0" },
        "cache": { "image": "redis:7.2-alpine" },
        "weird": { "image": "ghcr.io/acme/thing:2" }
      }
    }"#;

    fn resolved() -> serde_json::Value {
        serde_json::from_str(RESOLVED).unwrap()
    }

    #[test]
    fn a_registry_port_is_not_a_tag() {
        assert_eq!(
            split_image("mysql:8.0"),
            ("mysql".into(), Some("8.0".into()))
        );
        assert_eq!(split_image("redis"), ("redis".into(), None));
        // The `:5000` belongs to the host, not to the image.
        assert_eq!(split_image("localhost:5000/mysql"), ("mysql".into(), None));
        assert_eq!(
            split_image("localhost:5000/mysql:8.0"),
            ("mysql".into(), Some("8.0".into()))
        );
        assert_eq!(
            split_image("docker.elastic.co/elasticsearch/elasticsearch:8.11.3"),
            ("elasticsearch".into(), Some("8.11.3".into()))
        );
        // A digest pin has no version to read.
        assert_eq!(split_image("mysql@sha256:abc").0, "mysql");
    }

    /// `latest` is not a version. Importing it as a pin invents a decision the
    /// source file deliberately did not make.
    #[test]
    fn only_a_tag_that_states_a_version_yields_one() {
        assert_eq!(version_from_tag("8.0"), Some("8.0".into()));
        assert_eq!(version_from_tag("7.2-alpine"), Some("7.2".into()));
        assert_eq!(version_from_tag("8.11.3"), Some("8.11.3".into()));
        assert_eq!(version_from_tag("latest"), None);
        assert_eq!(version_from_tag("alpine"), None);
        assert_eq!(version_from_tag("stable-bookworm"), None);
        assert_eq!(version_from_tag("8-fpm"), Some("8".into()));
    }

    #[test]
    fn a_host_rule_yields_its_domain() {
        assert_eq!(
            domain_from_rule("Host(`shop.test`)").as_deref(),
            Some("shop.test")
        );
        assert_eq!(
            domain_from_rule("Host(`shop.test`) && PathPrefix(`/api`)").as_deref(),
            Some("shop.test")
        );
        assert_eq!(
            domain_from_rule("Host(`a.test`, `b.test`)").as_deref(),
            Some("a.test")
        );
        // No Host clause is no answer, not a guess.
        assert_eq!(domain_from_rule("PathPrefix(`/api`)"), None);
    }

    #[test]
    fn extensions_survive_a_multi_line_run() {
        let dockerfile = "FROM php:8.2-fpm\n\
                          RUN docker-php-ext-install -j$(nproc) pdo_mysql gd zip \\\n\
                          && pecl install redis-6.0.2 \\\n\
                          && docker-php-ext-enable redis\n";
        let found = extensions_from_dockerfile(dockerfile);
        assert_eq!(found, ["pdo_mysql", "gd", "zip", "redis"]);
    }

    #[test]
    fn a_container_path_becomes_a_relative_document_root() {
        assert_eq!(
            relative_root("/var/www/html/public", "/var/www/html").as_deref(),
            Some("public")
        );
        // The mount of the project itself is not a document root.
        assert_eq!(relative_root("/var/www/html", "/var/www/html"), None);
        // Nor is something two levels down, which is a bind of one directory
        // rather than a statement about where the front controller is.
        assert_eq!(relative_root("/var/www/html/a/b", "/var/www/html"), None);
        assert_eq!(relative_root("/srv/other", "/var/www/html"), None);
    }

    #[test]
    fn a_real_compose_project_maps_to_a_stackvo_one() {
        let m = from_config("/w/shop/docker-compose.yml", &resolved(), None);

        assert_eq!(m.app_service.as_deref(), Some("app"));
        assert_eq!(m.domain.as_deref(), Some("shop.test"));
        // nginx becomes the SERVER, not a service: StackVo runs the web server
        // inside the project container, so importing it as a sidecar would give
        // the project two.
        assert_eq!(m.server.as_deref(), Some("nginx"));
        assert_eq!(m.document_root.as_deref(), Some("public"));

        let ids: Vec<&str> = m.services.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["mysql", "redis"]);
        assert_eq!(m.services[0].version.as_deref(), Some("8.0"));
        assert_eq!(m.services[1].version.as_deref(), Some("7.2"));

        // The one nothing could be made of is named, not dropped.
        assert_eq!(m.unmapped, ["weird (ghcr.io/acme/thing:2)"]);
    }

    /// The compose file says `build:` and nothing else; the Dockerfile beside it
    /// is where the version and the extensions actually are.
    #[test]
    fn the_dockerfile_fills_in_what_compose_could_not() {
        let dockerfile = "FROM php:8.3-fpm\nRUN docker-php-ext-install pdo_mysql\n";
        let m = from_config("/w/shop/docker-compose.yml", &resolved(), Some(dockerfile));

        assert_eq!(m.runtime.as_deref(), Some("php"));
        assert_eq!(m.php_version.as_deref(), Some("8.3"));
        assert_eq!(m.extensions, ["pdo_mysql"]);
    }

    /// Two compose services can be the same catalog service. Keeping both would
    /// plan the same .env key twice, with different versions.
    #[test]
    fn two_services_of_one_kind_collapse_to_one() {
        let config = serde_json::json!({
          "services": {
            "db":      { "image": "mysql:8.0" },
            "db_test": { "image": "mysql:8.0" }
          }
        });
        let m = from_config("x", &config, None);
        assert_eq!(m.services.len(), 1);
        assert_eq!(m.services[0].id, "mysql");
    }

    /// Every mapped id has to be one the contract actually knows, or the import
    /// writes a SERVICE_<JUNK>_ENABLE key and a compose profile matching
    /// nothing — CONFLICTS.md C-09, reached by a different road.
    #[test]
    fn every_mapping_target_is_in_the_catalog() {
        let catalog = crate::contracts::env_schema().service_catalog();
        for (image, id) in IMAGE_TO_SERVICE {
            assert!(
                catalog.iter().any(|(known, _)| known == id),
                "{image} maps to `{id}`, which is not in the service catalog"
            );
        }
    }

    #[test]
    fn the_preset_it_produces_enables_exactly_what_was_found() {
        let m = from_config("/w/shop/docker-compose.yml", &resolved(), None);
        let preset = to_preset(&m, Some("shop".into()));

        assert_eq!(preset.kind, crate::preset::KIND);
        assert_eq!(preset.services.len(), 2);
        assert!(preset.services["mysql"].enabled);
        assert_eq!(preset.services["mysql"].version.as_deref(), Some("8.0"));
        // Nothing is turned off by an import: a compose file describes what a
        // project needs, not what the rest of your machine must stop running.
        assert!(preset.services.values().all(|s| s.enabled));
    }

    #[test]
    fn an_empty_or_serviceless_config_is_an_empty_migration() {
        let m = from_config("x", &serde_json::json!({}), None);
        assert!(m.services.is_empty());
        assert!(m.app_service.is_none());
    }
}
