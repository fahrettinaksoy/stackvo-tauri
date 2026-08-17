//! Commands a project asks to have run when it starts, stops or is rebuilt.
//!
//! B-3. Every comparable tool has these and they are genuinely useful: after a
//! rebuild you want `composer install`, after a start you want `artisan
//! migrate`, and doing it by hand every time is how a stack that "works" is one
//! nobody trusts.
//!
//! They are also the single most dangerous feature in this application, and the
//! rest of this comment is mostly about that.
//!
//! ## The threat is a clone, not an attacker on the network
//!
//! A hook lives in `stackvo.json`, which lives in a repository. So the sequence
//! is: somebody clones a repository, opens it here, presses Start — and a list
//! of commands written by whoever wrote that repository runs. That is the same
//! shape as a malicious `package.json` `postinstall`, and it is worth naming
//! plainly because the feature is worth having anyway. What follows is how the
//! cost is paid rather than hidden.
//!
//! ## Two kinds of step, and they are not the same risk
//!
//! * [`Kind::Exec`] runs **inside the project's own container**. That container
//!   already runs the repository's code — its entrypoint, its dependencies, its
//!   application. A repository that can run a command in its own container has
//!   gained nothing it did not already have, so these need no gate beyond the
//!   feature being on.
//!
//! * [`Kind::Host`] runs **on the developer's machine**, with their files,
//!   their ssh agent and their credentials. This is the one that turns a clone
//!   into arbitrary code execution, and it is gated: see [`Consent`].
//!
//! Collapsing the two into "a hook" would have made the safe case pay for the
//! dangerous one, or — far worse — let the dangerous one ride on the safe one's
//! reputation.
//!
//! ## A step is an argv array. There is no shell.
//!
//! ```json
//! "hooks": {
//!   "post-start": [
//!     { "exec": ["php", "artisan", "migrate", "--force"] }
//!   ]
//! }
//! ```
//!
//! Everything in this codebase spawns an argv array and never a shell — see
//! [`crate::runner`] and [`crate::quickcmd`], where that rule *is* the security
//! model. A hook taking a command string would be the one place a shell came
//! back, and it would come back holding text from a repository somebody cloned.
//!
//! The cost is real and is not waved away: no `&&`, no pipes, no globbing, no
//! `$VAR`. The answer is that a step needing those is a script, and a script is
//! one argv element — `{"exec": ["sh", "scripts/seed.sh"]}`. That is the user
//! choosing a shell and naming the file it runs, which is a different act from
//! this app deciding that every hook is shell text.
//!
//! ## Three events, and why not more
//!
//! `post-build`, `post-start`, `pre-stop`. Each is a moment where "the
//! container just changed state and something has to happen" is a real
//! sentence.
//!
//! `pre-start` is deliberately absent. There is no container to run in before a
//! start, so every `pre-start` step would have to be a host step — the
//! dangerous kind — and offering a slot whose only possible occupant is the
//! dangerous kind is an invitation rather than a feature.
//!
//! ## Consent is to a list of commands, not to a project
//!
//! [`Consent`] records the digest of a project's **host** steps. Granting it
//! means "I have read these commands and they may run on my machine"; changing
//! a hook — or pulling a commit that changes one — changes the digest and asks
//! again. A per-project boolean would have meant reviewing a repository once
//! and then trusting whatever it grew afterwards, which is the property that
//! makes supply-chain attacks work.
//!
//! The record is written by this module and by nothing else. It is deliberately
//! not in `preferences.json`: that file is a free-form object the front end
//! writes through `prefs_set`, and a consent record any webview code could
//! forge is not a consent record.

use crate::error::{Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// When a hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    PostBuild,
    PostStart,
    PreStop,
}

impl Event {
    /// The spelling used in `stackvo.json`.
    pub fn key(self) -> &'static str {
        match self {
            Event::PostBuild => "post-build",
            Event::PostStart => "post-start",
            Event::PreStop => "pre-stop",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "post-build" => Some(Event::PostBuild),
            "post-start" => Some(Event::PostStart),
            "pre-stop" => Some(Event::PreStop),
            _ => None,
        }
    }

    pub const ALL: [Event; 3] = [Event::PostBuild, Event::PostStart, Event::PreStop];
}

/// Where a step runs, which is the whole of its risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Inside the project's container.
    Exec,
    /// On this machine.
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub kind: Kind,
    /// Program first, then arguments. Never passed to a shell.
    pub argv: Vec<String>,
}

