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

/// One service's shape, resolved from `.env` and the engine.
struct Spec {
    service: &'static str,
    kind: Kind,
    /// The port inside the container — the second half of the template's
    /// `ports:` mapping, which no `.env` key can move.
    container_port: u16,
    /// Keys the published host port may be under, in the order they are
    /// honoured. Two spellings because the tree has two: six services use
    /// `SERVICE_<ID>_HOST_PORT` and the rest `HOST_PORT_<ID>`, which is the
    /// inconsistency `contracts/env.schema.json` records under `servicePattern`
    /// and `config.rs` aliases three of.
    port_keys: &'static [&'static str],
    user_key: Option<&'static str>,
    /// Used when `user_key` is unset or empty — the account the image itself
    /// creates. Mirrors `db.rs`'s `default_user` for the four engines both
    /// modules know about.
    default_user: Option<&'static str>,
    password_key: Option<&'static str>,
    database_key: Option<&'static str>,
    /// Mirrors the `| default(...)` in the service's compose template, which is
    /// where the value lives when `.env` says nothing — the images create this
    /// database on first boot whether or not anyone configured it.
    default_database: Option<&'static str>,
}

/// Every service with a connection string, and what it is made of.
///
/// A table rather than a `match` per question so that adding a service is one
/// row and not five edits, and so the tests below can walk it.
const SPECS: &[Spec] = &[
    Spec {
        service: "mysql",
        kind: Kind::Mysql,
        container_port: 3306,
        port_keys: &["SERVICE_MYSQL_HOST_PORT", "HOST_PORT_MYSQL"],
        user_key: None,
        default_user: Some("root"),
        password_key: Some("SERVICE_MYSQL_ROOT_PASSWORD"),
        database_key: Some("SERVICE_MYSQL_DATABASE"),
        default_database: Some("stackvo"),
    },
    Spec {
        service: "mariadb",
        kind: Kind::Mysql,
        container_port: 3306,
        port_keys: &["SERVICE_MARIADB_HOST_PORT", "HOST_PORT_MARIADB"],
        user_key: None,
        default_user: Some("root"),
        password_key: Some("SERVICE_MARIADB_ROOT_PASSWORD"),
        database_key: Some("SERVICE_MARIADB_DATABASE"),
        default_database: Some("stackvo"),
    },
    Spec {
        service: "postgres",
        kind: Kind::Postgres,
        container_port: 5432,
        port_keys: &["SERVICE_POSTGRES_HOST_PORT", "HOST_PORT_POSTGRES"],
        user_key: Some("SERVICE_POSTGRES_USER"),
        default_user: Some("postgres"),
        password_key: Some("SERVICE_POSTGRES_PASSWORD"),
        database_key: Some("SERVICE_POSTGRES_DB"),
        default_database: Some("stackvo"),
    },
    Spec {
        service: "mongo",
        kind: Kind::Mongo,
        container_port: 27017,
        port_keys: &["SERVICE_MONGO_HOST_PORT", "HOST_PORT_MONGO"],
        user_key: Some("SERVICE_MONGO_INITDB_ROOT_USERNAME"),
        default_user: Some("root"),
        password_key: Some("SERVICE_MONGO_INITDB_ROOT_PASSWORD"),
        database_key: Some("SERVICE_MONGO_DATABASE"),
        default_database: Some("stackvo"),
    },
    Spec {
        service: "redis",
        kind: Kind::Redis,
        container_port: 6379,
        port_keys: &["SERVICE_REDIS_HOST_PORT", "HOST_PORT_REDIS"],
        user_key: None,
        default_user: None,
        // Deliberately none, and `SERVICE_REDIS_PASSWORD` exists. The shipped
        // `redis.conf` template leaves `requirepass` commented out, so the key
        // configures nothing — putting it in the URI would produce a string
        // that fails against a server with no password set, which is worse
        // than omitting a password the server does not want.
        password_key: None,
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "memcached",
        kind: Kind::Memcached,
        container_port: 11211,
        port_keys: &["SERVICE_MEMCACHED_HOST_PORT", "HOST_PORT_MEMCACHED"],
        user_key: None,
        default_user: None,
        password_key: None,
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "rabbitmq",
        kind: Kind::Amqp,
        container_port: 5672,
        port_keys: &["SERVICE_RABBITMQ_HOST_PORT", "HOST_PORT_RABBITMQ"],
        user_key: Some("SERVICE_RABBITMQ_DEFAULT_USER"),
        default_user: Some("guest"),
        password_key: Some("SERVICE_RABBITMQ_DEFAULT_PASS"),
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "elasticsearch",
        kind: Kind::Http,
        container_port: 9200,
        port_keys: &["SERVICE_ELASTICSEARCH_HOST_PORT", "HOST_PORT_ELASTICSEARCH"],
        // The template sets `xpack.security.enabled` from `ELASTIC_SECURITY`,
        // which defaults to false — so the shipped cluster takes no credentials
        // and a URI carrying them would be rejected rather than ignored.
        user_key: None,
        default_user: None,
        password_key: None,
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "cassandra",
        kind: Kind::HostPort,
        container_port: 9042,
        port_keys: &["SERVICE_CASSANDRA_HOST_PORT", "HOST_PORT_CASSANDRA"],
        user_key: None,
        default_user: None,
        password_key: None,
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "mailpit",
        kind: Kind::Smtp,
        container_port: 1025,
        port_keys: &["SERVICE_MAILPIT_SMTP_HOST_PORT", "HOST_PORT_MAILPIT_SMTP"],
        user_key: None,
        default_user: None,
        password_key: None,
        database_key: None,
        default_database: None,
    },
    Spec {
        service: "mailhog",
        kind: Kind::Smtp,
        container_port: 1025,
        port_keys: &["SERVICE_MAILHOG_SMTP_HOST_PORT", "HOST_PORT_MAILHOG_SMTP"],
        user_key: None,
        default_user: None,
        password_key: None,
        database_key: None,
        default_database: None,
    },
];

