//! What `service_connection` returns for the checkout on this machine.
//!
//! The unit tests in `connect.rs` are pure — they prove the shapes without a
//! `.env` or an engine anywhere near them. This is the other half: it reads the
//! real workspace and the running containers, which is the only way to see that
//! the published port came from Docker rather than from a guess.
//!
//!   cargo run --example connection_probe
fn main() {
    let workspace = stackvo_desktop_lib::workspace::resolve();
    let root = workspace.require_root().expect("a StackVo workspace");
    println!("root {}\n", root.display());

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    for service in ["mongo", "mysql", "redis", "mailpit", "mongo-express"] {
        match runtime.block_on(stackvo_desktop_lib::connect::of(&root, service, false)) {
            Ok(Some(connection)) => {
                println!("{service}");
                match &connection.from_host {
                    Some(endpoint) => println!("  host      {}", endpoint.uri),
                    None => println!("  host      (nothing published)"),
                }
                println!("  container {}", connection.from_container.uri);
            }
            Ok(None) => println!("{service}\n  (no connection string)"),
            Err(e) => println!("{service}\n  error: {e:?}"),
        }
        println!();
    }
}
