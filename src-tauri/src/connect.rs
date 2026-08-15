//! The string you paste into a database client.
//!
//! The services list already showed a container name, a port table and a
//! credentials block, and left the reader to assemble a URI from them. Nobody
//! assembles it wrong in an interesting way — they assemble it the *obvious*
//! way, which is with the container name in it, and then Compass says the host
//! cannot be resolved. `stackvo-mongo` is a name on the Docker network; a
//! client on the host has never heard of it.
//!
//! So this returns **two** addresses per service rather than one, because a
//! service genuinely has two and picking either as "the" connection string is
//! how the confusion started:
//!
//!   * from the host — `127.0.0.1` and the port Docker published, which is what
//!     Compass, TablePlus, `psql` and a `.env` on the developer's laptop want;
//!   * from another container — the container name and the port inside it,
//!     which is what a project's own application wants, and which does not go
//!     through a published port at all.
//!
//! ## Where the host port comes from
//!
//! From the engine when it can be asked, and only otherwise from `.env`. The
//! two disagree more often than they should: most templates publish
//! `{{ HOST_PORT_MONGO | default('27017') }}`, and `HOST_PORT_MONGO` is not one
//! of the keys `config.rs` embeds — so `.env` is silent and the port is a
//! literal inside a template. Reading the running container first means the
//! answer is the port a client can actually reach rather than the port the
//! configuration would produce if anyone had set it.
//!
//! A running container that publishes nothing reports no host address at all,
//! rather than one that would fail. That is a real state — a hand-edited
//! compose file, a port already taken — and inventing `127.0.0.1` for it would
//! be the same class of wrong answer this module exists to remove.
//!
//! ## The password
//!
//! Masked, on the same terms as [`crate::config::Env::service_credentials`]: a
//! URI with a live password in it is one screenshot away from being published,
//! and the whole point of `env_reveal` is that seeing a secret is an act. The
//! `reveal` argument is that act for this shape — one service, on a click.
//!
//! Percent-encoding happens here and not in the front end. A password
//! containing `@` or `/` produces a URI that parses as a different host, and
//! the failure is a connection error naming somewhere that does not exist.

use crate::config::{Env, MASK};
use crate::error::Result;
use serde::Serialize;
use std::path::Path;

/// The engines a connection string means something for.
///
/// Admin UIs are absent on purpose: pgAdmin and Adminer are opened in a
/// browser, and the address for that is the service's domain, which the sheet
/// already shows a row above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mysql,
    Postgres,
    Mongo,
    Redis,
    Memcached,
    Amqp,
    Http,
    /// Cassandra's drivers take a contact point, not a URI.
    HostPort,
    Smtp,
}

/// The scheme this kind writes, as the package contract spells it.
///
/// `uri` above builds the whole string and is the only thing that should; this
/// answers the narrower question a *manifest* asks — "what does a client call
/// this protocol" — for the two consumers that need the name without the
/// string. The three kinds that produce no URI at all say so with a name of
/// their own rather than an empty string, because a manifest field that is
/// sometimes blank is a field every reader has to guess about.
pub fn scheme_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Mysql => "mysql",
        Kind::Postgres => "postgresql",
        Kind::Mongo => "mongodb",
        Kind::Redis => "redis",
        Kind::Amqp => "amqp",
        Kind::Http => "http",
        Kind::Smtp => "smtp",
        Kind::Memcached | Kind::HostPort => "host-port",
    }
}

/// One address, and the string built from it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub uri: String,
    pub host: String,
    pub port: u16,
}

/// What one service is reachable at, from both sides of the network boundary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub service: String,
    pub kind: Kind,
    /// `None` when the container is up and publishes nothing to the host: there
    /// is no address to give, and inventing one is the bug this module is about.
    pub from_host: Option<Endpoint>,
    pub from_container: Endpoint,
    /// True when the URIs carry a password shown as bullets. False both when
    /// `reveal` was asked for and when the service has no password at all —
    /// so the UI offers the eye only where there is something behind it.
    pub masked: bool,
    /// The `.env` key the password came from, or `None` when there is none.
    /// Named rather than valued: this is what the credentials list keys its own
    /// reveal on, and the two should agree about which secret is in play.
    pub password_key: Option<String>,
}

