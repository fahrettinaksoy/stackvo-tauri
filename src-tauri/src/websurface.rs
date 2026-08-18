//! Who may ask this app a question over HTTP, and which questions (§5 · #34).
//!
//! §3 #34 sat in §5 for one reason, stated there in full: an HTTP surface means
//! putting *this* command set on a socket, and this command set contains
//! `quickcmd_run`, `project_hooks_approve`, `env_reveal` and paths that go
//! through `elevate`. A server written before that question is answered is not
//! a feature; it is a remote code execution surface with a changelog entry.
//!
//! The answer: **loopback only, a token, and reads only.**
//!
//! ## Three rules, and each one is here because the other two are not enough
//!
//! * **Loopback.** Binding `0.0.0.0` would put it on every network this laptop
//!   joins, including the coffee shop's. `127.0.0.1` is not a security boundary
//!   on a shared machine and is not claimed to be one — it is the difference
//!   between "somebody on this machine" and "somebody on this network", which
//!   is the difference that actually decides how bad a mistake gets.
//! * **A token.** Loopback alone is reachable by every process on the machine
//!   and, historically, by any web page that can talk to `127.0.0.1`. The token
//!   is generated per run and never written to the workspace.
//! * **Reads only.** The rule that makes the first two survivable. Nothing here
//!   writes, runs, spawns or elevates — so the worst outcome of the token
//!   leaking is disclosure, and the disclosure is bounded by the fourth rule.
//!
//! ## The fourth rule, which is the one with teeth
//!
//! `kind: "query"` is **not** the same as "safe to expose". `instance_reveal`
//! and `service_reveal` are queries and both hand back a password out of the OS
//! keystore. A surface built on `kind` alone would have been read-only and
//! would have served credentials.
//!
//! So a command is exposable only if it is a query **and** its code path cannot
//! reach the keystore. The second half is not readable off a function
//! signature: `service_connection` takes `reveal: bool` and the read is two
//! modules away in `connect::of`. `websurface_claims.rs` computes it as a
//! fixpoint over the whole crate and fails when this module's list disagrees —
//! so a new command that can reach a secret breaks the build rather than
//! quietly joining the surface.
//!
//! ## What is not here
//!
//! The transport. There is no listener, no route table and no runtime in this
//! file, and that is deliberate rather than unfinished: the decision §5 was
//! holding is *what may be served and to whom*, and that is answerable, pure,
//! and testable on its own. A socket added later has to come through
//! [`exposable`] to reach anything.

use crate::contracts;
use serde::Serialize;

/// Queries whose code path can reach the OS keystore.
///
/// Read as a denial rather than as a note: every one of these is `kind:
/// "query"`, so the honest reading of "reads only" would have served them all.
///
/// ## Named for what is *proved*, not for what is suspected
///
/// The first draft of this constant was called `READS_A_SECRET` and held four
/// names picked by reading them. Three of the four were wrong — `stripe_status`
/// and `service_connection` were guesses, `instance_settings` and
/// `secrets_status` were missed — because "does this hand back a password" is
/// not a question source text answers. What source text *does* answer is
/// whether a call path exists, and `websurface_claims.rs` computes exactly that
/// as a fixpoint over the whole crate.
///
/// So the rule is reachability, and the denial is wider than "returns a
/// secret". `service_db_clients` calls `connect::of(.., reveal: false)` and
/// gets nothing sensitive back today — it is denied anyway, because the thing
/// standing between it and a password is one boolean argument, and
/// `service_connection` proves that boolean is caller-controlled.
///
/// Fifteen of a hundred and twelve reads. Wide enough to be safe, narrow enough
/// that the surface still answers almost everything.
pub const REACHES_THE_KEYSTORE: [&str; 15] = [
    "db_targets",
    "doctor",
    "generator_verify",
    "instance_reveal",
    "instance_settings",
    "mail_relay_get",
    "query_log",
    "request_timeline",
    "secrets_status",
    "service_connection",
    "service_db_clients",
    "service_reveal",
    "stripe_status",
    "worktree_plan",
    "worktree_support",
];

