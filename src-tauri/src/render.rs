//! Assembling `docker-compose.dynamic.yml` from the instance table.
//!
//! The last piece of Faz 2 in `docs/servis-market-mimarisi.md`. What
//! `template::render_dynamic_compose` does today is walk a fixed array of
//! twenty-five compiled-in templates and include the ones whose
//! `SERVICE_<ID>_ENABLE` says `true`. This does the same job from the other
//! end: it walks [`instances::Table`] and renders each instance's package
//! fragment.
//!
//! ADR 0002 is untouched and is the reason this is safe to swap: the file has
//! always been rendered from scratch on every run and never edited in place, so
//! changing what it is rendered *from* changes no invariant anybody depends on.
//!
//! ## The render context is the whole of what a fragment can see
//!
//! A fragment gets `image`, `instance.*`, `port.*`, `volume.*`, `file.*`,
//! `settings.*` and `network` — nothing else, and every one of them is built
//! here from the instance and its manifest. That is a security boundary rather
//! than a convenience: `template::render` substitutes any name matching its
//! prefix list, so a downloaded fragment handed the workspace's variables could
//! write somebody's `SERVICE_MYSQL_ROOT_PASSWORD` into an `environment:` line
//! of its own and out to wherever it liked.
//!
//! ## Every fragment passes the policy before it is assembled
//!
//! [`compose_policy::check`] runs on each rendered block, and it runs *here*
//! rather than only in the packages repository's CI because those two ask
//! different questions. That one asks "should we ship this"; this one asks
//! "should this machine run it", and only the second is still standing when a
//! repository has been taken over or a mirror is lying.
//!
//! It runs after substitution on purpose — see that module — which is why this
//! one hands it the exact set of mount sources it produced. After rendering
//! there are no handles left to compare against.

use crate::error::{Code, Error, Result};
use crate::instances::{Instance, Table};
use crate::pkg::{Catalogue, Manifest};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Where an instance's rendered config files are written.
///
/// Per instance, not per service: two versions of one service each mount their
/// own `my.cnf`, and a shared path would mean whichever rendered last decided
/// for both.
pub fn config_dir(root: &Path, instance: &str) -> std::path::PathBuf {
    root.join("generated").join("configs").join(instance)
}

