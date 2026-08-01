//! The mail catcher's inbox, in the app.
//!
//! StackVo has shipped a mail catcher all along and never showed it: the only
//! way to read a captured message was to leave for a browser tab, which is
//! precisely the round trip Herd, EnvKit, FlyEnv and ServBay all charge for
//! removing.
//!
//! ## Why two APIs
//!
//! Both catchers ship as catalog services — neither enabled by default. The
//! Mail page offers to enable Mailpit (the maintained one) on first visit,
//! and one yes runs the whole chain: flag, regenerate, `up -d`. MailHog is
//! kept by explicit decision for stacks that already run it.
//! The two APIs disagree about almost everything, so both are normalised
//! here, once, into the shape the UI renders. When both are enabled, Mailpit
//! wins: it is the maintained one, and the two cannot share port 8025 at the
//! same time anyway.
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
    /// Cc as the message declares it. MailHog keeps it in a header, Mailpit
    /// in a field; both normalise here.
    pub cc: Vec<String>,
    /// Bcc — Mailpit derives it from the SMTP envelope (RCPT TO minus the
    /// headers), which is the only honest source. MailHog cannot tell a Bcc
    /// from a To, so there it stays empty rather than guessed.
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
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
    /// Every header the catcher reported, flattened to one row per value and
    /// sorted by name — serde's JSON map keeps no order, so alphabetical is
    /// the only order that is honest rather than accidental.
    pub headers: Vec<MailHeader>,
    /// Attachments, Mailpit only — MailHog returns the raw MIME document and
    /// decoding it here would be a MIME parser this app has no reason to own.
    pub attachments: Vec<MailAttachment>,
    /// Total message size in bytes, when the catcher reports one.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailHeader {
    pub name: String,
    pub value: String,
}

/// One attachment, as the catcher lists it. `part_id` is the handle the
/// download route takes — never a path, for the same reason `app_log_open`
/// takes a handle: a byte route that accepts arbitrary input from its own
/// front end is a file reader.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailAttachment {
    pub part_id: String,
    pub file_name: String,
    pub content_type: String,
    pub size: u64,
}

/// Mailpit's HTML compatibility report — the thing a developer actually opens
/// a mail catcher for: which of 186 client features this markup survives.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlCheck {
    /// Percent of tested client features fully supported.
    pub supported: f64,
    pub partial: f64,
    pub unsupported: f64,
    /// How many client/feature combinations were tested.
    pub tests: u32,
    pub warnings: Vec<HtmlWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlWarning {
    pub title: String,
    pub category: String,
    /// How many times this construct appears in the message.
    pub found: u32,
    pub supported: f64,
    pub partial: f64,
    pub unsupported: f64,
}

