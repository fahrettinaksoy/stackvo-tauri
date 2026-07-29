//! The mail catcher's inbox, in the app.
//!
//! StackVo has shipped a mail catcher all along and never showed it: the only
//! way to read a captured message was to leave for a browser tab, which is
//! precisely the round trip Herd, EnvKit, FlyEnv and ServBay all charge for
//! removing.
//!
//! ## Why two APIs
//!
//! The template upstream installs `mailhog/mailhog`, which is unmaintained;
//! Mailpit is what every competitor ships now and what StackVo should move to.
//! That move is a one-line image change in a repository this app does not own,
//! and it renames a service, its `.env` keys, its container and its volume — a
//! migration for everyone with a running stack.
//!
//! So this reads both. Not as a hedge: the app manages a checkout it does not
//! dictate, exactly as it does for `.env` and `stackvo.json`, and a user on
//! either image should get an inbox. The two APIs disagree about almost
//! everything — envelope shape, field names, where the subject lives, whether
//! there is a snippet at all — so both are normalised here, once, into the
//! shape the UI renders.
//!
//! ## Why this is not done in the webview
//!
//! `tauri.conf.json` sets `connect-src 'self' ipc:`. Fetching `localhost:8025`
//! from the front end would mean widening that for every page the app renders,
//! forever, to save one command. The narrow CSP is the same decision as the
//! narrow capability list, and it is the inverse of the web UI's
//! `chmod 666 /var/run/docker.sock`.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// Which catcher is installed. They speak different APIs under the same job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mailhog,
    Mailpit,
}

impl Kind {
    pub fn service(self) -> &'static str {
        match self {
            Kind::Mailhog => "mailhog",
            Kind::Mailpit => "mailpit",
        }
    }

    /// The `.env` prefix each one keeps its settings under.
    fn prefix(self) -> &'static str {
        match self {
            Kind::Mailhog => "SERVICE_MAILHOG",
            Kind::Mailpit => "SERVICE_MAILPIT",
        }
    }

    fn port_key(self) -> &'static str {
        match self {
            Kind::Mailhog => "HOST_PORT_MAILHOG_UI",
            Kind::Mailpit => "HOST_PORT_MAILPIT_UI",
        }
    }

    /// Both default to 8025; Mailpit chose MailHog's port deliberately so it
    /// could be dropped in.
    fn default_port(self) -> u16 {
        8025
    }

    fn list_path(self, limit: u32) -> String {
        match self {
            Kind::Mailhog => format!("/api/v2/messages?limit={limit}"),
            Kind::Mailpit => format!("/api/v1/messages?limit={limit}"),
        }
    }

    fn message_path(self, id: &str) -> String {
        match self {
            Kind::Mailhog => format!("/api/v1/messages/{id}"),
            Kind::Mailpit => format!("/api/v1/message/{id}"),
        }
    }

    fn clear_path(self) -> &'static str {
        // The same route on both, and the only thing they agree on.
        "/api/v1/messages"
    }
}

