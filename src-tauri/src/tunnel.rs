//! A shareable public URL per project — webhook testing's missing answer.
//!
//! Stripe, GitHub and every other webhook sender needs to reach the site from
//! the internet, and `myapp.loc` does not exist there. The fix is a
//! `cloudflared` *quick tunnel* run as a sidecar container on the stack's own
//! network: no account, no token, no config — Cloudflare hands back a random
//! `https://….trycloudflare.com` URL that forwards to the project container
//! for as long as the sidecar runs.
//!
//! The sidecar targets the project container directly rather than going
//! through Traefik: with SSL on, every project router listens on `websecure`
//! only, and a public visitor cannot complete a TLS handshake against a
//! hostname that exists in no DNS. The container's internal port is plain
//! HTTP and derived the same way the generator derives the Traefik
//! `loadbalancer.server.port` label — node projects on their manifest port,
//! Swoole on its own 8000, every other PHP server on 80.
//!
//! The URL is deliberately not stored anywhere: it appears in the sidecar's
//! log when Cloudflare assigns it, and reading it from the log on every
//! status call means the answer is always what is actually live — an app
//! restart, a container restart, a crashed tunnel all stay truthful for free.

use crate::error::{Code, Error, Result};
use serde::Serialize;

/// Sidecar containers are `stackvo-tunnel-<project>`; the id handed to
/// `engine::*` (which prefixes `stackvo-` itself) is `tunnel-<project>`.
pub const ID_PREFIX: &str = "tunnel-";

pub const IMAGE: &str = "cloudflare/cloudflared:latest";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub project: String,
    pub running: bool,
    /// The assigned public URL, once Cloudflare has printed it. `None` while
    /// the sidecar is still connecting — the UI polls until it appears.
    pub url: Option<String>,
    pub container: String,
}

/// The engine-facing id of a project's sidecar.
pub fn container_id(project: &str) -> String {
    format!("{ID_PREFIX}{project}")
}

/// The port the project's own container serves HTTP on.
///
/// Mirrors `generator::render_compose_service`, which writes the same number
/// into the Traefik `loadbalancer.server.port` label — the generator is the
/// authority, this is its arithmetic repeated on the same manifest.
pub fn internal_port(manifest: &crate::manifest::Manifest) -> u16 {
    if manifest.runtime == "node" {
        return manifest.node.as_ref().map(|n| n.port).unwrap_or(3000);
    }
    match manifest.server.as_deref() {
        Some("swoole") => 8000,
        _ => 80,
    }
}

/// The first quick-tunnel URL in a log. Cloudflare prints it boxed in a
/// banner; the shape of the hostname is the stable part.
pub fn find_url(log: &str) -> Option<String> {
    for line in log.lines() {
        let Some(start) = line.find("https://") else {
            continue;
        };
        let candidate: String = line[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-'))
            .collect();
        if candidate.ends_with(".trycloudflare.com") {
            return Some(candidate);
        }
    }
    None
}

/// Every tunnel sidecar the engine knows about, with its URL where one has
/// been assigned.
pub async fn status_all() -> Result<Vec<TunnelStatus>> {
    use futures_util::StreamExt;

    let containers = crate::engine::stackvo_containers().await?;
    let mut out = Vec::new();

    for (id, info) in containers {
        let Some(project) = id.strip_prefix(ID_PREFIX) else {
            continue;
        };

        // The URL lives in the first lines the sidecar ever printed, so a
        // bounded tail-from-start read is enough; follow=false ends on its own.
        let url = if info.running {
            match crate::engine::logs_stream(&id, 200, false) {
                Ok(stream) => {
                    let lines: Vec<String> = stream.map(|l| l.text).collect().await;
                    find_url(&lines.join("\n"))
                }
                Err(_) => None,
            }
        } else {
            None
        };

        out.push(TunnelStatus {
            project: project.to_string(),
            running: info.running,
            url,
            container: info.name,
        });
    }

    out.sort_by(|a, b| a.project.cmp(&b.project));
    Ok(out)
}

/// The `docker run` invocation for one project's sidecar.
///
/// Returned as arguments rather than executed here so the caller can drive it
/// through `runner::run_operation` — the first start pulls the image, which
/// can take minutes and belongs in the operation console, not behind a frozen
/// button.
pub fn run_args(project: &str, domain: Option<&str>, port: u16, network: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "run",
        "-d",
        "--rm",
        "--name",
        &format!("stackvo-{}", container_id(project)),
        "--network",
        network,
        IMAGE,
        "tunnel",
        "--no-autoupdate",
        "--url",
        &format!("http://{}:{port}", crate::engine::container_name(project)),
    ]
    .into_iter()
    .map(String::from)
    .collect();

    // Present the local domain as the Host header so name-based vhosts and
    // framework URL checks behave exactly as they do locally.
    if let Some(domain) = domain {
        args.push("--http-host-header".into());
        args.push(domain.into());
    }
    args
}

