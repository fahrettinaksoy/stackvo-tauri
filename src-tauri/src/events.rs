//! Typed event emission — see `contracts/ipc.json` → `events`.
//!
//! Long-running work does not block its `invoke()`. A command returns an
//! `OperationId` immediately and reports progress through these events. That is
//! what replaces the web UI's approach of holding an HTTP request open for up
//! to ten minutes (`proxy_read_timeout 600s` in its nginx config) while a
//! Docker build ran.
//!
//! Event names are preserved from Socket.io so the ported listeners keep
//! working, with one deliberate change: the terminal events move from
//! `terminal-*` to `terminal:*` for consistency.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// Monotonic within a process run. Enough to correlate an operation's events
/// with the call that started it; not meant to be globally unique or durable.
static COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_operation_id(prefix: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{n}")
}

/// Emit and swallow. A failed emit means the window is gone, which is not a
/// reason to fail the operation that was reporting progress.
pub fn emit<P: Serialize + Clone>(app: &AppHandle, event: &str, payload: P) {
    let _ = app.emit(event, payload);
}

/// Where an operation's progress goes.
///
/// Operations were welded to `AppHandle`, which made every one of them
/// unreachable from anything without a window — the MCP stdio server above
/// all. The split is the observation that events were only ever *progress
/// reporting*: `run_operation` awaits the process to completion and returns a
/// `Result`, so a caller with no window loses nothing by dropping the
/// play-by-play — it reads the outcome from the return value, exactly as an
/// MCP client wants to.
#[derive(Clone)]
pub enum Sink {
    /// The desktop window: progress reaches the webview as before.
    App(AppHandle),
    /// No window — the MCP server or a headless example. Progress is dropped;
    /// the operation's `Result` is the report.
    Headless,
}

impl Sink {
    pub fn emit<P: Serialize + Clone>(&self, event: &str, payload: P) {
        if let Sink::App(app) = self {
            let _ = app.emit(event, payload);
        }
    }
}

/// The window sink, seen as the Tauri-free abstraction in [`crate::progress`].
///
/// This impl is the whole seam. Everything below `commands.rs` now asks for a
/// `&dyn ProgressSink`, so it compiles without Tauri in scope and can be handed
/// a `Recording` by a test — while the desktop app keeps handing it this, and
/// the webview receives byte-identical payloads.
///
/// `to_value` in `progress::emit` and Tauri's own serialisation produce the same
/// JSON for these payload structs, `#[serde(rename_all)]` and
/// `skip_serializing_if` included — pinned by a test in `progress`, because "the
/// same JSON" is exactly the kind of claim that is true until it is not.
impl crate::progress::ProgressSink for Sink {
    fn event(&self, name: &str, payload: serde_json::Value) {
        if let Sink::App(app) = self {
            let _ = app.emit(name, payload);
        }
    }
}

/// The window-backed sink, spelled as a helper so thirteen call sites read
/// `&events::sink(app)` rather than each constructing the variant.
pub fn sink(app: &AppHandle) -> Sink {
    Sink::App(app.clone())
}

// ---------------------------------------------------------------- payloads

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectEvent {
    /// Project name or service id, without the `stackvo-` prefix.
    pub project: Option<String>,
    pub service: Option<String>,
    pub running: Option<bool>,
    pub error: Option<String>,
}

impl SubjectEvent {
    pub fn project(name: &str) -> Self {
        Self {
            project: Some(name.into()),
            service: None,
            running: None,
            error: None,
        }
    }

    pub fn service(id: &str) -> Self {
        Self {
            project: None,
            service: Some(id.into()),
            running: None,
            error: None,
        }
    }

    pub fn running(mut self, running: bool) -> Self {
        self.running = Some(running);
        self
    }

    pub fn error(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub operation_id: String,
    pub subject: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedEvent {
    pub operation_id: String,
    pub subject: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLineEvent {
    pub stream_id: String,
    /// The container the line came from, or — for the cross-project tail — the
    /// project. One stream has one origin; a fanout has one per line, which is
    /// why this is per-event rather than settled when the stream opens.
    pub container: String,
    pub line: String,
    /// `stdout` or `stderr`.
    pub stream: String,
    /// The `LogFile.id` this line was read from, on a fanout only. Omitted from
    /// the payload otherwise, so a single-source stream is byte-identical to
    /// what it emitted before the fanout existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// True for the fanout's seed — lines already in the file when the tail
    /// started. Omitted elsewhere, for the same byte-identity reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historic: Option<bool>,
}

/// Which side of a container/service lifecycle transition an event describes.
pub struct Lifecycle {
    pub pending: &'static str,
    pub done: &'static str,
    pub running_after: bool,
}

pub const START: Lifecycle = Lifecycle {
    pending: "starting",
    done: "started",
    running_after: true,
};
pub const STOP: Lifecycle = Lifecycle {
    pending: "stopping",
    done: "stopped",
    running_after: false,
};
pub const RESTART: Lifecycle = Lifecycle {
    pending: "restarting",
    done: "restarted",
    running_after: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ids_are_unique_within_a_run() {
        let a = next_operation_id("build");
        let b = next_operation_id("build");
        assert_ne!(a, b);
        assert!(a.starts_with("build-"));
    }

    #[test]
    fn subject_events_carry_one_subject_kind() {
        let p = SubjectEvent::project("shop").running(true);
        assert_eq!(p.project.as_deref(), Some("shop"));
        assert!(p.service.is_none());

        let s = SubjectEvent::service("mysql").error("boom");
        assert_eq!(s.service.as_deref(), Some("mysql"));
        assert_eq!(s.error.as_deref(), Some("boom"));
    }
}