// ------------------------------------------------------------- pure logic
//
// The parsers are plain functions over JSON so both wire formats are pinned by
// tests against real payloads. A field that silently moved between MailHog and
// Mailpit would otherwise show up as an inbox of blank rows.

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMessage {
    pub id: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    /// Whatever the server said, unparsed — the UI formats dates in the user's
    /// locale and the two servers disagree on the field, not the format.
    pub date: Option<String>,
    pub snippet: Option<String>,
    pub read: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailBody {
    pub text: Option<String>,
    pub html: Option<String>,
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str().map(str::to_string)
}

/// MailHog spells an address as `{ Mailbox, Domain }`, never as a string.
fn mailhog_address(value: &Value) -> Option<String> {
    let mailbox = value.get("Mailbox")?.as_str()?;
    let domain = value.get("Domain")?.as_str().unwrap_or("");
    Some(if domain.is_empty() {
        mailbox.to_string()
    } else {
        format!("{mailbox}@{domain}")
    })
}

/// Mailpit spells it `{ Name, Address }` and the name is often empty.
fn mailpit_address(value: &Value) -> Option<String> {
    let address = value.get("Address")?.as_str()?;
    match value.get("Name").and_then(|n| n.as_str()) {
        Some(name) if !name.is_empty() => Some(format!("{name} <{address}>")),
        _ => Some(address.to_string()),
    }
}

/// MailHog keeps the subject in a header array, not a field.
fn mailhog_header(item: &Value, name: &str) -> Option<String> {
    item.get("Content")?
        .get("Headers")?
        .get(name)?
        .as_array()?
        .first()?
        .as_str()
        .map(str::to_string)
}

/// Normalise a message list from either server.
pub fn parse_list(kind: Kind, body: &Value) -> Vec<MailMessage> {
    let items = match kind {
        Kind::Mailhog => body.get("items"),
        Kind::Mailpit => body.get("messages"),
    };
    let Some(items) = items.and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    items
        .iter()
        .map(|item| match kind {
            Kind::Mailhog => MailMessage {
                id: string_at(item, &["ID"]).unwrap_or_default(),
                from: item
                    .get("From")
                    .and_then(mailhog_address)
                    .unwrap_or_default(),
                to: item
                    .get("To")
                    .and_then(|v| v.as_array())
                    .map(|list| list.iter().filter_map(mailhog_address).collect())
                    .unwrap_or_default(),
                subject: mailhog_header(item, "Subject").unwrap_or_default(),
                date: mailhog_header(item, "Date").or_else(|| string_at(item, &["Created"])),
                // MailHog has no snippet; inventing one from the raw body would
                // mean rendering MIME boundaries as preview text.
                snippet: None,
                // Nor a read flag. Claiming everything is unread would badge
                // the inbox permanently.
                read: true,
            },
            Kind::Mailpit => MailMessage {
                id: string_at(item, &["ID"]).unwrap_or_default(),
                from: item
                    .get("From")
                    .and_then(mailpit_address)
                    .unwrap_or_default(),
                to: item
                    .get("To")
                    .and_then(|v| v.as_array())
                    .map(|list| list.iter().filter_map(mailpit_address).collect())
                    .unwrap_or_default(),
                subject: string_at(item, &["Subject"]).unwrap_or_default(),
                date: string_at(item, &["Created"]),
                snippet: string_at(item, &["Snippet"]).filter(|s| !s.is_empty()),
                read: item.get("Read").and_then(|v| v.as_bool()).unwrap_or(false),
            },
        })
        .collect()
}

/// How many messages the server is holding, and how many are unread.
///
/// MailHog does not track reads, so `unread` is None there rather than 0 — a
/// zero would render as "all caught up" on a server that cannot know.
pub fn parse_counts(kind: Kind, body: &Value) -> (u64, Option<u64>) {
    let total = body.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    let unread = match kind {
        Kind::Mailpit => body.get("unread").and_then(|v| v.as_u64()),
        Kind::Mailhog => None,
    };
    (total, unread)
}

/// Normalise one message's body.
pub fn parse_body(kind: Kind, body: &Value) -> MailBody {
    match kind {
        // MailHog returns the raw MIME document and leaves decoding to the
        // caller. Rendering the whole thing is honest — a half-decoded
        // multipart shown as if it were the message is not.
        Kind::Mailhog => MailBody {
            text: string_at(body, &["Content", "Body"]),
            html: None,
        },
        Kind::Mailpit => MailBody {
            text: string_at(body, &["Text"]).filter(|s| !s.is_empty()),
            html: string_at(body, &["HTML"]).filter(|s| !s.is_empty()),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailStatus {
    /// False when neither catcher is in this checkout at all.
    pub available: bool,
    pub kind: Option<Kind>,
    pub service: Option<String>,
    pub enabled: bool,
    pub running: bool,
    /// Where the browser would open it, for the "open outside" escape hatch.
    pub ui_url: Option<String>,
    pub total: u64,
    pub unread: Option<u64>,
    /// Set when the container is up but its API did not answer — a state that
    /// otherwise renders as an empty inbox, which is a lie.
    pub error: Option<String>,
}

// ------------------------------------------------------------------- I/O

/// Which catcher this checkout has, if any.
///
/// Mailpit first: on a checkout that somehow has both, the maintained one wins.
fn detect(env: &crate::config::Env) -> Option<Kind> {
    [Kind::Mailpit, Kind::Mailhog]
        .into_iter()
        .find(|kind| env.get(&format!("{}_ENABLE", kind.prefix())).is_some())
}

fn base_url(env: &crate::config::Env, kind: Kind) -> String {
    let port = env
        .get(kind.port_key())
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or_else(|| kind.default_port());
    // The published host port, not the container name: this process is on the
    // host, which is the entire reason the port moved out of a container.
    format!("http://127.0.0.1:{port}")
}

async fn get(url: &str) -> Result<Value> {
    let response = reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("the mail API did not answer: {e}"),
            )
            .with_hint("The container may still be starting, or its UI port may be taken.")
        })?;

    if !response.status().is_success() {
        return Err(Error::new(
            Code::EngineUnreachable,
            format!("the mail API returned {}", response.status()),
        ));
    }

    response.json().await.map_err(|e| {
        Error::new(
            Code::InvalidInput,
            format!("unreadable mail API reply: {e}"),
        )
    })
}

/// What the inbox panel needs before it renders anything.
pub async fn status(root: &Path) -> Result<MailStatus> {
    let env = crate::config::Env::load(root)?;

    let Some(kind) = detect(&env) else {
        return Ok(MailStatus {
            available: false,
            kind: None,
            service: None,
            enabled: false,
            running: false,
            ui_url: None,
            total: 0,
            unread: None,
            error: None,
        });
    };

    let enabled = env.bool(&format!("{}_ENABLE", kind.prefix()));
    let running = crate::engine::inspect(kind.service())
        .await
        .map(|d| d.running)
        .unwrap_or(false);
    let base = base_url(&env, kind);

    // Only asked when there is something to ask: a five-second timeout against
    // a stopped container on every panel open is five seconds of nothing.
    let (total, unread, error) = if running {
        match get(&format!("{base}{}", kind.list_path(1))).await {
            Ok(body) => {
                let (total, unread) = parse_counts(kind, &body);
                (total, unread, None)
            }
            Err(e) => (0, None, Some(e.message)),
        }
    } else {
        (0, None, None)
    };

    Ok(MailStatus {
        available: true,
        kind: Some(kind),
        service: Some(kind.service().to_string()),
        enabled,
        running,
        ui_url: Some(base),
        total,
        unread,
        error,
    })
}

fn resolve(root: &Path) -> Result<(Kind, String)> {
    let env = crate::config::Env::load(root)?;
    let kind = detect(&env).ok_or_else(|| {
        Error::new(Code::NotFound, "this checkout has no mail catcher")
            .with_hint("Enable mailhog (or mailpit) in .env, then regenerate.")
    })?;
    Ok((kind, base_url(&env, kind)))
}

pub async fn messages(root: &Path, limit: u32) -> Result<Vec<MailMessage>> {
    let (kind, base) = resolve(root)?;
    let body = get(&format!("{base}{}", kind.list_path(limit))).await?;
    Ok(parse_list(kind, &body))
}

pub async fn message(root: &Path, id: &str) -> Result<MailBody> {
    let (kind, base) = resolve(root)?;
    let body = get(&format!("{base}{}", kind.message_path(id))).await?;
    Ok(parse_body(kind, &body))
}

/// Empty the inbox.
pub async fn clear(root: &Path) -> Result<()> {
    let (kind, base) = resolve(root)?;

    let response = reqwest::Client::new()
        .delete(format!("{base}{}", kind.clear_path()))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("the mail API did not answer: {e}"),
            )
        })?;

    if !response.status().is_success() {
        return Err(Error::new(
            Code::EngineUnreachable,
            format!("the mail API returned {}", response.status()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real MailHog v2 payload, trimmed. Its shape is the whole reason this
    /// module has parsers rather than one serde struct: nothing here is spelled
    /// the way Mailpit spells it.
    const MAILHOG: &str = r#"{
      "total": 2, "count": 1, "start": 0,
      "items": [{
        "ID": "abc@mailhog.example",
        "From": { "Mailbox": "app", "Domain": "shop.loc" },
        "To": [{ "Mailbox": "dev", "Domain": "example.com" }],
        "Content": {
          "Headers": {
            "Subject": ["Password reset"],
            "Date": ["Wed, 29 Jul 2026 14:05:33 +0000"]
          },
          "Body": "Click here to reset."
        },
        "Created": "2026-07-29T14:05:33.1Z"
      }]
    }"#;

    /// A real Mailpit payload, trimmed.
    const MAILPIT: &str = r#"{
      "total": 2, "unread": 1, "count": 1, "start": 0,
      "messages": [{
        "ID": "xyz",
        "From": { "Name": "Shop", "Address": "app@shop.loc" },
        "To": [{ "Name": "", "Address": "dev@example.com" }],
        "Subject": "Password reset",
        "Created": "2026-07-29T14:05:33Z",
        "Snippet": "Click here to reset.",
        "Read": false
      }]
    }"#;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).expect("fixture should parse")
    }

    /// Both servers, one shape. The addresses are the sharp edge: MailHog
    /// splits them into mailbox and domain and never sends the joined form, so
    /// reading `From` as a string yields an empty sender on every row.
    #[test]
    fn both_wire_formats_normalise_to_the_same_message() {
        let hog = parse_list(Kind::Mailhog, &json(MAILHOG));
        let pit = parse_list(Kind::Mailpit, &json(MAILPIT));

        assert_eq!(hog.len(), 1);
        assert_eq!(pit.len(), 1);

        assert_eq!(hog[0].from, "app@shop.loc");
        assert_eq!(hog[0].to, vec!["dev@example.com"]);
        assert_eq!(hog[0].subject, "Password reset");

        assert_eq!(pit[0].from, "Shop <app@shop.loc>");
        assert_eq!(pit[0].to, vec!["dev@example.com"]);
        assert_eq!(pit[0].subject, "Password reset");
    }

    /// MailHog buries the subject in a header array. Reading it as a field
    /// gives an inbox of blank rows that still look like messages.
    #[test]
    fn mailhogs_subject_is_a_header_not_a_field() {
        let body = json(MAILHOG);
        assert!(
            body["items"][0].get("Subject").is_none(),
            "the fixture must actually lack the field this guards"
        );
        assert_eq!(
            parse_list(Kind::Mailhog, &body)[0].subject,
            "Password reset"
        );
    }

    /// MailHog cannot know what has been read. Reporting zero unread would
    /// render as "all caught up" on a server with no such concept.
    #[test]
    fn unread_is_unknown_rather_than_zero_on_mailhog() {
        assert_eq!(parse_counts(Kind::Mailhog, &json(MAILHOG)), (2, None));
        assert_eq!(parse_counts(Kind::Mailpit, &json(MAILPIT)), (2, Some(1)));
    }

    #[test]
    fn a_nameless_mailpit_sender_is_just_the_address() {
        let value = json(r#"{ "Name": "", "Address": "noreply@shop.loc" }"#);
        assert_eq!(mailpit_address(&value).unwrap(), "noreply@shop.loc");
    }

    #[test]
    fn an_empty_or_foreign_payload_yields_no_messages_rather_than_panicking() {
        for text in ["{}", r#"{"items": null}"#, r#"{"messages": "nope"}"#, "[]"] {
            assert!(parse_list(Kind::Mailhog, &json(text)).is_empty());
            assert!(parse_list(Kind::Mailpit, &json(text)).is_empty());
        }
    }

    #[test]
    fn bodies_normalise_too() {
        let hog = parse_body(Kind::Mailhog, &json(MAILHOG)["items"][0]);
        assert_eq!(hog.text.as_deref(), Some("Click here to reset."));
        assert!(hog.html.is_none(), "MailHog returns raw MIME, not HTML");

        let pit = parse_body(
            Kind::Mailpit,
            &json(r#"{ "Text": "plain", "HTML": "<p>rich</p>" }"#),
        );
        assert_eq!(pit.text.as_deref(), Some("plain"));
        assert_eq!(pit.html.as_deref(), Some("<p>rich</p>"));
    }

    /// An empty HTML field is not an HTML body; rendering one would replace a
    /// perfectly good plain-text message with a blank panel.
    #[test]
    fn an_empty_body_field_is_treated_as_absent() {
        let body = parse_body(Kind::Mailpit, &json(r#"{ "Text": "plain", "HTML": "" }"#));
        assert!(body.html.is_none());
    }

    /// Mailpit took MailHog's port on purpose so it could be dropped in; the
    /// app must not assume they differ.
    #[test]
    fn both_default_to_the_same_ui_port() {
        assert_eq!(Kind::Mailhog.default_port(), 8025);
        assert_eq!(Kind::Mailpit.default_port(), 8025);
    }

    #[test]
    fn the_two_apis_are_versioned_differently_for_listing() {
        assert!(Kind::Mailhog.list_path(50).starts_with("/api/v2/"));
        assert!(Kind::Mailpit.list_path(50).starts_with("/api/v1/"));
    }
}
