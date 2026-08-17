//! The address a phone on the same Wi-Fi can reach the project at.
//!
//! E-3. `shop.loc` exists in exactly one place — this machine's `/etc/hosts` —
//! so testing a layout on a real phone, or showing a colleague across the desk,
//! means either editing a file on their device or not doing it. Neither is what
//! anybody does; what they do is give up and use the desktop browser's device
//! emulation, which is not the same thing and is exactly the class of bug it
//! fails to show.
//!
//! ## Why a wildcard DNS service and not our own resolver
//!
//! `sslip.io` answers `anything.192-168-1-5.sslip.io` with `192.168.1.5`. It is
//! a public resolver doing arithmetic on the name, so **nothing is registered,
//! nothing is published, and no packet from the visiting device leaves the LAN
//! except the DNS lookup itself**. The alternative is E-1 — running a real
//! resolver here and pointing other devices at it — which is a bigger item that
//! this one does not block and is not blocked by.
//!
//! `nip.io` does the same and is a one-word change; `sslip.io` is chosen for
//! having a published source and a documented fallback resolver, not because
//! the arithmetic differs.
//!
//! ## The dashed form, not the dotted one
//!
//! Both `192.168.1.5.sslip.io` and `192-168-1-5.sslip.io` resolve. The dashed
//! one is used because the address is then a **single label**, so the project's
//! own name sits one level up — `shop.192-168-1-5.sslip.io` — and the whole
//! host has a fixed shape whatever the address is. With the dotted form the
//! label count changes with the address, and every rule that counts labels (a
//! wildcard certificate, a `HostRegexp`) would have to change with it.
//!
//! ## What this cannot do, said here rather than found later
//!
//! The certificate is issued by the local CA, and the visiting device has never
//! heard of that CA. **The browser on the phone will warn.** The connection is
//! real and the name is right; what is missing is a trust anchor on a device
//! this app cannot reach. Installing the CA there is a manual step and the
//! status below names it rather than the app pretending the warning is a bug.
//!
//! Serving the LAN name over plain HTTP instead would remove the warning and is
//! deliberately not done: every project router in this app targets `websecure`,
//! and a second entry point that answered the same site without TLS would be a
//! way to reach it that no other name has.
//!
//! ## The address is derived, never stored
//!
//! For the reason `tunnel.rs` gives about its public URL: a DHCP lease expires,
//! a laptop moves between networks, and a stored address becomes a name that
//! points somewhere else — possibly at somebody else's machine on the new
//! network. What is stored is the **intent** (`lan_share` in the manifest); the
//! name is computed from whatever the address is at the moment it is asked for.
//!
//! The compose file and the certificate are the exception, because both are
//! written to disk with the name inside them. That is why [`stale`] exists: it
//! compares what was rendered with what would be rendered now, so a moved
//! laptop is reported rather than silently serving a name that no longer
//! resolves here.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// The wildcard DNS suffix. One constant because it appears in the rendered
/// host, in the certificate SAN and in the explanation on screen, and those
/// three disagreeing is a name that resolves and a certificate that does not
/// cover it.
pub const SUFFIX: &str = "sslip.io";

/// This machine's address on the network it would reach a phone over.
///
/// Found by asking the kernel which local address it would use to reach an
/// off-link destination, which is what the routing table is for. **No packet is
/// sent** — a connected UDP socket only fixes the peer, and `192.0.2.1` is
/// TEST-NET-1, reserved by RFC 5737 precisely so that it never routes anywhere.
///
/// The alternative is enumerating interfaces, which needs `getifaddrs` and so a
/// dependency, and then needs a rule for choosing between the six addresses a
/// laptop with Docker and a VPN has. Asking the routing table returns the one
/// the machine would actually use, which is the same one a phone's packets will
/// arrive on.
///
/// `None` when the answer is not a private address. That covers the machine
/// being offline, and it covers the case worth being careful about: a public
/// address here means this is not a laptop behind a router, and handing out
/// `shop.<public-ip>.sslip.io` would be publishing a development site to the
/// internet under a name anybody can resolve.
pub fn address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;

    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if is_lan(v4) => Some(v4),
        _ => None,
    }
}

