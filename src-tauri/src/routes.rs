//! Pointing a development name at something this app did not start.
//!
//! E-4. Traefik already routes every project and every service in the
//! catalogue, and it routes nothing else — so a Vite server somebody started by
//! hand, an API running in another tool, or a staging host they want to reach
//! under a local name has no way in. That is the whole gap: the proxy is here,
//! the certificate is here, and the only addresses either of them will serve
//! are the ones this app generated.
//!
//! ## `localhost` is the mistake, and it is the entire feature
//!
//! Somebody typing `http://localhost:3000` means "the thing on my machine".
//! Traefik reads that string **inside its own container**, where `localhost` is
//! Traefik. The route loads without complaint, the browser gets a 502, and
//! nothing anywhere says why — the config is valid, the name resolves, the
//! certificate is fine.
//!
//! So [`Route::normalise`] rewrites `localhost` and `127.0.0.1` to
//! `host.docker.internal`, and the screen says it did. Refusing them instead
//! would be technically defensible and practically useless: it is the address
//! people have, and correcting it is a thing this app knows how to do.
//!
//! ## What is deliberately not offered
//!
//! * **Path rewriting, headers, middleware.** Traefik has all of it and a form
//!   for it here would be a worse Traefik. A route is a name and a target.
//! * **A target that is not http or https.** The proxy speaks those; a `tcp://`
//!   target would need an entry point that does not exist, and accepting the
//!   scheme while ignoring it is how a feature gets a reputation.
//! * **Routes outside the workspace suffix.** Accepted, but reported: the
//!   wildcard certificate covers `*.<suffix>` and nothing else, so a name
//!   outside it is served over a certificate the browser will refuse. Warned
//!   rather than blocked, because somebody who has their own certificate in the
//!   store is not wrong.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Bumped when the shape changes. The reason `preferences.json` grew one: an
/// absent version leaves no way to tell "old file" from "never written".
const SCHEMA_VERSION: u32 = 1;

/// The host a container uses to reach the machine it runs on.
///
/// Docker Desktop provides it on macOS and Windows. On Linux it resolves only
/// when the container asks for it — the generated compose adds
/// `host.docker.internal:host-gateway` for exactly this, and a route that could
/// not be reached would otherwise be a 502 with no explanation.
pub const HOST_GATEWAY: &str = "host.docker.internal";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// The name the browser asks for.
    pub domain: String,
    /// Where the proxy sends it, as a URL.
    pub target: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// A route as it will actually behave, with everything that was changed or is
/// worth saying.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub domain: String,
    /// The target after normalisation — what Traefik is given.
    pub target: String,
    /// What the user typed, when it differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rewritten_from: Option<String>,
    pub enabled: bool,
    /// Things that are true and not errors: a name outside the suffix, a target
    /// that was corrected.
    pub notes: Vec<String>,
}

impl Route {
    /// Check the pair, and correct the one thing that is always wrong.
    pub fn normalise(&self, suffix: &str) -> Result<Checked> {
        let domain = self.domain.trim().to_ascii_lowercase();
        if !crate::hosts::is_valid_domain(&domain) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("{:?} is not a hostname", self.domain),
            ));
        }

        let target = self.target.trim();
        let (scheme, rest) = target.split_once("://").ok_or_else(|| {
            Error::new(
                Code::InvalidInput,
                format!("{target:?} has no scheme; write http://host:port"),
            )
        })?;
        if scheme != "http" && scheme != "https" {
            return Err(Error::new(
                Code::InvalidInput,
                format!(
                    "{scheme:?} is not a scheme the proxy speaks — http or https. \
                     Accepting it and routing something else is worse than refusing it"
                ),
            ));
        }

        // Everything after the authority is dropped rather than carried.
        // Traefik's `loadBalancer.servers.url` is an origin; a path there is
        // silently ignored, and a field that accepts something it discards is a
        // field somebody will spend an afternoon on.
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
        if authority.is_empty() {
            return Err(Error::new(
                Code::InvalidInput,
                format!("{target:?} names no host"),
            ));
        }

        let mut notes = Vec::new();
        if rest.len() > authority.len() {
            notes.push(format!(
                "the path was dropped: a proxy target is an origin, and Traefik ignores \
                 anything after {authority}"
            ));
        }

        let (host, port) = split_authority(authority)?;
        let rewritten = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]");
        let final_host = if rewritten {
            notes.push(format!(
                "{host} means the proxy's own container, not this machine — sending it to \
                 {HOST_GATEWAY} instead"
            ));
            HOST_GATEWAY.to_string()
        } else {
            host
        };

        let suffix = suffix.trim_matches('.').to_ascii_lowercase();
        let tld = suffix.rsplit('.').next().unwrap_or(&suffix);
        if !(domain == tld || domain.ends_with(&format!(".{tld}"))) {
            notes.push(format!(
                "{domain} is outside .{tld}, so the wildcard certificate does not cover it — \
                 the browser will refuse the connection unless you have your own"
            ));
        }

        let normalised = match port {
            Some(port) => format!("{scheme}://{final_host}:{port}"),
            None => format!("{scheme}://{final_host}"),
        };

        Ok(Checked {
            domain,
            rewritten_from: (normalised != target).then(|| target.to_string()),
            target: normalised,
            enabled: self.enabled,
            notes,
        })
    }
}