fn spec_for(service: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.service == service)
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

/// A `.env` value that is set and not blank.
fn value<'a>(env: &'a Env, key: Option<&str>) -> Option<&'a str> {
    env.get(key?).filter(|v| !v.is_empty())
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

/// Everything one service is reachable at, or `None` when it is not the kind of
/// service anybody connects to with a string.
pub async fn of(root: &Path, service: &str, reveal: bool) -> Result<Option<Connection>> {
    let Some(spec) = spec_for(service) else {
        return Ok(None);
    };

    let env = Env::load(root)?;

    let user = value(&env, spec.user_key)
        .map(str::to_string)
        .or_else(|| spec.default_user.map(str::to_string));
    let secret = value(&env, spec.password_key).map(str::to_string);
    let database = value(&env, spec.database_key)
        .map(str::to_string)
        .or_else(|| spec.default_database.map(str::to_string));

    // A user with no password is not authentication anyone asked for: every
    // engine here that names an account also ships one. Dropping it keeps the
    // URI from claiming a login that would be refused.
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
            spec.kind,
            host,
            port,
            rendered_user.as_deref(),
            rendered.as_deref(),
            database.as_deref(),
        ),
        host: host.to_string(),
        port,
    };

    // The configured port, for when the engine has nothing to say. First key
    // wins, so a checkout carrying the older spelling keeps the port it has.
    let configured = spec
        .port_keys
        .iter()
        .find_map(|key| env.get(key).and_then(|v| v.parse::<u16>().ok()))
        .unwrap_or(spec.container_port);

    let from_host = match published(service, spec.container_port).await {
        Published::Port(port) => Some(build("127.0.0.1", port)),
        Published::Unknown => Some(build("127.0.0.1", configured)),
        Published::Nothing => None,
    };

    Ok(Some(Connection {
        service: service.to_string(),
        kind: spec.kind,
        from_host,
        from_container: build(
            &format!("{}{service}", crate::engine::CONTAINER_PREFIX),
            spec.container_port,
        ),
        masked: secret.is_some() && !reveal,
        password_key: spec.password_key.map(str::to_string),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The admin UIs are opened in a browser and the sheet shows their domain a
    /// row above. A `mysql://` string for phpMyAdmin would be a third address
    /// for a thing that has two.
    #[test]
    fn only_services_you_connect_to_with_a_string_have_one() {
        assert!(spec_for("mongo").is_some());
        assert!(spec_for("mailpit").is_some());
        assert!(spec_for("mongo-express").is_none());
        assert!(spec_for("phpmyadmin").is_none());
        assert!(spec_for("traefik").is_none());
    }

    /// Every row names the port its own template publishes. A wrong number here
    /// is a connection string that looks right and reaches nothing.
    #[test]
    fn each_spec_carries_the_port_inside_its_container() {
        let expected = [
            ("mysql", 3306u16),
            ("mariadb", 3306),
            ("postgres", 5432),
            ("mongo", 27017),
            ("redis", 6379),
            ("memcached", 11211),
            ("rabbitmq", 5672),
            ("elasticsearch", 9200),
            ("cassandra", 9042),
            ("mailpit", 1025),
            ("mailhog", 1025),
        ];

        assert_eq!(
            expected.len(),
            SPECS.len(),
            "a service was added or removed"
        );
        for (service, port) in expected {
            let spec = spec_for(service).expect("in the table");
            assert_eq!(spec.container_port, port, "{service}");
        }
    }

    /// Both spellings, current first. A checkout that carries the older key
    /// keeps the port it has — the same rule `config.rs`'s alias table applies,
    /// and the reason `HOST_PORT_MONGO` is read at all.
    #[test]
    fn the_port_is_looked_for_under_both_spellings() {
        for spec in SPECS {
            assert_eq!(spec.port_keys.len(), 2, "{}", spec.service);
            assert!(
                spec.port_keys[0].starts_with("SERVICE_"),
                "{} does not try the current spelling first",
                spec.service
            );
            assert!(
                spec.port_keys[1].starts_with("HOST_PORT_"),
                "{} does not fall back to the older spelling",
                spec.service
            );
        }
    }

    /// Naming an account the server will refuse is worse than naming none: the
    /// error comes back as an authentication failure, which reads as a wrong
    /// password rather than as a login nobody configured.
    #[test]
    fn a_service_with_no_password_gets_no_user_either() {
        for spec in SPECS {
            if spec.password_key.is_none() {
                assert!(
                    spec.user_key.is_none() && spec.default_user.is_none(),
                    "{} would put a user in a URI with no password behind it",
                    spec.service
                );
            }
        }
    }
}