/// RFC 1918, and nothing else.
///
/// Link-local (`169.254/16`) is excluded on purpose: it means DHCP did not
/// answer, so the machine has no working network and the name would resolve to
/// an address nothing can reach. Loopback likewise — `127.0.0.1.sslip.io`
/// resolves, and on the visiting phone it resolves to *that phone*.
fn is_lan(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

/// `192.168.1.5` → `192-168-1-5`, the single label sslip.io reads as an address.
pub fn label(ip: Ipv4Addr) -> String {
    ip.octets().map(|o| o.to_string()).join("-")
}

/// The hostname this project answers on from another device.
///
/// Built from the project's **name** rather than its domain, because the domain
/// carries the workspace suffix — `shop.loc` would produce
/// `shop.loc.192-168-1-5.sslip.io`, which resolves and works and reads like a
/// mistake. The name is already the thing the project is called everywhere else
/// in the app.
pub fn domain_for(project: &str, ip: Ipv4Addr) -> String {
    format!("{}.{}.{SUFFIX}", sanitise(project), label(ip))
}

/// A project name as a DNS label.
///
/// Project names allow dots (`parser.ajans`, which `traefik_name` already has
/// to flatten) and those would add labels to the host — harmless to resolve,
/// but it stops being one predictable shape. Anything not a letter, digit or
/// hyphen becomes a hyphen, and the result is trimmed of leading and trailing
/// hyphens because a label may not start or end with one.
fn sanitise(project: &str) -> String {
    let mapped: String = project
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    mapped.trim_matches('-').to_string()
}

/// Is a name that was rendered into compose or a certificate still the name
/// this machine would produce?
///
/// The one question a derived address cannot answer by being derived: the
/// compose file and the certificate hold a copy, and a laptop that changed
/// networks now serves a host nothing on this LAN resolves to. Returns the
/// address that is baked in, when it differs from the current one.
pub fn stale(rendered: &[String], now: Option<Ipv4Addr>) -> Option<String> {
    let current = now.map(label);
    rendered
        .iter()
        .filter(|name| name.ends_with(SUFFIX))
        .find(|name| {
            let Some(rest) = name.strip_suffix(&format!(".{SUFFIX}")) else {
                return false;
            };
            match (rest.rsplit('.').next(), current.as_deref()) {
                (Some(baked), Some(live)) => baked != live,
                // No address now: every baked name is stale, because none of
                // them resolves to a machine that can be reached.
                (Some(_), None) => true,
                _ => false,
            }
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_becomes_a_single_label() {
        assert_eq!(label(Ipv4Addr::new(192, 168, 1, 5)), "192-168-1-5");
        assert_eq!(label(Ipv4Addr::new(10, 0, 0, 42)), "10-0-0-42");
    }

    /// The domain is built from the name, not the domain: `shop.loc` would give
    /// `shop.loc.192-168-1-5.sslip.io`, which works and reads as a bug.
    #[test]
    fn the_host_is_the_project_name_over_the_address() {
        assert_eq!(
            domain_for("shop", Ipv4Addr::new(192, 168, 1, 5)),
            "shop.192-168-1-5.sslip.io"
        );
    }

    /// `parser.ajans` is a real project name in this repository's own fixtures,
    /// and left alone it would add a label to every LAN host.
    #[test]
    fn a_dotted_project_name_stays_one_label() {
        let host = domain_for("parser.ajans", Ipv4Addr::new(10, 1, 2, 3));
        assert_eq!(host, "parser-ajans.10-1-2-3.sslip.io");
        assert_eq!(host.split('.').count(), 4, "name, address, sslip, io");
    }

    #[test]
    fn a_name_that_would_start_or_end_with_a_hyphen_does_not() {
        assert_eq!(sanitise("_shop_"), "shop");
        assert_eq!(sanitise("--a--b--"), "a--b");
    }

    /// Every host this produces has to survive the same validation an alias
    /// typed by hand does, or it is rejected on the way into the router rule.
    #[test]
    fn the_generated_host_is_one_the_rest_of_the_app_accepts() {
        for name in ["shop", "parser.ajans", "My_Project"] {
            let host = domain_for(name, Ipv4Addr::new(192, 168, 1, 5));
            assert!(
                crate::hosts::is_valid_domain(&host),
                "{host} is not a hostname the app would accept"
            );
        }
    }

    /// A public address is not offered. Handing out `shop.<public>.sslip.io`
    /// would publish a development site under a name anybody can resolve.
    #[test]
    fn only_private_addresses_count_as_a_lan() {
        assert!(is_lan(Ipv4Addr::new(192, 168, 1, 5)));
        assert!(is_lan(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_lan(Ipv4Addr::new(172, 16, 0, 1)));

        assert!(!is_lan(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_lan(Ipv4Addr::new(203, 0, 113, 7)));
        // DHCP did not answer: the machine has no working network.
        assert!(!is_lan(Ipv4Addr::new(169, 254, 3, 4)));
        assert!(!is_lan(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn a_rendered_name_from_another_network_is_reported() {
        let rendered = vec![
            "shop.loc".to_string(),
            "shop.192-168-1-5.sslip.io".to_string(),
        ];

        assert_eq!(
            stale(&rendered, Some(Ipv4Addr::new(192, 168, 1, 5))),
            None,
            "the same address is not stale"
        );
        assert_eq!(
            stale(&rendered, Some(Ipv4Addr::new(10, 0, 0, 9))).as_deref(),
            Some("shop.192-168-1-5.sslip.io"),
            "a moved laptop is reported"
        );
        assert_eq!(
            stale(&rendered, None).as_deref(),
            Some("shop.192-168-1-5.sslip.io"),
            "no address now means nothing baked in resolves here"
        );
    }

    /// A workspace that never asked for LAN sharing must never be told it is
    /// stale — the check keys on the suffix, not on the presence of a dot.
    #[test]
    fn a_workspace_without_lan_names_is_never_stale() {
        let rendered = vec!["shop.loc".to_string(), "api.shop.loc".to_string()];
        assert_eq!(stale(&rendered, None), None);
        assert_eq!(stale(&rendered, Some(Ipv4Addr::new(10, 0, 0, 9))), None);
    }

    /// Whatever this machine is on, the answer has to be usable or absent —
    /// never a public address and never a name the app would reject.
    #[test]
    fn this_machine_answers_with_something_usable_or_nothing() {
        // `if let` rather than a `match` with an empty `None` arm: no answer is
        // the legitimate case here — offline, or behind no router — and an arm
        // that does nothing reads as one somebody forgot to fill in.
        if let Some(ip) = address() {
            assert!(is_lan(ip), "{ip} was offered and is not a private address");
            assert!(crate::hosts::is_valid_domain(&domain_for("shop", ip)));
        }
    }
}
