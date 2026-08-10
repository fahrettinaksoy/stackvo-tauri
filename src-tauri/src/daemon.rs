//! What an answer from the Docker daemon *means*.
//!
//! `engine.rs` calls bollard in twenty-four places, and the readiness review
//! filed that as "no abstraction, so 'what happens if the daemon returns this'
//! cannot be tested". True — but the interesting half of that sentence is
//! narrower than it looks, and taking it literally would have produced the
//! wrong thing.
//!
//! ## Why this is not a trait over bollard
//!
//! A `trait DockerApi` with twenty-four methods and a fake implementation is
//! the obvious reading. It would also be almost all passthrough: each method
//! forwards its arguments and returns what came back, so the tests written
//! against the fake would be testing that a forwarding function forwards.
//! Meanwhile the code that is genuinely hard to reach — and genuinely
//! consequential — is not the call, it is the **match arm after it**:
//!
//! ```text
//! Err(DockerResponseServerError { status_code: 304, .. }) => Ok(())
//! ```
//!
//! Six of those are scattered through `engine.rs`, each one deciding that a
//! particular HTTP status from the daemon means success, or absence, or a fault
//! worth raising. Every one of them requires a live daemon in a specific state
//! to exercise, which is exactly why none of them ever was. They are also the
//! ones where being wrong is silent: a status misread as success reports a
//! container stopped that is still running.
//!
//! So the seam is drawn around the *decision* rather than the transport. This
//! module is pure — no bollard type crosses its boundary, no I/O, no async —
//! and `engine.rs` keeps its direct calls and asks here what the answer meant.
//!
//! ## What "idempotent" means per action, and why it differs
//!
//! The same 404 is a failure for one action and a success for another, and the
//! difference is what the caller asked for:
//!
//!   * **start / stop / restart** — a 404 means the container is not there, and
//!     the caller asked for it to be *running* or *stopped*. It cannot be
//!     either. That is a genuine NOT_FOUND.
//!   * **remove** — a 404 means it is already gone, which is precisely what the
//!     caller asked for. Reporting that as an error would make deleting a
//!     project fail on its second attempt.
//!
//! And 409 is idempotent for images alone: it means another container still
//! holds the image, and leaving it is correct rather than a failure to report.
//! Two services on the same `mysql:8.0` is an ordinary arrangement.

use crate::error::{Code, Error};

/// What was asked of the daemon.
///
/// Carried as an enum rather than the `&str` `lifecycle_error` used, because
/// the whole point here is that the action decides how a status is read — and a
/// string that only ever reached a format argument could not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Inspect,
    ReadStats,
    RemoveContainer,
    RemoveImage,
    RemoveVolume,
    CreateNetwork,
}

impl Action {
    /// The verb for the message, in the words the old code used.
    pub fn verb(self) -> &'static str {
        match self {
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Restart => "restart",
            Action::Inspect => "inspect",
            Action::ReadStats => "read stats for",
            Action::RemoveContainer => "remove",
            Action::RemoveImage => "remove image",
            Action::RemoveVolume => "remove volume",
            Action::CreateNetwork => "create network",
        }
    }

    /// What kind of thing this acts on.
    ///
    /// Extracting the old `lifecycle_error` turned up a bug it had carried all
    /// along: it wrote `container {name}` into every NOT_FOUND, so a missing
    /// **volume** was reported as a missing container, and so was a network.
    /// The message named the wrong kind of object and the hint told the user to
    /// build a project that had nothing to do with it.
    pub fn subject_noun(self) -> &'static str {
        match self {
            Action::Start
            | Action::Stop
            | Action::Restart
            | Action::Inspect
            | Action::ReadStats
            | Action::RemoveContainer => "container",
            Action::RemoveImage => "image",
            Action::RemoveVolume => "volume",
            Action::CreateNetwork => "network",
        }
    }

    /// Does "it is not there" satisfy this request?
    ///
    /// Only for the removals. This is the single rule that used to be six
    /// separate match arms, and the one that a seventh call site would have had
    /// to guess at.
    pub fn absence_is_success(self) -> bool {
        matches!(
            self,
            Action::RemoveContainer | Action::RemoveImage | Action::RemoveVolume
        )
    }
}

/// What the app should do about an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The request is satisfied — either it happened, or it was already so.
    Satisfied,
    /// The subject does not exist and the caller needed it to.
    NotFound,
    /// Anything else. Deliberately the default for a status nobody has
    /// considered: guessing that an unrecognised code meant success is how a
    /// failed stop gets reported as a stopped container.
    Fault,
}

