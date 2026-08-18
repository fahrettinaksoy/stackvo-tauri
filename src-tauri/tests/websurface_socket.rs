//! The loopback surface, over a real socket.
//!
//! `websurface.rs`'s own tests cover the pure halves — parsing, routing, the
//! token comparison — and every one of them hands a `Request` to a function.
//! That is the right shape for the logic and it proves nothing about the
//! listener: whether it binds where it says, whether it reads a body to the end
//! rather than until the peer gives up, whether a caller with no token gets the
//! same answer for a path that exists and one that does not.
//!
//! Those are the questions a socket answers. `elevate_probe.rs` makes the same
//! argument for `pkexec` and states it better: a coder testing against their own
//! expectation only agrees with themselves.
//!
//! ## No client library
//!
//! The requests below are written as bytes over `TcpStream`. ADR 0019's method
//! and `tests/driver/webdriver.js`'s: the thing being tested is what goes on the
//! wire, and a client that normalises a malformed request before sending it
//! would hide exactly the case worth checking.
//!
//! ## Why `multi_thread`
//!
//! `#[tokio::test]` gives a current-thread runtime, and every request below is
//! written with blocking `std::net` calls on the test thread. On one thread the
//! accept loop never gets polled while the test is inside `read_to_string`, so
//! the first version of this file deadlocked until the read timeout and
//! reported the server as silent. The server was fine; the test was holding the
//! only thread it had.

use std::io::{Read, Write};
use std::net::TcpStream;

use stackvo_desktop_lib::websurface;

/// Start a surface and give back what reaches it.
async fn started() -> websurface::Bound {
    websurface::serve(0).await.expect("the surface binds")
}

/// One request, one reply, as text.
fn speak(address: &str, request: &str) -> String {
    let mut socket = TcpStream::connect(address).expect("the surface accepts a connection");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a read timeout can be set");
    socket
        .write_all(request.as_bytes())
        .expect("the request is written");
    socket.flush().expect("and flushed");

    let mut reply = String::new();
    socket
        .read_to_string(&mut reply)
        .expect("the surface replies and closes");
    reply
}