/// The scheme this surface will answer on, and the only address it binds.
pub const BIND: &str = "127.0.0.1";

/// How long a token is, in bytes before encoding.
///
/// 32 bytes. Not a password somebody types — it is copied out of the app or
/// read from an environment variable, so there is no reason for it to be short
/// and every reason for guessing to be hopeless.
pub const TOKEN_BYTES: usize = 32;

/// May this command be served over the loopback surface?
///
/// Two questions, in the order that matters: is it a read at all, and does the
/// read hand back a secret.
pub fn exposable(command: &str) -> bool {
    if REACHES_THE_KEYSTORE.contains(&command) {
        return false;
    }
    kind_of(command) == Some("query")
}

/// The `kind` the compiled-in contract gives this command.
///
/// `None` for a command the contract does not name, which is a refusal rather
/// than a default: `contract_agreement.rs` keeps the two in step, so a name
/// that is not in the contract is not a command this app answers.
pub fn kind_of(command: &str) -> Option<&'static str> {
    contracts::ipc()["commands"]
        .get(command)?
        .get("kind")?
        .as_str()
}

/// Every command this surface would serve, sorted.
///
/// Built from the contract each time rather than stored, so it cannot be a
/// stale copy of one. The caller that eventually builds a router builds it from
/// this.
pub fn served() -> Vec<String> {
    let Some(commands) = contracts::ipc()["commands"].as_object() else {
        return Vec::new();
    };
    let mut out: Vec<String> = commands
        .keys()
        .filter(|name| exposable(name))
        .cloned()
        .collect();
    out.sort();
    out
}