/// The variables one instance's fragment may read.
///
/// Built rather than borrowed. The workspace's own `.env` map is not passed
/// through at any point, which is what stops a fragment from naming a key it
/// was never given.
fn context(
    root: &Path,
    instance: &Instance,
    manifest: &Manifest,
    network: &str,
    tld: &str,
    secrets: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();

    vars.insert("image".into(), manifest.image.reference());
    vars.insert("network".into(), network.to_string());
    vars.insert("instance.container".into(), instance.container());
    vars.insert("instance.slug".into(), instance.id.clone());
    vars.insert(
        "instance.aliases".into(),
        format!(
            "[{}]",
            instance
                .aliases()
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    vars.insert(
        "instance.logs".into(),
        crate::paths::to_docker_mount(&instance.logs(root).display().to_string()),
    );
    if let Some(url) = &manifest.url {
        vars.insert("instance.domain".into(), instance.domain(&url.subdomain, tld));
    }

    for port in &manifest.ports {
        // A port with no allocation is a bug in whoever built the instance, and
        // the manifest's preference is the least surprising thing to fall back
        // to — but it is a fallback, not a plan: `ports::allocate` is what makes
        // a number safe to publish.
        let chosen = instance
            .ports
            .get(&port.name)
            .copied()
            .unwrap_or(port.preferred);
        vars.insert(format!("port.{}", port.name), chosen.to_string());
    }

    for volume in &manifest.volumes {
        vars.insert(
            format!("volume.{}", volume.name),
            instance.volume(&volume.name),
        );
    }

    for file in &manifest.files {
        let name = file
            .template
            .strip_prefix("files/")
            .unwrap_or(&file.template);
        vars.insert(
            format!("file.{}", file.name),
            crate::paths::to_docker_mount(
                &config_dir(root, &instance.id)
                    .join(name)
                    .display()
                    .to_string(),
            ),
        );
    }

    // A companion's network name, readable from the *main* fragment.
    //
    // The old templates put both containers in one file and wrote
    // `KAFKA_ZOOKEEPER_CONNECT: zookeeper:2181` — which resolved, because
    // `zookeeper` was the compose service key and Compose gives every key a DNS
    // name. Per instance the key is `kafka-7-5-0-zookeeper`, so that literal
    // resolves to nothing and the broker never connects. The name is derived in
    // exactly one place, and this is a fragment's only way to ask for it.
    for companion in &manifest.companions {
        vars.insert(
            format!("companion.{}.host", companion.name),
            format!("stackvo-{}-{}", instance.id, companion.name),
        );
        for port in &companion.ports {
            vars.insert(
                format!("companion.{}.port.{}", companion.name, port.name),
                port.container.to_string(),
            );
        }
    }

    for setting in &manifest.settings {
        // The value, then what the keystore holds, then the manifest's default.
        // ADR 0010: a secret lives in the keystore and is rendered into
        // `generated/`, which is output rewritten on every run rather than a
        // file anybody keeps.
        let value = instance
            .settings
            .get(&setting.key)
            .cloned()
            .or_else(|| {
                instance
                    .secret_refs
                    .get(&setting.key)
                    .and_then(|reference| secrets(reference))
            })
            .or_else(|| setting.default_text());
        vars.insert(
            format!("settings.{}", setting.key),
            value.unwrap_or_default(),
        );
    }

    vars
}

/// Substitute `{{ name }}` from an exact map.
///
/// Deliberately not [`crate::template::render`]: that one implements the Bash
/// generator's grammar, where an unknown name is *left alone* for Compose to
/// interpolate later. That rule is right for a template this repository ships
/// and wrong for one it downloaded — `${SOME_HOST_VAR}` surviving into the
/// output is a package reading the process environment. Here an unknown name is
/// a refusal.
fn substitute(fragment: &str, vars: &BTreeMap<String, String>, who: &str) -> Result<String> {
    let mut out = String::with_capacity(fragment.len());
    let mut rest = fragment;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find("}}") else {
            return Err(Error::new(
                Code::InvalidManifest,
                format!("{who}: an unclosed {{{{ in the compose fragment"),
            ));
        };
        let name = rest[start + 2..start + len].trim();
        match vars.get(name) {
            Some(value) => out.push_str(value),
            None => {
                return Err(Error::new(
                    Code::InvalidManifest,
                    format!(
                        "{who}: the fragment reads {{{{ {name} }}}}, which its manifest does \
                         not declare — a name this renderer does not know is refused rather \
                         than left for Compose, because leaving it is how a package reads the \
                         process environment"
                    ),
                )
                .with_hint(crate::hints::PACKAGE_CONTENT_CHANGED))
            }
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The mount sources this render produced, and the image it asked for.
///
/// Derived from the context rather than rebuilt, so the policy is comparing
/// against exactly what was substituted. Anything in a `volumes:` line that is
/// not in this set is a path the package wrote itself.
fn permitted(vars: &BTreeMap<String, String>, image: &str) -> crate::compose_policy::Allowed {
    crate::compose_policy::Allowed {
        image: image.to_string(),
        mounts: vars
            .iter()
            .filter(|(name, _)| {
                let bare = name.strip_prefix("companion.").unwrap_or(name);
                bare.starts_with("volume.") || bare.starts_with("file.") || bare == "instance.logs"
            })
            .map(|(_, value)| value.clone())
            .collect(),
    }
}

/// A string, as a YAML double-quoted scalar.
///
/// Every byte of a healthcheck comes out of a manifest, and a manifest is a file
/// somebody else wrote. `[.]` versus `\.` in E-2's router rule is the same
/// lesson from the other side: a value that reaches YAML unescaped is a value
/// that can stop being a value. The schema constrains the *shape* of `health`
/// and says nothing about the bytes inside a test argument.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `healthcheck:`, written by the app rather than by the package.
///
/// The fragment may not declare one — `contracts/compose-policy.json` refuses
/// the key — for the same reason it may not declare `image:`: this is the field
/// `depends_on: condition: service_healthy` reads, and a second author for it is
/// a second answer to "is this up".
///
/// It is emitted **after** [`crate::compose_policy::check`], beside `profiles:`,
/// because it is not package text and checking the app's own output against a
/// policy written for downloaded text proves nothing.
///
/// The list form is not a style choice. Compose runs the string form through a
/// shell, so `test: "pg_isready; curl evil"` would be two commands; the schema
/// requires an array and this writes one.
fn healthcheck(health: &crate::pkg::Health) -> String {
    let mut out = String::from("healthcheck:\n");
    let _ = writeln!(
        out,
        "  test: [{}]",
        health
            .test
            .iter()
            .map(|arg| quoted(arg))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (key, value) in [
        ("interval", health.interval.as_deref()),
        ("timeout", health.timeout.as_deref()),
        ("start_period", health.start_period.as_deref()),
    ] {
        if let Some(value) = value {
            let _ = writeln!(out, "  {key}: {}", quoted(value));
        }
    }
    if let Some(retries) = health.retries {
        let _ = writeln!(out, "  retries: {retries}");
    }
    out
}

/// One service block, at the indentation the assembled file wants.
fn block(key: &str, profiles: &[String], body: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "  {key}:");
    let _ = writeln!(
        out,
        "    profiles: [{}]",
        profiles
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "    {line}");
        }
    }
    out
}

/// A rendered config file, waiting to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub path: std::path::PathBuf,
    pub contents: String,
}

/// Everything one pass produces.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub compose: String,
    pub configs: Vec<ConfigFile>,
}

