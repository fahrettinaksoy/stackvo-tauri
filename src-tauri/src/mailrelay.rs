//! Letting a caught message leave (M-2).
//!
//! [`crate::mail`] catches everything the stack sends and shows it in the app.
//! That is the right default and it is not the whole job: the message somebody
//! actually needs to check is the one that renders differently in Outlook, or
//! the invoice a colleague has to look at, and both of those mean **this one
//! message, to a real address**. Every rival in the category calls that
//! "release" and StackVo could not do it.
//!
//! ## Why this is not "turn the catcher off"
//!
//! Pointing the application at a real SMTP server instead of the catcher would
//! send everything — including the password reset your test suite generates
//! forty times an hour, to whatever address the fixtures happen to contain.
//! Release is the opposite shape: the catcher still catches everything, and one
//! message leaves when somebody asks for it, to a recipient they typed.
//!
//! ## Where the relay settings go, and where the password does not
//!
//! Mailpit does the relaying; it needs a host, a port and usually credentials,
//! and it reads them from its own environment. This app reaches that
//! environment through a **compose overlay**, the same mechanism
//! [`crate::site`] and [`crate::perf`] use — so the service package is not
//! touched, nothing is re-sealed, and a workspace that has never configured a
//! relay renders exactly the bytes it did before.
//!
//! The password is in the OS keystore ([`crate::secrets`]) and only the
//! keystore. It is written into the rendered overlay under `generated/`, which
//! is the same partial answer ADR 0010 already states for service passwords and
//! is stated here rather than implied: `generated/` is output, rewritten on
//! every run and never hand-maintained, while the settings file this module
//! writes carries a reference and never the value.
//!
//! ## What is deliberately absent
//!
//! * **A default relay.** There is no address this app can guess, and a field
//!   pre-filled with somebody's provider is a message sent to the wrong server.
//! * **Sending on the application's behalf.** The stack still talks to the
//!   catcher. Nothing here changes what the application does; it changes what
//!   the catcher can be asked to do afterwards.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The keystore entry the relay password lives under.
pub const SECRET: &str = "mail-relay-password";

/// Where the settings are kept, per workspace.
pub fn config_path(root: &Path) -> PathBuf {
    root.join(".stackvo").join("mail-relay.json")
}

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.mailrelay.yml")
}

/// How the connection is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Port 587 with `STARTTLS`, which is what almost every provider wants.
    #[default]
    Starttls,
    /// Port 465, TLS from the first byte.
    Tls,
    /// Nothing. Offered because a relay on this machine — a local test one —
    /// is a real case, and refusing it would push people to a checkbox they
    /// tick without reading.
    None,
}

impl Security {
    fn env(self) -> (&'static str, &'static str) {
        match self {
            Security::Starttls => ("true", "false"),
            Security::Tls => ("false", "true"),
            Security::None => ("false", "false"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub security: Security,
    /// The envelope sender. Providers reject a `From` they do not own, and the
    /// error comes back from the relay as a number.
    pub from: String,
    /// Addresses a release is allowed to go to.
    ///
    /// Empty means "anywhere", which is Mailpit's own default. It is offered
    /// because the alternative — a development stack that can email any address
    /// somebody types — is one typo away from a real customer.
    pub allowed_recipients: Vec<String>,
}

/// Not a validator for email addresses, and deliberately not.
///
/// The only thing checked is what would break the file this ends up in: a
/// newline ends a YAML scalar, and everything after it is read as configuration
/// somebody else wrote. Whether an address exists is the relay's answer to
/// give, and a client-side pattern that refuses a valid address is worse than
/// one that lets the server say no.
pub fn checked_value(value: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        return Err(Error::new(
            Code::InvalidInput,
            "a relay setting cannot contain a line break",
        ));
    }
    Ok(())
}

pub fn checked(config: &Config) -> Result<()> {
    for value in [&config.host, &config.username, &config.from] {
        checked_value(value)?;
    }
    for recipient in &config.allowed_recipients {
        checked_value(recipient)?;
    }
    if config.enabled {
        if config.host.trim().is_empty() {
            return Err(Error::new(
                Code::InvalidInput,
                "a relay needs a host to send through",
            ));
        }
        if config.port == 0 {
            return Err(Error::new(Code::InvalidInput, "a relay needs a port"));
        }
        if config.from.trim().is_empty() {
            return Err(Error::new(
                Code::InvalidInput,
                "a relay needs a sender address; providers reject a From they do not own",
            ));
        }
    }
    Ok(())
}

pub fn read(root: &Path) -> Config {
    std::fs::read_to_string(config_path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .unwrap_or_default()
}

pub fn write(root: &Path, config: &Config) -> Result<()> {
    checked(config)?;
    let path = config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("making {}", parent.display()), e))?;
    }
    let text = serde_json::to_string_pretty(config)
        .map_err(|e| Error::new(Code::InvalidInput, format!("serialising: {e}")))?;
    crate::atomic::write(&path, &format!("{text}\n"))
}