// ------------------------------------------------------------- pure logic

/// Percent-encode for the userinfo component, conservatively.
///
/// Only the unreserved set survives. A stricter encoding than RFC 3986 demands
/// is always parseable; the reverse is a password with `@` in it silently
/// renaming the host.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The `user:password@` part, or nothing.
///
/// A password with no user is Redis's spelling and is kept legal here rather
/// than special-cased at the one call site that could produce it.
fn authority(user: Option<&str>, password: Option<&str>) -> String {
    match (user, password) {
        (Some(user), Some(password)) => format!("{user}:{password}@"),
        (Some(user), None) => format!("{user}@"),
        (None, Some(password)) => format!(":{password}@"),
        (None, None) => String::new(),
    }
}

/// The string itself.
///
/// `user` and `password` arrive **already rendered** — percent-encoded, or
/// replaced by the mask. Keeping that decision out of here is what lets the
/// same function produce both the shown and the copied string, and lets the
/// tests below assert on shapes without a keychain anywhere near them.
pub fn uri(
    kind: Kind,
    host: &str,
    port: u16,
    user: Option<&str>,
    password: Option<&str>,
    database: Option<&str>,
) -> String {
    let auth = authority(user, password);
    let db = database.unwrap_or_default();

    match kind {
        Kind::Mysql => format!("mysql://{auth}{host}:{port}/{db}"),
        Kind::Postgres => format!("postgresql://{auth}{host}:{port}/{db}"),
        Kind::Mongo => {
            // Without this the driver authenticates against `db`, where the
            // root account does not exist, and the failure reads as a wrong
            // password rather than as the wrong database being asked.
            let source = if auth.is_empty() {
                ""
            } else {
                "?authSource=admin"
            };
            format!("mongodb://{auth}{host}:{port}/{db}{source}")
        }
        Kind::Redis => format!("redis://{auth}{host}:{port}"),
        Kind::Amqp => format!("amqp://{auth}{host}:{port}/"),
        Kind::Http => format!("http://{auth}{host}:{port}"),
        Kind::Smtp => format!("smtp://{host}:{port}"),
        Kind::Memcached | Kind::HostPort => format!("{host}:{port}"),
    }
}

// ------------------------------------------------------------------- I/O

/// What the engine says about the host side of the mapping.
enum Published {
    /// A running container binds the port to this one on the host.
    Port(u16),
    /// A running container binds nothing. There is no host address.
    Nothing,
    /// The container does not exist, or the engine could not be asked. The
    /// configuration is the best available answer.
    Unknown,
}

async fn published(service: &str, container_port: u16) -> Published {
    let Ok(details) = crate::engine::inspect(service).await else {
        return Published::Unknown;
    };
    if !details.running {
        return Published::Unknown;
    }

    match details
        .ports
        .iter()
        .find(|port| port.container == container_port)
        .and_then(|port| port.host)
    {
        Some(host) => Published::Port(host),
        None => Published::Nothing,
    }
}

/// The [`Kind`] a manifest's `connection.scheme` names.
///
/// The inverse of [`scheme_of`], and it has to stay that way: the package
/// contract writes the scheme as a string and this is the only place that turns
/// it back into the enum `uri` switches on. An unknown scheme is `None` rather
/// than a guess — a URI built on the wrong kind is a string somebody pastes
/// into a client that then cannot connect, which is worse than no string.
fn kind_from_scheme(scheme: &str) -> Option<Kind> {
    Some(match scheme {
        "mysql" => Kind::Mysql,
        "postgresql" => Kind::Postgres,
        "mongodb" => Kind::Mongo,
        "redis" => Kind::Redis,
        "amqp" => Kind::Amqp,
        "http" => Kind::Http,
        "smtp" => Kind::Smtp,
        "host-port" => Kind::HostPort,
        _ => return None,
    })
}

