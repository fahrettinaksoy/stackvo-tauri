//! What is this folder, and how should StackVo run it?
//!
//! The gap this closes is narrow and concrete: `project_create` refuses when
//! the directory already exists, so a folder someone cloned into `projects/`
//! could not be adopted at all. On the checkout this was written against, 11 of
//! 21 directories under `projects/` were in exactly that state — real code,
//! sitting unmanaged, because writing `stackvo.json` by hand is the only way in.
//!
//! Lerd auto-detects nine frameworks and Laragon's "Quick app" carries its whole
//! onboarding story; both are the same idea from the other end.
//!
//! ## Evidence, not a verdict
//!
//! Every detection carries the files it was based on and a confidence. That is
//! not decoration. Inferring a document root wrong produces a project that
//! builds, starts, and serves a 404 — a failure with no error attached to it —
//! and the user's only defence is being able to see that the guess came from
//! `index.php` in the root rather than from an `artisan` file that is not there.
//!
//! ## Why the inference is pure
//!
//! `fingerprint` touches the disk; `infer` does not. Everything worth arguing
//! about — does a `composer.json` naming `laravel/framework` outrank an
//! `index.php` in the root, what happens when a repository holds both a PHP API
//! and a Vite front end — is a decision over a struct, tested without a fixture
//! tree on disk.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// The marker files a project directory does or does not have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub artisan: bool,
    pub composer_json: bool,
    pub package_json: bool,
    pub bin_console: bool,
    pub wp_config: bool,
    pub wp_includes: bool,
    pub index_php_root: bool,
    pub index_php_public: bool,
    pub public_dir: bool,
    pub web_dir: bool,
    pub html_dir: bool,
    /// Package names seen in `composer.json` require blocks.
    pub composer_requires: Vec<String>,
    /// Package names seen in `package.json` dependencies.
    pub node_dependencies: Vec<String>,
    /// The `php` constraint from composer.json, verbatim.
    pub php_constraint: Option<String>,
    /// `.nvmrc`, or `engines.node`.
    pub node_constraint: Option<String>,
    /// A `dev`/`start` script exists in package.json.
    pub node_scripts: Vec<String>,
    /// `go.mod` — a marker only a Go module has.
    pub go_mod: bool,
    /// `Cargo.toml` — only Rust.
    pub cargo_toml: bool,
    /// `Gemfile` — only Ruby.
    pub gemfile: bool,
    /// `manage.py` — Django, and nothing else, puts this at the root.
    pub manage_py: bool,
    /// `requirements.txt` or `pyproject.toml` — Python, with less certainty
    /// than the Django marker: other ecosystems' repos occasionally carry one
    /// for tooling.
    pub python_deps: bool,
    /// `deno.json`, `deno.jsonc` or `deno.lock`.
    ///
    /// Deno's own manifest, and a repository carrying one has committed to
    /// Deno resolving its imports — which `node` cannot do, so this is a
    /// certainty rather than a preference between two runtimes that would both
    /// work.
    pub deno_config: bool,
    /// `bun.lock`, `bun.lockb` or `bunfig.toml`.
    ///
    /// Weaker than Deno's marker and deliberately so: a Bun project's
    /// `package.json` is a Node project's `package.json`, and the only thing
    /// separating them is which tool wrote the lockfile. A repository with both
    /// a Bun lockfile and an npm one has been installed both ways, and there is
    /// no honest way to say which the author meant.
    pub bun_lock: bool,
    /// `package-lock.json`, `yarn.lock` or `pnpm-lock.yaml` — an npm-family
    /// lockfile, which is what makes a Bun marker ambiguous rather than
    /// decisive.
    pub npm_lock: bool,
}

/// How sure the inference is, said plainly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// A framework marker only that framework has.
    Certain,
    /// A shape that is almost always what it looks like.
    Likely,
    /// Defaults, because nothing recognisable was there.
    Guess,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub framework: Option<&'static str>,
    pub runtime: &'static str,
    pub server: &'static str,
    pub document_root: Option<String>,
    pub php_version: Option<String>,
    pub node_version: Option<String>,
    pub node_port: Option<u16>,
    pub node_start: Option<String>,
    pub confidence: Confidence,
    /// The files this was read from, so the guess can be checked.
    pub evidence: Vec<String>,
}

// ------------------------------------------------------------- pure logic

fn has(list: &[String], needle: &str) -> bool {
    list.iter().any(|item| item == needle)
}

