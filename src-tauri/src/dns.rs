//! Answering for this machine's development names, so `/etc/hosts` stops being
//! the only way a project resolves.
//!
//! E-1. Every new project needs a line in `/etc/hosts`, and writing that file
//! needs an administrator password. That is a prompt per project, and it is
//! also the reason E-2's wildcards do not work at all: `/etc/hosts` maps names,
//! one at a time, and `*.shop.loc` is not a name.
//!
//! ## Why not dnsmasq
//!
//! Every comparable tool ships dnsmasq — DDEV and Valet both do — and it would
//! have worked. It also means a second binary to package for three platforms, a
//! config file generated from this app's state, a process supervised by
//! something, and a failure mode where the machine's name resolution depends on
//! a container being up. For a responder whose entire job is "say 127.0.0.1 to
//! anything under one suffix", that is a great deal of moving parts around a
//! function that fits on a page.
//!
//! ## It is not a resolver, and that is the security property
//!
//! This answers for **one suffix** and refuses everything else. It never
//! forwards, has no upstream, and holds no cache. An open forwarder listening
//! on a machine is a thing that can be pointed at — for amplification, for
//! poisoning what the machine believes — and a development tool has no business
//! becoming the resolver for anything it did not create.
//!
//! Concretely: a query for `shop.loc` is answered, a query for `google.com` is
//! `REFUSED`, and there is no code path that opens a socket to anywhere. It
//! binds loopback only, so nothing off this machine can reach it either.
//!
//! ## A high port, because port 53 needs root and does not need to
//!
//! Binding 53 means privilege at every start. macOS's resolver files take a
//! `port` directive, so `/etc/resolver/loc` can name 127.0.0.1 port 15353 and
//! the responder runs as the user like the rest of the app. The one privileged
//! act is writing that file, which happens once and goes through the same
//! staged-copy path `hosts::apply` already uses.
//!
//! ## Three platforms, three different answers, and one of them is "no"
//!
//! * **macOS** — `/etc/resolver/<tld>` is exactly this feature, per suffix,
//!   supported by the system resolver since 10.4.
//! * **Linux** — no `/etc/resolver`. The equivalent is a drop-in for whatever
//!   is in front of `resolv.conf`: `server=/loc/127.0.0.1#15353` for dnsmasq or
//!   NetworkManager, or a `~/.config/systemd` unit for systemd-resolved. Which
//!   of those is present is a question about the user's distribution, so this
//!   reports the line to write rather than guessing at the file to write it in.
//! * **Windows** — has no per-suffix resolver at all. The only mechanism is
//!   setting the DNS server for an adapter, which redirects *everything* and is
//!   exactly what the section above refuses to be. So Windows keeps `/etc/hosts`
//!   — [`Support::Unsupported`] says so rather than offering a switch that
//!   quietly does nothing.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;

/// The port the responder listens on.
///
/// Above 1024 so no privilege is needed, and not one of the ports something
/// else is likely to want. 15353 rather than 5353: 5353 is mDNS, and a
/// development tool binding the port Bonjour uses is a support question.
pub const PORT: u16 = 15353;

/// The largest DNS message this reads. A UDP query that does not fit in 512
/// bytes is one this responder has no business answering.
const MAX: usize = 512;

/// What answering for a suffix costs on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    /// A file this app can write, with a password.
    Resolver,
    /// A line the user has to put somewhere only they know.
    Manual,
    /// No per-suffix mechanism exists.
    Unsupported,
}

pub fn support() -> Support {
    if cfg!(target_os = "macos") {
        Support::Resolver
    } else if cfg!(target_os = "linux") {
        Support::Manual
    } else {
        Support::Unsupported
    }
}

/// Where macOS looks for a per-suffix resolver.
///
/// Keyed by the **last label** of the suffix, which is what the system reads:
/// a workspace on `stackvo.loc` is served by `/etc/resolver/loc`, because macOS
/// matches a resolver file against the domain's tail. Naming the file
/// `stackvo.loc` would work too and would be narrower — but then two workspaces
/// with different prefixes under one TLD would each need their own file and
/// each would answer for the other's names anyway.
pub fn resolver_path(suffix: &str) -> Option<PathBuf> {
    if support() != Support::Resolver {
        return None;
    }
    let tld = suffix.rsplit('.').next()?;
    if tld.is_empty() {
        return None;
    }
    Some(PathBuf::from("/etc/resolver").join(tld))
}

