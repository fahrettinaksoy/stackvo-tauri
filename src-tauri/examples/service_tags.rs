//! Does every tag the version picker offers still exist?
//!
//! `SERVICE_<ID>_VERSIONS` is a list of image tags compiled into the binary,
//! and the failure it invites is silent: a registry drops a series, the list
//! keeps offering it, and the first anyone hears is a pull error after the
//! sheet said Apply. The unit tests hold the shape — every service has a list,
//! every default is on its own list — because those are facts about the source
//! and a test suite must not need a network. Whether a tag resolves is a fact
//! about the world, so it lives here.
//!
//!   cargo run --example service_tags
//!
//! Two details are the reason this is a program rather than a note to check by
//! hand. The reference it probes is read out of the template, not written down
//! again, so RabbitMQ is checked as `4.3-management` — the plain tags exist for
//! series the management ones do not, and verifying the bare name would pass
//! while the pull failed. And Elasticsearch comes from `docker.elastic.co`,
//! which is a different registry from the Hub with its own token handshake and
//! its own idea of which tags exist.

use stackvo_desktop_lib::{config::Env, contracts::env_schema, skeleton};

/// The `image:` value a service's compose template renders, split into the
/// repository and whatever the template appends after the tag.
///
/// Returns `None` for a template whose image does not interpolate the version
/// key at all — `cp-zookeeper:latest` is written in full inside the Kafka
/// template, and nothing here can say anything about a tag nobody chooses.
fn image_of(service: &str) -> Option<(String, String)> {
    let relative = format!("core/templates/services/{service}/docker-compose.{service}.tpl");
    let text = skeleton::read_template(std::path::Path::new("/nonexistent"), &relative)?;
    let placeholder = format!("{{{{ {}VERSION }}}}", Env::service_prefix(service));

    let line = text.lines().find(|l| l.contains(&placeholder))?;
    let reference = line.split_once("image:")?.1.trim().trim_matches('"');
    let (repository, rest) = reference.split_once(&placeholder)?;

    Some((
        repository.trim_end_matches(':').to_string(),
        rest.to_string(),
    ))
}

/// Docker Hub's own API rather than the registry v2 one, because it answers
/// for `library/*` and user repositories through a single unauthenticated
/// path. `docker.elastic.co` gets the v2 handshake it insists on.
async fn resolves(client: &reqwest::Client, repository: &str, tag: &str) -> bool {
    if let Some(path) = repository.strip_prefix("docker.elastic.co/") {
        return elastic_resolves(client, path, tag).await;
    }
    let repository = if repository.contains('/') {
        repository.to_string()
    } else {
        format!("library/{repository}")
    };
    let url = format!("https://hub.docker.com/v2/repositories/{repository}/tags/{tag}");
    matches!(client.get(&url).send().await, Ok(r) if r.status().is_success())
}

async fn elastic_resolves(client: &reqwest::Client, repository: &str, tag: &str) -> bool {
    let url = format!("https://docker.elastic.co/v2/{repository}/manifests/{tag}");
    let accept = "application/vnd.docker.distribution.manifest.list.v2+json, \
                  application/vnd.oci.image.index.v1+json";

    let Ok(first) = client.head(&url).header("Accept", accept).send().await else {
        return false;
    };
    if first.status().is_success() {
        return true;
    }

    // 401 carries the realm to ask for an anonymous pull token. Parsed rather
    // than hardcoded so a move of Elastic's token endpoint shows up as a
    // failure to authenticate and not as every tag reported missing.
    let challenge = first
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let field = |name: &str| {
        challenge
            .split_once(&format!("{name}=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(value, _)| value.to_string())
    };
    let (Some(realm), Some(service)) = (field("realm"), field("service")) else {
        return false;
    };

    let token_url = format!("{realm}?service={service}&scope=repository:{repository}:pull");
    let Ok(token) = client.get(&token_url).send().await else {
        return false;
    };
    let Ok(body) = token.json::<serde_json::Value>().await else {
        return false;
    };
    let Some(token) = body.get("token").and_then(|t| t.as_str()) else {
        return false;
    };

    matches!(
        client.get(&url).bearer_auth(token).header("Accept", accept).send().await,
        Ok(r) if r.status().is_success()
    )
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let client = reqwest::Client::new();
    // `parse` of nothing, not `default()`. A workspace is not required — the
    // point is to check what the binary ships — but the defaults arrive by
    // being merged in `parse`, and `Env::default()` is an empty map that
    // answers every list with zero entries. This program reported twenty-five
    // services "ok" on the strength of that.
    let env = Env::parse("");
    let mut missing = 0;

    for (service, _) in env_schema().service_catalog() {
        let versions = env.service_versions(&service);
        if versions.is_empty() {
            missing += 1;
            println!("{service:<14} EMPTY  no versions offered");
            continue;
        }
        // Not a shrug. A service whose image cannot be read is a service whose
        // list is unchecked, and reporting that as a blank line beside
        // twenty-four "ok"s is how a broken probe reads as a clean run — which
        // is exactly what the first version of this file did.
        let Some((repository, suffix)) = image_of(&service) else {
            missing += 1;
            println!("{service:<14} UNREADABLE could not find the image line in its template");
            continue;
        };

        let checked: Vec<(String, bool)> = versions
            .iter()
            .map(|version| {
                let tag = format!("{version}{suffix}");
                let ok = runtime.block_on(resolves(&client, &repository, &tag));
                (tag, ok)
            })
            .collect();

        let gone: Vec<&str> = checked
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(tag, _)| tag.as_str())
            .collect();

        if gone.is_empty() {
            println!("{service:<14} ok   {repository} {}", versions.join(","));
        } else {
            missing += gone.len();
            println!("{service:<14} GONE {repository} {}", gone.join(" "));
        }
    }

    println!();
    if missing == 0 {
        println!("every offered tag resolves");
    } else {
        println!("{missing} problem(s) — update EMBEDDED in config.rs");
        std::process::exit(1);
    }
}
