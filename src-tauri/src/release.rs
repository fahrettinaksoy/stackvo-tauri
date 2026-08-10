//! Turning a development project into a deployable image.
//!
//! P3-20 — "export a production image, what `laradock ship` does" — is the item
//! the analysis calls a long-horizon differentiator, because the container
//! lineage makes it possible and no native-binary competitor can follow. It is
//! also the one with a real question inside it, and the question was answered
//! by looking at what the images actually contain rather than by assuming.
//!
//! ## The dev image is not a production image
//!
//! Measured against this checkout's own images:
//!
//! * A PHP project's image holds **no application code at all** —
//!   `/var/www/html` contains one file, `index.nginx-debian.html`, because the
//!   source arrives through a bind mount. Exporting that image ships a web
//!   server and nothing to serve.
//! * It also has **Xdebug loaded**: `php -m` lists it and
//!   `docker-php-ext-xdebug.ini` is in `conf.d`. Shipping a step debugger that
//!   opens a connection on every request is not a deployment, it is an
//!   incident.
//! * A **node** project's image is the opposite: its Dockerfile does `COPY . .`
//!   and `npm install`, so the image already holds the code and the build. That
//!   is a genuine per-runtime difference, and it is why there are two
//!   strategies rather than one clever one.
//!
//! ## `.env` is the thing that must not ship
//!
//! The project directory this was written against holds **five** of them —
//! `.env.local`, `.env.main`, `.env.stage`, `.env.testing`, and a `.bak` of the
//! last — each with real credentials. A `COPY . .` puts every one into an image
//! layer, where deleting it later does not remove it.
//!
//! So the exclusion list is not housekeeping, it is the feature's safety
//! property, and it is **verified after the build** rather than assumed: the
//! image is run and asked whether it has an `.env` and whether Xdebug is
//! active. A security guarantee that is only true in the source is the class of
//! bug this project keeps finding by testing against the running stack.
//!
//! The ignore file is written **beside the generated Dockerfile**, not into the
//! user's repository — BuildKit reads `<dockerfile>.dockerignore` in preference
//! to the context's, which was confirmed to work before anything depended on
//! it. Writing a `.dockerignore` into somebody's project to build their image
//! would be a side effect nobody asked for.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How a project's production image is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// PHP: the dev image has no application code, so the production image is a
    /// build that adds it and removes the debugger.
    Layer,
    /// Node: the dev image already carries the code and the build output, so
    /// there is nothing to add. Rebuilding from it would replace a Linux
    /// `node_modules` with whatever the host happens to have.
    Retag,
}

/// Patterns kept out of the image, each with the reason.
///
/// Named rather than listed: an exclusion the user cannot explain is one they
/// will work around, and the `.env` line in particular is the whole point.
pub const EXCLUDED: [(&str, &str); 12] = [
    (".env", "local credentials"),
    (
        ".env.*",
        "local credentials — this project has five of them",
    ),
    ("*.env", "local credentials"),
    (".git", "history, and often larger than the application"),
    (".gitignore", "not needed at runtime"),
    (".gitattributes", "not needed at runtime"),
    (".stackvo", "StackVo's own per-project settings"),
    ("node_modules", "built for the host, not for the image"),
    (".idea", "editor state"),
    (".vscode", "editor state"),
    (".DS_Store", "editor state"),
    ("*.log", "development output"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub strategy: Strategy,
    /// The image this is built from — the one the project already runs.
    pub base_image: String,
    /// What the result is tagged.
    pub tag: String,
    /// The Dockerfile, shown before it is built. Empty for `Retag`.
    pub dockerfile: String,
    /// `(pattern, reason)`, so the exclusions can be read rather than trusted.
    pub excluded: Vec<(String, String)>,
    /// Things true of the result that the user should know before shipping it.
    pub warnings: Vec<String>,
    /// Where the source is copied to, for `Layer`.
    pub app_path: String,
    pub runtime: String,
}

/// What the finished image was actually found to contain.
///
/// Read out of the built image, not inferred from the Dockerfile. The
/// guarantees this feature makes are exactly the kind that are easy to state
/// and easy to get wrong.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    /// Env files found in the application directory. Must be empty.
    pub env_files: Vec<String>,
    /// Whether `php -m` still lists Xdebug. Null for a node image.
    pub xdebug_active: Option<bool>,
    /// Whether the application directory holds anything at all — a `Layer`
    /// build whose context was empty would otherwise look like a success.
    pub has_app: bool,
    /// True when every check passed.
    pub clean: bool,
}

