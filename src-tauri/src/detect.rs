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
}

/// Every directory under `projects/` with no `stackvo.json`.
pub fn adoptable(root: &Path) -> Vec<Adoptable> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("projects")) else {
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
}
