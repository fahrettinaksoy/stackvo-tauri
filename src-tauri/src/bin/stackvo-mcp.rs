//! The MCP server, as a process an assistant can launch.
//!
//!   stackvo-mcp                 # read-only, the default
//!   stackvo-mcp --allow-writes  # plus the few tools that change things
//!   STACKVO_ROOT=/path stackvo-mcp
//!
//! Registered with Claude Code as:
//!
//! ```json
//! { "mcpServers": { "stackvo": { "command": "/path/to/stackvo-mcp" } } }
//! ```
//!
//! A separate binary rather than something the running app hosts, for the
//! reason `examples/diagnose.rs` is also a separate binary: the interesting
//! questions are answerable from a checkout and a Docker socket, and requiring
//! a window to be open to answer them would be a requirement invented here.
//!
//! **stdout carries the protocol and nothing else.** A stray `println!` in this
//! process is not a cosmetic bug — it is a parse error in the client, which
//! usually surfaces as the server "not working" with nothing to go on. Anything
//! this binary has to say goes to stderr.

use stackvo_desktop_lib::mcp;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() {
    // This process is launched by a client that usually hides its stderr, so a
    // panic here reads as "the server stopped responding" with nothing to go
    // on. The report lands in the same directory the app writes its own to.
    stackvo_desktop_lib::crash::install();

    let allow_writes = std::env::args().any(|arg| arg == "--allow-writes");

    eprintln!(
        "stackvo-mcp {} — {} tools ({})",
        env!("CARGO_PKG_VERSION"),
        mcp::visible(allow_writes).count(),
        if allow_writes {
            "reads and writes"
        } else {
            "read-only; pass --allow-writes to enable the rest"
        }
    );

    // Said once, up front: every tool resolves the workspace, and "no StackVo
    // directory selected" repeated per call reads like a broken server rather
    // than an unconfigured one.
    let ws = stackvo_desktop_lib::workspace::resolve();
    match (&ws.root, ws.valid) {
        (Some(root), true) => eprintln!("workspace: {root}"),
        _ => eprintln!(
            "workspace: none found — set STACKVO_ROOT, or open the app once to choose one"
        ),
    }

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    // Newline-delimited JSON, one message per line.
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(e) => {
                // Unparseable input has no id to answer against, so there is
                // nobody to reply to; the client is the one that is confused.
                eprintln!("ignoring unparseable message: {e}");
                continue;
            }
        };

        let Some(response) = mcp::handle(&request, allow_writes).await else {
            continue; // a notification
        };

        let mut payload = response.to_string();
        payload.push('\n');
        if stdout.write_all(payload.as_bytes()).await.is_err() {
            break; // the client went away
        }
        let _ = stdout.flush().await;
    }
}