/// The file macOS wants, and the line Linux wants, are the same two facts.
pub fn resolver_text() -> String {
    format!("nameserver 127.0.0.1\nport {PORT}\n")
}

/// The dnsmasq / NetworkManager line, for the platforms with no resolver dir.
pub fn forward_line(suffix: &str) -> String {
    let tld = suffix.rsplit('.').next().unwrap_or(suffix);
    format!("server=/{tld}/127.0.0.1#{PORT}")
}

/// What is set up and what is not, for a screen that has to explain it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub support: Support,
    /// The suffix this workspace's names end in.
    pub suffix: String,
    pub port: u16,
    /// Whether the responder is answering right now.
    pub listening: bool,
    /// `/etc/resolver/<tld>`, when the platform has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver_file: Option<String>,
    /// Whether that file exists and says what it should.
    pub resolver_configured: bool,
    /// The text this app would write, or the line the user must place.
    pub instruction: String,
}

/// Is the file there and does it point at us?
///
/// Compared on the two facts rather than byte for byte: a user who added a
/// comment or a `search` line has a working resolver, and rewriting their file
/// to make it match a literal would be this app taking a file it did not
/// create.
fn resolver_ok(path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut nameserver = false;
    let mut port = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "nameserver 127.0.0.1" || line == "nameserver ::1" {
            nameserver = true;
        }
        if line == format!("port {PORT}") {
            port = true;
        }
    }
    nameserver && port
}

pub fn status(suffix: &str, listening: bool) -> Status {
    let path = resolver_path(suffix);
    Status {
        support: support(),
        suffix: suffix.to_string(),
        port: PORT,
        listening,
        resolver_configured: path.as_deref().is_some_and(resolver_ok),
        instruction: match support() {
            Support::Resolver => resolver_text(),
            Support::Manual => forward_line(suffix),
            Support::Unsupported => String::new(),
        },
        resolver_file: path.map(|p| p.display().to_string()),
    }
}

/// Write `/etc/resolver/<tld>`, prompting for administrator rights.
///
/// The same staged-copy shape as [`crate::hosts::apply`], and for the same
/// reason: the elevated step is a plain copy of a file whose contents the user
/// has already seen, never a shell command carrying a value from anywhere else.
pub fn install_resolver(suffix: &str) -> Result<()> {
    let path = resolver_path(suffix).ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            "this platform has no per-suffix resolver directory",
        )
    })?;

    let staged = std::env::temp_dir().join("stackvo-resolver-staged");
    std::fs::write(&staged, resolver_text())
        .map_err(|e| Error::io("staging the resolver file", e))?;

    // The directory may not exist on a machine that has never had one, and it
    // is created in the same elevated step rather than in a second prompt.
    let ok = crate::elevate::run(&[
        "/bin/sh",
        "-c",
        &format!(
            "mkdir -p /etc/resolver && cp {} {}",
            shell_quote(&staged.display().to_string()),
            shell_quote(&path.display().to_string())
        ),
    ])?;
    let _ = std::fs::remove_file(&staged);

    if !ok {
        return Err(Error::new(
            Code::PermissionDenied,
            format!("{} was not written.", path.display()),
        ));
    }
    Ok(())
}

/// Remove it again, so turning this off is as easy as turning it on.
pub fn remove_resolver(suffix: &str) -> Result<()> {
    let path = resolver_path(suffix).ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            "this platform has no per-suffix resolver directory",
        )
    })?;
    if !path.exists() {
        return Ok(());
    }
    let ok = crate::elevate::run(&["/bin/rm", "-f", &path.display().to_string()])?;
    if !ok {
        return Err(Error::new(
            Code::PermissionDenied,
            format!("{} was not removed.", path.display()),
        ));
    }
    Ok(())
}

/// Single-quote a path for `sh -c`.
///
/// Both paths here are built by this module — a temp file and `/etc/resolver`
/// plus one label already checked by [`resolver_path`] — so nothing
/// user-supplied reaches this. It is here anyway because the next person to add
/// a third path will not know that, and a quoting function that exists is
/// cheaper than the review that would have caught its absence.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ------------------------------------------------------------ the responder

/// One question, as far as this needs to understand it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
pub const CLASS_IN: u16 = 1;

