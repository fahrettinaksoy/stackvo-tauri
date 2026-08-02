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

/// The domains, as one line of detail — or None when nothing is missing.
///
/// Capped because this ends up on a single row of a card sized for a sentence,
/// and a workspace with thirty projects has thirty domains to name.
fn summarise(missing: &[String]) -> Option<String> {
    const SHOWN: usize = 4;
    if missing.is_empty() {
        return None;
    }
    let mut text = missing
        .iter()
        .take(SHOWN)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(rest) = missing.len().checked_sub(SHOWN).filter(|n| *n > 0) {
        text.push_str(&format!(" (+{rest})"));
    }
    Some(text)
}

pub async fn run() -> Preflight {
    let ws = workspace::resolve();
    let root = ws.root.as_ref().map(Path::new).filter(|_| ws.valid);

    let mut out = Vec::new();

    // ---- somewhere to keep the projects ------------------------------------
    //
    // This used to ask for the app's own directory, which was never a question
    // the person answering had any information about: the templates ship in the
    // binary, so the folder they were being asked to find was one the app was
    // about to create. It derives that now, and asks the only thing it cannot
    // work out — where the user keeps their code.
    out.push(Requirement {
        id: "workspace",
        state: if ws.valid { State::Ok } else { State::Fail },
        detail: ws.projects_dir.clone(),
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

    // A `projects` requirement used to sit here, checking that
    // `<root>/projects` was a directory. With the project tree chosen rather
    // than derived, that is the same fact as the `workspace` row above, stated
    // twice — and a gate that lists one problem as two teaches people to skim
    // it. The directory is created when it is chosen, so there is nothing left
    // for a second row to catch.

    // ---- the names a browser has to resolve --------------------------------
    //
    // A warning rather than a failure, for the same reason as mkcert below: the
    // stack comes up either way, it is only unreachable by name. And the repair
    // asks for an administrator password — putting a system auth prompt between
    // a new user and the app opening at all is a worse first run than two
    // missing lines.
    //
    // The list is `missing_hosts`, the same one the dashboard banner and the
    // project pages already use. A second idea of "which domains belong in the
    // file" is exactly how `stackvo.loc` and `traefik.<suffix>` went unoffered:
    // the retired Bash CLI wrote those two lines itself, so every checkout that
    // came through it had them and no code path ever had to. A workspace this
    // app creates does not.
    let missing = match root {
        Some(r) => crate::commands::missing_hosts_by_owner(r).await,
        None => Default::default(),
    };
    // Core first, so the two lines the whole thing is addressed through are
    // what the row names.
    let all: Vec<String> = missing
        .core
        .iter()
        .chain(missing.rest.iter())
        .cloned()
        .collect();

    out.push(Requirement {
        id: "hosts",
        state: match root {
            // No workspace means no `.env`, so not even the domain suffix is
            // known — "nothing is missing" would be an answer we do not have.
            None => State::Unknown,
            // `<suffix>` and `traefik.<suffix>` are a failure, not a warning.
            // They were a warning, and the consequence was the thing this row
            // exists to prevent: with everything else green the gate closed
            // over it, and an install came up with neither name in the file —
            // a numbered step that vanished without being done.
            Some(_) if !missing.core.is_empty() => State::Fail,
            // Everything else is one thing being unreachable rather than the
            // stack being unreachable. Holding the whole app shut over
            // phpMyAdmin, or over a project nobody has opened yet, is the same
            // mistake pointing the other way.
            Some(_) if !missing.rest.is_empty() => State::Warn,
            Some(_) => State::Ok,
        },
        detail: summarise(&all),
        fixable: !all.is_empty(),
    });

    // bash was a requirement here until the generator takeover: the app spawned
    // `core/cli/stackvo.sh` for every generate. The Rust generator writes the
    // files itself, certificates call mkcert directly, and nothing else on the
    // host is a shell script — so the requirement is gone, and with it the WSL
    // story on Windows.

    // ---- mkcert, for trusted HTTPS -----------------------------------------
    //
    // A warning rather than a failure: without mkcert the stack still runs, it
    // just serves a certificate nothing trusts, and a browser warning on every
    // project is a degraded state rather than a broken one. It is listed at all
    // because it was previously invisible — `SSL_ENABLE=true` is the default,
    // every generated Traefik router points at the `websecure` entry point, and
    // the only signal that the certificate behind it was never issued was the
    // browser refusing to open the site.
    //
    // Only reported when SSL is actually on: telling someone they are missing a
    // tool they have no use for is how a preflight gate trains people to ignore
    // it.
    let ssl_on = root
        .and_then(|r| crate::config::Env::load(r).ok())
        .is_some_and(|env| env.bool("SSL_ENABLE"));

    if ssl_on {
        let mkcert = crate::certs::mkcert().await;
        out.push(Requirement {
            id: "mkcert",
            state: if mkcert.available {
                State::Ok
            } else {
                State::Warn
            },
            detail: mkcert.version,
            fixable: false,
        });
    }

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
        // The UI reaches engine start through `engine_start` (it polls for the
        // daemon coming up); this arm exists so headless callers of the fix
        // surface — the diagnose example, an MCP client — get the same repair.
        "engine" => engine::start(),
        // Likewise the UI: it opens the same review dialog every other hosts
        // write goes through, because this one rewrites a system file and the
        // rule everywhere else is that the diff is shown before the password is
        // asked for. This arm is the headless equivalent.
        "hosts" => {
            let root = ws.require_root()?;
            // The two this row blocks on, and nothing else — the same set the
            // button in the UI writes. Repairing a requirement means making
            // that requirement true, not writing every line the file could
            // eventually want.
            let missing = crate::commands::missing_hosts_by_owner(&root).await.core;
            if missing.is_empty() {
                return Ok(());
            }
            crate::hosts::apply(&missing, &[]).map(|_| ())
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

    #[test]
    fn the_domain_summary_names_a_few_and_counts_the_rest() {
        assert_eq!(summarise(&[]), None);
        assert_eq!(
            summarise(&["stackvo.loc".into(), "traefik.stackvo.loc".into()]),
            Some("stackvo.loc, traefik.stackvo.loc".to_string())
        );

        let many: Vec<String> = (0..7).map(|i| format!("p{i}.loc")).collect();
        assert_eq!(
            summarise(&many),
            Some("p0.loc, p1.loc, p2.loc, p3.loc (+3)".to_string())
        );

        // Exactly at the cap adds no counter — "(+0)" is noise that reads as a
        // truncation that did not happen.
        let four: Vec<String> = (0..4).map(|i| format!("p{i}.loc")).collect();
        assert_eq!(
            summarise(&four),
            Some("p0.loc, p1.loc, p2.loc, p3.loc".to_string())
        );
    }

    /// The rule that a numbered step cannot quietly stop being one — and the
    /// exact set it applies to.
    ///
    /// `ready` is "nothing failed", so a warning does not hold the gate. That
    /// is right for mkcert and was wrong for the two names the stack is
    /// addressed through: with the workspace chosen and Docker up, the screen
    /// closed with `stackvo.loc` and `traefik.<suffix>` still absent, having
    /// listed them as step five a moment earlier.
    ///
    /// The first fix over-corrected and blocked on every enabled service's
    /// admin UI as well, which would have held the whole app shut over
    /// phpMyAdmin. Two names, and only those two.
    #[test]
    fn only_the_two_names_the_stack_is_addressed_through_hold_the_gate() {
        use crate::commands::MissingHosts;

        // The shape of the decision, stated where it can be read: this mirrors
        // the match in `run`, which cannot be called here — it resolves the
        // real workspace and reads the real hosts file.
        let state = |m: &MissingHosts| {
            if !m.core.is_empty() {
                State::Fail
            } else if !m.rest.is_empty() {
                State::Warn
            } else {
                State::Ok
            }
        };

        assert_eq!(state(&MissingHosts::default()), State::Ok);

        // A service's admin UI and a project's domain are the same kind of
        // thing here: something specific is unreachable, not the stack.
        let rest_only = MissingHosts {
            core: vec![],
            rest: vec![
                "phpmyadmin.stackvo.loc".into(),
                "rabbitmq.stackvo.loc".into(),
                "shop.stackvo.loc".into(),
            ],
        };
        assert_eq!(
            state(&rest_only),
            State::Warn,
            "phpMyAdmin must not hold the app shut"
        );

        let core_missing = MissingHosts {
            core: vec!["stackvo.loc".into(), "traefik.stackvo.loc".into()],
            rest: vec![],
        };
        assert_eq!(state(&core_missing), State::Fail);

        // And a core name failing outranks the rest warning, rather than the
        // first-listed winning.
        let both = MissingHosts {
            core: vec!["stackvo.loc".into()],
            rest: vec!["phpmyadmin.stackvo.loc".into()],
        };
        assert_eq!(state(&both), State::Fail);
    }

    /// The blocking set is two names, and neither of them can be switched off.
    ///
    /// Asserted on the function rather than on the match above, because the
    /// over-correction was not in the match — it was in what got handed to it.
    #[test]
    fn the_core_domains_are_the_suffix_and_the_proxy() {
        let dir = std::env::temp_dir().join(format!("stackvo-core-domains-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Every service on, so anything that leaks in from the catalogue shows
        // up here rather than in somebody's install.
        std::fs::write(
            dir.join(".env"),
            "DEFAULT_TLD_SUFFIX=example.test\n\
             SERVICE_PHPMYADMIN_ENABLE=true\n\
             SERVICE_RABBITMQ_ENABLE=true\n\
             SERVICE_MAILPIT_ENABLE=true\n",
        )
        .unwrap();

        assert_eq!(
            crate::commands::core_domains_for_test(&dir),
            vec![
                "example.test".to_string(),
                "traefik.example.test".to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fresh install has exactly two names to ask for.
    ///
    /// phpMyAdmin and RabbitMQ ship with `SERVICE_*_ENABLE=true`, so the first
    /// thing a new user saw was the dashboard reporting two missing hosts
    /// entries — for two containers that had never been created. The question
    /// is whether the thing exists, not whether it is listed in a profile.
    ///
    /// Nothing here starts Docker, and that is the point: `stackvo_containers`
    /// fails, nothing is running, and the answer has to be the two core names
    /// and no more.
    #[tokio::test]
    async fn a_fresh_install_asks_for_the_two_core_names_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("stackvo-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        crate::workspace::point_at_projects(&dir, &dir.join("projects")).unwrap();

        // The shipped defaults, spelled out: on, and never started.
        std::fs::write(
            dir.join(".env"),
            "DEFAULT_TLD_SUFFIX=example.test\n\
             SERVICE_PHPMYADMIN_ENABLE=true\n\
             SERVICE_RABBITMQ_ENABLE=true\n",
        )
        .unwrap();

        let missing = crate::commands::missing_hosts_by_owner(&dir).await;
        assert_eq!(
            missing.core,
            vec![
                "example.test".to_string(),
                "traefik.example.test".to_string()
            ]
        );
        assert!(
            missing.rest.is_empty(),
            "a service nobody has started is not a missing address: {:?}",
            missing.rest
        );

        // And the settings pane sees the same thing. It is a separate screen
        // reading a separate function, and while that function had its own
        // wider idea of "wanted" it went on listing phpMyAdmin and RabbitMQ
        // after the dashboard had stopped — the same complaint, a third time,
        // because the fix had been applied to consumers rather than to the
        // definition.
        let listed = crate::commands::wanted_domains_for_test(&dir).await;
        assert_eq!(
            listed,
            vec![
                "example.test".to_string(),
                "traefik.example.test".to_string()
            ],
            "the settings pane lists names for services that have never run"
        );

        let _ = std::fs::remove_dir_all(&dir);
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
        // prerequisite nobody is told about. bash left the list with the
        // generator takeover — nothing on the host is a shell script now.
        const ALWAYS: [&str; 5] = ["workspace", "engine", "compose", "network", "hosts"];
        assert_eq!(&ids[..ALWAYS.len()], &ALWAYS);

        // mkcert is reported only when SSL_ENABLE is on, so the tail varies
        // with the checkout this runs against — and it is the only thing
        // allowed to vary. Asserting a fixed list again would make the test
        // pass or fail on a setting rather than on the code.
        assert!(
            ids[ALWAYS.len()..].iter().all(|id| *id == "mkcert"),
            "unexpected requirement in {ids:?}"
        );
        assert_eq!(
            ids.len(),
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            "a requirement reported twice: {ids:?}"
        );

        // Warnings do not hold the app back. mkcert is the first requirement
        // that can be absent on a machine where everything else is fine, so
        // this rule now has something to protect.
        assert_eq!(
            result.ready,
            !result.requirements.iter().any(|r| r.state == State::Fail)
        );
    }
}
