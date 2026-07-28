//! What has to be true before StackVo can do anything.
//!
//! The app is a front end for `docker compose` over a checkout of shell
//! scripts, and every one of those words is a prerequisite that can be missing
//! on a fresh machine. Until this existed the app opened regardless and each
//! button failed on its own terms: a compose plugin from 2019 produced
//! "unknown flag: --profile", a missing `stackvo-net` produced "network
//! declared as external, but could not be found", and on Windows the generator
//! failed with "program not found: bash". Three different errors, one cause
//! each, none of them stated up front.
//!
//! The set below is not invented here — it is what `core/cli/commands/install.sh`
//! checks before it will install, plus the two things only a desktop app needs
//! (a chosen checkout, and a shell to run the generator with).

use crate::error::Result;
use crate::{engine, workspace};
use serde::Serialize;
use std::path::Path;

/// Whether a requirement is met, could not be tested, or blocks the app.
///
/// `Unknown` is its own state rather than a failure: when the engine is down
/// there is no answer to "does the network exist", and reporting one as a
/// failure sends the user after the wrong problem.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Ok,
    Warn,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// Stable key; the UI holds the label and the instructions for it.
    pub id: &'static str,
    pub state: State,
    /// The facts — a version, a path, the daemon's own error. Not translated:
    /// these are what the machine said.
    pub detail: Option<String>,
    /// The id to hand back to `preflight_fix`, when the app can do it itself.
    pub fixable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    /// `macos` | `windows` | `linux` — the UI's instructions differ per platform.
    pub os: &'static str,
    pub requirements: Vec<Requirement>,
    /// True when nothing is in `Fail`. Warnings do not hold the app back.
    pub ready: bool,
}

const OS: &str = if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "windows") {
    "windows"
} else {
    "linux"
};

/// The network name the generator writes into every compose file.
fn network_name(root: Option<&Path>) -> String {
    root.and_then(|r| crate::config::Env::load(r).ok())
        .and_then(|env| env.get("DOCKER_DEFAULT_NETWORK").map(str::to_string))
        .unwrap_or_else(|| "stackvo-net".to_string())
}

/// First line of `<program> <args…>`, or None when it cannot be run at all.
async fn probe(program: &str, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

/// Major version out of anything shaped like `v2.29.7` or `2.29.7`.
fn major(version: &str) -> Option<u32> {
    version
        .trim_start_matches('v')
        .split('.')
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

pub async fn run() -> Preflight {
    let ws = workspace::resolve();
    let root = ws.root.as_ref().map(Path::new).filter(|_| ws.valid);

    let mut out = Vec::new();

    // ---- the checkout -------------------------------------------------------
    out.push(Requirement {
        id: "workspace",
        state: if root.is_some() { State::Ok } else { State::Fail },
        detail: ws.root.clone(),
        fixable: true,
    });

    // ---- the daemon ---------------------------------------------------------
    let engine = engine::status().await;
    out.push(Requirement {
        id: "engine",
        state: if engine.reachable {
            State::Ok
        } else {
            State::Fail
        },
        detail: if engine.reachable {
            engine.version.clone()
        } else {
            engine.error.clone()
        },
        fixable: true,
    });

    // ---- the compose plugin -------------------------------------------------
    //
    // Version 2 is not a preference: the app drives compose with `--profile`,
    // which v1 does not have. install.sh refuses to install below 2.0 for the
    // same reason.
    let compose = probe("docker", &["compose", "version", "--short"])
        .await
        .or(probe("docker", &["compose", "version"]).await);

    out.push(match compose.as_deref() {
        Some(v) if major(v).is_some_and(|m| m >= 2) => Requirement {
            id: "compose",
            state: State::Ok,
            detail: Some(v.to_string()),
            fixable: false,
        },
        Some(v) => Requirement {
            id: "compose",
            state: State::Fail,
            detail: Some(v.to_string()),
            fixable: false,
        },
        None => Requirement {
            id: "compose",
            state: State::Fail,
            detail: None,
            fixable: false,
        },
    });

    // ---- the shared network -------------------------------------------------
    //
    // Every generated compose file declares it `external: true`, so compose
    // will not create it — it fails instead, once per service.
    let name = network_name(root);
    out.push(if !engine.reachable {
        Requirement {
            id: "network",
            state: State::Unknown,
            detail: Some(name),
            fixable: false,
        }
    } else {
        let exists = engine::network_exists(&name).await;
        Requirement {
            id: "network",
            state: if exists { State::Ok } else { State::Fail },
            detail: Some(name),
            fixable: true,
        }
    });

    // ---- the projects directory --------------------------------------------
    out.push(match root {
        Some(r) if r.join("projects").is_dir() => Requirement {
            id: "projects",
            state: State::Ok,
            detail: None,
            fixable: false,
        },
        Some(r) => Requirement {
            id: "projects",
            state: State::Fail,
            detail: Some(r.join("projects").display().to_string()),
            fixable: true,
        },
        None => Requirement {
            id: "projects",
            state: State::Unknown,
            detail: None,
            fixable: false,
        },
    });

    // ---- a shell for the generator -----------------------------------------
    //
    // The generator is `core/cli/stackvo.sh`, spawned as `bash`. There is a
    // Rust port running alongside it, but it does not write build inputs yet —
    // so on a machine without bash the app can read a stack and not change one.
    let bash = probe("bash", &["--version"]).await;
    out.push(Requirement {
        id: "bash",
        state: if bash.is_some() {
            State::Ok
        } else {
            State::Fail
        },
        detail: bash,
        fixable: false,
    });

    let ready = !out.iter().any(|r| r.state == State::Fail);
    Preflight {
        os: OS,
        requirements: out,
        ready,
    }
}

/// Do the one thing this requirement needs, where the app can do it itself.
pub async fn fix(id: &str) -> Result<()> {
    let ws = workspace::resolve();
    let root = ws.root.as_ref().map(Path::new).filter(|_| ws.valid);

    match id {
        "network" => engine::network_create(&network_name(root)).await,
        "projects" => {
            let root = ws.require_root()?;
            std::fs::create_dir_all(root.join("projects"))
                .map_err(|e| crate::error::Error::io("creating the projects directory", e))
        }
        other => Err(crate::error::Error::new(
            crate::error::Code::InvalidInput,
            format!("{other} is not something the app can fix"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_reads_both_shapes_compose_prints() {
        assert_eq!(major("v2.29.7"), Some(2));
        assert_eq!(major("2.29.7"), Some(2));
        // `docker compose version` without --short prints a sentence.
        assert_eq!(major("Docker Compose version v2.29.7"), None);
        assert_eq!(major("1.29.2"), Some(1));
    }

    #[tokio::test]
    async fn probe_reports_none_for_a_program_that_is_not_there() {
        assert!(probe("stackvo-not-a-real-program", &["--version"])
            .await
            .is_none());
    }

    #[tokio::test]
    async fn every_requirement_is_reported_once() {
        let result = run().await;
        let ids: Vec<&str> = result.requirements.iter().map(|r| r.id).collect();

        // The gate is only honest if it is complete: a missing entry is a
        // prerequisite nobody is told about.
        assert_eq!(
            ids,
            vec!["workspace", "engine", "compose", "network", "projects", "bash"]
        );
        assert_eq!(result.ready, !result.requirements.iter().any(|r| r.state == State::Fail));
    }
}
