//! Where an operation's play-by-play goes — with no idea what a window is.
//!
//! **There is no `use tauri::` in this file, and that is the point.** It is the
//! first piece of the split the readiness review called the highest-return
//! change in the project: business logic that reports progress should depend on
//! *the idea of reporting progress*, not on `AppHandle`.
//!
//! ## What was already right
//!
//! [`crate::events::Sink`] got the hard part right before this module existed:
//! it observed that events are only ever progress reporting, and that a caller
//! with no window loses nothing by dropping them because `run_operation` awaits
//! the process and returns a `Result`. That is why `stackvo-mcp` can drive
//! `stack_up` at all.
//!
//! ## What it could not do
//!
//! It was an enum with two variants — a window, or silence — so there was no
//! third answer, and the third answer is the one tests need. `run_operation` is
//! the function every long operation in this app funnels through: eleven
//! commands, every compose run, every build, every clone. It had **no tests at
//! all**, not because nobody tried but because there was no way to observe what
//! it emitted. `Sink::App` needs a running Tauri app; `Sink::Headless` throws
//! everything away.
//!
//! So the abstraction moves from an enum to this trait, and gains
//! [`Recording`] — an implementation whose whole job is to be read back in an
//! assertion. `Sink` stays exactly as it was and implements the trait, so the
//! desktop path is unchanged and the MCP path is unchanged.
//!
//! ## Why the payload is `serde_json::Value`
//!
//! A `dyn`-compatible trait cannot have a generic method, and `&dyn
//! ProgressSink` is what lets one function serve a window, a stdio server and a
//! test without being generic over all three. `Value` is the honest common
//! type: every one of these payloads was on its way to being JSON for the
//! webview regardless, so nothing is lost in translation — and it is what makes
//! [`Recording`] able to assert on a field rather than on a debug string.

use serde::Serialize;
use serde_json::Value;

/// Somewhere to report progress to.
///
/// `Send + Sync` because the sink is shared into the line callback that
/// `runner::stream_with_env` drives across await points.
pub trait ProgressSink: Send + Sync {
    /// One event. Never fails: a sink that cannot deliver — a closed window, a
    /// client that hung up — is not a reason to fail the operation that was
    /// reporting it. That rule predates this trait; it is why
    /// [`crate::events::emit`] has always swallowed its error.
    fn event(&self, name: &str, payload: Value);
}

/// Emit a typed payload, keeping call sites reading the way they always have.
///
/// The `Value` conversion lives here rather than at each call site so that a
/// payload struct is still what a reader sees at the point the event is sent.
/// A struct that cannot serialise emits `null` rather than vanishing: an event
/// that arrives empty is debuggable, an event that silently never happened is
/// the bug this whole module exists to make visible.
pub fn emit<P: Serialize>(sink: &dyn ProgressSink, name: &str, payload: P) {
    sink.event(name, serde_json::to_value(payload).unwrap_or(Value::Null));
}

/// Drops everything.
///
/// For callers with nobody to report to — the MCP server, `examples/diagnose`,
/// and any test that cares about an operation's `Result` rather than its
/// narration.
pub struct Null;

impl ProgressSink for Null {
    fn event(&self, _name: &str, _payload: Value) {}
}

/// One event, as it was emitted.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub name: String,
    pub payload: Value,
}

impl Recorded {
    /// A field of the payload, for the common case of asserting on one thing.
    ///
    /// Returns `None` for a payload that is not an object or has no such key,
    /// which an assertion reads as a failure rather than a panic — a test that
    /// blows up inside its own helper reports the wrong problem.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.payload.get(key)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }
}

/// Keeps every event, so a test can say what an operation reported.
///
/// This is the implementation that did not exist, and its absence is the reason
/// `run_operation` — the funnel for every long-running command in the app — was
/// never covered by anything.
#[derive(Default)]
pub struct Recording {
    // Poison-tolerant on read, deliberately. A panicking test elsewhere in the
    // process must not turn this into a second, unrelated failure; the events
    // collected before the panic are still exactly what was emitted.
    events: std::sync::Mutex<Vec<Recorded>>,
}

impl Recording {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything emitted so far, in order.
    pub fn events(&self) -> Vec<Recorded> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Just the event names, which is what most assertions are about — that a
    /// sequence happened, and in which order.
    pub fn names(&self) -> Vec<String> {
        self.events().into_iter().map(|e| e.name).collect()
    }

    /// Every event with this name.
    pub fn named(&self, name: &str) -> Vec<Recorded> {
        self.events()
            .into_iter()
            .filter(|e| e.name == name)
            .collect()
    }

    /// The last event with this name — usually the terminal one.
    pub fn last(&self, name: &str) -> Option<Recorded> {
        self.named(name).pop()
    }

    pub fn is_empty(&self) -> bool {
        self.events().is_empty()
    }
}

impl ProgressSink for Recording {
    fn event(&self, name: &str, payload: Value) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Recorded {
                name: name.to_string(),
                payload,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        operation_id: &'static str,
        line: &'static str,
    }

    #[test]
    fn a_recording_keeps_order_and_payloads() {
        let sink = Recording::new();
        assert!(sink.is_empty());

        emit(
            &sink,
            "generate:progress",
            Payload {
                operation_id: "gen-1",
                line: "step one",
            },
        );
        emit(
            &sink,
            "generate:progress",
            Payload {
                operation_id: "gen-1",
                line: "step two",
            },
        );
        emit(
            &sink,
            "generate:finished",
            serde_json::json!({ "success": true }),
        );

        assert_eq!(
            sink.names(),
            [
                "generate:progress",
                "generate:progress",
                "generate:finished"
            ]
        );
        assert_eq!(sink.named("generate:progress").len(), 2);
        assert_eq!(
            sink.last("generate:progress").unwrap().str("line"),
            Some("step two")
        );
        assert_eq!(
            sink.last("generate:finished").unwrap().get("success"),
            Some(&Value::Bool(true))
        );
    }

    /// The serde attributes on the payload structs have to survive the trip
    /// through `Value`, or a test would be asserting on field names the webview
    /// never sees.
    #[test]
    fn serde_renaming_survives_the_value_conversion() {
        let sink = Recording::new();
        emit(
            &sink,
            "x",
            Payload {
                operation_id: "gen-1",
                line: "l",
            },
        );

        let event = sink.last("x").unwrap();
        assert_eq!(
            event.str("operationId"),
            Some("gen-1"),
            "camelCase renaming was lost: {:?}",
            event.payload
        );
        assert!(event.get("operation_id").is_none());
    }

    #[test]
    fn the_null_sink_accepts_everything_and_keeps_nothing() {
        let sink = Null;
        emit(&sink, "anything", serde_json::json!({ "a": 1 }));
        // Nothing to assert but the absence of a panic — which is the contract:
        // a caller with no window must be able to run the same code path.
    }

    /// The reason the trait exists: **one** function body, more than one
    /// destination, no generics. `run_operation` is this shape, and if this
    /// stops compiling the abstraction has been lost.
    #[test]
    fn one_function_serves_every_destination() {
        fn report(sink: &dyn ProgressSink) {
            emit(sink, "step", serde_json::json!({ "n": 1 }));
        }

        let recording = Recording::new();
        report(&recording);
        report(&Null);

        assert_eq!(
            recording.names(),
            ["step"],
            "the recording sink saw its own call and only its own"
        );
    }
}