const RCODE_FORMERR: u8 = 1;
const RCODE_REFUSED: u8 = 5;

/// Read the question out of a query.
///
/// Compression pointers are refused rather than followed. A pointer in the
/// question section is not something a real resolver emits, and following one
/// means implementing loop detection for a message this responder is only ever
/// going to answer with 127.0.0.1 — an attack surface bought for nothing.
pub fn parse_question(message: &[u8]) -> std::result::Result<Question, u8> {
    if message.len() < 12 {
        return Err(RCODE_FORMERR);
    }
    let qdcount = u16::from_be_bytes([message[4], message[5]]);
    if qdcount != 1 {
        // Zero questions is not a query; more than one is legal on the wire and
        // implemented by nothing, and answering the first while ignoring the
        // rest is the kind of half-answer that confuses a resolver.
        return Err(RCODE_FORMERR);
    }

    let mut at = 12;
    let mut name = String::new();
    loop {
        let Some(&len) = message.get(at) else {
            return Err(RCODE_FORMERR);
        };
        if len & 0xC0 != 0 {
            return Err(RCODE_FORMERR);
        }
        at += 1;
        if len == 0 {
            break;
        }
        let end = at + len as usize;
        let Some(label) = message.get(at..end) else {
            return Err(RCODE_FORMERR);
        };
        // A label is bytes, not necessarily UTF-8. A name this responder cannot
        // read as text is a name it does not serve, which is a refusal rather
        // than a parse failure — but it must not be a panic either.
        let Ok(text) = std::str::from_utf8(label) else {
            return Err(RCODE_REFUSED);
        };
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&text.to_ascii_lowercase());
        at = end;
    }

    let Some(tail) = message.get(at..at + 4) else {
        return Err(RCODE_FORMERR);
    };
    Ok(Question {
        name,
        qtype: u16::from_be_bytes([tail[0], tail[1]]),
        qclass: u16::from_be_bytes([tail[2], tail[3]]),
    })
}

/// Build the reply to a query, or `None` when there is nothing to reply to.
///
/// `suffix` is the workspace's, and the match is on a label boundary: a
/// workspace on `loc` serves `shop.loc` and `a.b.shop.loc` — which is E-2's
/// wildcard, for free, because a suffix match does not care how many labels
/// precede it — and does not serve `notloc` or `evil-loc.com`.
pub fn reply(message: &[u8], suffix: &str) -> Option<Vec<u8>> {
    if message.len() < 12 {
        return None;
    }
    // A response, not a query. Answering one would be a loop.
    if message[2] & 0x80 != 0 {
        return None;
    }

    let question = match parse_question(message) {
        Ok(question) => question,
        Err(rcode) => return Some(header_only(message, rcode)),
    };

    if question.qclass != CLASS_IN || !serves(&question.name, suffix) {
        return Some(header_only(message, RCODE_REFUSED));
    }

    // NODATA, not NXDOMAIN: the name exists, this record type does not. A
    // resolver told NXDOMAIN for an MX query caches the *name* as absent, and
    // the next A query for it never reaches here.
    if question.qtype != TYPE_A && question.qtype != TYPE_AAAA {
        return Some(header_only(message, 0));
    }

    let mut out = header_only(message, 0);
    out[7] = 1; // ANCOUNT

    // The question, echoed. Copying the bytes rather than re-encoding the
    // parsed name: a resolver compares the question it gets back with the one
    // it sent, byte for byte.
    let question_len = question_bytes(message);
    out.extend_from_slice(&message[12..12 + question_len]);

    // A compression pointer to the question's name at offset 12, which is where
    // every DNS answer puts it.
    out.extend_from_slice(&[0xC0, 0x0C]);
    out.extend_from_slice(&question.qtype.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    // Sixty seconds. Long enough that a page load does not re-query per asset,
    // short enough that removing the resolver takes effect while somebody is
    // still looking at the screen they removed it from.
    out.extend_from_slice(&60u32.to_be_bytes());

    if question.qtype == TYPE_A {
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&Ipv4Addr::LOCALHOST.octets());
    } else {
        out.extend_from_slice(&16u16.to_be_bytes());
        out.extend_from_slice(&std::net::Ipv6Addr::LOCALHOST.octets());
    }

    Some(out)
}

