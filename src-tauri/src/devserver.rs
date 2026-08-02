//! Hot reload for `runtime: node`, behind Traefik.
//!
//! ## What is actually broken
//!
//! P2-15 was written as "proxy the Node dev server through Traefik with HMR",
//! which sounds like a routing change. It is not. Reading
//! `core/cli/lib/generators/project/compose/node.sh` and its Dockerfile
//! generator against the other five servers turns up something larger:
//!
//! **A node project has no bind mount at all.** `nginx.sh`, `caddy.sh`,
//! `apache.sh`, `swoole.sh` and `frankenphp.sh` all call
//! `generate_common_volumes`; `node.sh` calls nothing, and the Dockerfile does
//! `COPY . .` and `RUN npm install` at build time. The container holds a
//! *snapshot* of the source taken when the image was built.
//!
//! So hot reload is not misconfigured — it is structurally impossible. Editing
//! a file on the host changes nothing inside the container, and no amount of
//! WebSocket plumbing helps, because there is nothing to reload. `runtime:
//! node` today is a production-style container: build the app into the image,
//! run it. That is a legitimate mode and it stays the default; what is missing
//! is the other one.
//!
//! ## Three things have to be true at once
//!
//! Reported separately, because they come apart and each has a different fix:
//!
//! 1. **The source is live** — a bind mount over the app directory, plus an
//!    anonymous volume on `node_modules` so the mount does not shadow the
//!    install the image did for its own platform. Delivered by the overlay.
//! 2. **The dev server is what is running** — `npm run dev`, not the manifest's
//!    production start command. Delivered by the overlay's `command:`.
//! 3. **The dev server accepts the request** — and this one is in the *user's*
//!    repository, so it is generated and shown, never written.
//!
//! ## Why the third is not ours to fix
//!
//! Read out of Vite's own source (`hostValidationMiddleware`, verified against
//! the copy in this repository's `node_modules`): a request whose `Host` header
//! is not `localhost`, not `*.localhost`, not an IP literal and not matched by
//! `server.allowedHosts` gets a flat **403**. A `.loc` domain is exactly that
//! case, so the symptom is a site that is plainly up and returns 403 — not a
//! crash, not a log line, nothing that points at the dev server's config.
//!
//! Vite's matcher also accepts a leading dot as a suffix (`.loc` covers
//! `shop.loc`), which is what makes a one-line answer possible.
//!
//! The HMR client is the second half of it. StackVo's node router is
//! `websecure`-only with `tls=true`, so the page loads over 443 while the dev
//! server listens on 3000; Vite's client would dial `wss://shop.loc:3000`,
//! which nothing routes, and silently degrade to full page reloads. It has to
//! be told the client port.
//!
//! Both live in `vite.config.js` — the user's source file, in the user's
//! repository, under the user's review. This app generates the exact snippet,
//! with their domain already in it, and stops there. Silently rewriting
//! somebody's build config is not a thing a local environment manager should
//! ever do, and a snippet they paste is one they have read.
//!
//! ## Why an overlay, for the third time
//!
//! Same reason as [`crate::xdebug`] and [`crate::phpini`]: the generator's
//! output is under a byte-for-byte contract with the Bash implementation. This
//! writes a sixth `-f`, re-derived on every compose invocation, never stored.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the node Dockerfile puts the application.
pub const CONTAINER_PATH: &str = "/app";

/// The state file. Its existence is the switch; its contents are the command.
///
/// A file rather than a flag in the manifest, for two reasons: the manifest
/// schema is `additionalProperties: false` and cannot grow a key, and a file
/// under `.stackvo/` is commit-friendly — a teammate's clone gets dev mode with
/// everything else. Same shape as `.stackvo/php.ini`.
pub const CONFIG_FILE: &str = "devserver.json";

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.devserver.yml")
}

pub fn config_path(root: &Path, name: &str) -> PathBuf {
    crate::workspace::projects_root(root)
        .unwrap_or_default()
        .join(name)
        .join(crate::phpini::CONFIG_DIR)
        .join(CONFIG_FILE)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevConfig {
    /// What to run instead of the manifest's production start command.
    pub command: String,
}

/// Which dev server the project uses, because the config it needs differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Vite,
    /// Nuxt runs Vite underneath, and takes the same `vite.server` block.
    Nuxt,
    Next,
    /// Something that serves HTTP and reloads itself; the mount is still worth
    /// having, but this module has no config advice for it.
    Unknown,
}

