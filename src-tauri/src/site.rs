//! Two per-project settings that reach the container through this app's own
//! layers rather than through the manifest (M-5, M-6).
//!
//! ## Why they share a file
//!
//! They are the same kind of thing: a switch somebody sets on one project, that
//! the generator cannot read from the manifest because
//! `contracts/project.schema.json` is `additionalProperties: false` and the
//! contract is frozen. `.stackvo/` is the established answer — `php.ini`,
//! `xdebug.json`, `devserver.json` and `perf.json` are all there, and all of
//! them travel with the project when a teammate clones it.
//!
//! Two settings, one file, because a second file per switch is how a directory
//! becomes a place people stop looking.
//!
//! ## Where each one lands, and why that is not the same place
//!
//! **Environment variables** go into the compose overlay, as `environment:` on
//! the project's service. They are the container's, not the web server's, and a
//! `.env` inside the project would be the application's own file — which this
//! app has no business writing, because Laravel, Symfony and every framework
//! since already own it.
//!
//! **Directory listing** goes into the *server* configuration, because that is
//! what it is: `autoindex on` for nginx, `file_server browse` for Caddy. It
//! reuses the per-project server-directives mechanism that already exists
//! rather than adding a second path into the same file.
//!
//! ## What this refuses
//!
//! A value with a newline in it. The overlay is YAML and the directive block is
//! nginx configuration, and in both a newline in a value is not an escaping
//! problem to solve — it is a second directive somebody else wrote.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "site.json";

/// One project's settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Forward the host's SSH agent into the container (M-10).
    ///
    /// What it is for: `composer install` on a private repository, and
    /// `git pull` inside the container. Both need a key, and the alternative
    /// people reach for is copying `~/.ssh/id_ed25519` into the image — a
    /// private key baked into a layer that is then pushed somewhere.
    ///
    /// Off by default, and per project rather than global: forwarding an agent
    /// lets anything running in that container **sign with every key in it**
    /// for as long as it is up. That is the right trade while a dependency
    /// install is running and the wrong one as a permanent setting on a
    /// container running somebody else's code.
    #[serde(default)]
    pub ssh_agent: bool,
    /// Environment variables for the project's own container (M-5).
    ///
    /// Sorted, because the overlay is rendered from it before every compose
    /// command and a map that reorders would rewrite the file for nothing.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Serve a directory index where there is no index file (M-6).
    #[serde(default)]
    pub directory_listing: bool,
}

pub fn config_path(root: &Path, name: &str) -> PathBuf {
    crate::workspace::projects_root(root)
        .unwrap_or_default()
        .join(name)
        .join(crate::phpini::CONFIG_DIR)
        .join(FILE_NAME)
}

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.site.yml")
}

/// Read one project's settings, dropping anything that is not writable back.
pub fn read(root: &Path, name: &str) -> Config {
    std::fs::read_to_string(config_path(root, name))
        .ok()
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .map(|mut config| {
            config
                .env
                .retain(|key, value| checked_key(key).is_ok() && checked_value(value).is_ok());
            config
        })
        .unwrap_or_default()
}

pub fn write(root: &Path, name: &str, config: &Config) -> Result<()> {
    for (key, value) in &config.env {
        checked_key(key)?;
        checked_value(value)?;
    }

    let path = config_path(root, name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    let text = serde_json::to_string_pretty(config)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the settings: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))
}

/// Is this a name a shell would hand to a process?
///
/// The POSIX rule, and the same one `env_writer` enforces on the workspace
/// `.env`: letters, digits and underscores, not starting with a digit. A key
/// with a space in it is not a variable nothing can read — it is a line compose
/// parses as something else.
pub fn checked_key(key: &str) -> Result<()> {
    let ok = !key.is_empty()
        && key.len() <= 128
        && !key.starts_with(|c: char| c.is_ascii_digit())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

    if !ok {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{key}\" is not a usable environment variable name"),
        )
        .with_hint(crate::hints::ENV_KEY_CHARSET));
    }
    Ok(())
}

