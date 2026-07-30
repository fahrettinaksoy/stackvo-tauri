//! Scaffold a brand-new framework project — the create half of P0-1.
//!
//! The import half shipped first: point the app at an existing folder and
//! detection infers the manifest. This half fills the folder in the first
//! place, by running the framework's own installer in a **throwaway
//! container** — `composer create-project` for Laravel and Symfony, wp-cli
//! for WordPress, `create-next-app` for Next.js. Nothing is installed on the
//! host; the container is `--rm` and only the bind-mounted project directory
//! survives it.
//!
//! Scaffolding deliberately ends where adoption begins: once the installer
//! has written the files, the existing `project_adopt` path detects runtime,
//! server and document root from what is actually on disk — the same
//! machinery, whether the code arrived by `git clone` or by this module.
//!
//! On Linux the container would otherwise write root-owned files into the
//! user's checkout, so the invocation carries `--user <uid>:<gid>` on unix.
//! (Docker Desktop on macOS maps ownership anyway; the flag is harmless.)

use serde::Serialize;

/// A framework this module knows how to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Laravel,
    Wordpress,
    Symfony,
    Nextjs,
}

impl Template {
    pub const ALL: [Template; 4] = [
        Template::Laravel,
        Template::Wordpress,
        Template::Symfony,
        Template::Nextjs,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Template::Laravel => "laravel",
            Template::Wordpress => "wordpress",
            Template::Symfony => "symfony",
            Template::Nextjs => "nextjs",
        }
    }

    pub fn parse(s: &str) -> Option<Template> {
        Template::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Image and command for the installer container.
    ///
    /// Every command is fully specified so nothing prompts: an installer
    /// waiting for interactive input inside a `-d`-less `docker run` driven
    /// by an operation console is a hang, not a question.
    fn install(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Template::Laravel => (
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "laravel/laravel",
                    ".",
                ],
            ),
            Template::Symfony => (
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "symfony/skeleton",
                    ".",
                ],
            ),
            Template::Wordpress => ("wordpress:cli", &["wp", "core", "download"]),
            Template::Nextjs => (
                "node:22",
                // Every choice pinned; `--yes` alone still asks about Turbopack
                // on some versions.
                &[
                    "npx",
                    "--yes",
                    "create-next-app@latest",
                    ".",
                    "--ts",
                    "--eslint",
                    "--app",
                    "--no-tailwind",
                    "--no-src-dir",
                    "--import-alias",
                    "@/*",
                    "--use-npm",
                ],
            ),
        }
    }
}

/// The `docker run` invocation that fills `host_dir` with a new project.
///
/// `user` is `Some("uid:gid")` on unix so the files land owned by the person
/// who asked for them, not by root.
pub fn run_args(template: Template, host_dir: &str, user: Option<&str>) -> Vec<String> {
    let (image, command) = template.install();
    let mount = format!("{}:/app", crate::paths::to_docker_mount(host_dir));

    let mut args: Vec<String> = ["run", "--rm", "-v", &mount, "-w", "/app"]
        .into_iter()
        .map(String::from)
        .collect();

    if let Some(user) = user {
        args.push("--user".into());
        args.push(user.into());
        // A non-root user in a stock image has no writable HOME; installers
        // (composer, npm, wp-cli) all want a cache directory.
        args.push("-e".into());
        args.push("HOME=/tmp".into());
    }

    args.push(image.into());
    args.extend(command.iter().map(|s| s.to_string()));
    args
}

/// `uid:gid` of the invoking user, unix only.
pub async fn current_user() -> Option<String> {
    #[cfg(unix)]
    {
        let read = |flag: &str| {
            let flag = flag.to_string();
            async move {
                let out = tokio::process::Command::new("id")
                    .arg(flag)
                    .output()
                    .await
                    .ok()?;
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse::<u32>()
                    .ok()
            }
        };
        let uid = read("-u").await?;
        let gid = read("-g").await?;
        Some(format!("{uid}:{gid}"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_parses_its_own_name() {
        for t in Template::ALL {
            assert_eq!(Template::parse(t.as_str()), Some(t));
        }
        assert_eq!(Template::parse("rails"), None);
    }

    #[test]
    fn the_installer_runs_throwaway_with_only_the_project_dir_surviving() {
        let args = run_args(Template::Laravel, "/Users/x/stackvo/projects/shop", None);
        let line = args.join(" ");
        assert!(line.starts_with("run --rm"));
        assert!(line.contains("-v /Users/x/stackvo/projects/shop:/app"));
        assert!(line.contains("composer:2 create-project"));
        assert!(line.contains("--no-interaction"));
        assert!(line.ends_with("laravel/laravel ."));
    }

    #[test]
    fn a_unix_user_gets_ownership_and_a_writable_home() {
        let args = run_args(Template::Nextjs, "/x/projects/web", Some("501:20"));
        let line = args.join(" ");
        assert!(line.contains("--user 501:20"));
        assert!(line.contains("-e HOME=/tmp"));
        // The flags come before the image, or docker treats them as the
        // container command.
        assert!(line.find("--user").unwrap() < line.find("node:22").unwrap());
    }

    #[test]
    fn nothing_this_runs_can_prompt() {
        for t in Template::ALL {
            let line = run_args(t, "/x", None).join(" ");
            // Interactive installers hang the operation console; each command
            // must either be non-interactive by nature or say so explicitly.
            let non_interactive = line.contains("--no-interaction")
                || line.contains("--yes")
                || line.contains("wp core download");
            assert!(non_interactive, "{t:?} could prompt: {line}");
        }
    }
}