// -------------------------------------------------------------- pure logic

/// Where the application lives inside the image, per runtime.
pub fn app_path(runtime: &str) -> &'static str {
    if runtime == "php" {
        crate::xdebug::CONTAINER_PATH
    } else {
        // node and the lang runtimes all build snapshot images with the
        // source at /app.
        crate::devserver::CONTAINER_PATH
    }
}

pub fn strategy(runtime: &str) -> Strategy {
    // Snapshot images (node and the lang runtimes) already hold the code and
    // the build — re-tag them. Only PHP's bind-mount image needs the source
    // layered in.
    if runtime == "php" {
        Strategy::Layer
    } else {
        Strategy::Retag
    }
}

/// The generated production Dockerfile.
///
/// Deliberately short. It is not a rewrite of the project's build — the base
/// image already did the hard part — it is the three things the development
/// image is missing or carrying wrongly.
pub fn dockerfile(base: &str, runtime: &str) -> String {
    let app = app_path(runtime);

    format!(
        "# Generated by StackVo Desktop. Review before you ship it.\n\
         #\n\
         # Built FROM the image this project already runs, so the PHP version,\n\
         # the extensions and the web server are the ones you developed against.\n\
         # That lineage is the whole point: nothing here re-derives your build.\n\
         FROM {base}\n\
         \n\
         # The development image has no application code — the source arrives\n\
         # through a bind mount. This is what adds it.\n\
         COPY . {app}\n\
         \n\
         # Xdebug opens a debugging connection on every request. Removing its\n\
         # ini is the supported way to switch it off; the extension binary is\n\
         # still in the image, so this is \"not active\", not \"not present\".\n\
         RUN rm -f /usr/local/etc/php/conf.d/docker-php-ext-xdebug.ini \\\n\
         \x20         /usr/local/etc/php/conf.d/zzz-stackvo-xdebug.ini \\\n\
         \x20         /usr/local/etc/php/conf.d/zz-stackvo.ini\n"
    )
}

/// The ignore file, written beside the Dockerfile rather than into the project.
pub fn dockerignore() -> String {
    let mut out = String::from(
        "# Generated by StackVo Desktop.\n\
         #\n\
         # Read by BuildKit as <dockerfile>.dockerignore, so nothing is written\n\
         # into the project directory to build its image.\n",
    );
    for (pattern, reason) in EXCLUDED {
        out.push_str(&format!("# {reason}\n{pattern}\n"));
    }
    out
}

/// A Docker tag this app is willing to produce.
///
/// Checked because it reaches a command line as one argument and an image
/// registry as a name. Lowercase, because Docker rejects an uppercase
/// repository with an error that does not say so.
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 200
        && tag.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-' | '/' | ':')
        })
        && tag.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !tag.contains("::")
}