impl Step {
    /// The command as one line, for a screen and for the digest.
    ///
    /// Display only — nothing parses this back. An argv that round-tripped
    /// through a string would be an argv that can be re-split, which is the
    /// property this whole module exists to not have.
    pub fn display(&self) -> String {
        self.argv.join(" ")
    }
}

/// A project's hooks, by event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Hooks {
    #[serde(flatten)]
    by_event: BTreeMap<String, Vec<Step>>,
}

impl Hooks {
    pub fn is_empty(&self) -> bool {
        self.by_event.values().all(|steps| steps.is_empty())
    }

    pub fn steps(&self, event: Event) -> &[Step] {
        self.by_event
            .get(event.key())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every host step in the project, in event order.
    ///
    /// The order is [`Event::ALL`]'s and not the file's, so two manifests that
    /// declare the same commands under the same events digest the same however
    /// the keys happen to be laid out.
    pub fn host_steps(&self) -> Vec<(Event, &Step)> {
        Event::ALL
            .iter()
            .flat_map(|event| {
                self.steps(*event)
                    .iter()
                    .filter(|step| step.kind == Kind::Host)
                    .map(move |step| (*event, step))
            })
            .collect()
    }

    /// What consent is granted against.
    ///
    /// Over the **host** steps alone. A container step needs no consent, so
    /// including it would invalidate a grant every time somebody edited a
    /// command that was never gated — asking a question whose answer cannot
    /// change anything, which is how a prompt becomes something people click
    /// through without reading.
    ///
    /// `None` when there are no host steps: there is nothing to consent to, and
    /// a digest of an empty list would be a constant that looks like a decision.
    pub fn host_digest(&self) -> Option<String> {
        let steps = self.host_steps();
        if steps.is_empty() {
            return None;
        }
        // Length-prefixed rather than joined by a separator: `["a b"]` and
        // `["a", "b"]` are different commands and must not hash the same, and
        // any separator is a byte one of them could contain.
        let mut buf = String::new();
        for (event, step) in steps {
            buf.push_str(event.key());
            buf.push('\u{1}');
            for arg in &step.argv {
                buf.push_str(&arg.len().to_string());
                buf.push(':');
                buf.push_str(arg);
            }
            buf.push('\u{2}');
        }
        Some(crate::pkg::sha256_hex(buf.as_bytes()))
    }
}

/// One problem found while reading a `hooks` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub message: String,
}