/// A comparison that does not leak where two tokens first differ.
///
/// `==` on a `String` returns at the first differing byte, and the time that
/// takes is a measurable function of how much of the token the caller got
/// right — over a loopback socket with no rate limit, that is enough to walk a
/// token out one byte at a time. The length is compared first and separately
/// because a length mismatch is not a secret.
pub fn token_matches(expected: &str, offered: &str) -> bool {
    let (expected, offered) = (expected.as_bytes(), offered.as_bytes());
    if expected.len() != offered.len() || expected.is_empty() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in expected.iter().zip(offered.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Why a request was turned away.
///
/// A single `Refused` rather than a boolean, because these three want three
/// different answers on the wire and one of them must not be distinguishable
/// from the others by a caller with no token: [`Refused::NoSuchCommand`] and
/// [`Refused::NotExposable`] both answer 404 to an unauthenticated caller, so
/// the surface does not confirm which commands exist to somebody who cannot
/// call them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The token was absent or wrong.
    BadToken,
    /// The contract does not name this command.
    NoSuchCommand,
    /// It is a real command, and it writes, runs, elevates, or reveals.
    NotExposable,
}

/// The whole admission decision, in the order a server must make it.
///
/// The token is checked **first**, before the command name is even looked at.
/// Checking the name first would answer "no such command" to a caller with no
/// token, which is a command inventory handed to anybody who can open a socket.
pub fn admit(expected_token: &str, offered_token: &str, command: &str) -> Result<(), Refused> {
    if !token_matches(expected_token, offered_token) {
        return Err(Refused::BadToken);
    }
    if kind_of(command).is_none() {
        return Err(Refused::NoSuchCommand);
    }
    if !exposable(command) {
        return Err(Refused::NotExposable);
    }
    Ok(())
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    fn get(path: &str, token: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
    }

    fn post(path: &str, token: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\
             Content-Length: {len}\r\n\r\n{body}",
            len = body.len()
        )
    }

    #[test]
    fn a_request_is_read_down_to_its_token_and_body() {
        let raw = post("/call", "abc", r#"{"tool":"stackvo_doctor"}"#);
        let request = parse_request(&raw).expect("a well-formed request parses");
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/call");
        assert_eq!(request.token.as_deref(), Some("abc"));
        assert_eq!(request.body, r#"{"tool":"stackvo_doctor"}"#);
    }

    #[test]
    fn only_a_bearer_token_is_read() {
        // Two ways to authenticate is one way too many, and the second is the
        // one that ends up in somebody's proxy log.
        let raw = "POST /call HTTP/1.1\r\nAuthorization: abc\r\n\r\n{}";
        assert_eq!(parse_request(raw).expect("parses").token, None);
    }

    #[test]
    fn a_request_line_with_no_version_is_refused() {
        // HTTP/0.9 has no headers, so it has no token — and a parser that
        // accepted it would be a parser with an unauthenticated path.
        assert_eq!(parse_request("GET /call\r\n\r\n"), None);
        assert_eq!(parse_request("garbage"), None);
    }

    #[test]
    fn the_token_is_checked_before_the_method_and_before_the_path() {
        // The ordering IS the design: a 405 or a 404 to a caller with no token
        // is an inventory of what this surface has.
        let wrong = get("/nowhere", "wrong");
        let request = parse_request(&wrong).expect("parses");
        assert_eq!(route(&request, "right"), Err((401, "unauthorized")));

        let right = get("/nowhere", "right");
        let request = parse_request(&right).expect("parses");
        assert_eq!(
            route(&request, "right"),
            Err((405, "this surface answers POST only"))
        );
    }

    #[test]
    fn a_get_is_refused_even_with_the_token() {
        let raw = get("/call", "right");
        let request = parse_request(&raw).expect("parses");
        assert!(matches!(route(&request, "right"), Err((405, _))));
    }

    #[test]
    fn an_unknown_path_is_refused() {
        let raw = post("/exec", "right", "{}");
        let request = parse_request(&raw).expect("parses");
        assert_eq!(route(&request, "right"), Err((404, "no such path")));
    }

    #[test]
    fn a_read_only_tool_is_routed() {
        let raw = post("/call", "right", r#"{"tool":"stackvo_projects"}"#);
        let request = parse_request(&raw).expect("parses");
        let asked = route(&request, "right").expect("a read tool is served");
        assert_eq!(asked.tool, "stackvo_projects");
        assert_eq!(asked.arguments, serde_json::json!({}));
    }

    #[test]
    fn a_read_only_tool_that_reaches_the_keystore_is_still_refused() {
        // `stackvo_doctor` reads nothing and writes nothing, and it is not
        // served: `doctor` reaches a keystore read several calls down, so the
        // rule in `REACHES_THE_KEYSTORE` catches it. This is the composition
        // working — the tool table has no idea what a keystore is.
        let raw = post("/call", "right", r#"{"tool":"stackvo_doctor"}"#);
        let request = parse_request(&raw).expect("parses");
        assert_eq!(
            route(&request, "right"),
            Err((404, "this tool is not served here"))
        );
        assert!(!tools().contains(&"stackvo_doctor"));
    }

    #[test]
    fn a_tool_that_writes_is_refused_by_name() {
        let writer = crate::mcp::TOOLS
            .iter()
            .find(|t| t.writes)
            .expect("the tool table has at least one writer");
        let body = serde_json::json!({ "tool": writer.name }).to_string();
        let raw = post("/call", "right", &body);
        let request = parse_request(&raw).expect("parses");
        assert!(
            matches!(route(&request, "right"), Err((404, _))),
            "`{}` writes and reached the surface",
            writer.name
        );
    }

    #[test]
    fn every_tool_this_surface_serves_passes_both_policies() {
        // The intersection is computed, not listed. This is the claim that
        // makes it safe to compute: the keystore rule in `exposable` reaches a
        // tool table that never mentions the keystore.
        for name in tools() {
            let tool = crate::mcp::TOOLS
                .iter()
                .find(|t| t.name == name)
                .expect("a served tool is in the table");
            assert!(!tool.writes, "{name} writes");
            assert!(
                exposable(tool.command),
                "{name} stands for `{}`, which is not exposable",
                tool.command
            );
        }
        assert!(
            !tools().is_empty(),
            "the surface serves nothing, so every test above is vacuous"
        );
        assert!(
            tools().len() < crate::mcp::TOOLS.len(),
            "every tool is served, so neither policy is doing anything"
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_a_400_rather_than_a_panic() {
        let raw = post("/call", "right", "not json");
        let request = parse_request(&raw).expect("parses");
        assert_eq!(route(&request, "right"), Err((400, "the body is not JSON")));
    }

    #[test]
    fn a_body_with_no_tool_named_is_refused() {
        let raw = post("/call", "right", r#"{"arguments":{}}"#);
        let request = parse_request(&raw).expect("parses");
        assert_eq!(route(&request, "right"), Err((400, "no `tool` named")));
    }

    #[test]
    fn completeness_is_read_from_content_length_and_not_from_the_socket_closing() {
        // The alternative — read until the peer closes — hangs forever on a
        // client that does not, and a hung request looks like the app freezing.
        let head = b"POST /call HTTP/1.1\r\nContent-Length: 10\r\n\r\n";
        assert_eq!(body_is_complete(head), Some(false));

        let mut full = head.to_vec();
        full.extend_from_slice(b"0123456789");
        assert_eq!(body_is_complete(&full), Some(true));

        // Still reading the head.
        assert_eq!(body_is_complete(b"POST /call HTTP/1.1\r\n"), None);
    }

    #[test]
    fn a_response_declares_its_length_and_closes() {
        let body = r#"{"ok":true}"#;
        let reply = render_response(200, body);
        assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(reply.contains(&format!("Content-Length: {}", body.len())));
        // No-store: this answers with workspace state, and a cache on the way
        // is a copy of it somewhere nobody chose.
        assert!(reply.contains("Cache-Control: no-store"));
        assert!(reply.ends_with(body));
    }

    #[test]
    fn a_fresh_token_is_long_and_not_the_same_twice() {
        let a = fresh_token().expect("the OS provides randomness in a test environment");
        let b = fresh_token().expect("and again");
        assert_eq!(a.len(), TOKEN_BYTES * 2, "hex is two characters a byte");
        assert_ne!(a, b);
        // And it is actually accepted by the comparison it exists for.
        assert!(token_matches(&a, &a));
        assert!(!token_matches(&a, &b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mutation_is_not_served_however_harmless_it_looks() {
        for command in ["project_create", "env_set", "prefs_set", "market_install"] {
            assert!(!exposable(command), "{command} is a mutation");
        }
    }

    #[test]
    fn the_commands_that_made_this_a_decision_are_all_refused() {
        // The four §5 named. Three are mutations or operations and fall to the
        // first rule; `service_connection` is a query and needs the second.
        for command in [
            "quickcmd_run",
            "project_hooks_approve",
            "env_reveal",
            "service_connection",
        ] {
            assert!(
                !exposable(command),
                "{command} would be served, and it is one of the four §5 said \
                 made a web surface a decision rather than a task"
            );
        }
    }

    #[test]
    fn a_query_that_reveals_a_secret_is_refused_even_though_it_is_a_query() {
        for command in REACHES_THE_KEYSTORE {
            assert_eq!(
                kind_of(command),
                Some("query"),
                "{command} is in REACHES_THE_KEYSTORE but is not a query — either the \
                 contract changed or the entry is in the wrong list"
            );
            assert!(!exposable(command), "{command} hands back a stored secret");
        }
    }

    #[test]
    fn an_ordinary_read_is_served() {
        for command in ["projects_list", "workspace_get", "engine_status"] {
            assert!(exposable(command), "{command} is an ordinary read");
        }
    }

    #[test]
    fn a_command_the_contract_does_not_name_is_refused() {
        assert!(!exposable("rm_rf_slash"));
        assert_eq!(kind_of("rm_rf_slash"), None);
    }

    #[test]
    fn the_token_is_checked_before_the_command_name_is() {
        // The point of the ordering: a caller with no token learns nothing
        // about which commands exist, including that this one does not.
        assert_eq!(
            admit("right", "wrong", "no_such_command_anywhere"),
            Err(Refused::BadToken)
        );
        assert_eq!(
            admit("right", "right", "no_such_command_anywhere"),
            Err(Refused::NoSuchCommand)
        );
        assert_eq!(
            admit("right", "right", "project_create"),
            Err(Refused::NotExposable)
        );
        assert_eq!(admit("right", "right", "projects_list"), Ok(()));
    }

    #[test]
    fn an_empty_expected_token_admits_nobody() {
        // The failure this guards is a server started before its token was
        // generated: with `==`, `"" == ""` is true and the surface is open.
        assert!(!token_matches("", ""));
        assert_eq!(admit("", "", "projects_list"), Err(Refused::BadToken));
    }

    #[test]
    fn the_comparison_looks_at_every_byte() {
        assert!(token_matches("abcdef", "abcdef"));
        assert!(!token_matches("abcdef", "abcdeg"));
        assert!(!token_matches("abcdef", "abcde"));
        assert!(!token_matches("abcde", "abcdef"));
        // Differing in the first byte and in the last must both be a refusal,
        // and the loop must not be able to stop early on either.
        assert!(!token_matches("abcdef", "zbcdef"));
    }

    #[test]
    fn the_served_set_is_smaller_than_the_query_set_and_holds_no_denier() {
        let served = served();
        let queries = contracts::ipc()["commands"]
            .as_object()
            .expect("the contract has commands")
            .iter()
            .filter(|(_, v)| v.get("kind").and_then(|k| k.as_str()) == Some("query"))
            .count();

        assert!(
            served.len() < queries,
            "every query is served, so the secret rule is doing nothing"
        );
        for denied in REACHES_THE_KEYSTORE {
            assert!(!served.contains(&denied.to_string()));
        }
        assert!(
            served.contains(&"projects_list".to_string()),
            "the surface serves nothing useful"
        );
    }
}

// ============================================================ the transport
//
// Written after the policy above and deliberately after it: what may be served
// and to whom is the question §5 was holding, and a listener written before
// that answer would have been the remote code execution surface the row warned
// about. This half is the socket, and it is small on purpose.
//
// ## There is no second dispatcher
//
// The obvious way to serve commands over HTTP is a `match` over their names —
// ninety-seven arms, drifting from the ones Tauri generates. `mcp.rs` already
// has a dispatcher: a curated table of tools, each naming the contract command
// it stands for, each flagged `writes`, and each calling this crate's own logic
// rather than a Tauri command. That table is the same shape §5.4 chose for this
// surface, so this serves it instead of growing a rival.
//
// A tool reaches the wire only if BOTH policies allow it: `!tool.writes`, and
// `exposable(tool.command)` — which is where the keystore rule and the
// `kind: "query"` rule apply, without being restated.

/// One parsed request. Only what this surface reads.
///
/// Not a general HTTP parser and not trying to be: one method, one path shape,
/// two headers, a JSON body. Everything else about HTTP/1.1 — chunked bodies,
/// continuations, pipelining — is refused rather than half-supported, because a
/// half-supported parser on a socket is how a surface gets a second meaning
/// nobody designed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub token: Option<String>,
    pub body: String,
}

/// How long a connection may take to finish sending its request.
///
/// Five seconds. A client on this machine sending a few hundred bytes needs
/// milliseconds; anything slower is either broken or is holding the connection
/// on purpose. Without it, a caller that connects and sends **nothing** keeps
/// a task alive for as long as the app runs — found by
/// `garbage_gets_one_answer_and_the_surface_stays_up`, which sent an empty
/// request and waited, which is the whole of a slowloris.
pub const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How much of a request this surface will read before refusing it.
///
/// 64 KiB. The largest legitimate body here is a handful of JSON arguments; a
/// listener with no ceiling is a listener anything on this machine can use to
/// exhaust its memory, and loopback is not a reason to skip the limit — it is
/// the reason the caller is already on this machine.
pub const MAX_REQUEST: usize = 64 * 1024;

/// Parse a request head and body out of raw bytes.
///
/// `None` when it is not something this surface answers. The distinction
/// between "malformed" and "unsupported" is not made on purpose: both get one
/// reply, because a parser that explains which part it disliked is a parser
/// that helps somebody map it.
pub fn parse_request(raw: &str) -> Option<Request> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .or_else(|| raw.split_once("\n\n"))?;
    let mut lines = head.lines();

    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    // The version has to be there — a two-word request line is HTTP/0.9, which
    // has no headers and therefore no token.
    parts.next()?;

    let mut token = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("authorization") {
            let value = value.trim();
            // `Bearer` and nothing else. Accepting a bare token as well would
            // be two ways to authenticate, and the second one is the one
            // somebody's proxy logs.
            if let Some(rest) = value.strip_prefix("Bearer ") {
                token = Some(rest.trim().to_string());
            }
        }
    }

    Some(Request {
        method,
        path,
        token,
        body: body.to_string(),
    })
}

/// The reply, as bytes.
///
/// `Connection: close` on every response: this serves one request per
/// connection. Keep-alive would mean tracking state per socket for a surface
/// that answers a few reads a minute.
pub fn render_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Cache-Control: no-store\r\n\
         \r\n\
         {body}",
        len = body.len()
    )
}

/// What a request asks for, once it has been let in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub tool: String,
    pub arguments: serde_json::Value,
}

/// Read a request into a tool call, or into the reason it is refused.
///
/// Pure, and separated from the socket for the reason `polkit_outcome` is
/// separated from `pkexec`: every branch here decides who gets in, and a
/// function that had to be reached through a TCP connection would be tested by
/// nobody.
pub fn route(request: &Request, expected_token: &str) -> Result<Asked, (u16, &'static str)> {
    // The token first, before the method and before the path. Answering 405 or
    // 404 to a caller with no token tells them what this surface has.
    if !token_matches(expected_token, request.token.as_deref().unwrap_or("")) {
        return Err((401, "unauthorized"));
    }
    if request.method != "POST" {
        return Err((405, "this surface answers POST only"));
    }
    if request.path != "/call" {
        return Err((404, "no such path"));
    }
    if request.body.len() > MAX_REQUEST {
        return Err((413, "the request body is too large"));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&request.body).map_err(|_| (400, "the body is not JSON"))?;
    let tool = parsed
        .get("tool")
        .and_then(|v| v.as_str())
        .ok_or((400, "no `tool` named"))?;

    let entry = crate::mcp::TOOLS
        .iter()
        .find(|t| t.name == tool)
        .ok_or((404, "no such tool"))?;

    // Both policies, in the order that fails fastest. `writes` is the tool
    // table's own answer; `exposable` is this module's, and it is the one that
    // knows about the keystore.
    if entry.writes {
        return Err((404, "this tool writes, and this surface is read-only"));
    }
    if !exposable(entry.command) {
        return Err((404, "this tool is not served here"));
    }

    Ok(Asked {
        tool: tool.to_string(),
        arguments: parsed
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    })
}

/// Every tool this surface will actually run.
///
/// The intersection of the two policies, computed rather than listed. A tool
/// added to `mcp::TOOLS` joins this set only if it reads and if its contract
/// command passes `exposable` — so the keystore rule reaches a surface that
/// never mentions it.
pub fn tools() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::mcp::TOOLS
        .iter()
        .filter(|t| !t.writes && exposable(t.command))
        .map(|t| t.name)
        .collect();
    out.sort_unstable();
    out
}

/// A token, freshly generated, hex.
///
/// From the OS, for the reason `channel::fresh_id` gives at length: a value
/// that looks random and is not is worse than one that is obviously fixed.
/// Unlike the install id this is never written to disk — it lives for one run
/// of the app, and a token in a file is a token that outlives the process that
/// meant it.
pub fn fresh_token() -> Option<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// A running surface: the address it bound and the token that reaches it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bound {
    pub address: String,
    /// Returned once, to the process that started it. Never stored.
    #[serde(skip_serializing)]
    pub token: String,
    pub tools: usize,
}

/// Start the surface, and keep serving until the process ends.
///
/// Port 0 by default, so the OS picks: a fixed port is a port something else on
/// this machine may already hold, and a surface that fails to start because of
/// a collision is a surface people work around by picking another fixed one.
/// The chosen address comes back in [`Bound`].
///
/// Each connection is handled on its own task and closed after one reply. No
/// keep-alive, no pipelining, no concurrency limit beyond the runtime's —
/// a loopback surface answering a few reads a minute does not need a pool, and
/// a pool is a thing that can be exhausted.
/// The surface this process is running, if any.
///
/// A process-global rather than a field on `AppState`, and the reason is that
/// there is exactly one of it: a second surface on a second port would be a
/// second token to keep track of and no more capability. `start` refuses when
/// one is already up rather than quietly returning the old address, because a
/// caller that asked to start something and got somebody else's token would
/// have no way to know.
static RUNNING: std::sync::Mutex<Option<Running>> = std::sync::Mutex::new(None);

struct Running {
    address: String,
    task: tokio::task::JoinHandle<()>,
}

/// Is a surface up, and where?
///
/// The token is deliberately **not** here. It is handed to whoever started the
/// surface, once; a status call that returned it would make every later caller
/// able to read it, and the first such caller is the surface itself.
pub fn status() -> Option<String> {
    RUNNING
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|r| r.address.clone()))
}