/// The first `major.minor` in a version constraint like `^8.2` or `>=8.1 <9.0`.
///
/// Constraint syntax is not parsed properly on purpose: the answer only has to
/// pick a supported PHP line, and a resolver would be a dependency plus a
/// source of disagreement with Composer's own.
pub fn first_version(constraint: &str) -> Option<String> {
    let bytes: Vec<char> = constraint.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                i += 1;
            }
            let raw: String = bytes[start..i].iter().collect();
            let mut parts = raw.split('.').filter(|p| !p.is_empty());
            let major = parts.next()?;
            // A bare major (`8`, from `^8`) is not a version StackVo can pin.
            let minor = parts.next()?;
            return Some(format!("{major}.{minor}"));
        }
        i += 1;
    }
    None
}

/// The document root, from whichever convention the directory follows.
fn document_root(print: &Fingerprint) -> Option<String> {
    if print.index_php_public || print.public_dir {
        return Some("public".into());
    }
    if print.web_dir {
        return Some("web".into());
    }
    if print.html_dir {
        return Some("html".into());
    }
    // Serving from the project root is what WordPress and most legacy PHP do.
    // Named explicitly rather than left to the `public` default, which would
    // produce a project that builds, starts and serves nothing.
    if print.index_php_root {
        return Some(".".into());
    }
    None
}

/// Read a fingerprint and say what should run it.
///
/// Order is load-bearing. A Laravel repository has a `package.json` for its
/// front-end assets and a Next.js one may carry a `composer.json` for tooling;
/// deciding on "whichever manifest we noticed first" flips with directory
/// order. Framework markers are checked before generic ones, and PHP before
/// Node, because a PHP framework's Node dependencies are a build step whereas a
/// Node app's PHP dependencies are not a web server.
pub fn infer(print: &Fingerprint) -> Detected {
    let php_version = print.php_constraint.as_deref().and_then(first_version);
    let node_version = print.node_constraint.as_deref().and_then(|c| {
        // `.nvmrc` is often a bare major (`22`), which is what the generator
        // wants, so a major on its own is kept rather than rejected.
        let digits: String = c
            .trim()
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        (!digits.is_empty()).then(|| digits.trim_end_matches('.').to_string())
    });

    let php = |framework: Option<&'static str>,
               doc: &str,
               confidence: Confidence,
               evidence: Vec<String>| Detected {
        framework,
        runtime: "php",
        server: "nginx",
        document_root: Some(doc.to_string()),
        php_version: php_version.clone(),
        node_version: None,
        node_port: None,
        node_start: None,
        confidence,
        evidence,
    };

    // ---- PHP frameworks, by a marker only they have ------------------------

    if print.artisan {
        return php(
            Some("laravel"),
            "public",
            Confidence::Certain,
            vec!["artisan".into()],
        );
    }

    if print.wp_config || print.wp_includes {
        let mut evidence = Vec::new();
        if print.wp_config {
            evidence.push("wp-config.php".into());
        }
        if print.wp_includes {
            evidence.push("wp-includes/".into());
        }
        // WordPress serves from the directory it is installed in.
        return php(Some("wordpress"), ".", Confidence::Certain, evidence);
    }

    if print.bin_console
        && print
            .composer_requires
            .iter()
            .any(|r| r.starts_with("symfony/"))
    {
        return php(
            Some("symfony"),
            "public",
            Confidence::Certain,
            vec!["bin/console".into(), "composer.json".into()],
        );
    }

    // ---- the lang runtimes, by markers only they have ----------------------
    //
    // Placed after the PHP framework markers (which are absolute) and before
    // the generic composer/package heuristics, because a Go or Rust repository
    // never carries `artisan` but often carries a `package.json` for tooling —
    // scored generically, it would adopt as a node project that cannot start.
    let lang = |runtime: &'static str, confidence: Confidence, evidence: &str| Detected {
        framework: None,
        runtime,
        server: "nginx", // ignored for lang runtimes; kept for the struct
        document_root: None,
        php_version: None,
        node_version: None,
        node_port: None,
        node_start: None,
        confidence,
        evidence: vec![evidence.to_string()],
    };

    // Deno first among the JavaScript runtimes, and before Go for the same
    // reason the whole block sits where it does: `deno.json` is Deno's import
    // map and nothing else reads it, so a repository carrying one has already
    // committed. Node would not resolve its imports at all.
    if print.deno_config {
        return lang("deno", Confidence::Certain, "deno.json / deno.lock");
    }
    // Bun only when nothing says npm. A Bun project's `package.json` is a Node
    // project's `package.json`; the lockfile is the only difference, and a
    // repository holding both has been installed both ways. Guessing Bun there
    // would pick a runtime on the strength of whichever install ran last —
    // so it falls through to the node heuristics below, which is the answer
    // that was right before Bun was an option.
    if print.bun_lock && !print.npm_lock {
        return lang("bun", Confidence::Certain, "bun.lock / bunfig.toml");
    }
    if print.go_mod {
        return lang("go", Confidence::Certain, "go.mod");
    }
    if print.cargo_toml {
        return lang("rust", Confidence::Certain, "Cargo.toml");
    }
    if print.gemfile {
        return lang("ruby", Confidence::Certain, "Gemfile");
    }
    if print.manage_py {
        return lang("python", Confidence::Certain, "manage.py");
    }
    // Weaker than the Django marker: a composer.json beside it means a PHP
    // repo that happens to ship a Python tool, and PHP wins below.
    if print.python_deps && !print.composer_json {
        return lang(
            "python",
            Confidence::Likely,
            "requirements.txt / pyproject.toml",
        );
    }

    for (package, name) in [
        ("statamic/cms", "statamic"),
        ("drupal/core", "drupal"),
        ("magento/product-community-edition", "magento"),
        ("cakephp/cakephp", "cakephp"),
        ("codeigniter4/framework", "codeigniter"),
        ("slim/slim", "slim"),
    ] {
        if has(&print.composer_requires, package) {
            let doc = document_root(print).unwrap_or_else(|| "public".into());
            return php(
                Some(name),
                &doc,
                Confidence::Certain,
                vec!["composer.json".into()],
            );
        }
    }

    // ---- Node frameworks ---------------------------------------------------

    let node = |framework: Option<&'static str>,
                port: u16,
                start: &str,
                confidence: Confidence,
                evidence: Vec<String>| Detected {
        framework,
        runtime: "node",
        server: "nginx",
        document_root: None,
        php_version: None,
        node_version: node_version.clone(),
        node_port: Some(port),
        node_start: Some(start.to_string()),
        confidence,
        evidence,
    };

    if print.package_json && !print.composer_json {
        for (package, name, port) in [
            ("next", "next", 3000u16),
            ("nuxt", "nuxt", 3000),
            ("@remix-run/dev", "remix", 3000),
            ("@sveltejs/kit", "sveltekit", 5173),
            ("astro", "astro", 4321),
            ("@nestjs/core", "nestjs", 3000),
            ("vite", "vite", 5173),
        ] {
            if has(&print.node_dependencies, package) {
                let start = if has(&print.node_scripts, "start") {
                    "npm run start"
                } else {
                    "npm run dev"
                };
                return node(
                    Some(name),
                    port,
                    start,
                    Confidence::Certain,
                    vec!["package.json".into()],
                );
            }
        }
    }

    // ---- generic shapes ----------------------------------------------------

    if print.composer_json || print.index_php_root || print.index_php_public || print.public_dir {
        let doc = document_root(print).unwrap_or_else(|| "public".into());
        let mut evidence = Vec::new();
        if print.composer_json {
            evidence.push("composer.json".into());
        }
        if print.index_php_public {
            evidence.push("public/index.php".into());
        } else if print.index_php_root {
            evidence.push("index.php".into());
        }
        return php(None, &doc, Confidence::Likely, evidence);
    }

    if print.package_json {
        let start = if has(&print.node_scripts, "start") {
            "npm run start"
        } else {
            "npm run dev"
        };
        return node(
            None,
            3000,
            start,
            Confidence::Likely,
            vec!["package.json".into()],
        );
    }

    // Nothing recognisable. Reported as a guess with no evidence, so the form
    // shows defaults the user is expected to correct rather than an answer.
    Detected {
        framework: None,
        runtime: "php",
        server: "nginx",
        document_root: Some("public".into()),
        php_version: None,
        node_version: None,
        node_port: None,
        node_start: None,
        confidence: Confidence::Guess,
        evidence: Vec::new(),
    }
}