/// Assemble the file.
///
/// `secrets` resolves a `keychain:` reference; the caller owns the keystore and
/// this module never touches it. Passing it in is what keeps this testable
/// without one.
pub fn dynamic_compose(
    root: &Path,
    table: &Table,
    catalogue: &dyn Catalogue,
    network: &str,
    tld: &str,
    secrets: &dyn Fn(&str) -> Option<String>,
) -> Result<Rendered> {
    let enabled: Vec<&Instance> = table.instances.iter().filter(|i| i.enabled).collect();

    // `services:` with nothing under it is not an empty mapping, it is null,
    // and Compose rejects the whole file — taking Traefik down with it, because
    // these files are merged. Switching everything off is a thing the app lets
    // anybody do. The spelling is the one the existing renderer uses.
    let mut compose = String::from(if enabled.is_empty() {
        "services: {}\n\n"
    } else {
        "services:\n\n"
    });

    let mut configs = Vec::new();
    let mut volumes: Vec<String> = Vec::new();

    for instance in enabled {
        let who = format!("{}@{}", instance.service, instance.version);
        let manifest = catalogue
            .manifest(&instance.service, &instance.version)
            .ok_or_else(|| {
                Error::not_found(format!("package {who}"))
                    .with_hint(crate::hints::PACKAGE_NOT_INSTALLED)
            })?;

        let vars = context(root, instance, &manifest, network, tld, secrets);

        let fragment = shipped(catalogue, instance, &manifest.compose.file, &who)?;
        let mut body = substitute(&fragment, &vars, &who)?;
        crate::compose_policy::check(&who, &body, &permitted(&vars, &manifest.image.reference()))?;
        if let Some(health) = &manifest.health {
            body.push_str(&healthcheck(health));
        }
        compose.push_str(&block(
            &instance.id,
            &["services".to_string(), instance.id.clone()],
            &body,
        ));
        compose.push('\n');

        for volume in &manifest.volumes {
            volumes.push(instance.volume(&volume.name));
        }

        for file in &manifest.files {
            let name = file
                .template
                .strip_prefix("files/")
                .unwrap_or(&file.template);
            let template = shipped(catalogue, instance, &file.template, &who)?;
            configs.push(ConfigFile {
                path: config_dir(root, &instance.id).join(name),
                contents: substitute(&template, &vars, &who)?,
            });
        }

        // A companion is named against the instance, so two Kafkas get two
        // Zookeepers rather than one they both depend on.
        for companion in &manifest.companions {
            let id = format!("{}-{}", instance.id, companion.name);
            let mut side = vars.clone();
            side.insert("companion.image".into(), companion.image.reference());
            side.insert(
                "companion.instance.container".into(),
                format!("stackvo-{id}"),
            );
            side.insert("companion.instance.slug".into(), id.clone());
            side.insert(
                "companion.instance.aliases".into(),
                format!("[\"stackvo-{id}\"]"),
            );
            side.insert(
                "companion.instance.logs".into(),
                crate::paths::to_docker_mount(
                    &root
                        .join("logs")
                        .join("services")
                        .join(&id)
                        .display()
                        .to_string(),
                ),
            );
            for port in &companion.ports {
                side.insert(
                    format!("companion.port.{}", port.name),
                    port.preferred.to_string(),
                );
            }
            for volume in &companion.volumes {
                let name = format!("stackvo-{id}-{}", volume.name);
                side.insert(format!("companion.volume.{}", volume.name), name.clone());
                volumes.push(name);
            }

            let fragment = shipped(catalogue, instance, &companion.compose.file, &who)?;
            let mut body = substitute(&fragment, &side, &who)?;
            crate::compose_policy::check(
                &who,
                &body,
                &permitted(&side, &companion.image.reference()),
            )?;
            if let Some(health) = &companion.health {
                body.push_str(&healthcheck(health));
            }
            compose.push_str(&block(
                &id,
                &["services".to_string(), instance.id.clone()],
                &body,
            ));
            compose.push('\n');
        }
    }

    if !volumes.is_empty() {
        volumes.sort();
        volumes.dedup();
        compose.push_str("volumes:\n");
        for volume in volumes {
            let _ = writeln!(compose, "  {volume}:");
        }
    }

    Ok(Rendered { compose, configs })
}