impl Tool {
    fn config_names(self) -> &'static [&'static str] {
        match self {
            Tool::Vite => &[
                "vite.config.ts",
                "vite.config.js",
                "vite.config.mjs",
                "vite.config.mts",
            ],
            Tool::Nuxt => &["nuxt.config.ts", "nuxt.config.js"],
            Tool::Next => &["next.config.ts", "next.config.js", "next.config.mjs"],
            Tool::Unknown => &[],
        }
    }

    /// The key that has to appear in that config for a `.loc` domain to be
    /// served at all.
    fn host_key(self) -> Option<&'static str> {
        match self {
            Tool::Vite | Tool::Nuxt => Some("allowedHosts"),
            Tool::Next => Some("allowedDevOrigins"),
            Tool::Unknown => None,
        }
    }
}

// -------------------------------------------------------------- pure logic

/// Read the tool and the dev command out of a `package.json`.
///
/// Order matters: Nuxt and Next both pull Vite or their own bundler in as a
/// dependency, so the framework is checked before the build tool. Getting this
/// backwards would hand a Nuxt project a `vite.config.js` snippet for a file it
/// does not have.
pub fn inspect_package_json(text: &str) -> (Tool, Option<String>) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return (Tool::Unknown, None);
    };

    let has = |name: &str| {
        ["dependencies", "devDependencies"].iter().any(|section| {
            json.get(section)
                .and_then(|v| v.as_object())
                .is_some_and(|deps| deps.contains_key(name))
        })
    };

    let tool = if has("nuxt") {
        Tool::Nuxt
    } else if has("next") {
        Tool::Next
    } else if has("vite") {
        Tool::Vite
    } else {
        Tool::Unknown
    };

    // `dev` is the convention every one of these scaffolds writes. `start` is
    // the fallback, and is deliberately second: in a Next project `start`
    // serves a production build, which is the mode this feature exists to get
    // out of.
    let script = json
        .get("scripts")
        .and_then(|v| v.as_object())
        .and_then(|scripts| {
            ["dev", "start"]
                .iter()
                .find(|key| scripts.contains_key(**key))
                .map(|key| format!("npm run {key}"))
        });

    (tool, script)
}

/// The snippet the project's own config needs, with its domain already in it.
///
/// Generated rather than written. See the module note: this is the user's build
/// config, and a snippet they paste is one they have read.
pub fn config_snippet(tool: Tool, domain: &str, port: u16, ssl: bool) -> Option<String> {
    // Behind Traefik the browser talks to 443 (or 80) while the dev server
    // listens on its own port. Vite's HMR client defaults to the server port,
    // dials a port nothing routes, and degrades to full reloads with no error.
    let (client_port, protocol) = if ssl { (443, "wss") } else { (80, "ws") };

    match tool {
        Tool::Vite => Some(format!(
            "// vite.config.js\nexport default {{\n  \
             server: {{\n    \
             host: true,\n    \
             port: {port},\n    \
             // Vite answers 403 to a Host header it does not know.\n    \
             allowedHosts: ['{domain}'],\n    \
             hmr: {{ host: '{domain}', clientPort: {client_port}, protocol: '{protocol}' }},\n  \
             }},\n}}\n"
        )),
        Tool::Nuxt => Some(format!(
            "// nuxt.config.ts\nexport default defineNuxtConfig({{\n  \
             devServer: {{ host: '0.0.0.0', port: {port} }},\n  \
             vite: {{\n    \
             server: {{\n      \
             allowedHosts: ['{domain}'],\n      \
             hmr: {{ host: '{domain}', clientPort: {client_port}, protocol: '{protocol}' }},\n    \
             }},\n  \
             }},\n}})\n"
        )),
        Tool::Next => Some(format!(
            "// next.config.js\nmodule.exports = {{\n  \
             // Next 15+ rejects cross-origin dev requests without this.\n  \
             allowedDevOrigins: ['{domain}'],\n}}\n"
        )),
        Tool::Unknown => None,
    }
}