// ------------------------------------------------------------------- I/O

#[derive(Deserialize)]
struct ComposerJson {
    #[serde(default)]
    require: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "require-dev")]
    require_dev: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct PackageJson {
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    scripts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    engines: std::collections::BTreeMap<String, String>,
}

// -------------------------------------------------- services from the .env

/// One reason a service was inferred: the `.env` key that said so.
///
/// The **key**, never the value. This function reads somebody's project `.env`,
/// which is where their production-shaped credentials sit, and the answer is
/// shown on screen and travels into a manifest. A value in here would be a
/// password in a UI nobody expected to hold one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHint {
    pub service: String,
    pub key: String,
}

/// A project's own `.env`, as `KEY=VALUE` pairs.
///
/// Not [`crate::config::Env::parse`], deliberately: that one merges StackVo's
/// own embedded defaults over the file, which is right for the workspace `.env`
/// and completely wrong here — it would report `SERVICE_MYSQL_ENABLE` as
/// something a Laravel project had asked for.
fn env_pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| {
            (
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches(['"', '\'']).to_ascii_lowercase(),
            )
        })
        .collect()
}

/// Does this host name point somewhere other than the machine itself?
///
/// The distinction that makes `REDIS_HOST` evidence or noise. Laravel's own
/// `.env.example` ships `REDIS_HOST=127.0.0.1` in **every** project, whether or
/// not Redis is used, so treating the key's presence as a declaration would put
/// Redis in the manifest of every Laravel application ever cloned. A host that
/// names something else was typed by somebody.
fn names_another_host(value: &str) -> bool {
    !value.is_empty() && !matches!(value, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

/// The services a project's `.env` implies, with the key that implied each.
///
/// Modelled on Herd's `herd init`, which reads the project's `.env` and guesses
/// — and the guesses are only useful if they are conservative. Two kinds of
/// evidence count:
///
/// * a **driver** key whose value names a service (`DB_CONNECTION=pgsql`,
///   `SCOUT_DRIVER=meilisearch`). Somebody chose that value;
/// * a **host** key pointing at something that is not this machine
///   (`REDIS_HOST=redis`), because the boilerplate default is localhost.
///
/// Everything produced is checked against the catalog before it is returned, so
/// a rule cannot invent an id that has no template — the failure CONFLICTS.md
/// C-09 records, arrived at from a new direction.
pub fn services_from_env(text: &str) -> Vec<ServiceHint> {
    // (key, value → service). A value of "" matches any non-empty value.
    const DRIVERS: &[(&str, &[(&str, &str)])] = &[
        (
            "DB_CONNECTION",
            &[
                ("mysql", "mysql"),
                ("mariadb", "mariadb"),
                ("pgsql", "postgres"),
                ("postgres", "postgres"),
                ("postgresql", "postgres"),
                ("mongodb", "mongo"),
            ],
        ),
        (
            "CACHE_STORE",
            &[("redis", "redis"), ("memcached", "memcached")],
        ),
        (
            "CACHE_DRIVER",
            &[("redis", "redis"), ("memcached", "memcached")],
        ),
        (
            "SESSION_DRIVER",
            &[("redis", "redis"), ("memcached", "memcached")],
        ),
        (
            "QUEUE_CONNECTION",
            &[("redis", "redis"), ("rabbitmq", "rabbitmq")],
        ),
        (
            "SCOUT_DRIVER",
            &[
                ("meilisearch", "meilisearch"),
                ("typesense", "typesense"),
                ("elasticsearch", "elasticsearch"),
                ("elastic", "elasticsearch"),
            ],
        ),
        // s3 is the API, not the implementation: MinIO is what serves it here.
        ("FILESYSTEM_DISK", &[("s3", "minio"), ("minio", "minio")]),
        ("FILESYSTEM_DRIVER", &[("s3", "minio"), ("minio", "minio")]),
    ];

    // (key, service) — counted only when the value names another host.
    const HOSTS: &[(&str, &str)] = &[
        ("REDIS_HOST", "redis"),
        ("MEMCACHED_HOST", "memcached"),
        ("RABBITMQ_HOST", "rabbitmq"),
        ("ELASTICSEARCH_HOST", "elasticsearch"),
        ("MEILISEARCH_HOST", "meilisearch"),
        ("TYPESENSE_HOST", "typesense"),
        ("AWS_ENDPOINT", "minio"),
        ("MONGODB_URI", "mongo"),
    ];

    let catalog = crate::contracts::env_schema();
    let pairs = env_pairs(text);
    let mut out: Vec<ServiceHint> = Vec::new();

    let add = |service: &str, key: &str, out: &mut Vec<ServiceHint>| {
        if !catalog.knows_service(service) {
            return;
        }
        if out.iter().any(|h| h.service == service) {
            return;
        }
        out.push(ServiceHint {
            service: service.to_string(),
            key: key.to_string(),
        });
    };

    for (key, value) in &pairs {
        for (driver, mapping) in DRIVERS {
            if key != driver {
                continue;
            }
            for (needle, service) in *mapping {
                if value == needle {
                    add(service, key, &mut out);
                }
            }
        }

        for (host, service) in HOSTS {
            if key == host && names_another_host(value) {
                add(service, key, &mut out);
            }
        }
    }

    // The mail catcher is its own rule: `MAIL_MAILER=smtp` is true of every
    // project that sends mail and says nothing about *what* receives it, so the
    // host is what decides — and it decides between two services that both
    // exist in the catalog.
    for (key, value) in &pairs {
        if key != "MAIL_HOST" {
            continue;
        }
        if value.contains("mailpit") {
            add("mailpit", key, &mut out);
        } else if value.contains("mailhog") {
            add("mailhog", key, &mut out);
        }
    }

    out.sort_by(|a, b| a.service.cmp(&b.service));
    out
}

/// The same, read off disk. `.env` first, then `.env.example` — a fresh clone
/// has only the second, and it is exactly the file that describes what the
/// project expects rather than what one developer happens to have configured.
pub fn services_of(dir: &Path) -> Vec<ServiceHint> {
    for name in [".env", ".env.example"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
            let found = services_from_env(&text);
            if !found.is_empty() {
                return found;
            }
        }
    }
    Vec::new()
}

/// Read the markers off disk. Anything unreadable is simply absent — a
/// malformed `composer.json` should narrow the answer, not fail the scan.
pub fn fingerprint(dir: &Path) -> Fingerprint {
    let mut print = Fingerprint {
        artisan: dir.join("artisan").is_file(),
        composer_json: dir.join("composer.json").is_file(),
        package_json: dir.join("package.json").is_file(),
        bin_console: dir.join("bin").join("console").is_file(),
        wp_config: dir.join("wp-config.php").is_file()
            || dir.join("wp-config-sample.php").is_file(),
        wp_includes: dir.join("wp-includes").is_dir(),
        index_php_root: dir.join("index.php").is_file(),
        index_php_public: dir.join("public").join("index.php").is_file(),
        public_dir: dir.join("public").is_dir(),
        web_dir: dir.join("web").is_dir(),
        html_dir: dir.join("html").is_dir(),
        go_mod: dir.join("go.mod").is_file(),
        cargo_toml: dir.join("Cargo.toml").is_file(),
        gemfile: dir.join("Gemfile").is_file(),
        manage_py: dir.join("manage.py").is_file(),
        python_deps: dir.join("requirements.txt").is_file() || dir.join("pyproject.toml").is_file(),
        deno_config: dir.join("deno.json").is_file()
            || dir.join("deno.jsonc").is_file()
            || dir.join("deno.lock").is_file(),
        // `bun.lockb` is the binary lockfile Bun wrote before 1.2; `bun.lock`
        // is the text one it writes now. Both are still on disk in the wild.
        bun_lock: dir.join("bun.lock").is_file()
            || dir.join("bun.lockb").is_file()
            || dir.join("bunfig.toml").is_file(),
        npm_lock: dir.join("package-lock.json").is_file()
            || dir.join("yarn.lock").is_file()
            || dir.join("pnpm-lock.yaml").is_file(),
        ..Default::default()
    };

    if let Ok(text) = std::fs::read_to_string(dir.join("composer.json")) {
        if let Ok(composer) = serde_json::from_str::<ComposerJson>(&text) {
            print.php_constraint = composer.require.get("php").cloned();
            print.composer_requires = composer
                .require
                .keys()
                .chain(composer.require_dev.keys())
                .cloned()
                .collect();
        }
    }

    if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(package) = serde_json::from_str::<PackageJson>(&text) {
            print.node_dependencies = package
                .dependencies
                .keys()
                .chain(package.dev_dependencies.keys())
                .cloned()
                .collect();
            print.node_scripts = package.scripts.keys().cloned().collect();
            print.node_constraint = package.engines.get("node").cloned();
        }
    }

    // `.nvmrc` wins: it is the file a developer edits to change the version,
    // whereas `engines` is usually a floor nobody revisits.
    if let Ok(text) = std::fs::read_to_string(dir.join(".nvmrc")) {
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            print.node_constraint = Some(trimmed);
        }
    }

    print
}