/// A file the package ships, or a refusal naming it.
///
/// The catalogue is asked rather than a path being joined here, so a package
/// whose bytes live somewhere other than a directory still renders — and so the
/// path check that guards the read stays in one place.
fn shipped(
    catalogue: &dyn Catalogue,
    instance: &Instance,
    file: &str,
    who: &str,
) -> Result<String> {
    catalogue
        .file(&instance.service, &instance.version, file)
        .ok_or_else(|| {
            Error::not_found(format!("{who}: {file}"))
                .with_hint(crate::hints::PACKAGE_CONTENT_CHANGED)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{PackageRef, SCHEMA_VERSION};
    use crate::pkg;

    fn no_secrets(_: &str) -> Option<String> {
        None
    }

    fn instance(service: &str, version: &str, primary: bool) -> Instance {
        Instance {
            id: crate::instances::slug(service, version).unwrap(),
            service: service.into(),
            version: version.into(),
            package: PackageRef {
                source: "official".into(),
                sha256: "0".repeat(64),
                installed_at: "2026-08-11T09:00:00Z".into(),
            },
            enabled: true,
            primary,
            ports: BTreeMap::new(),
            volumes: BTreeMap::new(),
            settings: BTreeMap::new(),
            secret_refs: BTreeMap::new(),
        }
    }

    /// A package tree on disk, because that is what the renderer reads.
    fn plant(root: &Path, service: &str, version: &str, fragment: &str) {
        plant_with(root, service, version, fragment, "")
    }

    /// The same, plus whatever else the manifest should say — `"health": {…},`
    /// and nothing else needs it yet, so it is a string rather than a builder.
    fn plant_with(root: &Path, service: &str, version: &str, fragment: &str, extra: &str) {
        let dir = root
            .join("packages/databases")
            .join(service)
            .join("versions")
            .join(version);
        std::fs::create_dir_all(dir.join("files")).unwrap();
        std::fs::write(dir.join("compose.yml.tpl"), fragment).unwrap();
        std::fs::write(dir.join("files/my.cnf.tpl"), "port = {{ port.main }}\n").unwrap();

        let manifest = format!(
            r#"{{"apiVersion": "{}", "service": "{service}", "version": "{version}",
                "image": {{"repository": "{service}", "tag": "{version}"}},
                "instancing": {{"multiple": true}},
                "ports": [{{"name": "main", "container": 3306, "preferred": 3306, "primary": true}}],
                "volumes": [{{"name": "data", "container": "/var/lib/mysql"}}],
                "files": [{{"name": "my_cnf", "template": "files/my.cnf.tpl",
                            "target": "/etc/my.cnf", "sha256": "{}"}}],
                "settings": [{{"key": "DATABASE", "type": "string", "default": "stackvo"}},
                             {{"key": "ROOT_PASSWORD", "type": "secret", "default": "root"}}],
                {extra}
                "compose": {{"file": "compose.yml.tpl", "sha256": "{}"}},
                "support": {{"status": "supported"}}}}"#,
            pkg::API_VERSION,
            pkg::sha256_hex(b"port = {{ port.main }}\n"),
            pkg::sha256_hex(fragment.as_bytes())
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(
            root.join("packages/databases")
                .join(service)
                .join("package.json"),
            format!(
                r#"{{"apiVersion": "{}", "service": "{service}", "category": "databases",
                    "name": {{"en": "{service}"}}, "recommendedVersion": "{version}"}}"#,
                pkg::API_VERSION
            ),
        )
        .unwrap();
    }

    const FRAGMENT: &str = "\
image: \"{{ image }}\"
container_name: \"{{ instance.container }}\"
environment:
  MYSQL_DATABASE: \"{{ settings.DATABASE }}\"
  MYSQL_ROOT_PASSWORD: \"{{ settings.ROOT_PASSWORD }}\"