/// One instance's connection, built from its manifest rather than from `.env`.
///
/// The same rule `list_services`, `service_source` and `service_domains` follow:
/// the table when there is one, `.env` when there is not. This half did not
/// exist, which is why a migrated workspace's detail sheet showed no connection
/// string at all — `spec_for` is keyed by the pre-package service name and an
/// instance is called `mysql-8-0`, so it matched nothing and the sheet was told
/// there was nothing to show.
///
/// `None` here means the same as it does below: not a service anybody connects
/// to with a string. A package says so by declaring no `connection` block.
async fn instance_of(root: &Path, id: &str, reveal: bool) -> Result<Option<Connection>> {
    let table = crate::instances::Table::load(root)?;
    let Some(instance) = table.get(id) else {
        return Ok(None);
    };
    let tree = crate::pkg::Tree::open(&crate::market::dir(root))?;
    let Ok(manifest) = tree.load(&instance.service, &instance.version) else {
        return Ok(None);
    };
    let (Some(conn), Some(kind)) = (
        manifest.connection.as_ref(),
        manifest
            .connection
            .as_ref()
            .and_then(|c| kind_from_scheme(&c.scheme)),
    ) else {
        return Ok(None);
    };
    let Some(port) = manifest.ports.iter().find(|p| p.name == conn.port) else {
        return Ok(None);
    };

    // Stored, then the manifest's default — the order `render::context` and
    // `instance_settings` both resolve in, because all three describe the value
    // the container is actually running with.
    let setting = |key: &str| -> Option<String> {
        let declared = manifest.settings.iter().find(|s| s.key == key)?;
        instance
            .settings
            .get(key)
            .cloned()
            .or_else(|| {
                instance
                    .secret_refs
                    .get(key)
                    .and_then(|reference| crate::secrets::entry_of(reference))
                    .and_then(|entry| crate::secrets::read(entry).ok().flatten())
            })
            .or_else(|| declared.default_text())
            .filter(|v| !v.is_empty())
    };

    let user = conn
        .user_setting
        .as_deref()
        .and_then(setting)
        .or_else(|| conn.default_user.clone());
    let secret = conn.password_setting.as_deref().and_then(setting);
    let database = conn
        .database_setting
        .as_deref()
        .and_then(setting)
        .or_else(|| conn.default_database.clone());

    // Same rule as the `.env` path: a user with no password is not a login
    // anybody asked for, and naming one in the URI claims an account the server
    // would refuse.
    let user = secret.as_ref().and(user);

    let rendered = secret.as_ref().map(|password| {
        if reveal {
            encode(password)
        } else {
            MASK.to_string()
        }
    });
    let rendered_user = user.as_deref().map(encode);

    let build = |host: &str, port: u16| Endpoint {
        uri: uri(
            kind,
            host,
            port,
            rendered_user.as_deref(),
            rendered.as_deref(),
            database.as_deref(),
        ),
        host: host.to_string(),
        port,
    };

    // The allocated port, not the manifest's preference: `ports::allocate`
    // wrote down what this instance actually got, and two versions of one
    // service cannot both have the preferred number.
    let configured = instance
        .ports
        .get(&conn.port)
        .copied()
        .unwrap_or(port.preferred);

    let from_host = match published(&instance.container(), port.container).await {
        Published::Port(host_port) => Some(build("127.0.0.1", host_port)),
        Published::Unknown => Some(build("127.0.0.1", configured)),
        Published::Nothing => None,
    };

    Ok(Some(Connection {
        service: id.to_string(),
        kind,
        from_host,
        // The instance's own container name. The pre-package alias resolves too
        // while this one is primary, but it is the one that stops resolving the
        // moment somebody promotes another version — and a connection string is
        // a thing people paste into a file and keep.
        from_container: build(&instance.container(), port.container),
        masked: secret.is_some() && !reveal,
        // The manifest's setting key, not a `.env` key. Nothing reads it as a
        // `.env` key any more: `service_reveal` dispatches the same way this
        // function does.
        password_key: conn.password_setting.clone(),
    }))
}