/// Read a `hooks` block, naming everything wrong with it.
///
/// Findings rather than a failure: a project whose hooks are malformed should
/// still open, still build and still run — it just runs without the step that
/// could not be read, and the reason is on screen. The alternative is a project
/// that will not load because of a typo in an optional convenience.
///
/// Absent and empty are the same thing and neither is a problem.
pub fn parse(json: &serde_json::Value) -> (Hooks, Vec<Problem>) {
    let mut hooks = Hooks::default();
    let mut problems = Vec::new();

    let Some(block) = json.get("hooks") else {
        return (hooks, problems);
    };
    let Some(map) = block.as_object() else {
        problems.push(Problem {
            path: "hooks".into(),
            message: "`hooks` must be an object keyed by event".into(),
        });
        return (hooks, problems);
    };

    for (key, value) in map {
        let Some(event) = Event::parse(key) else {
            problems.push(Problem {
                path: format!("hooks.{key}"),
                message: format!(
                    "\"{key}\" is not a hook event; the events are {}",
                    Event::ALL
                        .iter()
                        .map(|e| e.key())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
            continue;
        };

        let Some(list) = value.as_array() else {
            problems.push(Problem {
                path: format!("hooks.{key}"),
                message: format!("`hooks.{key}` must be a list of steps"),
            });
            continue;
        };

        let mut steps = Vec::new();
        for (index, item) in list.iter().enumerate() {
            let at = format!("hooks.{key}[{index}]");
            match step_of(item, &at) {
                Ok(step) => steps.push(step),
                Err(problem) => problems.push(problem),
            }
        }
        hooks.by_event.insert(event.key().to_string(), steps);
    }

    (hooks, problems)
}

fn step_of(item: &serde_json::Value, at: &str) -> std::result::Result<Step, Problem> {
    let bad = |message: String| Problem {
        path: at.to_string(),
        message,
    };

    let Some(object) = item.as_object() else {
        return Err(bad("a step is an object with `exec` or `host`".into()));
    };

    let kind = match (object.get("exec"), object.get("host")) {
        (Some(_), Some(_)) => return Err(bad(
            "a step declares `exec` or `host`, not both; where it runs is the whole of its risk"
                .into(),
        )),
        (Some(_), None) => Kind::Exec,
        (None, Some(_)) => Kind::Host,
        (None, None) => return Err(bad("a step needs `exec` or `host`".into())),
    };

    let raw = object
        .get(if kind == Kind::Exec { "exec" } else { "host" })
        .expect("matched just above");

    // A string is the shape people will reach for, so it gets its own message
    // rather than "expected array" — the reason it is refused is the point.
    let Some(list) = raw.as_array() else {
        return Err(bad(
            "a command is a list of arguments, not a string: nothing here runs through a shell, \
             so write [\"sh\", \"scripts/seed.sh\"] rather than a line of shell"
                .into(),
        ));
    };

    let mut argv = Vec::with_capacity(list.len());
    for arg in list {
        let Some(text) = arg.as_str() else {
            return Err(bad("every argument must be a string".into()));
        };
        argv.push(text.to_string());
    }

    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(bad("a command needs a program to run".into()));
    }

    Ok(Step { kind, argv })
}

// ------------------------------------------------------------------ consent

/// Which projects' host steps this machine has agreed to, by digest.
///
/// A file this module owns. See the note at the top for why it is not in
/// `preferences.json`.
#[derive(Debug, Clone, Default)]
pub struct Consent {
    granted: BTreeMap<String, String>,
}

impl Consent {
    /// Has this exact list of host commands been agreed to for this project?
    pub fn allows(&self, project: &str, digest: &str) -> bool {
        self.granted.get(project).map(String::as_str) == Some(digest)
    }

    pub fn grant(&mut self, project: &str, digest: &str) {
        self.granted.insert(project.to_string(), digest.to_string());
    }

    pub fn revoke(&mut self, project: &str) {
        self.granted.remove(project);
    }
}

/// Where the record lives, beside the other things this app owns.
pub fn consent_path() -> Option<PathBuf> {
    crate::appdir::config().map(|dir| dir.join("hook-consent.json"))
}

pub fn read_consent(path: &Path) -> Consent {
    // No file is "nothing has been agreed to", which is the correct starting
    // point and the safe one. An unreadable or malformed file gets the same
    // answer for the same reason: the failure mode of this parse must be
    // "asks again", never "assumes yes".
    let Ok(text) = std::fs::read_to_string(path) else {
        return Consent::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Consent::default();
    };
    let Some(map) = value.get("granted").and_then(|v| v.as_object()) else {
        return Consent::default();
    };

    let mut consent = Consent::default();
    for (project, digest) in map {
        if let Some(text) = digest.as_str() {
            consent.granted.insert(project.clone(), text.to_string());
        }
    }
    consent
}

pub fn write_consent(path: &Path, consent: &Consent) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io("creating the application directory", e))?;
    }
    let body = serde_json::json!({
        "schemaVersion": 1,
        "granted": consent.granted,
    });
    crate::atomic::write(
        path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        ),
    )
}

// --------------------------------------------------------------------- plan

/// Why a step will not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocked {
    /// An administrator turned hooks off entirely.
    PolicyOff,
    /// An administrator turned host steps off.
    PolicyHost,
    /// The commands have not been agreed to on this machine, or have changed.
    NeedsConsent,
}

/// What would run for one event, and what would not.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub event: Event,
    pub steps: Vec<PlannedStep>,
    /// The digest the consent screen would grant, if there is anything to grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStep {
    pub kind: Kind,
    pub command: String,
    /// `None` means it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<Blocked>,
}

impl Plan {
    /// The steps that would actually run, in order.
    pub fn runnable(&self) -> impl Iterator<Item = &PlannedStep> {
        self.steps.iter().filter(|step| step.blocked.is_none())
    }

    pub fn is_blocked(&self) -> bool {
        self.steps.iter().any(|step| step.blocked.is_some())
    }
}