/// A value that stays one value.
pub fn checked_value(value: &str) -> Result<()> {
    if value.len() > 4096 {
        return Err(Error::new(
            Code::InvalidInput,
            "that value is too long for an environment variable".to_string(),
        ));
    }
    // A newline in a YAML scalar ends it. Everything after would be read as
    // more configuration, at whatever indentation it happens to have.
    if value.contains(['\n', '\r']) || value.contains('\0') {
        return Err(Error::new(
            Code::InvalidInput,
            "an environment variable's value cannot contain a line break".to_string(),
        )
        .with_hint(crate::hints::ENV_IS_ONE_KEY_PER_LINE));
    }
    Ok(())
}

// ------------------------------------------------------------------ M-6

/// The server directives that turn a directory index on.
///
/// Returned per server rather than written once, because the two that have a
/// configuration file spell it differently and the three that do not have one
/// cannot do it at all — Apache is configured by `sed` inside its own
/// Dockerfile and Swoole by an inline script, which is the same boundary
/// `ServerExtras` already documents.
pub fn listing_directives(server: &str) -> Option<&'static str> {
    match server {
        // `autoindex_exact_size off` prints 4.0K rather than 4096, which is
        // what a person reading a directory listing wants.
        "nginx" => Some("autoindex on;\nautoindex_exact_size off;\nautoindex_localtime on;"),
        "caddy" | "frankenphp" => Some("file_server browse"),
        _ => None,
    }
}

// -------------------------------------------------------------- the overlay

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub service: String,
    pub env: BTreeMap<String, String>,
    /// Whether this project asked for the agent (M-10).
    pub ssh_agent: bool,
}

/// Where the host's agent socket is, as the container has to reach it.
///
/// **Not `$SSH_AUTH_SOCK`.** On macOS and Windows the daemon runs in a VM and
/// the host's socket path means nothing inside it; Docker Desktop publishes the
/// agent at a fixed path instead, and mounting that is the documented way. On
/// Linux the daemon is the host, so the real socket is the answer and the fixed
/// path does not exist.
///
/// Returned as `(host source, container target)` because they differ: the
/// target is always the same inside the container, so `SSH_AUTH_SOCK` can be a
/// constant and every project's container looks alike.
///
/// Measured on macOS rather than taken from documentation. The host's own
/// socket was `/var/run/com.apple.launchd.dV6erVP2st/Listeners`, and mounting
/// Docker Desktop's path instead produced a container that could **talk to the
/// agent**: `ssh-add -l` inside it answered *"The agent has no identities"* —
/// which is the agent replying. A socket that had not been forwarded gives
/// *"Could not open a connection to your authentication agent"*, and the two
/// messages are the whole difference between this working and not.
pub fn agent_socket() -> Option<(String, &'static str)> {
    const TARGET: &str = "/run/host-services/ssh-auth.sock";

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        // Docker Desktop's own magic path. It exists whether or not an agent is
        // running on the host, so the check that matters is the one below.
        std::env::var_os("SSH_AUTH_SOCK")
            .filter(|value| !value.is_empty())
            .map(|_| ("/run/host-services/ssh-auth.sock".to_string(), TARGET))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let socket = std::env::var_os("SSH_AUTH_SOCK")?;
        let path = std::path::PathBuf::from(&socket);
        path.exists().then(|| (path.display().to_string(), TARGET))
    }
}