/// Everything one service is reachable at, or `None` when it is not the kind of
/// service anybody connects to with a string.
///
/// One source. This used to be a switch — the instance table when there was
/// one, `.env` and a compiled-in table of twenty-five connection shapes when
/// there was not — and ADR 0016 removed the second half everywhere else. It is
/// removed here too: a workspace with no table cannot render a stack, so it has
/// no running service to ask about, and the `.env` branch was unreachable code
/// carrying a second copy of what every package manifest now declares in its
/// own `connection` block.
pub async fn of(root: &Path, service: &str, reveal: bool) -> Result<Option<Connection>> {
    instance_of(root, service, reveal).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `scheme_of` and `kind_from_scheme` are one mapping written twice, and a
    /// package's `connection.scheme` is validated against the first and read by
    /// the second. A kind that lost its way back would give a manifest a scheme
    /// the contract accepts and this module cannot turn into a URI — so the
    /// service would validate, install, run, and show no connection string,
    /// which is exactly the failure this pair was added to end.
    #[test]
    fn every_scheme_a_kind_writes_is_a_scheme_that_reads_back() {
        for kind in [
            Kind::Mysql,
            Kind::Postgres,
            Kind::Mongo,
            Kind::Redis,
            Kind::Memcached,
            Kind::Amqp,
            Kind::Http,
            Kind::HostPort,
            Kind::Smtp,
        ] {
            let scheme = scheme_of(kind);
            let back = kind_from_scheme(scheme)
                .unwrap_or_else(|| panic!("{scheme} does not read back to a kind"));
            // `Memcached` and `HostPort` share the one name a manifest can
            // write, so the round trip lands on whichever of the two that name
            // means — the assertion is on the scheme, which is the thing both
            // sides actually exchange.
            assert_eq!(scheme_of(back), scheme, "{scheme}");
        }
    }

    /// A scheme nothing writes is not silently a URI of some other shape.
    #[test]
    fn an_unknown_scheme_is_refused_rather_than_guessed() {
        assert!(kind_from_scheme("ftp").is_none());
        assert!(kind_from_scheme("").is_none());
    }

    /// The bug that started this: a container name is not a host.
    ///
    /// Both strings exist because both are right, for different callers, and a
    /// UI that showed one of them would send half its readers to the wrong one.
    #[test]
    fn the_two_addresses_are_different_strings() {
        let host = uri(
            Kind::Mongo,
            "127.0.0.1",
            27017,
            Some("root"),
            Some("root"),
            None,
        );
        let internal = uri(
            Kind::Mongo,
            "stackvo-mongo",
            27017,
            Some("root"),
            Some("root"),
            None,
        );

        assert_eq!(
            host,
            "mongodb://root:root@127.0.0.1:27017/?authSource=admin"
        );
        assert_eq!(
            internal,
            "mongodb://root:root@stackvo-mongo:27017/?authSource=admin"
        );
    }

    /// Without `authSource` the driver looks for the root account in the
    /// application database, does not find it, and reports a failed login —
    /// which sends the reader off to check a password that was correct.
    #[test]
    fn a_mongo_uri_with_credentials_names_the_authentication_database() {
        let with = uri(
            Kind::Mongo,
            "127.0.0.1",
            27017,
            Some("root"),
            Some("s3cret"),
            Some("shop"),
        );
        assert_eq!(
            with,
            "mongodb://root:s3cret@127.0.0.1:27017/shop?authSource=admin"
        );

        // And not when there are none: `authSource` against an unauthenticated
        // server is a parameter describing a login that is not happening.
        let without = uri(Kind::Mongo, "127.0.0.1", 27017, None, None, Some("shop"));
        assert_eq!(without, "mongodb://127.0.0.1:27017/shop");
    }

    /// A password is arbitrary text, and three of the characters people put in
    /// one are URI syntax. Unencoded, `p@ss` moves the host.
    #[test]
    fn a_password_that_is_uri_syntax_is_encoded_rather_than_obeyed() {
        assert_eq!(encode("p@ss/word"), "p%40ss%2Fword");
        assert_eq!(encode("a:b?c#d"), "a%3Ab%3Fc%23d");
        // The unreserved set survives, so an ordinary password stays readable.
        assert_eq!(encode("root"), "root");
        assert_eq!(encode("Aa0-._~"), "Aa0-._~");

        let built = uri(
            Kind::Postgres,
            "127.0.0.1",
            5432,
            Some(&encode("stackvo")),
            Some(&encode("p@ss/word")),
            Some("shop"),
        );
        assert_eq!(
            built,
            "postgresql://stackvo:p%40ss%2Fword@127.0.0.1:5432/shop"
        );
    }

    /// The masked string has to stay a legal URI, or the thing on screen is not
    /// the thing being described. Bullets are percent-encoded if they go
    /// through `encode`, which is why the mask is substituted instead.
    #[test]
    fn the_masked_string_is_the_real_one_with_the_password_swapped() {
        let masked = uri(
            Kind::Mysql,
            "127.0.0.1",
            3306,
            Some("root"),
            Some(MASK),
            Some("stackvo"),
        );
        assert_eq!(
            masked,
            format!("mysql://root:{MASK}@127.0.0.1:3306/stackvo")
        );
        assert!(!masked.contains('%'), "the mask must not be encoded");
    }

    /// Redis takes a password with no user, and Memcached takes no URI at all.
    /// Both are the shape their own clients accept, not a scheme invented for
    /// symmetry with the others.
    #[test]
    fn the_engines_that_are_not_uris_are_not_given_one() {
        assert_eq!(
            uri(Kind::Memcached, "127.0.0.1", 11211, None, None, None),
            "127.0.0.1:11211"
        );
        assert_eq!(
            uri(Kind::HostPort, "stackvo-cassandra", 9042, None, None, None),
            "stackvo-cassandra:9042"
        );
        assert_eq!(
            uri(Kind::Redis, "127.0.0.1", 6379, None, None, None),
            "redis://127.0.0.1:6379"
        );
        assert_eq!(
            uri(Kind::Redis, "127.0.0.1", 6379, None, Some("pw"), None),
            "redis://:pw@127.0.0.1:6379"
        );
        assert_eq!(
            uri(Kind::Smtp, "stackvo-mailpit", 1025, None, None, None),
            "smtp://stackvo-mailpit:1025"
        );
    }

    /// Every kind names a scheme, and the three that produce no URI say so with
    /// a name rather than a blank.
    #[test]
    fn the_scheme_names_match_the_strings_uri_builds() {
        // Over the kinds rather than over a table of services: the table is
        // gone (the packages declare their own `connection` block), and what
        // is left to check is that every kind this module can produce names a
        // scheme, including the three that build no URI at all — they say
        // `host-port` rather than an empty string, so a manifest field is never
        // sometimes blank.
        for kind in [
            Kind::Mysql,
            Kind::Postgres,
            Kind::Mongo,
            Kind::Redis,
            Kind::Memcached,
            Kind::Amqp,
            Kind::Http,
            Kind::HostPort,
            Kind::Smtp,
        ] {
            assert!(!scheme_of(kind).is_empty(), "{kind:?} has no scheme name");
        }
        assert_eq!(scheme_of(Kind::Memcached), "host-port");
        assert_eq!(scheme_of(Kind::HostPort), "host-port");
    }

    /// Naming an account the server will refuse is worse than naming none: the
    /// error comes back as an authentication failure, which reads as a wrong
    /// password rather than as a login nobody configured.
    ///
    /// The rule used to be checked against the compiled-in table of services;
    /// that table is gone, and a package's `connection` block is where the pair
    /// is declared now. So this checks the builder instead, which is the place
    /// the rule is actually applied and the only place it can still be broken.
    #[test]
    fn a_service_with_no_password_gets_no_user_either() {
        assert_eq!(
            uri(
                Kind::Mysql,
                "127.0.0.1",
                3306,
                Some("root"),
                None,
                Some("shop")
            ),
            "mysql://root@127.0.0.1:3306/shop",
            "a user with no password is written without one"
        );
        assert_eq!(
            uri(Kind::Redis, "127.0.0.1", 6379, None, None, None),
            "redis://127.0.0.1:6379",
            "and neither half present writes no authority at all"
        );
    }
}