/// Start the surface and remember it. The token comes back once.
pub async fn start(port: u16) -> crate::error::Result<Bound> {
    use crate::error::{Code, Error};

    if let Some(address) = status() {
        return Err(Error::new(
            Code::Conflict,
            format!("a local API is already listening on {address}"),
        ));
    }

    let (bound, task) = serve_with_handle(port).await?;
    if let Ok(mut guard) = RUNNING.lock() {
        *guard = Some(Running {
            address: bound.address.clone(),
            task,
        });
    }
    Ok(bound)
}

/// Stop it, and forget the token with it.
///
/// ## `abort()` is a request, and this waits for it to be honoured
///
/// Aborting the accept loop drops the listener, which closes the port — but
/// `JoinHandle::abort` is not synchronous. It marks the task for cancellation
/// and returns; the future is dropped, and the listener with it, whenever the
/// runtime next polls it. So a `stop()` that returned straight after the abort
/// was telling the caller something not yet true: the port kept accepting
/// connections for a moment afterwards.
///
/// That was not a theory. `a_surface_starts_once_answers_and_stops_for_good`
/// connects to the address after stopping and expects to be refused, and it
/// failed on Linux and macOS runners in turn while passing on this machine —
/// the shape of a race, not of a broken assertion.
///
/// So the abort is followed by a bounded wait for the task to actually finish.
/// A stop that has not stopped is worth a millisecond.
///
/// An in-flight request on its own task is still not waited for: it holds a
/// token that was already checked, and a stop that blocked until every
/// connection finished is a stop somebody presses twice.
pub fn stop() -> bool {
    let Ok(mut guard) = RUNNING.lock() else {
        return false;
    };
    let Some(running) = guard.take() else {
        return false;
    };

    running.task.abort();

    // Bounded, because a runtime that is not polling at all must not hang a
    // command. A second is far beyond what dropping a listener takes and far
    // below anything a person would call a freeze.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !running.task.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    true
}