fn status_of(reply: &str) -> u16 {
    reply
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn post(path: &str, token: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\n\
         Content-Length: {len}\r\n\r\n{body}",
        len = body.len()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_surface_binds_to_loopback_and_nowhere_else() {
    let bound = started().await;
    assert!(
        bound.address.starts_with("127.0.0.1:"),
        "bound to {} — ADR 0026 binds loopback, and `0.0.0.0` would put this on \
         every network this laptop joins",
        bound.address
    );
    assert!(
        !bound.address.ends_with(":0"),
        "the OS was asked for a port and the reported address still says 0, so \
         a caller cannot find it"
    );
    assert!(bound.tools > 0, "the surface serves nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_caller_with_no_token_learns_nothing_about_what_is_here() {
    let bound = started().await;

    // The same answer for a path that exists, a path that does not, and a
    // method this surface refuses. Anything else is an inventory.
    let mut seen = Vec::new();
    for request in [
        post("/call", "wrong", r#"{"tool":"stackvo_projects"}"#),
        post("/nowhere", "wrong", "{}"),
        "GET /call HTTP/1.1\r\nAuthorization: Bearer wrong\r\n\r\n".to_string(),
        "POST /call HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}".to_string(),
    ] {
        let reply = speak(&bound.address, &request);
        seen.push(status_of(&reply));
    }

    assert_eq!(
        seen,
        vec![401, 401, 401, 401],
        "an unauthenticated caller got different answers for different \
         requests, which is a map of this surface handed to somebody who \
         cannot use it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_read_answers_json() {
    let bound = started().await;
    let reply = speak(
        &bound.address,
        &post("/call", &bound.token, r#"{"tool":"stackvo_projects"}"#),
    );

    assert_eq!(status_of(&reply), 200, "reply was:\n{reply}");
    assert!(reply.contains("Content-Type: application/json"));
    assert!(
        reply.contains("Cache-Control: no-store"),
        "the answer is workspace state, and a cache on the way is a copy of it \
         somewhere nobody chose"
    );

    let body = reply
        .split_once("\r\n\r\n")
        .expect("a body follows the head")
        .1;
    serde_json::from_str::<serde_json::Value>(body)
        .unwrap_or_else(|e| panic!("the body is not JSON ({e}): {body}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_that_writes_is_refused_over_the_wire_and_not_only_in_the_router() {
    let bound = started().await;
    let writer = stackvo_desktop_lib::mcp::TOOLS
        .iter()
        .find(|t| t.writes)
        .expect("the tool table has a writer");

    let body = serde_json::json!({ "tool": writer.name }).to_string();
    let reply = speak(&bound.address, &post("/call", &bound.token, &body));

    assert_eq!(
        status_of(&reply),
        404,
        "`{}` writes and the socket accepted it",
        writer.name
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_sent_in_two_pieces_is_still_read_whole() {
    // The failure this guards: reading until the peer closes. A client that
    // sends a head, pauses, and then sends the body is ordinary; a server that
    // waits for a close instead of honouring `Content-Length` hangs on it, and
    // a hung request looks like the app freezing rather than like a request
    // that never finished.
    let bound = started().await;
    let body = r#"{"tool":"stackvo_projects"}"#;

    let mut socket = TcpStream::connect(&bound.address).expect("connects");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a read timeout can be set");
    let head = format!(
        "POST /call HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n",
        bound.token,
        body.len()
    );
    socket.write_all(head.as_bytes()).expect("head written");
    socket.flush().expect("head flushed");
    std::thread::sleep(std::time::Duration::from_millis(150));
    socket.write_all(body.as_bytes()).expect("body written");
    socket.flush().expect("body flushed");

    let mut reply = String::new();
    socket.read_to_string(&mut reply).expect("a reply arrives");
    assert_eq!(status_of(&reply), 200, "reply was:\n{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversized_request_is_cut_off_rather_than_read() {
    // A listener with no ceiling is one anything on this machine can use to
    // exhaust its memory, and loopback is not a reason to skip the limit — it
    // is the reason the caller is already here.
    let bound = started().await;
    let huge = "x".repeat(websurface::MAX_REQUEST + 4096);
    let request = post("/call", &bound.token, &huge);

    let mut socket = TcpStream::connect(&bound.address).expect("connects");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a read timeout can be set");
    // The write may fail partway, because the surface answers and closes while
    // the body is still arriving — which is the behaviour being checked.
    let _ = socket.write_all(request.as_bytes());
    let _ = socket.flush();

    let mut reply = String::new();
    let _ = socket.read_to_string(&mut reply);
    assert_eq!(
        status_of(&reply),
        413,
        "an oversized body was accepted; reply was:\n{reply}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_surfaces_do_not_share_a_token() {
    // Each run generates its own, and it is never written to disk: a token in a
    // file outlives the process that meant it.
    let (a, b) = (started().await, started().await);
    assert_ne!(a.token, b.token);
    assert_ne!(a.address, b.address);

    let reply = speak(
        &b.address,
        &post("/call", &a.token, r#"{"tool":"stackvo_projects"}"#),
    );
    assert_eq!(
        status_of(&reply),
        401,
        "one surface's token opened another's"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn garbage_gets_one_answer_and_the_surface_stays_up() {
    let bound = started().await;

    for junk in ["\r\n\r\n", "not http at all\r\n\r\n", "GET\r\n\r\n"] {
        let reply = speak(&bound.address, junk);
        let status = status_of(&reply);
        assert!(
            status == 400 || status == 401,
            "junk {junk:?} answered {status}"
        );
    }

    // The empty request is its own case, and it found a real hole: a caller
    // that connects and sends nothing held a task for as long as the app ran,
    // which is the whole of a slowloris. It gets a 408 now, and the number
    // being checked is the WAIT — under the surface's own deadline rather than
    // under this test's read timeout.
    let started = std::time::Instant::now();
    let reply = speak(&bound.address, "");
    assert_eq!(
        status_of(&reply),
        408,
        "an empty request answered:\n{reply}"
    );
    assert!(
        started.elapsed() < websurface::READ_TIMEOUT * 2,
        "the surface took {:?} to give up on a silent client",
        started.elapsed()
    );

    // And it is still serving afterwards, which is the half a fuzz case is
    // actually about.
    let reply = speak(
        &bound.address,
        &post("/call", &bound.token, r#"{"tool":"stackvo_projects"}"#),
    );
    assert_eq!(status_of(&reply), 200);
}

// ------------------------------------------------------- starting and stopping

/// The lifecycle, which is the half a user touches.
///
/// Kept in one test rather than three, deliberately: `start`/`stop` share a
/// process-global, and three tests racing over it would be a flake that looks
/// like a bug in the surface. The steps are ordered because the ordering is the
/// claim — a stop that does not free the port is a stop in name only.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_surface_starts_once_answers_and_stops_for_good() {
    assert!(
        websurface::status().is_none(),
        "something was already running before this test began"
    );
    assert!(
        !websurface::stop(),
        "stopping nothing reported that it stopped something"
    );

    let bound = websurface::start(0).await.expect("it starts");
    assert_eq!(
        websurface::status().as_deref(),
        Some(bound.address.as_str())
    );

    // A second start is a conflict, not a second surface. Silently handing back
    // the first one's address would give the caller a token it never saw.
    let again = websurface::start(0).await;
    assert!(
        again.is_err(),
        "a second surface started while the first was up"
    );
    assert_eq!(
        websurface::status().as_deref(),
        Some(bound.address.as_str()),
        "the failed second start disturbed the first"
    );

    let reply = speak(
        &bound.address,
        &post("/call", &bound.token, r#"{"tool":"stackvo_projects"}"#),
    );
    assert_eq!(status_of(&reply), 200);

    assert!(websurface::stop(), "stop reported nothing to stop");
    assert!(websurface::status().is_none());

    // The port is actually free: a listener that was only forgotten would still
    // accept, and this is the difference between stopping and losing track.
    let after = TcpStream::connect(&bound.address);
    assert!(
        after.is_err(),
        "the address still accepts connections after stop"
    );

    // And it can be started again afterwards, with a different token.
    let second = websurface::start(0).await.expect("it starts again");
    assert_ne!(second.token, bound.token, "the old token came back");
    assert!(websurface::stop());
}
