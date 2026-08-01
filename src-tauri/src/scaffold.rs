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

use crate::error::{Error, Result};
use serde::Serialize;
use std::path::Path;

/// The two ways a template can fill a directory.
///
/// A container run is the right answer when the ecosystem *has* a scaffolder —
/// `composer create-project` and friends know their own conventions better
/// than a template here ever will, and they stay current without anyone
/// editing this file.
///
/// Six of the frameworks people ask for have no scaffolder at all: Gin, Echo,
/// Flask, FastAPI, Sinatra and Rocket are each "write two files and run the
/// language's own build". Pulling a whole image to write 30 lines would be a
/// download for nothing, so those are written directly — and the dependency
/// install is not skipped, it *moves*: the project's own Dockerfile already
/// runs `pip install -r requirements.txt`, `go build`, `bundle install` or
/// `cargo build --release` for the container's platform, which is where those
/// belong anyway.
pub enum Installer {
    /// `docker run <image> <args…>` over the bind-mounted project directory.
    Container(&'static str, &'static [&'static str]),
    /// `(relative path, contents)` written into the project directory.
    Files(&'static [(&'static str, &'static str)]),
}

/// A framework this module knows how to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Laravel,
    Wordpress,
    Symfony,
    Nextjs,
    Nuxt,
    Vue,
    React,
    Svelte,
    Astro,
    // PHP frameworks and CMSes, all of them `composer create-project`.
    Cakephp,
    Yii,
    Codeigniter,
    Laminas,
    Drupal,
    Prestashop,
    // The lang runtimes, now that they have generators.
    Django,
    Rails,
    Slim,
    Nest,
    Tina,
    Angular,
    Typo3,
    // Written directly: these six have no scaffolder of their own.
    Gin,
    Echo,
    Flask,
    Fastapi,
    Sinatra,
    Rocket,
}

impl Template {
    pub const ALL: [Template; 28] = [
        Template::Laravel,
        Template::Wordpress,
        Template::Symfony,
        Template::Nextjs,
        Template::Nuxt,
        Template::Vue,
        Template::React,
        Template::Svelte,
        Template::Astro,
        Template::Cakephp,
        Template::Yii,
        Template::Codeigniter,
        Template::Laminas,
        Template::Drupal,
        Template::Prestashop,
        Template::Django,
        Template::Rails,
        Template::Slim,
        Template::Nest,
        Template::Tina,
        Template::Angular,
        Template::Typo3,
        Template::Gin,
        Template::Echo,
        Template::Flask,
        Template::Fastapi,
        Template::Sinatra,
        Template::Rocket,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Template::Laravel => "laravel",
            Template::Wordpress => "wordpress",
            Template::Symfony => "symfony",
            Template::Nextjs => "nextjs",
            Template::Nuxt => "nuxt",
            Template::Vue => "vue",
            Template::React => "react",
            Template::Svelte => "svelte",
            Template::Astro => "astro",
            Template::Cakephp => "cakephp",
            Template::Yii => "yii",
            Template::Codeigniter => "codeigniter",
            Template::Laminas => "laminas",
            Template::Drupal => "drupal",
            Template::Prestashop => "prestashop",
            Template::Django => "django",
            Template::Rails => "rails",
            Template::Slim => "slim",
            Template::Nest => "nest",
            Template::Tina => "tina",
            Template::Angular => "angular",
            Template::Typo3 => "typo3",
            Template::Gin => "gin",
            Template::Echo => "echo",
            Template::Flask => "flask",
            Template::Fastapi => "fastapi",
            Template::Sinatra => "sinatra",
            Template::Rocket => "rocket",
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
    pub fn installer(self) -> Installer {
        match self {
            Template::Laravel => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "laravel/laravel",
                    ".",
                ],
            ),
            Template::Symfony => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "symfony/skeleton",
                    ".",
                ],
            ),
            Template::Wordpress => Installer::Container("wordpress:cli", &["wp", "core", "download"]),

            // The JavaScript installers. `--no-install` is not a shortcut: a
            // node project's Dockerfile runs `npm install` for the container's
            // own platform, and a host `node_modules` copied into the image is
            // how an arm64 binary ends up in an amd64 container — the very
            // thing NODE_DOCKERIGNORE exists to prevent.
            Template::Nuxt => Installer::Container(
                "node:22",
                // `--template` and `--force` are both required: without the
                // first it refuses on a non-interactive terminal, without the
                // second it refuses because the mount point already exists.
                &[
                    "npx",
                    "nuxi@latest",
                    "init",
                    ".",
                    "--template",
                    "minimal",
                    "--packageManager",
                    "npm",
                    "--gitInit",
                    "false",
                    "--no-install",
                    "--force",
                ],
            ),
            Template::Vue => Installer::Container(
                "node:22",
                &[
                    "npm",
                    "create",
                    "vite@latest",
                    ".",
                    "--",
                    "--template",
                    "vue-ts",
                ],
            ),
            Template::React => Installer::Container(
                "node:22",
                &[
                    "npm",
                    "create",
                    "vite@latest",
                    ".",
                    "--",
                    "--template",
                    "react-ts",
                ],
            ),
            // SvelteKit rather than Vite's svelte template: the router and the
            // dev server are the part a local environment manager can host.
            Template::Svelte => Installer::Container(
                "node:22",
                &[
                    "npx",
                    "sv@latest",
                    "create",
                    "--template",
                    "minimal",
                    "--types",
                    "ts",
                    "--no-add-ons",
                    "--no-install",
                    ".",
                ],
            ),
            // `--ignore-platform-reqs` on every composer project except the
            // two that predate it: the *installer* container is not the
            // machine the code will run on. CakePHP asks for ext-intl and the
            // composer image has none, so without it the install fails on a
            // requirement StackVo's own PHP image satisfies. Measured: both
            // CakePHP and CodeIgniter exit 2 without the flag, 0 with it.
            Template::Cakephp => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "cakephp/app",
                    ".",
                ],
            ),
            Template::Yii => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "yiisoft/yii2-app-basic",
                    ".",
                ],
            ),
            Template::Codeigniter => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "codeigniter4/appstarter",
                    ".",
                ],
            ),
            // Zend Framework's maintained successor; the Zend packages are
            // abandoned and `composer create-project zendframework/*` installs
            // code nobody has patched since 2019.
            Template::Laminas => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "laminas/laminas-mvc-skeleton",
                    ".",
                ],
            ),
            Template::Drupal => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "drupal/recommended-project",
                    ".",
                ],
            ),
            Template::Prestashop => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "prestashop/prestashop",
                    ".",
                ],
            ),
            // The lang runtimes. Django ships its own scaffolder; Rails needs
            // the *full* ruby image — the slim one has no toolchain, and
            // installing rails there dies compiling websocket-driver's native
            // extension. Measured both ways.
            Template::Django => Installer::Container(
                "python:3.13-slim",
                &[
                    "sh",
                    "-c",
                    "pip install --quiet django && django-admin startproject app .",
                ],
            ),
            Template::Rails => Installer::Container(
                "ruby:3.3",
                &[
                    "sh",
                    "-c",
                    "gem install rails --no-document -q && rails new . --skip-bundle --skip-git --force",
                ],
            ),
            Template::Slim => Installer::Container(
                "composer:2",
                &[
                    "create-project",
                    "--prefer-dist",
                    "--no-interaction",
                    "--ignore-platform-reqs",
                    "slim/slim-skeleton",
                    ".",
                ],
            ),
            Template::Nest => Installer::Container(
                "node:22",
                &[
                    "npx",
                    "-y",
                    "@nestjs/cli",
                    "new",
                    ".",
                    "--skip-git",
                    "--skip-install",
                    "--package-manager",
                    "npm",
                ],
            ),
            // `--pkg-manager` is required, not tidiness: without it the
            // installer asks which one to use and an operation console has no
            // way to answer. Measured — it hangs on the prompt.
            Template::Tina => Installer::Container(
                "node:22",
                &[
                    "npx",
                    "-y",
                    "create-tina-app@latest",
                    ".",
                    "--template",
                    "basic",
                    "--pkg-manager",
                    "npm",
                ],
            ),
            Template::Angular => Installer::Container(
                "node:22",
                &[
                    "npx", "-y", "@angular/cli@latest", "new", "app",
                    "--directory", ".", "--skip-git", "--skip-install", "--defaults",
                ],
            ),
            Template::Typo3 => Installer::Container(
                "composer:2",
                &[
                    "create-project", "--prefer-dist", "--no-interaction",
                    "--ignore-platform-reqs", "typo3/cms-base-distribution", ".",
                ],
            ),

            // ---- written, not run ------------------------------------------
            //
            // Each is the framework's own hello-world, bound to 0.0.0.0 on the
            // port `manifest::lang_defaults` gives its runtime. The binding is
            // the load-bearing part: a server on 127.0.0.1 inside a container
            // answers nothing from outside it, and Traefik is outside it — the
            // same failure the node dev server taught.
            Template::Gin => Installer::Files(&[
                ("go.mod", GIN_MOD),
                ("main.go", GIN_MAIN),
            ]),
            Template::Echo => Installer::Files(&[
                ("go.mod", ECHO_MOD),
                ("main.go", ECHO_MAIN),
            ]),
            Template::Flask => Installer::Files(&[
                ("requirements.txt", "flask>=3.0\n"),
                ("main.py", FLASK_MAIN),
            ]),
            Template::Fastapi => Installer::Files(&[
                ("requirements.txt", "fastapi>=0.115\nuvicorn[standard]>=0.32\n"),
                ("main.py", FASTAPI_MAIN),
            ]),
            Template::Sinatra => Installer::Files(&[
                ("Gemfile", SINATRA_GEMFILE),
                ("app.rb", SINATRA_APP),
            ]),
            Template::Rocket => Installer::Files(&[
                ("Cargo.toml", ROCKET_CARGO),
                ("Rocket.toml", ROCKET_CONFIG),
                ("src/main.rs", ROCKET_MAIN),
            ]),
            Template::Astro => Installer::Container(
                "node:22",
                &[
                    "npm",
                    "create",
                    "astro@latest",
                    ".",
                    "--",
                    "--template",
                    "minimal",
                    "--no-install",
                    "--no-git",
                    "--skip-houston",
                    "--yes",
                ],
            ),
            Template::Nextjs => Installer::Container(
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
pub fn run_args(template: Template, host_dir: &str, user: Option<&str>) -> Option<Vec<String>> {
    let Installer::Container(image, command) = template.installer() else {
        // A written template runs nothing; the caller writes the files.
        return None;
    };
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
    Some(args)
}

/// Write a template's files into `dir`, creating parent directories as it
/// goes (`src/main.rs` needs `src/`). Returns what it wrote.
pub fn write_files(template: Template, dir: &Path) -> Result<Vec<String>> {
    let Installer::Files(files) = template.installer() else {
        return Ok(Vec::new());
    };

    let mut written = Vec::new();
    for (name, contents) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        std::fs::write(&path, contents)
            .map_err(|e| Error::io(format!("writing {}", path.display()), e))?;
        written.push((*name).to_string());
    }
    Ok(written)
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

// ---- the written templates ------------------------------------------------
//
// Raw strings so the code inside reads as the code it is. Every one binds to
// 0.0.0.0 and to the port `manifest::lang_defaults` gives its runtime, so the
// project the adopter writes and the server the file starts agree without
// anyone editing either.

const GIN_MOD: &str = r#"module app

go 1.23

require github.com/gin-gonic/gin v1.10.0
"#;

const GIN_MAIN: &str = r#"package main

import "github.com/gin-gonic/gin"

func main() {
	r := gin.Default()
	r.GET("/", func(c *gin.Context) {
		c.JSON(200, gin.H{"message": "Gin is running on StackVo"})
	})
	// 0.0.0.0, not localhost: Traefik reaches this from outside the container.
	_ = r.Run("0.0.0.0:8080")
}
"#;

const ECHO_MOD: &str = r#"module app

go 1.23

require github.com/labstack/echo/v4 v4.12.0
"#;

const ECHO_MAIN: &str = r#"package main

import (
	"net/http"

	"github.com/labstack/echo/v4"
)

func main() {
	e := echo.New()
	e.GET("/", func(c echo.Context) error {
		return c.JSON(http.StatusOK, map[string]string{"message": "Echo is running on StackVo"})
	})
	e.Logger.Fatal(e.Start("0.0.0.0:8080"))
}
"#;

const FLASK_MAIN: &str = r#"from flask import Flask

app = Flask(__name__)


@app.route("/")
def index():
    return {"message": "Flask is running on StackVo"}


if __name__ == "__main__":
    # host=0.0.0.0 so Traefik can reach it from outside the container.
    app.run(host="0.0.0.0", port=8000)
"#;

const FASTAPI_MAIN: &str = r#"from fastapi import FastAPI

app = FastAPI()


@app.get("/")
def read_root():
    return {"message": "FastAPI is running on StackVo"}


if __name__ == "__main__":
    # Started through main.py rather than the uvicorn CLI, so the manifest's
    # default start command (`python main.py`) is the one that works.
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
"#;

const SINATRA_GEMFILE: &str = r#"source "https://rubygems.org"

gem "sinatra", "~> 4.1"
gem "puma"
gem "rackup"
"#;

const SINATRA_APP: &str = r#"require "sinatra"

# bind, not localhost — a container's loopback is not reachable from outside.
set :bind, "0.0.0.0"
set :port, 4567

get "/" do
  content_type :json
  '{"message":"Sinatra is running on StackVo"}'
end
"#;

const ROCKET_CARGO: &str = r#"[package]
name = "app"
version = "0.1.0"
edition = "2021"

[dependencies]
rocket = { version = "0.5", features = ["json"] }
"#;

const ROCKET_CONFIG: &str = r#"[default]
# Rocket binds to 127.0.0.1 unless told otherwise, which inside a container
# means nothing outside it can connect.
address = "0.0.0.0"
port = 8080
"#;

const ROCKET_MAIN: &str = r#"#[macro_use]
extern crate rocket;

#[get("/")]
fn index() -> &'static str {
    "Rocket is running on StackVo"
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![index])
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_parses_its_own_name() {
        for t in Template::ALL {
            assert_eq!(Template::parse(t.as_str()), Some(t));
        }
        // Rails became real in the meantime; the unknown case needs a name
        // that is still not one of ours.
        assert_eq!(Template::parse("spring-boot"), None);
        // Nine, and each one is a different installer contract.
        assert_eq!(Template::ALL.len(), 28);
    }

    #[test]
    fn the_installer_runs_throwaway_with_only_the_project_dir_surviving() {
        let args = run_args(Template::Laravel, "/Users/x/stackvo/projects/shop", None).unwrap();
        let line = args.join(" ");
        assert!(line.starts_with("run --rm"));
        assert!(line.contains("-v /Users/x/stackvo/projects/shop:/app"));
        assert!(line.contains("composer:2 create-project"));
        assert!(line.contains("--no-interaction"));
        assert!(line.ends_with("laravel/laravel ."));
    }

    #[test]
    fn a_unix_user_gets_ownership_and_a_writable_home() {
        let args = run_args(Template::Nextjs, "/x/projects/web", Some("501:20")).unwrap();
        let line = args.join(" ");
        assert!(line.contains("--user 501:20"));
        assert!(line.contains("-e HOME=/tmp"));
        // The flags come before the image, or docker treats them as the
        // container command.
        assert!(line.find("--user").unwrap() < line.find("node:22").unwrap());
    }

    #[test]
    fn a_written_template_lands_its_files_and_binds_to_all_interfaces() {
        let dir = std::env::temp_dir().join(format!("stackvo-tpl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Rocket is the shape that proves the nesting: src/main.rs needs a
        // directory that does not exist yet.
        let written = write_files(Template::Rocket, &dir).unwrap();
        assert!(written.contains(&"src/main.rs".to_string()));
        assert!(dir.join("src/main.rs").is_file());

        // The one property every written template must hold: bound to
        // 0.0.0.0, never loopback. A container's localhost answers nothing
        // from outside it, and Traefik is outside it.
        for t in [
            Template::Gin,
            Template::Echo,
            Template::Flask,
            Template::Fastapi,
            Template::Sinatra,
            Template::Rocket,
        ] {
            let Installer::Files(files) = t.installer() else {
                panic!("{t:?} should be written, not run");
            };
            let body: String = files.iter().map(|(_, c)| *c).collect();
            assert!(body.contains("0.0.0.0"), "{t:?} does not bind to 0.0.0.0");

            // Comments are where these templates *explain* the loopback trap,
            // so scanning the whole file for 127.0.0.1 flags the explanation
            // as the bug. Only code lines count.
            let loopback = body
                .lines()
                .map(str::trim)
                .filter(|l| !l.starts_with('#') && !l.starts_with("//"))
                .any(|l| l.contains("127.0.0.1"));
            assert!(!loopback, "{t:?} binds to loopback in code");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_this_runs_can_prompt() {
        for t in Template::ALL {
            // A written template runs no process, so it cannot ask anything —
            // the guard is about installers.
            let Some(args) = run_args(t, "/x", None) else {
                continue;
            };
            let line = args.join(" ");
            // Interactive installers hang the operation console; each command
            // must either be non-interactive by nature or say so explicitly.
            // Each of these was measured against a real container: with the
            // flag the installer completes, without it the run either asks a
            // question (which an operation console cannot answer) or refuses.
            let non_interactive = line.contains("--no-interaction") // composer
                || line.contains("--yes") // astro
                || line.contains("wp core download") // wp-cli takes none
                || line.contains("--force") // nuxi, after --template
                || line.contains("--no-add-ons") // sv
                || line.contains("--template vue-ts") // vite
                || line.contains("--template react-ts")
                || line.contains("--skip-git --force") // rails
                || line.contains("django-admin startproject") // takes none
                || line.contains("--skip-install") // nest
                || line.contains("--pkg-manager npm"); // create-tina-app
            assert!(non_interactive, "{t:?} could prompt: {line}");
        }
    }
}