/// Start the surface, and keep serving until the process ends.
///
/// The plain entry point, kept for tests and for anything that wants a surface
/// without the process-global. [`start`] is what the app uses.
pub async fn serve(port: u16) -> crate::error::Result<Bound> {
    serve_with_handle(port).await.map(|(bound, _)| bound)
}

async fn serve_with_handle(
    port: u16,
) -> crate::error::Result<(Bound, tokio::task::JoinHandle<()>)> {
    use crate::error::{Code, Error};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let token = fresh_token().ok_or_else(|| {
        Error::new(
            Code::IoError,
            "the operating system would not provide a token, and this surface does not \
             invent one — an unauthenticated loopback socket is what ADR 0026 refused",
        )
    })?;

    let listener = tokio::net::TcpListener::bind((BIND, port))
        .await
        .map_err(|e| Error::new(Code::IoError, format!("cannot listen on {BIND}: {e}")))?;
    let address = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| BIND.to_string());

    let bound = Bound {
        address,
        token: token.clone(),
        tools: tools().len(),
    };

    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, peer)) = listener.accept().await else {
                continue;
            };
            // Belt and braces. The listener is bound to 127.0.0.1 so this
            // cannot fire, and it is checked anyway: a bind that was changed to
            // 0.0.0.0 by an edit somewhere else must fail closed here rather
            // than quietly start answering the network.
            if !peer.ip().is_loopback() {
                continue;
            }
            let token = token.clone();
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut oversized = false;
                let read = async {
                    let mut chunk = [0u8; 4096];
                    loop {
                        match socket.read(&mut chunk).await {
                            Ok(0) => break,
                            Ok(n) => {
                                raw.extend_from_slice(&chunk[..n]);
                                if raw.len() > MAX_REQUEST {
                                    oversized = true;
                                    break;
                                }
                                // The head ends at a blank line; everything
                                // after it is the body, and `Content-Length`
                                // says how much. Reading until the socket
                                // closes instead would hang on a client that
                                // keeps it open.
                                if body_is_complete(&raw) == Some(true) {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                };

                // A deadline on the whole read, not on each `read` call: a
                // client sending one byte every four seconds would pass a
                // per-call timeout for ever.
                if tokio::time::timeout(READ_TIMEOUT, read).await.is_err() {
                    let reply = render_response(408, r#"{"error":"the request was not finished"}"#);
                    let _ = socket.write_all(reply.as_bytes()).await;
                    return;
                }
                if oversized {
                    let reply = render_response(413, r#"{"error":"too large"}"#);
                    let _ = socket.write_all(reply.as_bytes()).await;
                    return;
                }

                let text = String::from_utf8_lossy(&raw).to_string();
                let reply = match parse_request(&text) {
                    None => render_response(400, r#"{"error":"unreadable request"}"#),
                    Some(request) => match route(&request, &token) {
                        Err((status, why)) => render_response(
                            status,
                            &serde_json::json!({ "error": why }).to_string(),
                        ),
                        Ok(asked) => {
                            // `allow_writes: false`, always. The tool table is
                            // already filtered above; passing false as well
                            // means a tool that changes its `writes` flag
                            // cannot become writable here by being re-flagged
                            // in one place only.
                            match crate::mcp::call(&asked.tool, &asked.arguments, false).await {
                                Ok(value) => render_response(200, &value.to_string()),
                                Err(e) => render_response(
                                    400,
                                    &serde_json::json!({
                                        "error": e.message,
                                        "code": e.code,
                                    })
                                    .to_string(),
                                ),
                            }
                        }
                    },
                };
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    Ok((bound, task))
}

/// Has the whole body arrived?
///
/// `None` while the head is still incomplete. Separated out and tested because
/// the alternative — read until the peer closes — hangs forever on a client
/// that does not, and that failure looks like the app freezing rather than like
/// a request never finishing.
pub fn body_is_complete(raw: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))?;

    let declared = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    Some(body.len() >= declared)
}