/// Render the overlay, or None when no project has dev mode on.
///
/// None rather than an empty document: compose rejects a `services` map with no
/// entries, so "nobody wants this" has to mean "no file".
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from projects/*/.stackvo/devserver.json before every compose\n\
         # command, so edits here are lost. Turn dev mode off in the app to remove it.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and will\n\
         # recreate these containers in production mode without the source mount.\n\
         services:\n",
    );

    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in sorted {
        out.push_str(&format!("  {}:\n", entry.service));
        // A JSON array, so the command is handed to the shell as one string and
        // `npm run dev -- --flag` survives intact.
        out.push_str(&format!(
            "    command: [\"sh\", \"-c\", {}]\n",
            serde_json::to_string(&entry.command).unwrap_or_else(|_| "\"npm run dev\"".into())
        ));
        out.push_str("    environment:\n");
        out.push_str("      NODE_ENV: \"development\"\n");
        out.push_str("    volumes:\n");
        out.push_str(&format!(
            "      - \"{}:{CONTAINER_PATH}\"\n",
            entry.host_path
        ));
        // The anonymous volume is the load-bearing line. Without it the bind
        // above shadows /app/node_modules, and the install the image did for
        // linux/arm64 disappears behind an empty directory — or, worse, behind
        // the host's macOS build of the same packages.
        out.push_str(&format!("      - \"{CONTAINER_PATH}/node_modules\"\n"));
    }

    Some(out)
}

/// One project's worth of overlay input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub service: String,
    pub host_path: String,
    pub command: String,
}

// ------------------------------------------------------------------- I/O

/// Every node project with dev mode on **and** a compose service to attach to.
///
/// The second half is not belt-and-braces — see [`crate::xdebug`]: naming a
/// service the generator did not emit declares one with neither an image nor a
/// build context, and compose then refuses every command against the stack.
fn entries(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();

    let generated =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = crate::xdebug::generated_services(&generated);

    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return out;
    };

    for dir in dirs.flatten() {
        let path = dir.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !services.iter().any(|s| s == name) {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(config_path(root, name)) else {
            continue;
        };
        let Ok(config) = serde_json::from_str::<DevConfig>(&text) else {
            continue;
        };
        if config.command.trim().is_empty() {
            continue;
        }

        // PHP projects have no `/app` and no dev server; mounting over their
        // application directory would replace the site with an empty one.
        let Ok(manifest) = crate::manifest::read(&path.join("stackvo.json"), name) else {
            continue;
        };
        if manifest.runtime != "node" {
            continue;
        }

        out.push(Entry {
            service: name.to_string(),
            host_path: path.display().to_string(),
            command: config.command.trim().to_string(),
        });
    }

    out
}

/// Re-render the overlay, and report whether it now exists.
pub fn sync(root: &Path) -> bool {
    let path = overlay_path(root);

    match overlay_yaml(&entries(root)) {
        Some(yaml) => {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
            match crate::atomic::write(&path, &yaml) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not write the dev-server overlay");
                    let _ = std::fs::remove_file(&path);
                    false
                }
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerStatus {
    /// False for PHP projects; there is no dev server to run.
    pub supported: bool,
    pub enabled: bool,
    pub tool: Tool,
    /// The command dev mode runs, whether or not it is on yet.
    pub command: String,
    /// What the manifest runs in production, for the contrast.
    pub production_command: Option<String>,
    /// Does the *running* container have the source mounted? None when nothing
    /// is running. Read from the container, never inferred — `stackvo up` from
    /// the Bash CLI layers three compose files, not six.
    pub mounted: Option<bool>,
    pub running: bool,
    /// True when dev mode is on but the running container predates it.
    pub needs_recreate: bool,
    /// The project's own dev-server config file, if it has one.
    pub config_file: Option<String>,
    /// Whether that file already names this host. None when there is no config
    /// file to look in — which is not the same as "it does not".
    pub host_allowed: Option<bool>,
    /// What to paste into it. None for a tool this module has no advice for.
    pub snippet: Option<String>,
    pub domain: Option<String>,
    pub port: u16,
    pub overlay_path: String,
}

/// What is true for one project, across all three layers.
pub async fn status(root: &Path, name: &str) -> Result<DevServerStatus> {
    let dir = crate::workspace::require_projects_root(root)?.join(name);
    let manifest_file = dir.join("stackvo.json");
    if !manifest_file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&manifest_file, name)?;
    let supported = manifest.runtime == "node";

    let package_json = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
    let (tool, detected_command) = inspect_package_json(&package_json);

    let stored = std::fs::read_to_string(config_path(root, name))
        .ok()
        .and_then(|text| serde_json::from_str::<DevConfig>(&text).ok());
    let enabled = stored.is_some();

    let command = stored
        .map(|c| c.command)
        .or(detected_command)
        .unwrap_or_else(|| "npm run dev".to_string());

    let port = manifest.node.as_ref().map(|n| n.port).unwrap_or(3000);

    // The project's own config, and whether it already names this host. Read as
    // text on purpose: parsing JavaScript to find out whether a key is present
    // is a much larger promise than "does the file mention it", and the answer
    // is only ever used to decide whether to show a snippet.
    let mut config_file = None;
    let mut host_allowed = None;
    if let Some(key) = tool.host_key() {
        for candidate in tool.config_names() {
            let path = dir.join(candidate);
            if let Ok(text) = std::fs::read_to_string(&path) {
                config_file = Some(path.display().to_string());
                host_allowed = Some(text.contains(key));
                break;
            }
        }
    }

    let ssl = crate::config::Env::load(root)
        .map(|env| env.bool("SSL_ENABLE"))
        .unwrap_or(true);

    let details = crate::engine::inspect(name).await.ok();
    let running = details.as_ref().is_some_and(|d| d.running);
    let mounted = details
        .as_ref()
        .map(|d| d.mounts.iter().any(|m| m.destination == CONTAINER_PATH));

    Ok(DevServerStatus {
        supported,
        enabled,
        tool,
        snippet: manifest
            .domain
            .as_deref()
            .and_then(|domain| config_snippet(tool, domain, port, ssl)),
        production_command: manifest.node.as_ref().map(|n| n.start.clone()),
        needs_recreate: enabled && mounted == Some(false),
        command,
        mounted,
        running,
        config_file,
        host_allowed,
        domain: manifest.domain.clone(),
        port,
        overlay_path: overlay_path(root).display().to_string(),
    })
}

/// Turn dev mode on or off.
pub async fn set(
    root: &Path,
    name: &str,
    enabled: bool,
    command: Option<String>,
) -> Result<DevServerStatus> {
    let dir = crate::workspace::require_projects_root(root)?.join(name);
    let manifest_file = dir.join("stackvo.json");
    if !manifest_file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&manifest_file, name)?;
    if manifest.runtime != "node" {
        return Err(Error::new(
            Code::Unsupported,
            format!(
                "{name} is a {} project; the dev server applies to node only",
                manifest.runtime
            ),
        ));
    }

    let path = config_path(root, name);

    if enabled {
        let command = command
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .or_else(|| {
                let text = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
                inspect_package_json(&text).1
            })
            .unwrap_or_else(|| "npm run dev".to_string());

        // A command with a newline would break out of the YAML scalar the
        // overlay writes it into. Refused rather than escaped: there is no
        // legitimate multi-line start command.
        if command.contains('\n') || command.contains('\r') {
            return Err(Error::new(
                Code::InvalidInput,
                "the dev command contains a line break",
            ));
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        let text = serde_json::to_string_pretty(&DevConfig { command })
            .map_err(|e| Error::new(Code::IoError, format!("serialising the dev config: {e}")))?;
        crate::atomic::write(&path, &format!("{text}\n"))?;
    } else {
        let _ = std::fs::remove_file(&path);
    }

    sync(root);
    status(root, name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nuxt and Next both pull a bundler in as a dependency. Checked in the
    /// wrong order, a Nuxt project gets told to edit a `vite.config.js` it does
    /// not have.
    #[test]
    fn the_framework_is_recognised_before_its_bundler() {
        let nuxt = r#"{"devDependencies":{"nuxt":"3.14.0","vite":"5.4.0"},
                       "scripts":{"dev":"nuxt dev"}}"#;
        assert_eq!(inspect_package_json(nuxt).0, Tool::Nuxt);

        let next = r#"{"dependencies":{"next":"15.0.0"},"scripts":{"dev":"next dev"}}"#;
        assert_eq!(inspect_package_json(next).0, Tool::Next);

        let vite = r#"{"devDependencies":{"vite":"7.0.0"},"scripts":{"dev":"vite"}}"#;
        assert_eq!(inspect_package_json(vite).0, Tool::Vite);

        let plain = r#"{"dependencies":{"express":"4"},"scripts":{"start":"node index.js"}}"#;
        assert_eq!(inspect_package_json(plain).0, Tool::Unknown);
    }

    /// `start` in a Next project serves a production build, which is the mode
    /// this feature exists to get out of. `dev` comes first for that reason.
    #[test]
    fn the_dev_script_is_preferred_over_start() {
        let both = r#"{"scripts":{"start":"next start","dev":"next dev"}}"#;
        assert_eq!(inspect_package_json(both).1.as_deref(), Some("npm run dev"));

        let only_start = r#"{"scripts":{"start":"node server.js"}}"#;
        assert_eq!(
            inspect_package_json(only_start).1.as_deref(),
            Some("npm run start")
        );

        let neither = r#"{"scripts":{"build":"tsc"}}"#;
        assert_eq!(inspect_package_json(neither).1, None);
    }

    #[test]
    fn a_malformed_package_json_narrows_the_answer_rather_than_failing() {
        assert_eq!(inspect_package_json("{not json"), (Tool::Unknown, None));
    }

    /// The two things Vite has to be told, and the reason each is needed.
    /// `allowedHosts` or the request is a 403; `clientPort` or the HMR socket
    /// dials the container port through a proxy that only listens on 443.
    #[test]
    fn the_vite_snippet_carries_the_host_and_the_client_port() {
        let snippet = config_snippet(Tool::Vite, "shop.loc", 5173, true).unwrap();

        assert!(snippet.contains("allowedHosts: ['shop.loc']"), "{snippet}");
        assert!(snippet.contains("clientPort: 443"), "{snippet}");
        assert!(snippet.contains("protocol: 'wss'"), "{snippet}");
        assert!(snippet.contains("port: 5173"), "{snippet}");
    }

    /// With TLS off the stack serves plain HTTP on 80, and a `wss` client on
    /// 443 would fail to connect against a listener that is not there.
    #[test]
    fn the_snippet_follows_whether_tls_is_on() {
        let plain = config_snippet(Tool::Vite, "shop.loc", 5173, false).unwrap();
        assert!(plain.contains("clientPort: 80"), "{plain}");
        assert!(plain.contains("protocol: 'ws'"), "{plain}");
    }

    #[test]
    fn next_gets_its_own_key_and_unknown_tools_get_no_advice() {
        let next = config_snippet(Tool::Next, "shop.loc", 3000, true).unwrap();
        assert!(next.contains("allowedDevOrigins: ['shop.loc']"), "{next}");
        // No `hmr` block: Next does not take one, and inventing config for a
        // tool that ignores it is worse than saying nothing.
        assert!(!next.contains("hmr"));

        assert!(config_snippet(Tool::Unknown, "shop.loc", 3000, true).is_none());
    }

    /// The line the whole feature turns on. Without the anonymous volume the
    /// bind shadows `/app/node_modules`, and the install the image did for its
    /// own platform vanishes behind an empty directory — or behind the host's
    /// macOS build of the same native packages, which is worse because it
    /// starts and then crashes somewhere unrelated.
    #[test]
    fn the_overlay_protects_node_modules_from_the_bind_mount() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            host_path: "/w/projects/shop".into(),
            command: "npm run dev".into(),
        }])
        .unwrap();

        assert!(yaml.contains("  shop:\n"));
        assert!(yaml.contains("- \"/w/projects/shop:/app\""), "{yaml}");
        assert!(yaml.contains("- \"/app/node_modules\""), "{yaml}");
        assert!(
            yaml.contains("command: [\"sh\", \"-c\", \"npm run dev\"]"),
            "{yaml}"
        );
        assert!(yaml.contains("NODE_ENV: \"development\""), "{yaml}");
    }

    /// A command with a quote in it has to survive into the YAML as one scalar,
    /// or the overlay is a syntax error and every compose command fails.
    #[test]
    fn a_command_with_quotes_is_serialised_safely() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            host_path: "/w/projects/shop".into(),
            command: r#"npm run dev -- --host "0.0.0.0""#.into(),
        }])
        .unwrap();

        assert!(yaml.contains(r#"\"0.0.0.0\""#), "{yaml}");
    }

    #[test]
    fn an_empty_overlay_is_no_overlay() {
        assert!(overlay_yaml(&[]).is_none());
    }
}