volumes:
  - \"{{ volume.data }}:/var/lib/mysql\"
  - \"{{ file.my_cnf }}:/etc/my.cnf:ro\"
ports:
  - \"{{ port.main }}:3306\"
networks:
  {{ network }}:
    aliases: {{ instance.aliases }}
";

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stackvo-render-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn render(root: &Path, table: &Table) -> Result<Rendered> {
        let tree = pkg::Tree::open(root)?;
        dynamic_compose(
            root,
            table,
            &tree,
            "stackvo-net",
            "stackvo.loc",
            &no_secrets,
        )
    }

    /// The whole point: two versions of one service in one file, sharing
    /// nothing that matters.
    #[test]
    fn two_versions_of_one_service_render_side_by_side() {
        let root = scratch("dual");
        plant(&root, "mysql", "8.0", FRAGMENT);
        plant(&root, "mysql", "9.4", FRAGMENT);

        let mut a = instance("mysql", "8.0", true);
        a.ports.insert("main".into(), 3306);
        a.volumes.insert("data".into(), "stackvo-mysql-data".into());
        let mut b = instance("mysql", "9.4", false);
        b.ports.insert("main".into(), 3316);

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![a, b],
            },
        )
        .unwrap();

        assert!(out.compose.contains("  mysql-8-0:"));
        assert!(out.compose.contains("  mysql-9-4:"));
        assert!(out
            .compose
            .contains("container_name: \"stackvo-mysql-8-0\""));
        assert!(out
            .compose
            .contains("container_name: \"stackvo-mysql-9-4\""));
        assert!(out.compose.contains("\"3306:3306\""));
        assert!(out.compose.contains("\"3316:3306\""));

        // Separate volumes, both declared. The adopted one keeps its old name.
        assert!(out.compose.contains("  stackvo-mysql-data:"));
        assert!(out.compose.contains("  stackvo-mysql-9-4-data:"));

        // And only the primary answers to the pre-package name.
        assert!(out
            .compose
            .contains("aliases: [\"stackvo-mysql-8-0\", \"stackvo-mysql\"]"));
        assert!(out.compose.contains("aliases: [\"stackvo-mysql-9-4\"]"));
    }

    /// Each instance renders its own config, into its own directory.
    #[test]
    fn config_files_are_rendered_per_instance() {
        let root = scratch("configs");
        plant(&root, "mysql", "8.0", FRAGMENT);
        plant(&root, "mysql", "9.4", FRAGMENT);

        let mut a = instance("mysql", "8.0", true);
        a.ports.insert("main".into(), 3306);
        let mut b = instance("mysql", "9.4", false);
        b.ports.insert("main".into(), 3316);

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![a, b],
            },
        )
        .unwrap();

        assert_eq!(out.configs.len(), 2);
        let find = |id: &str| {
            out.configs
                .iter()
                .find(|c| c.path.to_string_lossy().contains(id))
                .expect("a config for this instance")
        };
        assert_eq!(find("mysql-8-0").contents, "port = 3306\n");
        assert_eq!(find("mysql-9-4").contents, "port = 3316\n");
    }

    /// A disabled instance contributes nothing — not a block, not a volume.
    #[test]
    fn a_disabled_instance_is_absent_from_the_file() {
        let root = scratch("disabled");
        plant(&root, "mysql", "8.0", FRAGMENT);

        let mut a = instance("mysql", "8.0", true);
        a.enabled = false;

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![a],
            },
        )
        .unwrap();
        assert_eq!(out.compose, "services: {}\n\n");
    }

    /// Everything off must still be a file Compose accepts, because these are
    /// merged and a rejected one takes Traefik down with it.
    #[test]
    fn an_empty_table_renders_a_mapping_and_not_a_null() {
        let root = scratch("empty");
        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![],
            },
        )
        .unwrap();
        assert!(out.compose.starts_with("services: {}"));
    }

    /// The security boundary. A fragment naming anything its manifest did not
    /// declare is refused rather than passed through to Compose.
    #[test]
    fn a_fragment_reading_a_name_it_was_not_given_is_refused() {
        let root = scratch("escape");
        plant(
            &root,
            "mysql",
            "8.0",
            "image: \"{{ image }}\"\nenvironment:\n  LEAK: \"{{ SERVICE_POSTGRES_PASSWORD }}\"\n",
        );

        let err = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap_err();

        assert!(
            err.message.contains("SERVICE_POSTGRES_PASSWORD"),
            "{}",
            err.message
        );
        assert!(
            err.message.contains("process environment"),
            "{}",
            err.message
        );
    }

    /// A secret is resolved through the caller's keystore, never from the file.
    #[test]
    fn a_secret_comes_from_the_keystore_and_the_default_is_the_fallback() {
        let root = scratch("secret");
        plant(&root, "mysql", "8.0", FRAGMENT);

        let mut a = instance("mysql", "8.0", true);
        a.ports.insert("main".into(), 3306);
        a.secret_refs.insert(
            "ROOT_PASSWORD".into(),
            "keychain:stackvo/mysql-8-0/ROOT_PASSWORD".into(),
        );

        let table = Table {
            schema_version: SCHEMA_VERSION,
            instances: vec![a],
        };
        let tree = pkg::Tree::open(&root).unwrap();

        let held = |reference: &str| {
            (reference == "keychain:stackvo/mysql-8-0/ROOT_PASSWORD").then(|| "hunter2".to_string())
        };
        let out =
            dynamic_compose(&root, &table, &tree, "stackvo-net", "stackvo.loc", &held).unwrap();
        assert!(out.compose.contains("MYSQL_ROOT_PASSWORD: \"hunter2\""));

        // With nothing in the keystore the manifest's first-boot default stands
        // in, which is what makes a fresh install start at all.
        let out = dynamic_compose(
            &root,
            &table,
            &tree,
            "stackvo-net",
            "stackvo.loc",
            &no_secrets,
        )
        .unwrap();
        assert!(out.compose.contains("MYSQL_ROOT_PASSWORD: \"root\""));
    }

    /// A package that asks for the host's Docker socket does not render.
    ///
    /// The end-to-end proof that the policy is wired in, rather than a module
    /// with tests of its own that nothing calls. The packages repository would
    /// have refused this at publish time; this is the half that is still there
    /// when the package did not come from there.
    #[test]
    fn a_package_reaching_for_the_docker_socket_does_not_render() {
        let root = scratch("socket");
        plant(
            &root,
            "mysql",
            "8.0",
            "image: \"{{ image }}\"\nvolumes:\n  - \"/var/run/docker.sock:/var/run/docker.sock\"\n",
        );

        let err = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap_err();

        assert_eq!(err.code, Code::Forbidden);
        assert!(err.message.contains("docker.sock"), "{}", err.message);
    }

    /// And one that renames its own image, which is how a package would slip
    /// past the registry allowlist and the digest pin.
    #[test]
    fn a_package_that_names_its_own_image_does_not_render() {
        let root = scratch("image");
        plant(
            &root,
            "mysql",
            "8.0",
            "image: \"attacker/backdoor:latest\"\ncontainer_name: \"{{ instance.container }}\"\n",
        );

        let err = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap_err();

        assert_eq!(err.code, Code::Forbidden);
        assert!(err.message.contains("attacker/backdoor"), "{}", err.message);
    }

    /// An instance whose package is gone is a refusal that names it, not a
    /// compose file with a hole in it.
    #[test]
    fn an_instance_with_no_package_is_refused_by_name() {
        let root = scratch("gone");
        let err = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap_err();
        assert_eq!(err.code, Code::NotFound);
        assert!(err.message.contains("mysql@8.0"), "{}", err.message);
    }

    // ---------------------------------------------------------- healthcheck

    /// S-11. The manifest declares readiness and the compose file carries it —
    /// the half of that item that lives in this repository. Every package in
    /// the catalogue shipped with `health` empty, so `--wait` and
    /// `condition: service_healthy` both meant "the process exists".
    #[test]
    fn a_declared_healthcheck_reaches_the_compose_file() {
        let root = scratch("health");
        plant_with(
            &root,
            "mysql",
            "8.0",
            FRAGMENT,
            r#""health": {"test": ["CMD", "mysqladmin", "ping", "-h", "127.0.0.1"],
                          "interval": "10s", "retries": 12, "startPeriod": "30s"},"#,
        );

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap()
        .compose;

        assert!(out.contains("    healthcheck:\n"), "{out}");
        assert!(
            out.contains(r#"      test: ["CMD", "mysqladmin", "ping", "-h", "127.0.0.1"]"#),
            "{out}"
        );
        assert!(out.contains("      interval: \"10s\""), "{out}");
        assert!(out.contains("      start_period: \"30s\""), "{out}");
        assert!(out.contains("      retries: 12"), "{out}");
    }

    /// And a package that declares none gets none. The absence has to survive:
    /// a default healthcheck would be one that passes for a reason nobody
    /// chose, which is the state this item exists to leave.
    #[test]
    fn a_package_with_no_health_gets_no_healthcheck_key() {
        let root = scratch("nohealth");
        plant(&root, "mysql", "8.0", FRAGMENT);

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap()
        .compose;

        assert!(!out.contains("healthcheck"), "{out}");
    }

    /// A manifest is a file somebody else wrote, and the schema constrains the
    /// shape of `health` rather than the bytes inside a test argument. A quote
    /// that reached YAML unescaped would close the string and let the rest of
    /// the argument be read as compose keys.
    #[test]
    fn a_quote_in_a_health_argument_cannot_close_the_yaml_string() {
        let root = scratch("healthquote");
        plant_with(
            &root,
            "mysql",
            "8.0",
            FRAGMENT,
            r#""health": {"test": ["CMD-SHELL", "x\"]\nprivileged: true\n#"]},"#,
        );

        let out = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap()
        .compose;

        // One line, everything escaped inside it, and nothing that reads as a
        // key of the service.
        assert!(out.contains(r#"test: ["CMD-SHELL", "x\"]\nprivileged: true\n#"]"#), "{out}");
        assert!(
            !out.lines().any(|l| l.trim_start() == "privileged: true"),
            "{out}"
        );
    }

    /// A fragment may not write the key the app now owns. Two `healthcheck:`
    /// blocks in one service is a duplicate YAML key, and which one wins is
    /// not something either author can see.
    #[test]
    fn a_fragment_declaring_its_own_healthcheck_is_refused() {
        let root = scratch("healthdup");
        plant(
            &root,
            "mysql",
            "8.0",
            &format!("{FRAGMENT}healthcheck:\n  test: [\"CMD\", \"true\"]\n"),
        );

        let err = render(
            &root,
            &Table {
                schema_version: SCHEMA_VERSION,
                instances: vec![instance("mysql", "8.0", true)],
            },
        )
        .unwrap_err();

        assert_eq!(err.code, Code::Forbidden);
        assert!(err.message.contains("healthcheck"), "{}", err.message);
    }
}