/// Does this responder answer for this name?
fn serves(name: &str, suffix: &str) -> bool {
    let suffix = suffix.trim_matches('.').to_ascii_lowercase();
    if suffix.is_empty() {
        return false;
    }
    // The last label, so a workspace on `stackvo.loc` answers for every `.loc`
    // name — see `resolver_path` for why the resolver is registered that way
    // and why the two have to agree.
    let tld = suffix.rsplit('.').next().unwrap_or(&suffix);
    name == tld || name.ends_with(&format!(".{tld}"))
}

/// How many bytes the question occupies after the header.
fn question_bytes(message: &[u8]) -> usize {
    let mut at = 12;
    while let Some(&len) = message.get(at) {
        at += 1 + len as usize;
        if len == 0 {
            break;
        }
    }
    (at + 4 - 12).min(message.len() - 12)
}

/// A reply carrying the query's id and flags, an rcode and no records.
fn header_only(message: &[u8], rcode: u8) -> Vec<u8> {
    let mut out = vec![0u8; 12];
    out[0] = message[0];
    out[1] = message[1];
    // QR=1, AA=1, and RD copied back from the query the way every responder
    // does. RA stays 0: recursion is exactly what this does not offer.
    out[2] = 0x84 | (message[2] & 0x01);
    out[3] = rcode & 0x0F;
    out[4] = 0;
    out[5] = if rcode == RCODE_FORMERR { 0 } else { 1 };
    out
}

/// Serve until the socket is dropped.
///
/// Blocking, on its own thread. A responder this size has no reason to be
/// async, and the one thing it must not do is share a runtime with anything
/// that can be slow: name resolution that waits behind a Docker call is a
/// browser that hangs.
pub fn serve(
    socket: UdpSocket,
    suffix: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let mut buf = [0u8; MAX];
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(pair) => pair,
            // A timeout is how the stop flag gets looked at; anything else on a
            // bound loopback socket is not worth tearing the responder down for.
            Err(_) => continue,
        };
        if let Some(out) = reply(&buf[..len], &suffix) {
            let _ = socket.send_to(&out, from);
        }
    }
}