/// Refuse to start a tunnel to a container that is not running: cloudflared
/// would happily serve 502s from a URL that looks like it worked.
pub async fn ensure_project_running(project: &str) -> Result<()> {
    let containers = crate::engine::stackvo_containers().await?;
    match containers.get(project) {
        Some(info) if info.running => Ok(()),
        Some(_) => Err(
            Error::new(Code::Conflict, format!("{project} is not running"))
                .with_hint("Start the project first — the tunnel forwards to its container."),
        ),
        None => Err(Error::not_found(format!("container for {project}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assigned_url_is_read_out_of_the_banner() {
        let log = "\
2026-07-30T09:00:00Z INF Thank you for trying Cloudflare Tunnel.\n\
2026-07-30T09:00:01Z INF +--------------------------------------------------------------------------------------------+\n\
2026-07-30T09:00:01Z INF |  Your quick Tunnel has been created! Visit it at (it may take some time to be reachable):  |\n\
2026-07-30T09:00:01Z INF |  https://random-words-here.trycloudflare.com                                               |\n\
2026-07-30T09:00:01Z INF +--------------------------------------------------------------------------------------------+\n";
        assert_eq!(
            find_url(log).as_deref(),
            Some("https://random-words-here.trycloudflare.com")
        );
    }

    #[test]
    fn a_log_without_a_url_yields_none_not_a_guess() {
        assert_eq!(
            find_url(
                "INF Requesting new quick Tunnel on trycloudflare.com...\nERR failed to connect"
            ),
            None
        );
        // An https URL that is not a quick-tunnel URL is not the answer.
        assert_eq!(
            find_url("INF see https://developers.cloudflare.com/tunnel for docs"),
            None
        );
    }

    #[test]
    fn internal_port_mirrors_the_generators_arithmetic() {
        let mut m = crate::manifest::Manifest {
            name: "x".into(),
            domain: None,
            runtime: "php".into(),
            server: Some("nginx".into()),
            document_root: None,
            php: None,
            node: None,
            valid: true,
            errors: vec![],
            warnings: vec![],
        };
        assert_eq!(internal_port(&m), 80);

        m.server = Some("swoole".into());
        assert_eq!(internal_port(&m), 8000);

        m.runtime = "node".into();
        m.node = Some(crate::manifest::NodeConfig {
            version: "20".into(),
            install: "npm ci".into(),
            build: None,
            start: "npm start".into(),
            port: 4321,
        });
        assert_eq!(internal_port(&m), 4321);

        m.node = None;
        assert_eq!(internal_port(&m), 3000);
    }

    #[test]
    fn the_sidecar_forwards_to_the_container_with_the_local_host_header() {
        let args = run_args("myapp", Some("myapp.loc"), 80, "stackvo-net");
        let line = args.join(" ");
        assert!(line.contains("--name stackvo-tunnel-myapp"));
        assert!(line.contains("--network stackvo-net"));
        assert!(line.contains("--url http://stackvo-myapp:80"));
        assert!(line.contains("--http-host-header myapp.loc"));

        // No domain, no header — never an empty flag value.
        let bare = run_args("myapp", None, 3000, "stackvo-net");
        assert!(!bare.join(" ").contains("--http-host-header"));
    }
}