/// Render the overlay, or `None` when no project sets anything.
///
/// The same rule the other three overlays follow: compose rejects a file whose
/// `services` map is empty, so "nothing to add" has to mean "no file".
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    let agent = agent_socket();
    let entries: Vec<&Entry> = entries
        .iter()
        .filter(|e| !e.env.is_empty() || (e.ssh_agent && agent.is_some()))
        .collect();
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from projects/*/.stackvo/site.json before every compose\n\
         # command, so edits here are lost. Change them in the app instead.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and\n\
         # will recreate these containers without these variables.\n\
         services:\n",
    );

    let mut sorted = entries.clone();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in sorted {
        out.push_str(&format!("  {}:\n", entry.service));

        let forwarding = entry.ssh_agent.then_some(agent.as_ref()).flatten();
        if !entry.env.is_empty() || forwarding.is_some() {
            out.push_str("    environment:\n");
        }
        for (key, value) in &entry.env {
            // Double-quoted with the two characters YAML reads inside quotes
            // escaped. A value is one line by the time it gets here —
            // `checked_value` refuses the rest — so this is the whole of the
            // quoting problem rather than the start of one.
            out.push_str(&format!("      {key}: \"{}\"\n", escape(value)));
        }
        if let Some((source, target)) = forwarding {
            out.push_str(&format!("      SSH_AUTH_SOCK: \"{target}\"\n"));
            // Quoted because a Windows host path holds a colon, which is what
            // compose splits a mount on — the same rule `phpini` follows.
            out.push_str(&format!("    volumes:\n      - \"{source}:{target}\"\n"));
        }
    }

    Some(out)
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Every project with variables **and** a compose service to set them on.
fn entries(root: &Path) -> Vec<Entry> {
    let Some(projects) = crate::workspace::projects_root(root) else {
        return Vec::new();
    };
    let Ok(dir) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };

    let compose =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = crate::xdebug::generated_services(&compose);

    let mut out = Vec::new();
    for item in dir.flatten() {
        let Some(name) = item
            .path()
            .file_name()
            .and_then(|n| n.to_str().map(str::to_string))
        else {
            continue;
        };
        if !services.contains(&name) {
            continue;
        }
        let config = read(root, &name);
        if config.env.is_empty() && !config.ssh_agent {
            continue;
        }
        out.push(Entry {
            service: name,
            env: config.env,
            ssh_agent: config.ssh_agent,
        });
    }

    out.sort_by(|a, b| a.service.cmp(&b.service));
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
            crate::atomic::write(&path, &yaml).is_ok()
        }
        None => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)], listing: bool) -> Config {
        Config {
            env: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            directory_listing: listing,
            ssh_agent: false,
        }
    }

    /// M-10. The socket the container mounts is a fixed path on the two
    /// platforms where the daemon is in a VM, and the real one on the platform
    /// where it is not — mounting `$SSH_AUTH_SOCK` on macOS forwards a path
    /// that means nothing inside the VM.
    #[test]
    fn the_agent_is_forwarded_through_the_path_the_platform_has() {
        let entry = Entry {
            service: "shop".into(),
            env: BTreeMap::new(),
            ssh_agent: true,
        };

        match agent_socket() {
            Some((source, target)) => {
                let yaml = overlay_yaml(&[entry]).expect("an overlay");
                assert!(
                    yaml.contains(&format!("SSH_AUTH_SOCK: \"{target}\"")),
                    "{yaml}"
                );
                assert!(yaml.contains(&format!("- \"{source}:{target}\"")), "{yaml}");
                assert_eq!(target, "/run/host-services/ssh-auth.sock");
                #[cfg(target_os = "macos")]
                assert_eq!(
                    source, "/run/host-services/ssh-auth.sock",
                    "Docker Desktop publishes the agent here; the host path is \
                     meaningless inside the VM"
                );
            }
            // No agent running: asking for one has to produce nothing rather
            // than a mount of a socket that is not there, which is a container
            // that fails to start.
            None => assert!(overlay_yaml(&[entry]).is_none()),
        }
    }

    /// A name compose would read as something other than a variable.
    #[test]
    fn a_key_that_is_not_a_variable_name_is_refused() {
        for good in ["APP_ENV", "_X", "A1", "STRIPE_KEY"] {
            assert!(checked_key(good).is_ok(), "{good}");
        }
        for bad in [
            "", "1ST", "APP ENV", "APP-ENV", "APP.ENV", "APP=ENV", "ünlü",
        ] {
            assert!(checked_key(bad).is_err(), "{bad} was accepted");
        }
    }

    /// A newline in a YAML scalar ends it, and everything after is read as
    /// configuration somebody else wrote.
    #[test]
    fn a_value_cannot_carry_a_second_line() {
        assert!(checked_value("ordinary").is_ok());
        assert!(checked_value("with spaces and: colons").is_ok());
        assert!(checked_value("a\nb").is_err());
        assert!(checked_value("a\r\nb").is_err());
        assert!(checked_value("a\0b").is_err());
    }

    /// The quoting is what stands between a value and the file's structure.
    #[test]
    fn a_value_stays_a_value_in_the_overlay() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            ssh_agent: false,
            env: config(
                &[
                    ("APP_ENV", "local"),
                    ("QUOTED", r#"say "hi""#),
                    ("WINDOWS", r"C:\tmp"),
                    ("COLON", "a: b"),
                ],
                false,
            )
            .env,
        }])
        .expect("an overlay");

        assert!(yaml.contains("  shop:\n    environment:\n"), "{yaml}");
        assert!(yaml.contains(r#"      APP_ENV: "local""#), "{yaml}");
        assert!(yaml.contains(r#"      QUOTED: "say \"hi\"""#), "{yaml}");
        assert!(yaml.contains(r#"      WINDOWS: "C:\\tmp""#), "{yaml}");
        // A colon inside a quoted scalar is a colon, not a mapping.
        assert!(yaml.contains(r#"      COLON: "a: b""#), "{yaml}");
    }

    #[test]
    fn nothing_to_set_is_no_file_at_all() {
        assert!(overlay_yaml(&[]).is_none());
        assert!(overlay_yaml(&[Entry {
            service: "shop".into(),
            env: BTreeMap::new(),
            ssh_agent: false,
        }])
        .is_none());
    }

    /// A hand-edited file is read defensively — what is acted on is only ever
    /// what this module would have written.
    #[test]
    fn a_hostile_setting_on_disk_is_dropped_rather_than_rendered() {
        let dir = std::env::temp_dir().join(format!("stackvo-site-{}", std::process::id()));
        let projects = dir.join("projects");
        std::fs::create_dir_all(projects.join("shop").join(".stackvo")).unwrap();
        std::fs::write(dir.join("projects.path"), projects.display().to_string()).unwrap();
        std::fs::write(
            projects.join("shop").join(".stackvo").join(FILE_NAME),
            "{\"env\":{\"GOOD\":\"1\",\"BAD KEY\":\"2\",\"NEWLINE\":\"a\\nb\"},\"directoryListing\":true}",
        )
        .unwrap();

        let config = read(&dir, "shop");
        assert_eq!(config.env.len(), 1, "{:?}", config.env);
        assert_eq!(config.env.get("GOOD").map(String::as_str), Some("1"));
        assert!(config.directory_listing);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// M-6 is a server setting, and the servers that have no configuration file
    /// cannot do it — which is a sentence the pane has to be able to say.
    #[test]
    fn only_the_servers_with_a_config_file_can_list_a_directory() {
        assert!(listing_directives("nginx")
            .unwrap()
            .contains("autoindex on;"));
        assert!(listing_directives("caddy").unwrap().contains("browse"));
        assert!(listing_directives("frankenphp").is_some());
        assert_eq!(listing_directives("apache"), None);
        assert_eq!(listing_directives("swoole"), None);
    }

    /// Round trip: what is written is what comes back.
    #[test]
    fn what_is_written_is_what_is_read() {
        let dir = std::env::temp_dir().join(format!("stackvo-site-rt-{}", std::process::id()));
        let projects = dir.join("projects");
        std::fs::create_dir_all(projects.join("shop")).unwrap();
        std::fs::write(dir.join("projects.path"), projects.display().to_string()).unwrap();

        let written = config(&[("APP_ENV", "local"), ("DEBUG", "true")], true);
        write(&dir, "shop", &written).unwrap();
        assert_eq!(read(&dir, "shop"), written);

        // And refusing happens before anything is written.
        assert!(write(&dir, "shop", &config(&[("BAD KEY", "1")], false)).is_err());
        assert_eq!(read(&dir, "shop"), written, "the good file survived");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
