//! The single error type crossing the IPC boundary.
//!
//! Shape matches `contracts/ipc.json` → `errors.shape`: every failure carries a
//! stable machine-readable `code`, a user-facing `message`, and an optional
//! `hint` telling the user what to do about it.
//!
//! This replaces the Express convention where a 200 response could still mean
//! failure (`{ success: false }`) and a dead Docker daemon surfaced as a raw
//! ECONNREFUSED stack trace.

use serde::Serialize;

/// Error codes are a closed set — see `contracts/ipc.json` → `errors.codes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Code {
    EngineUnreachable,
    NotFound,
    AlreadyExists,
    InvalidInput,
    InvalidManifest,
    Unsupported,
    GenerateFailed,
    BuildFailed,
    PermissionDenied,
    IoError,
    Conflict,
    /// The StackVo working directory has not been chosen yet, or no longer
    /// looks like a StackVo checkout. Desktop-specific: the web UI could not
    /// hit this because it was mounted inside the repo it managed.
    NoWorkspace,
    /// An administrator's policy says no — see [`crate::policy`].
    ///
    /// Deliberately not `PERMISSION_DENIED`, which in this app means the OS
    /// refused: elevation cancelled, a file the user cannot write. Those are
    /// answered by retrying with a password. This one never is, and a UI that
    /// offered to elevate would be promising something that cannot happen.
    Forbidden,
}

impl Code {
    fn as_str(self) -> &'static str {
        match self {
            Code::EngineUnreachable => "ENGINE_UNREACHABLE",
            Code::NotFound => "NOT_FOUND",
            Code::AlreadyExists => "ALREADY_EXISTS",
            Code::InvalidInput => "INVALID_INPUT",
            Code::InvalidManifest => "INVALID_MANIFEST",
            Code::Unsupported => "UNSUPPORTED",
            Code::GenerateFailed => "GENERATE_FAILED",
            Code::BuildFailed => "BUILD_FAILED",
            Code::PermissionDenied => "PERMISSION_DENIED",
            Code::IoError => "IO_ERROR",
            Code::Conflict => "CONFLICT",
            Code::NoWorkspace => "NO_WORKSPACE",
            Code::Forbidden => "FORBIDDEN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Error {
    pub code: Code,
    pub message: String,
    pub hint: Option<String>,
    /// The locale key for [`Self::hint`], when the hint came from the
    /// [`crate::hints`] catalogue.
    ///
    /// `None` for the handful of hints built at runtime from a value only the
    /// caller has — a program name, a git failure. Those stay English, and the
    /// frontend falls back to `hint`, which is exactly what it did for all of
    /// them before.
    pub hint_key: Option<&'static str>,
    pub details: Option<serde_json::Value>,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            hint_key: None,
            details: None,
        }
    }

    /// Attach a suggestion.
    ///
    /// Takes either a [`crate::hints::Hint`] — which carries its own locale key
    /// and is what nearly every call site passes — or a plain string, for a hint
    /// assembled at runtime. Both fill `hint`; only the first fills `hint_key`.
    ///
    /// One method rather than two so the untranslatable case has to be written
    /// deliberately rather than reached for by habit.
    pub fn with_hint(mut self, hint: impl Into<Suggestion>) -> Self {
        let Suggestion { text, key } = hint.into();
        self.hint = Some(text);
        self.hint_key = key;
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn no_workspace() -> Self {
        Self::new(Code::NoWorkspace, "No StackVo directory selected yet.")
            .with_hint(crate::hints::CHOOSE_WORKSPACE)
    }

    pub fn not_found(what: impl std::fmt::Display) -> Self {
        Self::new(Code::NotFound, format!("{what} not found"))
    }

    pub fn io(context: impl std::fmt::Display, err: std::io::Error) -> Self {
        Self::new(Code::IoError, format!("{context}: {err}"))
    }
}

/// What [`Error::with_hint`] accepts.
///
/// Not a public vocabulary type — nothing constructs one by name. It exists so
/// that one method can take a catalogued hint or a runtime string without the
/// call sites having to say which they are handing over.
pub struct Suggestion {
    text: String,
    key: Option<&'static str>,
}

impl From<crate::hints::Hint> for Suggestion {
    fn from(hint: crate::hints::Hint) -> Self {
        Self {
            text: hint.english.to_string(),
            key: Some(hint.key),
        }
    }
}

impl From<String> for Suggestion {
    fn from(text: String) -> Self {
        Self { text, key: None }
    }
}

impl From<&str> for Suggestion {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            key: None,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for Error {}

/// Serialised as `{ code, message, hint?, details? }` so the JS side can switch
/// on `code` without string-matching the message.
impl Serialize for Error {
    // `std::result::Result` spelled out: the `Result<T>` alias below shadows it
    // in this module and would silently give the trait method the wrong shape.
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("code", self.code.as_str())?;
        m.serialize_entry("message", &self.message)?;
        if let Some(h) = &self.hint {
            m.serialize_entry("hint", h)?;
        }
        // Added beside `hint`, never instead of it. An MCP client and the log
        // want the English; only the webview has a locale to look a key up in,
        // and it falls back to `hint` when the key is absent.
        if let Some(k) = &self.hint_key {
            m.serialize_entry("hintKey", k)?;
        }
        if let Some(d) = &self.details {
            m.serialize_entry("details", d)?;
        }
        m.end()
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(Code::IoError, e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new(Code::InvalidInput, format!("malformed JSON: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
