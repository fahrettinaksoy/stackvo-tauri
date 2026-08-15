//! Telling an assistant working inside a project what it is working inside.
//!
//! K-2. `agents.rs` registers `stackvo-mcp` with the assistants on the *host*,
//! and that covers the case where somebody is running Claude Code or Cursor on
//! their machine. It does nothing for an agent running **in the container** —
//! a `claude` invocation from inside `stackvo-shop`, an agent in a devcontainer
//! — which has no MCP server to reach, because `stackvo-mcp` speaks stdio on
//! the host and there is no transport from inside a container to it.
//!
//! What such an agent needs is smaller than a server anyway. It needs to know
//! that it is in a StackVo project, which project, what the site is called, and
//! what is running around it. That is a file.
//!
//! ## Written into the project directory, not mounted
//!
//! The obvious shape is a read-only mount into the container. It is also the
//! one thing that cannot be done here: `tests/fixtures_differential.rs`
//! compares the generated project compose byte for byte against output frozen
//! from the Bash generator this port replaced, and a new `volumes:` entry would
//! fail it — correctly, because the proof that this port reproduces that one is
//! worth more than the tidier mount.
//!
//! So it is written to `<project>/.stackvo/context.json`. For a PHP project
//! that directory is already bind-mounted at `/var/www/html`, so the file is
//! live: regenerate and the container sees it. For a node project there is no
//! mount by design — the image is built from the source — so it arrives at the
//! next build, and that is stated rather than hidden.
//!
//! It also lands where an agent looks first, which the mount would not: an
//! assistant working in a repository reads the repository.
//!
//! ## Names and addresses. Never credentials.
//!
//! An agent needs to know Redis is at `stackvo-redis-7-2:6379`. It does not
//! need the password, and the application's own `.env` is where a password
//! belongs — this file is written into somebody's source tree, and a source
//! tree is a thing that gets committed by accident.
//!
//! That is a rule by construction rather than by filtering, the same shape
//! [`crate::preset`] uses: [`Service`] holds an id, a host and a port and has
//! nowhere to put a secret. It is not that a filter drops one; there is no
//! field.

use crate::error::Result;
use serde::Serialize;
use std::path::Path;

/// The directory this app writes into a project, and the one file in it.
pub const DIR: &str = ".stackvo";
pub const FILE: &str = "context.json";

/// Bumped when the shape changes, for the reason `preferences.json` grew one:
/// an absent version leaves no way to tell "old file" from "never written".
const SCHEMA_VERSION: u32 = 1;

/// A backing service, as an agent needs to reach it.
///
/// Three fields, and no fourth is possible — see the module note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// The catalogue id: `mysql`, `redis`.
    pub id: String,
    /// The hostname *inside the network*, which is the one that works from a
    /// container. The host port is deliberately absent: from inside, it is not
    /// the address, and offering both is how somebody uses the wrong one.
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub schema_version: u32,
    /// So a reader can tell this file apart from a JSON that is merely valid.
    pub kind: &'static str,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_root: Option<String>,
    /// Inside the container, which is the only path that means anything to a
    /// reader of this file.
    pub app_path: String,
    pub services: Vec<Service>,
    /// Things that are true and that an agent would otherwise have to discover
    /// by failing — the node build caveat, and what this file does not carry.
    pub notes: Vec<String>,
}

/// Which port a service answers on inside the network.
///
/// The container port, not the published one. A table rather than a lookup into
/// the package manifest because this has to work for a workspace whose
/// catalogue has not been fetched — and because a wrong answer here is a
/// connection string that fails rather than a missing file.
fn container_port(id: &str) -> Option<u16> {
    Some(match id {
        "mysql" | "mariadb" => 3306,
        "postgres" => 5432,
        "mongo" => 27017,
        "redis" | "valkey" => 6379,
        "memcached" => 11211,
        "rabbitmq" => 5672,
        "kafka" => 9092,
        "elasticsearch" => 9200,
        "meilisearch" => 7700,
        "typesense" => 8108,
        "minio" => 9000,
        "mailhog" | "mailpit" => 1025,
        "cassandra" => 9042,
        "clickhouse" => 8123,
        "solr" => 8983,
        _ => return None,
    })
}

