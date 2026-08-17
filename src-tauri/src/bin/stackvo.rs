//! `stackvo` — the helper CLI (A-1).
//!
//!   stackvo status
//!   stackvo logs shop --follow
//!   stackvo up --mode services
//!   stackvo doctor --json | jq '.ports[] | select(.state != "ok")'
//!
//! Everything is in [`stackvo_desktop_lib::cli`], including the entry point:
//! this file exists to give the crate a binary target and to turn the code that
//! function returns into a process exit status. Keeping `main` in the library
//! is what lets `cargo test` drive the whole thing — a `main` in a `bin` target
//! is reachable from nothing.
//!
//! A separate binary rather than a subcommand of the app, for the reason
//! `stackvo-mcp` is one: the interesting questions are answerable from a
//! workspace and a Docker socket, and requiring a window to be open to answer
//! them would be a requirement invented here.

#[tokio::main]
async fn main() {
    // The crash handler the app and the MCP server both install. A panic in a
    // terminal at least prints something; the report is what makes it useful
    // afterwards, and it lands in the same directory as the others.
    stackvo_desktop_lib::crash::install();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(stackvo_desktop_lib::cli::main(argv).await);
}