/// Read the verification out of what the checks printed.
///
/// Split from running them so the interpretation is testable without Docker.
pub fn interpret(runtime: &str, stdout: &str) -> Verification {
    let mut out = Verification::default();

    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ENV:") {
            let name = rest.trim();
            if !name.is_empty() {
                out.env_files.push(name.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("XDEBUG:") {
            out.xdebug_active = Some(rest.trim() == "1");
        } else if let Some(rest) = line.strip_prefix("FILES:") {
            out.has_app = rest.trim().parse::<u32>().unwrap_or(0) > 0;
        }
    }

    if runtime != "php" {
        out.xdebug_active = None;
    }

    out.clean = out.env_files.is_empty() && out.has_app && out.xdebug_active != Some(true);
    out
}

/// The one-liner run inside the finished image to check it.
///
/// `sh -c` with a fixed script — nothing here comes from the frontend, and the
/// only interpolation is a container path this module owns.
pub fn verify_script(app: &str) -> String {
    format!(
        "for f in {app}/.env {app}/.env.*; do [ -e \"$f\" ] && echo \"ENV:$(basename \"$f\")\"; done; \
         echo \"FILES:$(ls -A {app} 2>/dev/null | wc -l)\"; \
         command -v php >/dev/null 2>&1 && echo \"XDEBUG:$(php -m 2>/dev/null | grep -ci '^xdebug$')\"; \
         exit 0"
    )
}

// ------------------------------------------------------------------- I/O

pub fn out_dir(root: &Path, name: &str) -> PathBuf {
    root.join("generated").join("production").join(name)
}

pub fn dockerfile_path(root: &Path, name: &str) -> PathBuf {
    out_dir(root, name).join("Dockerfile")
}

/// What building a production image would do.
pub fn plan(root: &Path, name: &str, tag: Option<String>) -> Result<Plan> {
    let dir = crate::workspace::project_dir(root, name)?;
    let manifest = crate::manifest::read(&dir.join("stackvo.json"), name)?;

    let base = format!("{}{name}:latest", crate::engine::CONTAINER_PREFIX);
    let strategy = strategy(&manifest.runtime);

    let tag = tag
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("{name}:production").to_lowercase());

    if !valid_tag(&tag) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("`{tag}` is not a valid image tag"),
        )
        .with_hint(crate::hints::IMAGE_REFERENCE_CHARSET));
    }

    let mut warnings = Vec::new();

    // Said before the build, not discovered after. Every one of these is a
    // thing the user may want to do differently, and none of them is a decision
    // this app should make silently on their behalf.
    match strategy {
        Strategy::Layer => {
            warnings.push(
                "No .env is included. Supply configuration through the environment when you run it."
                    .into(),
            );
            if dir.join("vendor").is_dir() {
                warnings.push(
                    "`vendor/` ships as it is on disk. Run `composer install --no-dev` first if development packages should not be in the image."
                        .into(),
                );
            }
            warnings.push(
                "Xdebug's ini is removed, so it is not active — the extension binary is still in the base image."
                    .into(),
            );
        }
        Strategy::Retag => {
            warnings.push(
                "A node image already contains the code and the build from when it was last built. Rebuild the project first if the source has changed since."
                    .into(),
            );
        }
    }

    if !dir.join("stackvo.json").is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }

    Ok(Plan {
        dockerfile: match strategy {
            Strategy::Layer => dockerfile(&base, &manifest.runtime),
            Strategy::Retag => String::new(),
        },
        excluded: EXCLUDED
            .iter()
            .map(|(p, r)| (p.to_string(), r.to_string()))
            .collect(),
        app_path: app_path(&manifest.runtime).to_string(),
        runtime: manifest.runtime.clone(),
        base_image: base,
        strategy,
        tag,
        warnings,
    })
}

/// Write the Dockerfile and its ignore file, returning the Dockerfile's path.
pub fn write(root: &Path, name: &str, plan: &Plan) -> Result<PathBuf> {
    let dir = out_dir(root, name);
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::io(format!("creating {}", dir.display()), e))?;

    let path = dir.join("Dockerfile");
    crate::atomic::write(&path, &plan.dockerfile)?;
    // BuildKit reads this in preference to the context's, which is what keeps
    // the user's project directory untouched.
    crate::atomic::write(&dir.join("Dockerfile.dockerignore"), &dockerignore())?;

    Ok(path)
}

/// `docker build` argv for a plan.
pub fn build_argv(context: &Path, dockerfile: &Path, tag: &str) -> Vec<String> {
    vec![
        "build".to_string(),
        "-f".to_string(),
        dockerfile.display().to_string(),
        "-t".to_string(),
        tag.to_string(),
        context.display().to_string(),
    ]
}