/// Bind loopback, or say why not.
pub fn bind() -> Result<UdpSocket> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    let socket = UdpSocket::bind(addr).map_err(|e| {
        Error::new(
            Code::IoError,
            format!("the DNS responder could not bind 127.0.0.1:{PORT}: {e}"),
        )
    })?;
    // So the stop flag is read at least this often.
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .map_err(|e| Error::io("configuring the DNS socket", e))?;
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A query for `name`, as bytes.
    fn query(name: &str, qtype: u16) -> Vec<u8> {
        let mut out = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
        out.extend_from_slice(&qtype.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out
    }

    fn rcode(reply: &[u8]) -> u8 {
        reply[3] & 0x0F
    }

    #[test]
    fn a_name_under_the_suffix_is_answered_with_loopback() {
        let out = reply(&query("shop.loc", TYPE_A), "stackvo.loc").unwrap();
        assert_eq!(rcode(&out), 0);
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 1, "one answer");
        assert_eq!(&out[out.len() - 4..], &[127, 0, 0, 1]);
        // The id is echoed, and a resolver drops a reply whose id differs.
        assert_eq!(&out[..2], &[0x12, 0x34]);
    }

    /// The half `/etc/hosts` cannot do, and the reason E-2 was left at 🟡.
    #[test]
    fn a_wildcard_falls_out_of_a_suffix_match() {
        for name in ["a.shop.loc", "deep.nested.shop.loc", "loc"] {
            let out = reply(&query(name, TYPE_A), "stackvo.loc").unwrap();
            assert_eq!(rcode(&out), 0, "{name}");
        }
    }

    /// The security property. Not a resolver, not a forwarder, no upstream.
    #[test]
    fn anything_outside_the_suffix_is_refused() {
        for name in ["google.com", "notloc", "evil-loc.com", "loc.evil.com"] {
            let out = reply(&query(name, TYPE_A), "stackvo.loc").unwrap();
            assert_eq!(rcode(&out), RCODE_REFUSED, "{name} was not refused");
            assert_eq!(u16::from_be_bytes([out[6], out[7]]), 0, "{name}");
        }
    }

    #[test]
    fn aaaa_is_answered_with_the_v6_loopback() {
        let out = reply(&query("shop.loc", TYPE_AAAA), "loc").unwrap();
        assert_eq!(rcode(&out), 0);
        assert_eq!(
            &out[out.len() - 16..],
            &std::net::Ipv6Addr::LOCALHOST.octets()
        );
    }

    /// NXDOMAIN for an MX query would poison the name for the A query after it.
    #[test]
    fn an_unserved_type_is_nodata_and_not_nxdomain() {
        let out = reply(&query("shop.loc", 15), "loc").unwrap();
        // 0 is NOERROR; 3 would be NXDOMAIN, which is the wrong answer here
        // and the reason this test exists.
        assert_eq!(rcode(&out), 0);
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 0, "no answers");
    }

    /// A pointer in the question is refused rather than followed — loop
    /// detection bought for a responder that always says 127.0.0.1 is an attack
    /// surface bought for nothing.
    #[test]
    fn a_compression_pointer_in_the_question_is_a_format_error() {
        let mut message = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_FORMERR);
    }

    /// Every one of these used to be a panic waiting for a stray packet.
    #[test]
    fn a_truncated_or_absurd_message_never_panics() {
        for message in [
            vec![],
            vec![0x12],
            vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0],
            vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0, 9, b'a'],
            vec![0x12, 0x34, 0x01, 0x00, 0, 2, 0, 0, 0, 0, 0, 0],
        ] {
            let _ = reply(&message, "loc");
        }
    }

    /// Answering a response is how two responders talk to each other for ever.
    #[test]
    fn a_response_is_not_answered() {
        let mut message = query("shop.loc", TYPE_A);
        message[2] |= 0x80;
        assert!(reply(&message, "loc").is_none());
    }

    /// A resolver compares the echoed question with the one it sent.
    #[test]
    fn the_question_is_echoed_byte_for_byte() {
        let message = query("shop.loc", TYPE_A);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(&out[12..12 + (message.len() - 12)], &message[12..]);
    }

    /// A label that is not UTF-8 is a name this cannot serve, not a crash.
    #[test]
    fn a_non_utf8_label_is_refused() {
        let mut message = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        message.extend_from_slice(&[2, 0xff, 0xfe, 3, b'l', b'o', b'c', 0, 0, 1, 0, 1]);
        let out = reply(&message, "loc").unwrap();
        assert_eq!(rcode(&out), RCODE_REFUSED);
    }

    // ---- the platform half ------------------------------------------------

    #[test]
    fn the_resolver_file_is_named_after_the_last_label() {
        if support() != Support::Resolver {
            return;
        }
        assert_eq!(
            resolver_path("stackvo.loc"),
            Some(PathBuf::from("/etc/resolver/loc"))
        );
        assert_eq!(
            resolver_path("test"),
            Some(PathBuf::from("/etc/resolver/test"))
        );
    }

    /// The file the app writes and the suffix the responder serves have to
    /// agree, or the machine asks a responder that refuses.
    #[test]
    fn the_resolver_file_and_the_served_suffix_agree() {
        assert!(serves("shop.loc", "stackvo.loc"));
        assert!(serves("anything.loc", "stackvo.loc"));
        assert!(!serves("shop.test", "stackvo.loc"));
    }

    #[test]
    fn the_forward_line_names_the_same_port_the_responder_binds() {
        assert_eq!(
            forward_line("stackvo.loc"),
            format!("server=/loc/127.0.0.1#{PORT}")
        );
        assert!(resolver_text().contains(&format!("port {PORT}")));
    }

    /// A user's own comment or `search` line must not make this report the
    /// resolver as unconfigured — the app did not create that file's style.
    #[test]
    fn a_resolver_file_with_extra_lines_still_counts() {
        let dir = std::env::temp_dir().join(format!("stackvo-dns-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loc");

        std::fs::write(
            &path,
            format!("# stackvo\nnameserver 127.0.0.1\nport {PORT}\n"),
        )
        .unwrap();
        assert!(resolver_ok(&path));

        std::fs::write(&path, "nameserver 127.0.0.1\n").unwrap();
        assert!(!resolver_ok(&path), "a file with no port points at 53");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoting_survives_a_path_with_a_quote_in_it() {
        assert_eq!(shell_quote("/tmp/a'b"), r"'/tmp/a'\''b'");
    }
}