/// Classify a daemon response status for an action.
///
/// `None` is "the transport failed before any status existed" — a socket that
/// was not there, a connection that dropped. Never satisfied, whatever was
/// asked: nothing reached the daemon, so nothing is known about the subject.
pub fn classify(action: Action, status: Option<u16>) -> Verdict {
    match status {
        // 2xx. The daemon did it.
        Some(code) if (200..300).contains(&code) => Verdict::Satisfied,

        // 304 Not Modified — already in the requested state. Idempotent for
        // every action that can receive it.
        Some(304) => Verdict::Satisfied,

        Some(404) => {
            if action.absence_is_success() {
                Verdict::Satisfied
            } else {
                Verdict::NotFound
            }
        }

        // 409 Conflict on an image is "in use by another container", which is
        // not this caller's problem. On anything else it is a real conflict.
        Some(409) if action == Action::RemoveImage => Verdict::Satisfied,

        _ => Verdict::Fault,
    }
}

/// The typed error a [`Verdict::NotFound`] or [`Verdict::Fault`] becomes.
///
/// `subject` is the container, image or volume name; `cause` is whatever the
/// transport said, already rendered, so this module needs no bollard type.
pub fn error(action: Action, subject: &str, verdict: Verdict, cause: &str) -> Error {
    match verdict {
        // A caller that reached here with a satisfied verdict has a bug, but
        // producing a confusing error is worse than saying so.
        Verdict::Satisfied => Error::new(
            Code::EngineUnreachable,
            format!(
                "{} {subject}: reported as an error but the daemon was satisfied",
                action.verb()
            ),
        ),
        Verdict::NotFound => {
            let e = Error::not_found(format!("{} {subject}", action.subject_noun()));
            // The hint is about containers specifically — "this project may not
            // be built yet" explains a missing container and explains nothing
            // at all about a missing network.
            if action.subject_noun() == "container" {
                e.with_hint(crate::hints::PROJECT_MAY_NOT_BE_BUILT)
            } else {
                e
            }
        }
        Verdict::Fault => Error::new(
            Code::EngineUnreachable,
            format!("Cannot {} {subject}: {cause}", action.verb()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_ACTION: [Action; 9] = [
        Action::Start,
        Action::Stop,
        Action::Restart,
        Action::Inspect,
        Action::ReadStats,
        Action::RemoveContainer,
        Action::RemoveImage,
        Action::RemoveVolume,
        Action::CreateNetwork,
    ];

    /// The behaviour six inline match arms encoded, now stated once.
    #[test]
    fn already_in_the_requested_state_is_success_for_every_action() {
        for action in EVERY_ACTION {
            assert_eq!(
                classify(action, Some(304)),
                Verdict::Satisfied,
                "{action:?} on 304"
            );
        }
    }

    /// The rule that differs by action, which is the reason this is a function.
    #[test]
    fn a_missing_subject_is_success_only_when_removal_was_asked_for() {
        assert_eq!(
            classify(Action::RemoveContainer, Some(404)),
            Verdict::Satisfied
        );
        assert_eq!(classify(Action::RemoveImage, Some(404)), Verdict::Satisfied);
        assert_eq!(
            classify(Action::RemoveVolume, Some(404)),
            Verdict::Satisfied
        );

        // Asking for a container that does not exist to be running is not a
        // request that can be satisfied by its absence.
        assert_eq!(classify(Action::Start, Some(404)), Verdict::NotFound);
        assert_eq!(classify(Action::Stop, Some(404)), Verdict::NotFound);
        assert_eq!(classify(Action::Restart, Some(404)), Verdict::NotFound);
        assert_eq!(classify(Action::Inspect, Some(404)), Verdict::NotFound);
        assert_eq!(
            classify(Action::CreateNetwork, Some(404)),
            Verdict::NotFound
        );
    }

    /// The bug the extraction found.
    ///
    /// `lifecycle_error` wrote `container {name}` into every NOT_FOUND it
    /// produced, including the ones raised for a volume and a network. The
    /// message named the wrong kind of object, and the hint attached to it told
    /// the user to build a project that had nothing to do with the failure.
    #[test]
    fn a_missing_thing_is_named_as_the_kind_of_thing_it_is() {
        let volume = error(
            Action::RemoveVolume,
            "stackvo-data",
            Verdict::NotFound,
            "404",
        );
        assert!(
            volume.message.contains("volume stackvo-data"),
            "a missing volume is not a missing container: {}",
            volume.message
        );
        assert!(
            volume.hint.is_none(),
            "\"the project may not be built\" explains nothing about a volume"
        );

        let network = error(
            Action::CreateNetwork,
            "stackvo-net",
            Verdict::NotFound,
            "404",
        );
        assert!(
            network.message.contains("network stackvo-net"),
            "{}",
            network.message
        );

        // Containers keep both the noun and the hint.
        let container = error(Action::Start, "stackvo-shop", Verdict::NotFound, "404");
        assert!(container.message.contains("container stackvo-shop"));
        assert!(container.hint.is_some());
    }

    #[test]
    fn an_image_held_by_another_container_is_left_alone_rather_than_reported() {
        assert_eq!(classify(Action::RemoveImage, Some(409)), Verdict::Satisfied);

        // The same status anywhere else is a genuine conflict.
        for action in EVERY_ACTION.iter().filter(|a| **a != Action::RemoveImage) {
            assert_eq!(
                classify(*action, Some(409)),
                Verdict::Fault,
                "{action:?} on 409"
            );
        }
    }

    /// Nothing reached the daemon, so nothing is known.
    #[test]
    fn a_transport_failure_is_never_satisfied() {
        for action in EVERY_ACTION {
            assert_eq!(
                classify(action, None),
                Verdict::Fault,
                "{action:?} with no status"
            );
        }
    }

    /// The property that matters most, over every status a daemon could send.
    ///
    /// Exhaustive rather than sampled: there are only 65,536 of them, and a
    /// loop that covers the whole space beats a generator that covers some of
    /// it and calls the rest luck. The claim is the one a reader of
    /// `engine.rs` most needs to be able to rely on — **an unrecognised status
    /// is never read as success** — because the failure it prevents is silent:
    /// a stop that did not happen, reported as a stopped container.
    #[test]
    fn no_unlisted_status_is_ever_mistaken_for_success() {
        const KNOWN_SATISFYING: [u16; 2] = [304, 404];

        for action in EVERY_ACTION {
            for status in 0u16..=u16::MAX {
                let verdict = classify(action, Some(status));
                if verdict != Verdict::Satisfied {
                    continue;
                }

                let expected = (200..300).contains(&status)
                    || KNOWN_SATISFYING.contains(&status)
                    || (status == 409 && action == Action::RemoveImage);

                assert!(
                    expected,
                    "{action:?} read status {status} as success, and no rule says it should"
                );
            }
        }
    }

    /// Every action has a verb, and no two share one by accident.
    #[test]
    fn each_action_names_itself_distinctly() {
        let mut verbs: Vec<&str> = EVERY_ACTION.iter().map(|a| a.verb()).collect();
        verbs.sort_unstable();
        let count = verbs.len();
        verbs.dedup();
        assert_eq!(verbs.len(), count, "two actions share a verb: {verbs:?}");
    }

    #[test]
    fn a_not_found_carries_the_hint_that_explains_it() {
        let e = error(Action::Start, "stackvo-shop", Verdict::NotFound, "404");
        assert_eq!(e.code, Code::NotFound);
        assert!(
            e.hint.is_some(),
            "a missing container is usually an unbuilt one"
        );
    }

    #[test]
    fn a_fault_names_the_action_and_the_cause() {
        let e = error(
            Action::Stop,
            "stackvo-shop",
            Verdict::Fault,
            "connection reset",
        );
        assert_eq!(e.code, Code::EngineUnreachable);
        assert!(e.message.contains("stop"), "{}", e.message);
        assert!(e.message.contains("connection reset"), "{}", e.message);
    }
}

#[cfg(test)]
mod placement_tests {
    /// `engine.rs` asks this module what an answer meant, rather than deciding
    /// again.
    ///
    /// The decisions used to be six inline `match` arms on `status_code`, each
    /// written separately and none of them reachable from a test. Extracting
    /// them is only worth anything while they stay extracted: a seventh call
    /// site that pattern-matches a status inline is back where this started,
    /// and it would look completely ordinary in review.
    #[test]
    fn the_engine_reads_no_status_code_of_its_own() {
        let engine = include_str!("engine.rs");

        let offenders: Vec<String> = engine
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("status_code"))
            // `status_of` is the one adapter, and it is what this module is
            // fed by.
            .filter(|(_, line)| !line.contains("status_code, .."))
            .map(|(i, line)| format!("  line {}: {}", i + 1, line.trim()))
            .collect();

        assert!(
            offenders.is_empty(),
            "engine.rs classifies a daemon status without going through \
             `daemon::classify`:\n{}",
            offenders.join("\n")
        );
    }
}