/// Decide, for one event, what runs — before anything is spawned.
///
/// The same review-then-apply shape as `hosts_plan`/`hosts_apply` and
/// `preset::plan`/`apply`. A hook that is refused is *named* here rather than
/// silently skipped at run time, because "nothing happened and nothing said
/// why" is indistinguishable from a hook that ran and did nothing.
pub fn plan(
    project: &str,
    hooks: &Hooks,
    event: Event,
    policy: &crate::policy::Hooks,
    consent: &Consent,
) -> Plan {
    let digest = hooks.host_digest();
    let agreed = digest
        .as_deref()
        .is_some_and(|d| consent.allows(project, d));

    let steps = hooks
        .steps(event)
        .iter()
        .map(|step| PlannedStep {
            kind: step.kind,
            command: step.display(),
            blocked: if !policy.enabled {
                Some(Blocked::PolicyOff)
            } else if step.kind == Kind::Host && !policy.allow_host {
                Some(Blocked::PolicyHost)
            } else if step.kind == Kind::Host && !agreed {
                Some(Blocked::NeedsConsent)
            } else {
                None
            },
        })
        .collect();

    Plan {
        event,
        steps,
        digest,
    }
}

/// One event's worth of hooks, and everything needed to decide about them.
///
/// A struct rather than nine positional arguments, for the reason
/// [`crate::runner::Operation`] gives: at that count two adjacent `&str`
/// parameters swapped by mistake compile cleanly, and here the pair that would
/// swap silently is `project` and `container` — one names the consent record
/// and the other names what gets executed in.
pub struct Run<'a> {
    pub operation_id: &'a str,
    pub project: &'a str,
    /// The working directory a host step runs in: the project's own.
    pub dir: &'a Path,
    /// The container an `exec` step runs in.
    pub container: &'a str,
    pub hooks: &'a Hooks,
    pub event: Event,
    pub policy: &'a crate::policy::Hooks,
    pub consent: &'a Consent,
}

/// Run a planned event's steps, in order, stopping at the first failure.
///
/// Sequential and fail-fast, both deliberately. Hooks are ordered because
/// people write them as a procedure — install, then migrate, then seed — and a
/// concurrent runner would be a different feature wearing the same name. And a
/// step that fails leaves the ones after it unrun, because "migrate failed but
/// we seeded anyway" is worse than stopping.
///
/// The **failure does not fail the lifecycle operation**. A container that
/// started is started, and reporting the start as failed because a convenience
/// afterwards did not work would have people unable to tell which half broke.
/// The error is emitted and returned; the caller decides, and the callers here
/// log it and carry on.
pub async fn run(sink: &dyn crate::progress::ProgressSink, run: Run<'_>) -> Result<()> {
    let Run {
        operation_id,
        project,
        dir,
        container,
        hooks,
        event,
        policy,
        consent,
    } = run;
    let plan = plan(project, hooks, event, policy, consent);
    if plan.steps.is_empty() {
        return Ok(());
    }

    // Zipped with the real steps rather than driven by the plan alone: a
    // `PlannedStep` carries the command as a display string because it crosses
    // to a screen, and spawning from that would mean splitting it again on
    // spaces — re-introducing exactly the re-splitting this module refuses to
    // do anywhere else, and getting an argument with a space in it wrong.
    for (step, decision) in hooks.steps(event).iter().zip(plan.steps.iter()) {
        if let Some(reason) = decision.blocked {
            crate::progress::emit(
                sink,
                "hook:progress",
                crate::events::ProgressEvent {
                    operation_id: operation_id.to_string(),
                    subject: project.to_string(),
                    line: format!(
                        "skipped ({}): {}",
                        match reason {
                            Blocked::PolicyOff => "hooks are turned off by policy",
                            Blocked::PolicyHost => "host commands are turned off by policy",
                            Blocked::NeedsConsent =>
                                "these commands have not been approved on this machine",
                        },
                        decision.command
                    ),
                },
            );
            continue;
        }

        // `docker exec`, not the bollard API: the streaming here is the same
        // streaming every other operation in this app uses, and a second output
        // path would report a hook's lines differently from a build's for no
        // gain.
        let (program, args) = match step.kind {
            Kind::Exec => {
                let mut args = vec!["exec".to_string(), container.to_string()];
                args.extend(step.argv.iter().cloned());
                ("docker", args)
            }
            Kind::Host => (step.argv[0].as_str(), step.argv[1..].to_vec()),
        };

        crate::runner::run_operation(
            sink,
            crate::runner::Operation {
                operation_id,
                subject: project,
                progress_event: "hook:progress",
                finished_event: "hook:done",
                program,
                args: &args,
                cwd: dir,
                env: &[],
            },
        )
        .await?;
    }

    Ok(())
}