/// Build the context for one project.
pub fn build(root: &Path, manifest: &crate::manifest::Manifest) -> Result<Context> {
    let table = crate::instances::Table::load(root).unwrap_or_default();

    // Every enabled database-or-backing instance, not only the ones this
    // project declared. An agent asked "is there a Redis" should get the
    // answer the machine has, and `manifest.services` is a declaration of what
    // the project needs rather than an inventory of what is there.
    let mut services: Vec<Service> = table
        .instances
        .iter()
        .filter(|instance| instance.enabled)
        .filter_map(|instance| {
            Some(Service {
                port: container_port(&instance.service)?,
                // The instance's own container name, so a workspace running
                // MySQL 8.0 and 8.4 side by side gives an agent the two
                // addresses rather than one ambiguous `mysql`.
                host: instance.container(),
                id: instance.service.clone(),
            })
        })
        .collect();
    services.sort_by(|a, b| a.host.cmp(&b.host));

    let mut notes = vec![
        "This file carries names and addresses only. Credentials are in the project's own \
         .env and are deliberately not repeated here."
            .to_string(),
    ];
    if manifest.runtime != "php" {
        notes.push(
            "This runtime has no source bind mount — the image is built from the source — so \
             this file reaches the container at the next build rather than immediately."
                .to_string(),
        );
    }
    notes.push(format!(
        "Regenerated by StackVo. {DIR}/ is this machine's view of the stack and is not \
         meant to be committed."
    ));

    Ok(Context {
        schema_version: SCHEMA_VERSION,
        kind: "stackvo.agent-context",
        project: manifest.name.clone(),
        url: manifest.domain.as_ref().map(|d| format!("https://{d}")),
        domain: manifest.domain.clone(),
        app_path: crate::release::app_path(&manifest.runtime).to_string(),
        document_root: manifest.document_root.clone(),
        runtime: manifest.runtime.clone(),
        services,
        notes,
    })
}

/// Write it into the project directory, creating `.stackvo/` if needed.
///
/// Returns `Ok(())` and writes nothing when the project directory is gone: this
/// runs inside generation, over every project, and one deleted directory must
/// not stop the stack being regenerated.
pub fn write(project_dir: &Path, context: &Context) -> Result<()> {
    if !project_dir.is_dir() {
        return Ok(());
    }
    let dir = project_dir.join(DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::error::Error::io(format!("creating {}", dir.display()), e))?;

    let text = serde_json::to_string_pretty(context).map_err(|e| {
        crate::error::Error::new(
            crate::error::Code::IoError,
            format!("serialising the agent context: {e}"),
        )
    })?;
    crate::atomic::write(&dir.join(FILE), &format!("{text}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(runtime: &str) -> crate::manifest::Manifest {
        let json = serde_json::json!({
            "name": "shop",
            "domain": "shop.loc",
            "runtime": runtime,
            "document_root": "public",
            "php": { "version": "8.4" },
        });
        crate::manifest::normalize(&json, "", "shop")
    }

    /// A counter, not a timestamp. Two tests running on two threads can read
    /// the same nanosecond — this failed exactly that way in the full suite and
    /// passed on its own, which is the signature of it.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn empty_root() -> std::path::PathBuf {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stackvo-agentctx-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_context_names_the_project_and_where_it_answers() {
        let root = empty_root();
        let ctx = build(&root, &manifest("php")).unwrap();

        assert_eq!(ctx.project, "shop");
        assert_eq!(ctx.url.as_deref(), Some("https://shop.loc"));
        assert_eq!(ctx.app_path, "/var/www/html");
        assert_eq!(ctx.kind, "stackvo.agent-context");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The rule this module exists to keep, checked on the rendered bytes
    /// rather than on the struct: what matters is what lands in somebody's
    /// source tree.
    #[test]
    fn no_credential_can_appear_in_the_rendered_file() {
        let root = empty_root();
        let ctx = build(&root, &manifest("php")).unwrap();
        let text = serde_json::to_string(&ctx).unwrap().to_lowercase();

        for word in ["password", "secret", "token", "apikey", "credential"] {
            assert!(
                !text.contains(&format!("\"{word}")),
                "{word} reached the file: {text}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A node project's file arrives at the next build, and an agent reading a
    /// stale one would otherwise have no way to know that.
    #[test]
    fn a_runtime_with_no_source_mount_says_so() {
        let root = empty_root();
        let php = build(&root, &manifest("php")).unwrap();
        let node = build(&root, &manifest("node")).unwrap();

        assert!(!php.notes.iter().any(|n| n.contains("next build")));
        assert!(
            node.notes.iter().any(|n| n.contains("next build")),
            "{:?}",
            node.notes
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The container port, not the published one: from inside the network the
    /// published port is not the address, and offering both is how somebody
    /// uses the wrong one.
    #[test]
    fn the_port_is_the_one_that_works_from_inside() {
        assert_eq!(container_port("mysql"), Some(3306));
        assert_eq!(container_port("redis"), Some(6379));
        assert_eq!(container_port("mailpit"), Some(1025));
        assert_eq!(container_port("not-a-service"), None);
    }

    #[test]
    fn writing_into_a_directory_that_is_gone_is_not_a_failure() {
        let ctx = build(&empty_root(), &manifest("php")).unwrap();
        assert!(write(std::path::Path::new("/no/such/place"), &ctx).is_ok());
    }

    #[test]
    fn the_file_round_trips_as_json() {
        let root = empty_root();
        let ctx = build(&root, &manifest("php")).unwrap();
        write(&root, &ctx).unwrap();

        let text = std::fs::read_to_string(root.join(DIR).join(FILE)).unwrap();
        let back: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["kind"], "stackvo.agent-context");
        assert_eq!(back["project"], "shop");
        let _ = std::fs::remove_dir_all(&root);
    }
}