/// Split `host:port`, keeping a bracketed IPv6 literal in one piece.
fn split_authority(authority: &str) -> Result<(String, Option<u16>)> {
    let bad = |what: &str| Error::new(Code::InvalidInput, format!("{authority:?}: {what}"));

    // `[::1]:8080` — the colons inside the brackets are the address.
    if let Some(rest) = authority.strip_prefix('[') {
        let (address, tail) = rest.split_once(']').ok_or_else(|| bad("unclosed ["))?;
        let port = match tail.strip_prefix(':') {
            Some(text) => Some(
                text.parse::<u16>()
                    .map_err(|_| bad("port is not a number"))?,
            ),
            None if tail.is_empty() => None,
            None => return Err(bad("unexpected text after ]")),
        };
        return Ok((format!("[{address}]"), port));
    }

    match authority.rsplit_once(':') {
        Some((host, text)) => {
            let port = text
                .parse::<u16>()
                .map_err(|_| bad("port is not a number"))?;
            if port == 0 {
                return Err(bad("port 0"));
            }
            Ok((host.to_ascii_lowercase(), Some(port)))
        }
        None => Ok((authority.to_ascii_lowercase(), None)),
    }
}

// ------------------------------------------------------------------ storage

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Document {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    routes: Vec<Route>,
}

pub fn path(root: &Path) -> PathBuf {
    root.join("routes.json")
}

/// Read the routes, treating an unreadable file as none.
///
/// Not as an error: this is read on the generation path, and a workspace that
/// could not be regenerated because of a stray byte in an optional file would
/// be a stack nobody can start. A malformed file is reported by `list`, which
/// is where somebody is looking at it.
pub fn read(root: &Path) -> Vec<Route> {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Document>(&text).ok())
        .map(|doc| doc.routes)
        .unwrap_or_default()
}

pub fn write(root: &Path, routes: &[Route]) -> Result<()> {
    let doc = Document {
        schema_version: SCHEMA_VERSION,
        routes: routes.to_vec(),
    };
    let text = serde_json::to_string_pretty(&doc)
        .map_err(|e| Error::new(Code::IoError, format!("serialising the routes: {e}")))?;
    crate::atomic::write(&path(root), &format!("{text}\n"))
}

// ---------------------------------------------------------------- rendering

/// The Traefik router and service for one route.
///
/// A name derived from the domain rather than an index: a router keyed by
/// position changes identity when a route above it is deleted, and Traefik
/// treats that as one router removed and another added — which drops a live
/// connection for a route nobody touched.
pub fn router_name(domain: &str) -> String {
    let mut out = String::from("route-");
    for ch in domain.chars() {
        out.push(if ch.is_ascii_alphanumeric() { ch } else { '-' });
    }
    out
}