/// One project's hooks for one event: read the manifest, resolve consent and
/// policy, run what is allowed.
///
/// This was the body of `commands::run_hooks`, which needed an `AppHandle` for
/// one reason — building the sink to report into. Taking the sink instead is
/// ADR 0001's rule applied to the last thing in the lifecycle path that still
/// named a Tauri type, and it is what lets `stackvo start` and the start button
/// run the same hooks rather than two things wearing one name.
///
/// **Never fails the caller.** A manifest that will not parse, a project
/// directory that is not there, a step that exits non-zero — all of them are
/// logged and swallowed here, because [`run`] has already made the argument:
/// a container that started is started, and a convenience that failed
/// afterwards must not be reported as the start failing.
pub async fn run_for_project(
    sink: &dyn crate::progress::ProgressSink,
    root: &Path,
    name: &str,
    event: Event,
) {
    let Ok(dir) = crate::workspace::project_dir(root, name) else {
        return;
    };

    // The effective manifest, deliberately: `stackvo.local.json` may override
    // hooks the same way it overrides anything else, and a machine that wants
    // a different post-start step is exactly the case B-2 exists for.
    let Ok(manifest) = crate::manifest::read(&dir.join("stackvo.json"), name) else {
        return;
    };
    if manifest.hooks.is_empty() {
        return;
    }

    let consent = consent_path()
        .map(|path| read_consent(&path))
        .unwrap_or_default();

    let operation_id = crate::events::next_operation_id("hook");
    if let Err(e) = run(
        sink,
        Run {
            operation_id: &operation_id,
            project: name,
            dir: &dir,
            container: &format!("stackvo-{}", name.to_ascii_lowercase()),
            hooks: &manifest.hooks,
            event,
            policy: crate::policy::current().hooks(),
            consent: &consent,
        },
    )
    .await
    {
        tracing::warn!(project = name, event = event.key(), error = %e.message, "a hook failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooks_of(text: &str) -> (Hooks, Vec<Problem>) {
        parse(&serde_json::from_str(text).unwrap())
    }

    #[test]
    fn a_manifest_with_no_hooks_has_none_and_that_is_not_a_problem() {
        let (hooks, problems) = hooks_of(r#"{"name":"shop"}"#);
        assert!(hooks.is_empty());
        assert!(problems.is_empty());
    }

    #[test]
    fn steps_are_read_with_the_place_they_run() {
        let (hooks, problems) = hooks_of(
            r#"{"hooks":{"post-start":[
                 {"exec":["php","artisan","migrate"]},
                 {"host":["say","done"]}
               ]}}"#,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let steps = hooks.steps(Event::PostStart);
        assert_eq!(steps[0].kind, Kind::Exec);
        assert_eq!(steps[0].argv, vec!["php", "artisan", "migrate"]);
        assert_eq!(steps[1].kind, Kind::Host);
    }

    /// The message has to explain the rule, not restate the type error — this
    /// is the shape everybody will try first.
    #[test]
    fn a_command_written_as_shell_is_refused_with_the_reason() {
        let (_, problems) = hooks_of(r#"{"hooks":{"post-start":[{"exec":"composer install"}]}}"#);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("shell"), "{problems:?}");
    }

    #[test]
    fn a_step_that_is_both_kinds_is_refused() {
        let (_, problems) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"],"host":["b"]}]}}"#);
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn an_unknown_event_is_named_rather_than_ignored() {
        let (_, problems) = hooks_of(r#"{"hooks":{"pre-start":[{"host":["x"]}]}}"#);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("post-start"), "{problems:?}");
    }

    /// A bad step must not take the good ones with it — the project still runs.
    #[test]
    fn one_bad_step_does_not_discard_the_rest() {
        let (hooks, problems) =
            hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]},{"exec":42},{"exec":["b"]}]}}"#);
        assert_eq!(problems.len(), 1);
        assert_eq!(hooks.steps(Event::PostStart).len(), 2);
    }

    // ---- the digest ------------------------------------------------------

    #[test]
    fn a_project_with_no_host_steps_has_nothing_to_consent_to() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]}]}}"#);
        assert_eq!(hooks.host_digest(), None);
    }

    /// The property that makes consent worth anything: agreeing once must not
    /// agree to whatever the repository grows next.
    #[test]
    fn changing_a_host_command_changes_the_digest() {
        let (before, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["say","hi"]}]}}"#);
        let (after, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["say","bye"]}]}}"#);
        assert_ne!(before.host_digest(), after.host_digest());
    }

    /// Container steps are not gated, so editing one must not invalidate a
    /// grant — a prompt that appears for a change it cannot affect is a prompt
    /// people learn to click through.
    #[test]
    fn changing_a_container_command_leaves_the_digest_alone() {
        let (before, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]},{"host":["h"]}]}}"#);
        let (after, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["b"]},{"host":["h"]}]}}"#);
        assert_eq!(before.host_digest(), after.host_digest());
    }

    /// `["a b"]` and `["a", "b"]` are different commands. A digest that joined
    /// on a separator would agree they are the same.
    #[test]
    fn argument_boundaries_are_part_of_the_digest() {
        let (joined, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["a b"]}]}}"#);
        let (split, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["a","b"]}]}}"#);
        assert_ne!(joined.host_digest(), split.host_digest());
    }

    // ---- the gate --------------------------------------------------------

    fn policy(enabled: bool, allow_host: bool) -> crate::policy::Hooks {
        crate::policy::Hooks {
            enabled,
            allow_host,
        }
    }

    #[test]
    fn a_container_step_runs_without_any_consent() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]}]}}"#);
        let p = plan(
            "shop",
            &hooks,
            Event::PostStart,
            &policy(true, true),
            &Consent::default(),
        );
        assert_eq!(p.steps[0].blocked, None);
    }

    #[test]
    fn a_host_step_waits_for_consent_and_then_runs() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["say","hi"]}]}}"#);
        let mut consent = Consent::default();

        let before = plan(
            "shop",
            &hooks,
            Event::PostStart,
            &policy(true, true),
            &consent,
        );
        assert_eq!(before.steps[0].blocked, Some(Blocked::NeedsConsent));

        consent.grant("shop", before.digest.as_deref().unwrap());
        let after = plan(
            "shop",
            &hooks,
            Event::PostStart,
            &policy(true, true),
            &consent,
        );
        assert_eq!(after.steps[0].blocked, None);
    }

    /// Consent to one project is not consent to another with the same commands.
    #[test]
    fn consent_is_per_project() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"host":["say","hi"]}]}}"#);
        let mut consent = Consent::default();
        consent.grant("shop", &hooks.host_digest().unwrap());

        let other = plan(
            "blog",
            &hooks,
            Event::PostStart,
            &policy(true, true),
            &consent,
        );
        assert_eq!(other.steps[0].blocked, Some(Blocked::NeedsConsent));
    }

    #[test]
    fn policy_can_turn_host_steps_off_without_turning_hooks_off() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]},{"host":["b"]}]}}"#);
        let mut consent = Consent::default();
        consent.grant("shop", &hooks.host_digest().unwrap());

        let p = plan(
            "shop",
            &hooks,
            Event::PostStart,
            &policy(true, false),
            &consent,
        );
        assert_eq!(p.steps[0].blocked, None);
        assert_eq!(p.steps[1].blocked, Some(Blocked::PolicyHost));
    }

    #[test]
    fn policy_off_blocks_the_container_steps_too() {
        let (hooks, _) = hooks_of(r#"{"hooks":{"post-start":[{"exec":["a"]}]}}"#);
        let p = plan(
            "shop",
            &hooks,
            Event::PostStart,
            &policy(false, true),
            &Consent::default(),
        );
        assert_eq!(p.steps[0].blocked, Some(Blocked::PolicyOff));
    }

    // ---- the record ------------------------------------------------------

    /// The failure mode of reading this file must be "asks again", never
    /// "assumes yes".
    #[test]
    fn an_unreadable_consent_file_grants_nothing() {
        let dir = std::env::temp_dir().join(format!("stackvo-consent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hook-consent.json");

        std::fs::write(&path, "{ not json").unwrap();
        assert!(!read_consent(&path).allows("shop", "abc"));

        std::fs::write(&path, "[]").unwrap();
        assert!(!read_consent(&path).allows("shop", "abc"));

        assert!(!read_consent(&dir.join("absent.json")).allows("shop", "abc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_grant_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("stackvo-consent-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hook-consent.json");

        let mut consent = Consent::default();
        consent.grant("shop", "deadbeef");
        write_consent(&path, &consent).unwrap();

        let read = read_consent(&path);
        assert!(read.allows("shop", "deadbeef"));
        assert!(
            !read.allows("shop", "other"),
            "a changed list must ask again"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