pub fn detect(dir: &Path) -> Detected {
    infer(&fingerprint(dir))
}

/// A directory under `projects/` that StackVo is not managing yet.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adoptable {
    pub name: String,
    pub path: String,
    pub detected: Detected,
    /// False for a directory with nothing in it — there is nothing to adopt.
    pub has_files: bool,
    /// A `docker-compose.yml` beside the project, if it has one.
    ///
    /// Reported here so the list can offer to read it, and *only* reported —
    /// resolving it costs a `docker compose config` per project, which is not
    /// something to spend on a page that loads on every visit.
    pub compose_file: Option<String>,
}

/// The names Compose itself looks for, in its own precedence order.
const COMPOSE_NAMES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// The compose file a directory has, if any.
pub fn compose_file(dir: &Path) -> Option<std::path::PathBuf> {
    COMPOSE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Every directory under `projects/` with no `stackvo.json`.
pub fn adoptable(root: &Path) -> Vec<Adoptable> {
    let mut out = Vec::new();
    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&projects) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Dotfiles are not projects; `.DS_Store` directories and `.git` show up
        // here on a real machine.
        if name.starts_with('.') || path.join("stackvo.json").is_file() {
            continue;
        }

        // Dotfiles do not count as contents. On the checkout this was written
        // against, an empty directory held one `.DS_Store` and would otherwise
        // have been offered for adoption as if it had code in it.
        let has_files = std::fs::read_dir(&path)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| !e.file_name().to_string_lossy().starts_with('.'))
            })
            .unwrap_or(false);

        out.push(Adoptable {
            name: name.to_string(),
            path: path.display().to_string(),
            detected: detect(&path),
            has_files,
            compose_file: compose_file(&path).map(|p| p.display().to_string()),
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn the_lang_runtimes_are_recognised_by_their_exclusive_markers() {
        for (field, runtime) in [
            ("go_mod", "go"),
            ("cargo_toml", "rust"),
            ("gemfile", "ruby"),
            ("manage_py", "python"),
        ] {
            let mut print = Fingerprint {
                // A package.json for tooling must not drag the repo to node.
                package_json: true,
                node_dependencies: s(&["esbuild"]),
                ..Default::default()
            };
            match field {
                "go_mod" => print.go_mod = true,
                "cargo_toml" => print.cargo_toml = true,
                "gemfile" => print.gemfile = true,
                _ => print.manage_py = true,
            }
            let d = infer(&print);
            assert_eq!(d.runtime, runtime, "{field}");
            assert_eq!(d.confidence, Confidence::Certain);
        }
    }

    /// J-1. Deno's marker is exclusive; Bun's is not, and the difference is the
    /// whole of what these two cover.
    ///
    /// A repository with `deno.json` has committed — nothing else reads it, and
    /// Node would not resolve its imports at all. A repository with a Bun
    /// lockfile has a `package.json` that is indistinguishable from a Node
    /// project's, so the lockfile is the only evidence, and it stops being
    /// evidence the moment an npm-family lockfile sits beside it.
    #[test]
    fn deno_is_certain_from_its_own_manifest() {
        let print = Fingerprint {
            deno_config: true,
            // Deno 2 reads package.json too, so its presence proves nothing.
            package_json: true,
            node_dependencies: s(&["oak"]),
            ..Default::default()
        };
        let d = infer(&print);
        assert_eq!(d.runtime, "deno");
        assert_eq!(d.confidence, Confidence::Certain);
    }

    #[test]
    fn bun_wins_on_its_lockfile_and_yields_when_npm_also_installed() {
        let bun_only = Fingerprint {
            bun_lock: true,
            package_json: true,
            node_scripts: s(&["start"]),
            ..Default::default()
        };
        assert_eq!(infer(&bun_only).runtime, "bun");

        // Installed both ways. Picking Bun here would be picking a runtime on
        // the strength of whichever install happened to run last, so it falls
        // through to the node heuristics — the answer that was right before Bun
        // was an option.
        let both = Fingerprint {
            bun_lock: true,
            npm_lock: true,
            package_json: true,
            node_scripts: s(&["start"]),
            ..Default::default()
        };
        assert_eq!(infer(&both).runtime, "node");
    }

    #[test]
    fn a_requirements_txt_beside_a_composer_json_stays_php() {
        // A PHP repo shipping a Python tool is a PHP repo; the weaker Python
        // marker only wins when nothing PHP-shaped is present.
        let php_repo = Fingerprint {
            python_deps: true,
            composer_json: true,
            index_php_public: true,
            public_dir: true,
            ..Default::default()
        };
        assert_eq!(infer(&php_repo).runtime, "php");

        let python_repo = Fingerprint {
            python_deps: true,
            ..Default::default()
        };
        let d = infer(&python_repo);
        assert_eq!(d.runtime, "python");
        assert_eq!(d.confidence, Confidence::Likely);
    }

    #[test]
    fn an_artisan_file_outranks_every_lang_marker() {
        // Laravel repos can carry a Gemfile (fastlane) or requirements.txt;
        // the framework marker is absolute.
        let print = Fingerprint {
            artisan: true,
            gemfile: true,
            python_deps: true,
            ..Default::default()
        };
        assert_eq!(infer(&print).runtime, "php");
    }

    #[test]
    fn laravel_is_recognised_by_its_artisan_file() {
        let print = Fingerprint {
            artisan: true,
            composer_json: true,
            package_json: true,
            public_dir: true,
            index_php_public: true,
            composer_requires: s(&["laravel/framework", "php"]),
            php_constraint: Some("^8.2".into()),
            ..Default::default()
        };

        let out = infer(&print);
        assert_eq!(out.framework, Some("laravel"));
        assert_eq!(out.runtime, "php");
        assert_eq!(out.document_root.as_deref(), Some("public"));
        assert_eq!(out.php_version.as_deref(), Some("8.2"));
        assert_eq!(out.confidence, Confidence::Certain);
        assert_eq!(out.evidence, s(&["artisan"]));
    }

    /// The ordering bug this guards: a Laravel repository has a `package.json`
    /// for its front-end assets. Deciding on whichever manifest is noticed
    /// first turns a PHP application into a Node one, and the result builds.
    #[test]
    fn a_php_frameworks_front_end_assets_do_not_make_it_a_node_project() {
        let print = Fingerprint {
            artisan: true,
            composer_json: true,
            package_json: true,
            node_dependencies: s(&["vite", "axios"]),
            ..Default::default()
        };
        assert_eq!(infer(&print).runtime, "php");
    }

    /// WordPress serves from the directory it is installed in. Leaving this to
    /// the `public` default produces a project that builds, starts, and serves
    /// nothing — with no error anywhere to say why.
    #[test]
    fn wordpress_serves_from_the_project_root() {
        let print = Fingerprint {
            wp_config: true,
            wp_includes: true,
            index_php_root: true,
            ..Default::default()
        };

        let out = infer(&print);
        assert_eq!(out.framework, Some("wordpress"));
        assert_eq!(out.document_root.as_deref(), Some("."));
        assert!(out.evidence.contains(&"wp-config.php".to_string()));
    }

    /// A stock WordPress download ships `wp-config-sample.php` and no
    /// `wp-config.php` until it is installed, which is exactly when someone is
    /// setting the site up in StackVo.
    #[test]
    fn an_uninstalled_wordpress_is_still_wordpress() {
        let print = Fingerprint {
            wp_includes: true,
            ..Default::default()
        };
        assert_eq!(infer(&print).framework, Some("wordpress"));
    }

    #[test]
    fn symfony_needs_both_the_console_and_a_symfony_package() {
        let symfony = Fingerprint {
            bin_console: true,
            composer_json: true,
            composer_requires: s(&["symfony/framework-bundle"]),
            index_php_public: true,
            ..Default::default()
        };
        assert_eq!(infer(&symfony).framework, Some("symfony"));

        // `bin/console` alone is a convention plenty of projects borrow.
        let borrowed = Fingerprint {
            bin_console: true,
            composer_json: true,
            composer_requires: s(&["monolog/monolog"]),
            ..Default::default()
        };
        assert_eq!(infer(&borrowed).framework, None);
        assert_eq!(infer(&borrowed).runtime, "php");
    }

    #[test]
    fn node_frameworks_carry_their_own_dev_server_port() {
        for (package, name, port) in [
            ("next", "next", 3000u16),
            ("@sveltejs/kit", "sveltekit", 5173),
            ("astro", "astro", 4321),
        ] {
            let print = Fingerprint {
                package_json: true,
                node_dependencies: s(&[package]),
                node_scripts: s(&["dev", "build"]),
                ..Default::default()
            };
            let out = infer(&print);
            assert_eq!(out.framework, Some(name));
            assert_eq!(out.runtime, "node");
            assert_eq!(out.node_port, Some(port));
            assert_eq!(out.node_start.as_deref(), Some("npm run dev"));
        }
    }

    #[test]
    fn a_start_script_is_preferred_over_dev_when_present() {
        let print = Fingerprint {
            package_json: true,
            node_dependencies: s(&["next"]),
            node_scripts: s(&["dev", "start"]),
            ..Default::default()
        };
        assert_eq!(infer(&print).node_start.as_deref(), Some("npm run start"));
    }

    #[test]
    fn plain_php_in_the_root_is_served_from_the_root() {
        let print = Fingerprint {
            index_php_root: true,
            ..Default::default()
        };
        let out = infer(&print);
        assert_eq!(out.runtime, "php");
        assert_eq!(out.document_root.as_deref(), Some("."));
        assert_eq!(out.confidence, Confidence::Likely);
    }

    #[test]
    fn a_public_index_wins_over_a_root_one() {
        let print = Fingerprint {
            index_php_root: true,
            index_php_public: true,
            public_dir: true,
            ..Default::default()
        };
        assert_eq!(infer(&print).document_root.as_deref(), Some("public"));
    }

    /// An empty folder gets defaults and says so. Reporting `Likely` here would
    /// present a guess as an answer in the one case where there is no evidence
    /// at all.
    #[test]
    fn nothing_recognisable_is_reported_as_a_guess() {
        let out = infer(&Fingerprint::default());
        assert_eq!(out.confidence, Confidence::Guess);
        assert!(out.evidence.is_empty());
        assert_eq!(out.runtime, "php");
        assert_eq!(out.document_root.as_deref(), Some("public"));
    }

    #[test]
    fn php_constraints_yield_a_major_minor_or_nothing() {
        assert_eq!(first_version("^8.2").as_deref(), Some("8.2"));
        assert_eq!(first_version(">=8.1 <9.0").as_deref(), Some("8.1"));
        assert_eq!(first_version("~8.3.0").as_deref(), Some("8.3"));
        // A bare major is not a version StackVo can pin to an image.
        assert_eq!(first_version("^8"), None);
        assert_eq!(first_version("*"), None);
        assert_eq!(first_version(""), None);
    }

    #[test]
    fn a_bare_nvmrc_major_is_kept() {
        let print = Fingerprint {
            package_json: true,
            node_dependencies: s(&["next"]),
            node_constraint: Some("22".into()),
            ..Default::default()
        };
        assert_eq!(infer(&print).node_version.as_deref(), Some("22"));

        let with_v = Fingerprint {
            node_constraint: Some("v20.11.1".into()),
            package_json: true,
            ..Default::default()
        };
        assert_eq!(infer(&with_v).node_version.as_deref(), Some("20.11.1"));
    }

    // ------------------------------------------------ services from the .env

    fn ids(text: &str) -> Vec<String> {
        services_from_env(text)
            .into_iter()
            .map(|h| h.service)
            .collect()
    }

    /// The false positive the whole design is arranged around.
    ///
    /// Laravel ships `REDIS_HOST=127.0.0.1` in every `.env.example`, used or
    /// not. A rule keyed on the presence of the variable would put Redis in
    /// the manifest of every Laravel project ever cloned — and a declaration
    /// that is wrong everywhere is one nobody reads anywhere.
    #[test]
    fn the_boilerplate_a_framework_ships_is_not_a_declaration() {
        let laravel = "\
APP_NAME=Laravel
DB_CONNECTION=sqlite
REDIS_HOST=127.0.0.1
REDIS_PORT=6379
MEMCACHED_HOST=127.0.0.1
MAIL_MAILER=log
";
        assert!(ids(laravel).is_empty(), "{:?}", ids(laravel));
    }

    #[test]
    fn a_host_that_names_something_else_is_evidence() {
        let text = "REDIS_HOST=stackvo-redis\nMEMCACHED_HOST=cache.internal\n";
        assert_eq!(ids(text), ["memcached", "redis"]);

        // …and the key is what is reported, never the value: this function
        // reads a file full of passwords.
        let hints = services_from_env("REDIS_HOST=stackvo-redis\n");
        assert_eq!(hints[0].key, "REDIS_HOST");
        assert!(!format!("{hints:?}").contains("stackvo-redis"));
    }

    #[test]
    fn a_driver_value_names_the_service() {
        assert_eq!(ids("DB_CONNECTION=pgsql\n"), ["postgres"]);
        assert_eq!(ids("DB_CONNECTION=mysql\n"), ["mysql"]);
        assert_eq!(ids("SCOUT_DRIVER=meilisearch\n"), ["meilisearch"]);
        assert_eq!(ids("SCOUT_DRIVER=typesense\n"), ["typesense"]);
        // s3 is the API, MinIO is what serves it here.
        assert_eq!(ids("FILESYSTEM_DISK=s3\n"), ["minio"]);
        assert_eq!(ids("QUEUE_CONNECTION=rabbitmq\n"), ["rabbitmq"]);
    }

    /// Quoting, spacing, case and comments are all things a real `.env` has.
    #[test]
    fn the_reader_handles_a_file_a_person_wrote() {
        let text = "\
# the database
db_connection = \"pgsql\"

CACHE_STORE='redis'
REDIS_HOST=\"stackvo-redis\"
";
        assert_eq!(ids(text), ["postgres", "redis"]);
    }

    /// Two rules can name the same service; it must appear once, and the first
    /// key that proved it is the one worth showing.
    #[test]
    fn one_service_is_reported_once() {
        let hints = services_from_env("CACHE_STORE=redis\nREDIS_HOST=stackvo-redis\n");
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].key, "CACHE_STORE");
    }

    /// `MAIL_MAILER=smtp` is true of everything that sends mail and says
    /// nothing about what receives it; the host is what picks between two
    /// catchers that both exist.
    #[test]
    fn the_mail_catcher_is_decided_by_the_host_and_not_the_mailer() {
        assert!(ids("MAIL_MAILER=smtp\nMAIL_HOST=smtp.sendgrid.net\n").is_empty());
        assert_eq!(ids("MAIL_HOST=stackvo-mailpit\n"), ["mailpit"]);
        assert_eq!(ids("MAIL_HOST=mailhog\n"), ["mailhog"]);
    }

    /// A rule may only produce an id the catalog has a template for. Without
    /// this, a mapping added here would write `SERVICE_<JUNK>_ENABLE` into
    /// somebody's `.env` and bring up a compose profile matching nothing.
    #[test]
    fn every_service_a_rule_can_produce_is_in_the_catalog() {
        let schema = crate::contracts::env_schema();
        let text = "\
DB_CONNECTION=mongodb
CACHE_STORE=memcached
SESSION_DRIVER=redis
QUEUE_CONNECTION=rabbitmq
SCOUT_DRIVER=elasticsearch
FILESYSTEM_DISK=s3
MAIL_HOST=stackvo-mailpit
MONGODB_URI=mongodb://db:27017
TYPESENSE_HOST=search
";
        let found = services_from_env(text);
        assert!(found.len() >= 7, "{found:?}");
        for hint in found {
            assert!(
                schema.knows_service(&hint.service),
                "{} is not in the catalog",
                hint.service
            );
        }
    }
}