/// A link found in the message and what answered it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCheck {
    pub errors: u32,
    pub links: Vec<LinkResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkResult {
    pub url: String,
    pub status: String,
    pub status_code: u16,
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

/// A Mailpit address array field (`To`, `Cc`, `Bcc`, `ReplyTo`), normalised.
fn mailpit_addresses(item: &Value, field: &str) -> Vec<String> {
    item.get(field)
        .and_then(|v| v.as_array())
        .map(|list| list.iter().filter_map(mailpit_address).collect())
        .unwrap_or_default()
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
                // Headers carry a single joined string; kept as one entry
                // rather than re-split, because quoted display names make
                // comma-splitting a parser, not a convenience.
                cc: mailhog_header(item, "Cc")
                    .map(|v| vec![v])
                    .unwrap_or_default(),
                bcc: Vec::new(),
                reply_to: mailhog_header(item, "Reply-To")
                    .map(|v| vec![v])
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
                cc: mailpit_addresses(item, "Cc"),
                bcc: mailpit_addresses(item, "Bcc"),
                reply_to: mailpit_addresses(item, "ReplyTo"),
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
/// Mailpit's `Attachments` array. Inline parts (a logo referenced by cid:)
/// are deliberately not merged in: they are part of the rendering, not
/// something the recipient was sent to open.
pub fn parse_attachments(body: &Value) -> Vec<MailAttachment> {
    body.get("Attachments")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(MailAttachment {
                        part_id: a.get("PartID")?.as_str()?.to_string(),
                        file_name: a
                            .get("FileName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("attachment")
                            .to_string(),
                        content_type: a
                            .get("ContentType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("application/octet-stream")
                            .to_string(),
                        size: a.get("Size").and_then(|v| v.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn percent(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// Mailpit's `html-check` payload. Warnings are sorted worst-first — a report
/// whose first row is a construct every client supports buries the one that
/// breaks Outlook.
pub fn parse_html_check(body: &Value) -> HtmlCheck {
    let total = body.get("Total").cloned().unwrap_or(Value::Null);
    let mut warnings: Vec<HtmlWarning> = body
        .get("Warnings")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .map(|w| {
                    let score = w.get("Score").cloned().unwrap_or(Value::Null);
                    HtmlWarning {
                        title: w
                            .get("Title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        category: w
                            .get("Category")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        found: score.get("Found").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        supported: percent(&score, "Supported"),
                        partial: percent(&score, "Partial"),
                        unsupported: percent(&score, "Unsupported"),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    warnings.sort_by(|a, b| {
        b.unsupported
            .partial_cmp(&a.unsupported)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    HtmlCheck {
        supported: percent(&total, "Supported"),
        partial: percent(&total, "Partial"),
        unsupported: percent(&total, "Unsupported"),
        tests: total.get("Tests").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        warnings,
    }
}

/// Mailpit's `link-check` payload.
pub fn parse_link_check(body: &Value) -> LinkCheck {
    LinkCheck {
        errors: body.get("Errors").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        links: body
            .get("Links")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .map(|l| LinkResult {
                        url: l
                            .get("URL")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status: l
                            .get("Status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status_code: l.get("StatusCode").and_then(|v| v.as_u64()).unwrap_or(0)
                            as u16,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// `{ "Name": ["v1", "v2"] }` — the shape both servers use for header maps —
/// flattened and sorted case-insensitively.
pub fn parse_headers(map: &Value) -> Vec<MailHeader> {
    let Some(map) = map.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<MailHeader> = map
        .iter()
        .flat_map(|(name, values)| {
            values
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str())
                        .map(|v| MailHeader {
                            name: name.clone(),
                            value: v.to_string(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

pub fn parse_body(kind: Kind, body: &Value) -> MailBody {
    match kind {
        // MailHog returns the raw MIME document and leaves decoding to the
        // caller. Rendering the whole thing is honest — a half-decoded
        // multipart shown as if it were the message is not.
        Kind::Mailhog => MailBody {
            text: string_at(body, &["Content", "Body"]),
            html: None,
            headers: body
                .get("Content")
                .and_then(|c| c.get("Headers"))
                .map(parse_headers)
                .unwrap_or_default(),
            attachments: Vec::new(),
            size: None,
        },
        // Mailpit's detail payload has no header map; `message()` fetches it
        // from the dedicated /headers route and fills this in.
        Kind::Mailpit => MailBody {
            text: string_at(body, &["Text"]).filter(|s| !s.is_empty()),
            html: string_at(body, &["HTML"]).filter(|s| !s.is_empty()),
            headers: Vec::new(),
            attachments: parse_attachments(body),
            size: body.get("Size").and_then(|v| v.as_u64()),
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
    const ORDER: [Kind; 2] = [Kind::Mailpit, Kind::Mailhog];
    // The *enabled* one wins — both catchers now ship keys, so mere key
    // presence stopped being a signal. Mailpit first on a tie (both enabled
    // cannot actually run: they fight over port 8025). With neither enabled,
    // fall back to whichever is declared, so the status pane can still name
    // the catcher it would be enabling.
    ORDER
        .into_iter()
        .find(|kind| env.bool(&format!("{}_ENABLE", kind.prefix())))
        .or_else(|| {
            ORDER
                .into_iter()
                .find(|kind| env.get(&format!("{}_ENABLE", kind.prefix())).is_some())
        })
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
    let mut parsed = parse_body(kind, &body);

    // Mailpit keeps the header map on a route of its own. Best-effort: a
    // headers fetch that fails must not take the message body down with it —
    // the tab simply comes up empty.
    if kind == Kind::Mailpit {
        if let Ok(map) = get(&format!("{base}/api/v1/message/{id}/headers")).await {
            parsed.headers = parse_headers(&map);
        }
    }
    Ok(parsed)
}

/// Search the inbox. Both servers run the query server-side — Mailpit's own
/// syntax (`from:`, `to:`, `subject:`, quoted phrases) reaches it untouched,
/// because reimplementing it here would be a second, worse parser.
pub async fn search(root: &Path, query: &str, limit: u32) -> Result<Vec<MailMessage>> {
    let (kind, base) = resolve(root)?;
    let encoded = urlencode(query);
    let path = match kind {
        Kind::Mailhog => format!("/api/v2/search?kind=containing&query={encoded}&limit={limit}"),
        Kind::Mailpit => format!("/api/v1/search?query={encoded}&limit={limit}"),
    };
    let body = get(&format!("{base}{path}")).await?;
    Ok(parse_list(kind, &body))
}

/// Percent-encode a query for a URL. Small on purpose: the alternative is a
/// dependency for one call site whose input is a search box.
fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// How this message's HTML fares across real mail clients. Mailpit only —
/// MailHog has no equivalent, and `None` is how the UI knows to hide the tab
/// rather than show an empty one.
pub async fn html_check(root: &Path, id: &str) -> Result<Option<HtmlCheck>> {
    let (kind, base) = resolve(root)?;
    if kind != Kind::Mailpit {
        return Ok(None);
    }
    let body = get(&format!("{base}/api/v1/message/{id}/html-check")).await?;
    Ok(Some(parse_html_check(&body)))
}

/// Follow every link in the message and report what answered.
///
/// **This leaves the machine** — it is the one call in this module that talks
/// to the internet, so it is never run as part of opening a message; the UI
/// asks for it explicitly.
pub async fn link_check(root: &Path, id: &str) -> Result<Option<LinkCheck>> {
    let (kind, base) = resolve(root)?;
    if kind != Kind::Mailpit {
        return Ok(None);
    }
    // Longer than the module default: this waits on third-party servers.
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/message/{id}/link-check"))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| {
            Error::new(
                Code::EngineUnreachable,
                format!("the mail API did not answer: {e}"),
            )
        })?;
    let body: Value = response
        .json()
        .await
        .map_err(|e| Error::new(Code::EngineUnreachable, format!("unreadable reply: {e}")))?;
    Ok(Some(parse_link_check(&body)))
}

/// Write one attachment to a chosen path.
///
/// `part_id` is a handle the catcher issued, and `path` is chosen through the
/// system save dialog — the front end never names a source and never names a
/// destination this process did not receive from the user.
pub async fn save_attachment(root: &Path, id: &str, part_id: &str, path: &Path) -> Result<u64> {
    let (_, base) = resolve(root)?;
    let response = reqwest::Client::new()
        .get(format!("{base}/api/v1/message/{id}/part/{part_id}"))
        .timeout(std::time::Duration::from_secs(30))
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
            Code::NotFound,
            format!("the mail API returned {}", response.status()),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| Error::new(Code::IoError, format!("could not read the attachment: {e}")))?;
    std::fs::write(path, &bytes)
        .map_err(|e| Error::io(format!("writing {}", path.display()), e))?;
    Ok(bytes.len() as u64)
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

/// Delete one message — the two servers disagree here too. MailHog addresses
/// the message in the path; Mailpit takes a JSON body of ids on the
/// collection route (its path form does not exist).
pub async fn delete(root: &Path, id: &str) -> Result<()> {
    let (kind, base) = resolve(root)?;

    let request = match kind {
        Kind::Mailhog => reqwest::Client::new().delete(format!("{base}/api/v1/messages/{id}")),
        Kind::Mailpit => reqwest::Client::new()
            .delete(format!("{base}/api/v1/messages"))
            .json(&serde_json::json!({ "IDs": [id] })),
    };

    let response = request
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
            "Cc": ["qa@example.com"],
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
        "Cc": [{ "Name": "QA", "Address": "qa@example.com" }],
        "Bcc": [{ "Name": "", "Address": "audit@example.com" }],
        "ReplyTo": [{ "Name": "", "Address": "noreply@shop.loc" }],
        "Subject": "Password reset",
        "Created": "2026-07-29T14:05:33Z",
        "Snippet": "Click here to reset.",
        "Read": false
      }]
    }"#;

    #[test]
    fn headers_flatten_multi_values_and_sort_case_insensitively() {
        let map = json(
            r#"{
            "X-Priority": ["1"],
            "Received": ["from a", "from b"],
            "content-type": ["text/html"]
        }"#,
        );
        let rows = parse_headers(&map);
        let names: Vec<&str> = rows.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["content-type", "Received", "Received", "X-Priority"]
        );
        assert_eq!(rows[1].value, "from a");
        assert_eq!(rows[2].value, "from b");

        // MailHog's inline map reaches the same rows through parse_body.
        let hog = parse_body(Kind::Mailhog, &json(MAILHOG)["items"][0]);
        assert!(hog
            .headers
            .iter()
            .any(|h| h.name == "Subject" && h.value == "Password reset"));
    }

    #[test]
    fn attachments_and_checks_parse_real_payloads() {
        // A real Mailpit detail payload, trimmed to the parts read here.
        let detail = json(
            r#"{
          "Text": "t", "HTML": "<p>x</p>", "Size": 813,
          "Attachments": [{
            "PartID": "2", "FileName": "fatura.pdf",
            "ContentType": "application/pdf", "Size": 64
          }]
        }"#,
        );
        let body = parse_body(Kind::Mailpit, &detail);
        assert_eq!(body.size, Some(813));
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].part_id, "2");
        assert_eq!(body.attachments[0].file_name, "fatura.pdf");

        // MailHog returns raw MIME; decoding it would be a MIME parser this
        // module has no reason to own, so it reports none rather than guessing.
        let hog = parse_body(Kind::Mailhog, &json(MAILHOG)["items"][0]);
        assert!(hog.attachments.is_empty());
    }

    #[test]
    fn html_check_reports_worst_first() {
        let check = parse_html_check(&json(
            r#"{
          "Total": { "Tests": 186, "Supported": 84.7, "Partial": 7.1, "Unsupported": 8.2 },
          "Warnings": [
            { "Title": "safe", "Category": "html",
              "Score": { "Found": 1, "Supported": 99.0, "Partial": 1.0, "Unsupported": 0.0 } },
            { "Title": "breaks Outlook", "Category": "css",
              "Score": { "Found": 3, "Supported": 40.0, "Partial": 10.0, "Unsupported": 50.0 } }
          ]
        }"#,
        ));
        assert_eq!(check.tests, 186);
        // Worst first: a report led by a construct everything supports buries
        // the one that actually breaks a client.
        assert_eq!(check.warnings[0].title, "breaks Outlook");
        assert_eq!(check.warnings[0].found, 3);
    }

    #[test]
    fn a_query_is_encoded_rather_than_pasted_into_the_url() {
        assert_eq!(
            urlencode("from:a@b.c subject:\"ödeme\""),
            "from%3Aa%40b.c+subject%3A%22%C3%B6deme%22"
        );
    }

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
        assert_eq!(hog[0].cc, vec!["qa@example.com"]);
        assert!(hog[0].bcc.is_empty(), "MailHog cannot tell a Bcc from a To");

        assert_eq!(pit[0].cc, vec!["QA <qa@example.com>"]);
        assert_eq!(pit[0].bcc, vec!["audit@example.com"]);
        assert_eq!(pit[0].reply_to, vec!["noreply@shop.loc"]);
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