/// Run the finished image and ask it what it contains.
pub async fn verify(tag: &str, runtime: &str) -> Result<Verification> {
    let output = tokio::process::Command::new("docker")
        .args(["run", "--rm", "--entrypoint", "sh", tag, "-c"])
        .arg(verify_script(app_path(runtime)))
        .output()
        .await
        .map_err(|e| Error::io("running the image to check it", e))?;

    if !output.status.success() {
        return Err(Error::new(
            Code::GenerateFailed,
            format!(
                "the image was built but would not run: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(interpret(runtime, &String::from_utf8_lossy(&output.stdout)))
}

/// Write the image out as a tarball.
pub async fn save(tag: &str, path: &Path) -> Result<u64> {
    let output = tokio::process::Command::new("docker")
        .args(["save", "-o"])
        .arg(path)
        .arg(tag)
        .output()
        .await
        .map_err(|e| Error::io("running docker save", e))?;

    if !output.status.success() {
        return Err(Error::new(
            Code::GenerateFailed,
            format!(
                "docker save failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// Read a tarball written by `save` back into the local daemon.
///
/// The return trip `save` never had. A machine with no route to a registry
/// could be handed an image but not given one, which made the export half a
/// feature with nowhere to land — the readiness report tracks the pair as
/// "air-gapped install" and had it down as not started, because half a round
/// trip is not one.
///
/// The tags are returned rather than discarded. `docker load` writes them to
/// stdout as `Loaded image: name:tag`, and they are the only way the caller can
/// name what it just installed: the tarball's file name is whatever somebody
/// called it, and a stream can carry several images.
pub async fn load(path: &Path) -> Result<Vec<String>> {
    // Checked here rather than left to Docker: `docker load` answers a missing
    // file with an error about the *archive* being invalid, which reads as a
    // corrupt bundle and sends the user looking at the wrong thing.
    if !path.exists() {
        return Err(Error::new(
            Code::NotFound,
            format!("no image archive at {}", path.display()),
        ));
    }

    let output = tokio::process::Command::new("docker")
        .args(["load", "-i"])
        .arg(path)
        .output()
        .await
        .map_err(|e| Error::io("running docker load", e))?;

    if !output.status.success() {
        return Err(Error::new(
            Code::BuildFailed,
            format!(
                "docker load failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(loaded_tags(&String::from_utf8_lossy(&output.stdout)))
}

/// The image names in `docker load`'s output.
///
/// Split out to be testable without a daemon. Two shapes appear: `Loaded image:
/// x:1` for a tagged image and `Loaded image ID: sha256:…` for one that carries
/// none. The second is deliberately kept — an untagged image did load, and
/// reporting nothing at all would read as a bundle that contained nothing.
fn loaded_tags(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("Loaded image: ")
                .or_else(|| line.strip_prefix("Loaded image ID: "))
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docker load`'s output, both shapes it comes in.
    ///
    /// Parsed rather than assumed to be one line: a tarball can carry several
    /// images, and a bundle of a whole stack is the case this exists for.
    #[test]
    fn the_loaded_image_names_are_read_out_of_dockers_own_output() {
        let stdout = "Loaded image: stackvo-shop:1.4\n\
                      Loaded image: stackvo-blog:latest\n";
        assert_eq!(
            loaded_tags(stdout),
            vec!["stackvo-shop:1.4", "stackvo-blog:latest"]
        );

        // An untagged image reports its id. Dropping it would answer an empty
        // list for a load that genuinely installed something, which reads as an
        // empty bundle rather than an untagged one.
        let untagged = "Loaded image ID: sha256:9f2a1c\n";
        assert_eq!(loaded_tags(untagged), vec!["sha256:9f2a1c"]);

        // Progress chatter is not an image.
        let noisy = "The image stackvo-shop:1.4 already exists, renaming the old one\n\
                     Loaded image: stackvo-shop:1.4\n";
        assert_eq!(loaded_tags(noisy), vec!["stackvo-shop:1.4"]);

        assert!(loaded_tags("").is_empty());
    }

    /// A missing bundle is named as missing, without reaching for Docker.
    #[tokio::test]
    async fn a_bundle_that_is_not_there_is_not_a_corrupt_archive() {
        let path = std::env::temp_dir().join("stackvo-no-such-bundle-9e1a.tar");
        let _ = std::fs::remove_file(&path);

        let refused = load(&path).await.expect_err("a missing file cannot load");
        assert_eq!(refused.code, Code::NotFound);
        assert!(
            refused.message.contains("stackvo-no-such-bundle-9e1a"),
            "the error names the path it looked at, got: {}",
            refused.message
        );
    }

    /// Measured, not assumed. A PHP project's image holds one file in
    /// `/var/www/html` because the source is bind-mounted; a node image holds
    /// the whole application because its Dockerfile copies and installs. Two
    /// strategies rather than one clever one.
    #[test]
    fn the_strategy_follows_what_the_image_actually_contains() {
        assert_eq!(strategy("php"), Strategy::Layer);
        assert_eq!(strategy("node"), Strategy::Retag);
        assert_eq!(app_path("php"), "/var/www/html");
        assert_eq!(app_path("node"), "/app");
    }

    /// The safety property, stated in the artefact that enforces it. The
    /// project this was written against holds five env files; a `COPY .` puts
    /// every one into a layer, where deleting it later does not remove it.
    #[test]
    fn every_env_shape_is_excluded_with_a_reason() {
        let ignore = dockerignore();
        for pattern in [".env", ".env.*", "*.env"] {
            assert!(
                ignore.lines().any(|l| l == pattern),
                "{pattern} is not excluded:\n{ignore}"
            );
        }
        // Every pattern carries its reason, because an exclusion nobody can
        // explain is one somebody works around.
        for (pattern, reason) in EXCLUDED {
            assert!(ignore.contains(reason), "{pattern} has no reason given");
        }
    }

    /// Shipping a step debugger that dials out on every request is an incident,
    /// not a deployment.
    #[test]
    fn the_dockerfile_disables_xdebug_and_adds_the_code() {
        let text = dockerfile("stackvo-shop:latest", "php");

        assert!(text.contains("FROM stackvo-shop:latest"));
        assert!(text.contains("COPY . /var/www/html"));
        assert!(text.contains("rm -f /usr/local/etc/php/conf.d/docker-php-ext-xdebug.ini"));
        // The app's own overlay inis are development-only too.
        assert!(text.contains("zzz-stackvo-xdebug.ini"));
        assert!(text.contains("zz-stackvo.ini"));
    }

    /// Docker rejects an uppercase repository with an error that does not say
    /// so, and the tag reaches a command line as one argument.
    #[test]
    fn a_tag_is_checked_before_it_reaches_a_command_line() {
        assert!(valid_tag("shop:production"));
        assert!(valid_tag("registry.example.com/team/shop:1.2.3"));

        assert!(!valid_tag("Shop:production"), "uppercase");
        assert!(!valid_tag("shop; rm -rf /"), "shell");
        assert!(!valid_tag("shop:latest && echo"), "shell");
        assert!(!valid_tag(""), "empty");
        assert!(!valid_tag("-shop"), "leading dash reads as a flag");
    }

    /// The guarantees are read out of the built image rather than inferred from
    /// the Dockerfile — this is the class of claim that is easy to state and
    /// easy to get wrong.
    #[test]
    fn a_clean_image_is_one_with_no_env_an_app_and_no_xdebug() {
        let clean = interpret("php", "FILES:412\nXDEBUG:0\n");
        assert!(clean.clean);
        assert!(clean.env_files.is_empty());
        assert_eq!(clean.xdebug_active, Some(false));
        assert!(clean.has_app);
    }

    #[test]
    fn a_leaked_env_fails_the_check() {
        let leaked = interpret(
            "php",
            "ENV:.env.stage\nENV:.env.main\nFILES:412\nXDEBUG:0\n",
        );
        assert!(!leaked.clean);
        assert_eq!(leaked.env_files, [".env.stage", ".env.main"]);
    }

    #[test]
    fn an_active_xdebug_fails_the_check() {
        let debug = interpret("php", "FILES:412\nXDEBUG:1\n");
        assert!(!debug.clean);
        assert_eq!(debug.xdebug_active, Some(true));
    }

    /// A build whose context was empty produces an image that starts and serves
    /// nothing, which without this check looks exactly like a success.
    #[test]
    fn an_image_with_no_application_fails_the_check() {
        let empty = interpret("php", "FILES:0\nXDEBUG:0\n");
        assert!(!empty.clean);
        assert!(!empty.has_app);
    }

    /// A node image has no PHP to ask, so the answer is "not applicable"
    /// rather than "no".
    #[test]
    fn a_node_image_is_not_asked_about_xdebug() {
        let node = interpret("node", "FILES:22\n");
        assert_eq!(node.xdebug_active, None);
        assert!(node.clean);
    }

    #[test]
    fn the_build_arguments_are_separate_words() {
        let argv = build_argv(
            Path::new("/w/projects/shop"),
            Path::new("/w/generated/production/shop/Dockerfile"),
            "shop:production",
        );
        assert_eq!(argv[0], "build");
        assert!(argv.contains(&"-f".to_string()));
        assert_eq!(argv.last().unwrap(), "/w/projects/shop");
        assert!(!argv.iter().any(|a| a.contains(' ')));
    }
}