/// Which compose services are Mailpit, read out of the generated file.
///
/// By image rather than by name. The service key is `mailpit` on a workspace
/// that keeps its services in `.env` and `mailpit-1-30` on one that has moved
/// to the instance table, and a third shape is one package release away — but
/// the image is what makes a service the thing that can relay.
pub fn mailpit_services(compose: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut in_services = false;

    for line in compose.lines() {
        if !line.starts_with(' ') && !line.trim().is_empty() {
            in_services = line.starts_with("services:");
            current = None;
            continue;
        }
        if !in_services {
            continue;
        }
        // A service key: exactly two spaces of indent, then `name:`.
        if let Some(rest) = line.strip_prefix("  ") {
            if !rest.starts_with(' ') && rest.ends_with(':') {
                current = Some(rest.trim_end_matches(':').to_string());
                continue;
            }
        }
        let trimmed = line.trim();
        if let Some(image) = trimmed.strip_prefix("image:") {
            let image = image.trim().trim_matches(['"', '\'']);
            if image.starts_with("axllent/mailpit") {
                if let Some(name) = current.take() {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// The overlay, or `None` when there is nothing to add.
///
/// `password` is passed in rather than read here so this stays a pure function
/// — the one thing in this module worth testing with a credential in it is the
/// thing that must never read one from a keystore during a test.
pub fn overlay_yaml(services: &[String], config: &Config, password: &str) -> Option<String> {
    if !config.enabled || services.is_empty() {
        return None;
    }
    let (starttls, tls) = config.security.env();

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Re-rendered from .stackvo/mail-relay.json before every compose\n\
         # command, so edits here are lost. Change them in the app instead.\n\
         #\n\
         # This file carries the relay password, because the container reads it\n\
         # from its environment. `generated/` is output — rewritten on every run\n\
         # and never hand-maintained — which is the same partial answer ADR 0010\n\
         # states for service passwords.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file, and\n\
         # will recreate the catcher without a relay.\n\
         services:\n",
    );

    for service in services {
        out.push_str(&format!("  {service}:\n    environment:\n"));
        out.push_str(&format!(
            "      MP_SMTP_RELAY_HOST: \"{}\"\n",
            config.host.trim()
        ));
        out.push_str(&format!("      MP_SMTP_RELAY_PORT: \"{}\"\n", config.port));
        out.push_str(&format!("      MP_SMTP_RELAY_STARTTLS: \"{starttls}\"\n"));
        out.push_str(&format!("      MP_SMTP_RELAY_TLS: \"{tls}\"\n"));
        out.push_str(&format!(
            "      MP_SMTP_RELAY_RETURN_PATH: \"{}\"\n",
            config.from.trim()
        ));
        if !config.username.trim().is_empty() {
            out.push_str(&format!(
                "      MP_SMTP_RELAY_AUTH: \"login\"\n      MP_SMTP_RELAY_USERNAME: \"{}\"\n      MP_SMTP_RELAY_PASSWORD: \"{password}\"\n",
                config.username.trim()
            ));
        }
        if !config.allowed_recipients.is_empty() {
            // A regular expression to Mailpit, which is why the parts are
            // escaped: a dot in a domain would otherwise match any character,
            // so `me@test.com` in the list would also permit `me@testxcom`.
            let pattern = config
                .allowed_recipients
                .iter()
                .map(|r| r.trim().replace('.', "[.]"))
                .collect::<Vec<_>>()
                .join("|");
            out.push_str(&format!(
                "      MP_SMTP_RELAY_ALLOWED_RECIPIENTS: \"{pattern}\"\n"
            ));
        }
    }
    Some(out)
}

/// Render the overlay; `true` when there is one to layer.
pub fn sync(root: &Path) -> bool {
    let config = read(root);
    if !config.enabled {
        let _ = std::fs::remove_file(overlay_path(root));
        return false;
    }

    let compose =
        std::fs::read_to_string(root.join("generated").join("docker-compose.dynamic.yml"))
            .unwrap_or_default();
    let password = crate::secrets::read(SECRET)
        .ok()
        .flatten()
        .unwrap_or_default();

    let path = overlay_path(root);
    match overlay_yaml(&mailpit_services(&compose), &config, &password) {
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

    fn configured() -> Config {
        Config {
            enabled: true,
            host: "smtp.example.com".into(),
            port: 587,
            username: "postmaster@example.com".into(),
            security: Security::Starttls,
            from: "dev@example.com".into(),
            allowed_recipients: vec!["me@example.com".into()],
        }
    }

    /// The service key differs between a `.env` workspace and one that has
    /// moved to the instance table, and a third shape is one package release
    /// away. The image is what makes a service the thing that can relay.
    #[test]
    fn the_catcher_is_found_by_its_image_and_not_its_name() {
        let compose = r#"services:
  mariadb-12-3:
    image: "mariadb:12.3"
  mailpit-1-30:
    image: "axllent/mailpit:v1.30"
    container_name: "stackvo-mailpit-1-30"
  redis-8-10:
    image: "redis:8.10"
volumes:
  stackvo-mariadb-12-3-data:
"#;
        assert_eq!(mailpit_services(compose), ["mailpit-1-30"]);

        // The plain name, on a workspace that never migrated.
        let older = "services:\n  mailpit:\n    image: axllent/mailpit:latest\n";
        assert_eq!(mailpit_services(older), ["mailpit"]);

        // Nothing at all is the common case and must not be an error.
        assert!(mailpit_services("services:\n  redis:\n    image: redis\n").is_empty());
        assert!(mailpit_services("").is_empty());
    }

    /// A volume name that happens to contain the word must not be read as a
    /// service — the top-level block ends the service section.
    #[test]
    fn only_the_services_block_is_read() {
        let compose = "volumes:\n  mailpit-data:\n    image: axllent/mailpit\n";
        assert!(mailpit_services(compose).is_empty());
    }

    #[test]
    fn nothing_is_layered_until_it_is_switched_on() {
        let off = Config {
            enabled: false,
            ..configured()
        };
        assert!(overlay_yaml(&["mailpit".into()], &off, "pw").is_none());
        // And nothing to configure is not a reason to write an empty overlay.
        assert!(overlay_yaml(&[], &configured(), "pw").is_none());
    }

    #[test]
    fn the_overlay_says_what_mailpit_reads() {
        let yaml = overlay_yaml(&["mailpit-1-30".into()], &configured(), "hunter2").unwrap();
        assert!(yaml.contains("  mailpit-1-30:\n    environment:\n"));
        assert!(yaml.contains("MP_SMTP_RELAY_HOST: \"smtp.example.com\""));
        assert!(yaml.contains("MP_SMTP_RELAY_PORT: \"587\""));
        assert!(yaml.contains("MP_SMTP_RELAY_STARTTLS: \"true\""));
        assert!(yaml.contains("MP_SMTP_RELAY_TLS: \"false\""));
        assert!(yaml.contains("MP_SMTP_RELAY_PASSWORD: \"hunter2\""));
        assert!(yaml.contains("MP_SMTP_RELAY_RETURN_PATH: \"dev@example.com\""));
    }

    /// The allowed list is a regular expression to Mailpit. An unescaped dot
    /// matches any character, so `me@test.com` would also permit `me@testxcom`
    /// — which is a real address somebody could own.
    #[test]
    fn the_allowed_list_is_escaped_as_the_pattern_it_becomes() {
        let yaml = overlay_yaml(&["mailpit".into()], &configured(), "pw").unwrap();
        assert!(yaml.contains("MP_SMTP_RELAY_ALLOWED_RECIPIENTS: \"me@example[.]com\""));
    }

    /// No credentials block at all when there is no username: Mailpit reads
    /// `MP_SMTP_RELAY_AUTH` as an instruction to authenticate, and an empty
    /// login is a connection the relay closes.
    #[test]
    fn a_relay_without_credentials_is_not_told_to_authenticate() {
        let anonymous = Config {
            username: String::new(),
            ..configured()
        };
        let yaml = overlay_yaml(&["mailpit".into()], &anonymous, "").unwrap();
        assert!(!yaml.contains("MP_SMTP_RELAY_AUTH"));
        assert!(!yaml.contains("MP_SMTP_RELAY_PASSWORD"));
    }

    /// A newline in a value ends the YAML scalar, and everything after it is
    /// read as configuration somebody else wrote.
    #[test]
    fn a_line_break_cannot_reach_the_overlay() {
        let hostile = Config {
            host: "smtp.example.com\"\n      MP_SMTP_RELAY_ALLOWED_RECIPIENTS: \".*".into(),
            ..configured()
        };
        assert!(checked(&hostile).is_err());
        assert!(write(Path::new("/nonexistent"), &hostile).is_err());
    }

    /// Switched on with nothing to send through is a state the screen must not
    /// be able to save: the overlay would name a host of `""` and Mailpit would
    /// fail on the first release with a message about a connection.
    #[test]
    fn an_enabled_relay_has_to_be_a_relay() {
        for broken in [
            Config {
                host: String::new(),
                ..configured()
            },
            Config {
                port: 0,
                ..configured()
            },
            Config {
                from: String::new(),
                ..configured()
            },
        ] {
            assert!(checked(&broken).is_err());
        }
        // The same values are fine while it is off — half-filled settings are
        // what a form in progress looks like.
        assert!(checked(&Config::default()).is_ok());
    }
}