/// The `http.routers` and `http.services` entries, as two blocks.
///
/// Returned as a pair rather than one string because the caller interleaves
/// them with its own: `routes.yml` has one `routers:` map and one `services:`
/// map, and a function that emitted both headers would produce a document with
/// two of each — valid YAML, and the second silently wins.
pub fn render(routes: &[Checked]) -> (String, String) {
    let mut routers = String::new();
    let mut services = String::new();

    for route in routes.iter().filter(|route| route.enabled) {
        let name = router_name(&route.domain);
        routers.push_str(&format!(
            "    {name}:\n      rule: \"Host(`{}`)\"\n      entryPoints:\n        - websecure\n      service: {name}\n      tls: {{}}\n",
            route.domain
        ));
        services.push_str(&format!(
            "    {name}:\n      loadBalancer:\n        servers:\n          - url: \"{}\"\n",
            route.target
        ));
    }

    (routers, services)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(domain: &str, target: &str) -> Route {
        Route {
            domain: domain.into(),
            target: target.into(),
            enabled: true,
        }
    }

    /// The whole feature, in one assertion: the address people have is the one
    /// that cannot work, and correcting it is a thing this app knows how to do.
    #[test]
    fn localhost_is_rewritten_to_the_host_gateway_and_said_so() {
        let checked = route("api.loc", "http://localhost:3000")
            .normalise("stackvo.loc")
            .unwrap();
        assert_eq!(checked.target, format!("http://{HOST_GATEWAY}:3000"));
        assert_eq!(
            checked.rewritten_from.as_deref(),
            Some("http://localhost:3000")
        );
        assert!(
            checked.notes.iter().any(|n| n.contains("own container")),
            "{:?}",
            checked.notes
        );
    }

    #[test]
    fn the_v4_and_v6_loopbacks_are_rewritten_too() {
        for target in ["http://127.0.0.1:3000", "http://[::1]:3000"] {
            let checked = route("api.loc", target).normalise("loc").unwrap();
            assert_eq!(
                checked.target,
                format!("http://{HOST_GATEWAY}:3000"),
                "{target}"
            );
        }
    }

    /// A real host is left exactly as it was.
    #[test]
    fn a_target_that_is_not_loopback_is_untouched() {
        let checked = route("api.loc", "https://staging.example.com")
            .normalise("loc")
            .unwrap();
        assert_eq!(checked.target, "https://staging.example.com");
        assert_eq!(checked.rewritten_from, None);
    }

    /// Accepting a path and discarding it is a field somebody loses an
    /// afternoon to, so it is dropped *and reported*.
    #[test]
    fn a_path_is_dropped_and_the_note_says_so() {
        let checked = route("api.loc", "http://example.com:8080/v2/api")
            .normalise("loc")
            .unwrap();
        assert_eq!(checked.target, "http://example.com:8080");
        assert!(checked.notes.iter().any(|n| n.contains("path was dropped")));
    }

    #[test]
    fn a_scheme_the_proxy_does_not_speak_is_refused() {
        for target in ["tcp://x:1", "ftp://x", "x:1", ""] {
            assert!(
                route("api.loc", target).normalise("loc").is_err(),
                "{target}"
            );
        }
    }

    #[test]
    fn a_domain_that_is_not_a_hostname_is_refused() {
        for domain in ["not a host", "", "-bad.loc"] {
            assert!(
                route(domain, "http://x:1").normalise("loc").is_err(),
                "{domain:?}"
            );
        }
    }

    /// Warned, not blocked: somebody with their own certificate is not wrong.
    #[test]
    fn a_domain_outside_the_suffix_is_a_note_rather_than_a_refusal() {
        let checked = route("api.example.com", "http://x:1")
            .normalise("stackvo.loc")
            .unwrap();
        assert!(
            checked.notes.iter().any(|n| n.contains("certificate")),
            "{:?}",
            checked.notes
        );
    }

    #[test]
    fn a_port_that_is_not_a_number_is_refused() {
        assert!(route("api.loc", "http://x:abc").normalise("loc").is_err());
        assert!(route("api.loc", "http://x:0").normalise("loc").is_err());
        assert!(route("api.loc", "http://x:70000").normalise("loc").is_err());
    }

    /// An IPv6 literal's colons belong to the address, not to a port.
    #[test]
    fn a_bracketed_v6_address_keeps_its_colons() {
        let checked = route("api.loc", "http://[2001:db8::1]:8080")
            .normalise("loc")
            .unwrap();
        assert_eq!(checked.target, "http://[2001:db8::1]:8080");
    }

    // ---- rendering -------------------------------------------------------

    /// A router keyed by position changes identity when one above it is
    /// deleted, which Traefik reads as a different router and drops a live
    /// connection for a route nobody touched.
    #[test]
    fn the_router_name_comes_from_the_domain_and_not_a_position() {
        assert_eq!(router_name("api.shop.loc"), "route-api-shop-loc");
        assert_ne!(router_name("a.loc"), router_name("b.loc"));
    }

    #[test]
    fn a_disabled_route_renders_nothing() {
        let checked = Route {
            enabled: false,
            ..route("api.loc", "http://x:1")
        }
        .normalise("loc")
        .unwrap();
        let (routers, services) = render(&[checked]);
        assert!(routers.is_empty());
        assert!(services.is_empty());
    }

    #[test]
    fn a_route_renders_a_router_and_a_service_that_name_each_other() {
        let checked = route("api.loc", "http://localhost:3000")
            .normalise("loc")
            .unwrap();
        let (routers, services) = render(&[checked]);
        assert!(routers.contains("Host(`api.loc`)"));
        assert!(routers.contains("service: route-api-loc"));
        assert!(services.contains("route-api-loc:"));
        assert!(services.contains(&format!("http://{HOST_GATEWAY}:3000")));
    }

    // ---- storage ---------------------------------------------------------

    /// A stray byte in an optional file must not be why a stack cannot start.
    #[test]
    fn an_unreadable_file_reads_as_no_routes() {
        let dir = std::env::temp_dir().join(format!("stackvo-routes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        assert!(read(&dir).is_empty(), "an absent file is no routes");
        std::fs::write(path(&dir), "{ not json").unwrap();
        assert!(read(&dir).is_empty(), "a broken file is no routes");

        write(&dir, &[route("api.loc", "http://x:1")]).unwrap();
        assert_eq!(read(&dir).len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
