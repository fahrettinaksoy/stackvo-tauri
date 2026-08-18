//! Print the compose block a project with extra hostnames produces.
//!
//! Here for the reason `connection_probe.rs` is here: the interesting question
//! is not whether the renderer returns the string somebody expected, it is
//! whether Docker and Traefik accept what it returns. A unit test can assert
//! the bytes; only `docker compose config` can say the file parses, and only
//! Traefik can say the rule compiles.
//!
//! ```sh
//! cargo run --example router_rule -- shop.loc '*.shop.loc' api.shop.loc \
//!   > /tmp/probe.yml && docker compose -f /tmp/probe.yml config
//! ```
//!
//! The first argument is the domain, the rest are aliases.

use stackvo_desktop_lib::generator::{self, ComposeProject, Server};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (domain, aliases) = args
        .split_first()
        .map(|(d, rest)| (d.clone(), rest.to_vec()))
        .unwrap_or_else(|| ("shop.loc".to_string(), Vec::new()));

    // Nothing declared: this example is about the router rule, and a sidecar
    // would put a container in the output that has nothing to do with it.
    let sidecars = stackvo_desktop_lib::sidecar::Declared::default();
    let project = ComposeProject {
        name: "shop",
        domain: &domain,
        aliases: &aliases,
        runtime_server: Server::parse("nginx"),
        node_port: None,
        php_version: Some("8.4"),
        sidecars: &sidecars,
    };

    // A compose document rather than a fragment, so the output can be handed
    // straight to `docker compose config`.
    println!("services:");
    print!(
        "{}",
        generator::render_compose_service(&project, "/stackvo", "/stackvo/projects")
    );
    print!("\nnetworks:\n  stackvo-net:\n    external: true\n\n");
}
