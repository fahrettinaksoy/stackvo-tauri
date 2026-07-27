//! Locating the StackVo checkout this app drives.
//!
//! The web UI never had this problem: it was bind-mounted at `/app` inside the
//! repo it managed. A host app has to be told which checkout to drive, or work
//! it out — and if it guesses, it must say so, because silently driving the
//! wrong directory is worse than asking.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the current root was arrived at. Surfaced to the UI verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The user chose it and we persisted the choice.
    Stored,
    /// `STACKVO_ROOT` in the environment.
    Env,
    /// Found by looking in the usual places.
    Discovered,
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub root: Option<String>,
    pub valid: bool,
    pub source: Source,
    pub stackvo_version: Option<String>,
    pub projects_dir: Option<String>,
    pub env_file: Option<String>,
}

impl Workspace {
    pub fn none() -> Self {
        Self {
            root: None,
            valid: false,
            source: Source::None,
            stackvo_version: None,
            projects_dir: None,
            env_file: None,
        }
    }

    /// The root, or a NO_WORKSPACE error. Every command that touches the
    /// checkout goes through this rather than unwrapping an Option.
    pub fn require_root(&self) -> Result<PathBuf> {
        match (&self.root, self.valid) {
            (Some(r), true) => Ok(PathBuf::from(r)),
            _ => Err(Error::no_workspace()),
        }
    }
}

/// A directory is a StackVo checkout if it has the CLI entrypoint and a
/// projects directory. Both are required: `core/cli/stackvo.sh` alone could be
/// a partial clone, and `projects/` alone is far too generic a name to trust.
pub fn looks_like_stackvo(path: &Path) -> bool {
    path.join("core/cli/stackvo.sh").is_file() && path.join("projects").is_dir()
}

fn describe(root: PathBuf, source: Source) -> Workspace {
    let env_file = root.join(".env");
    let projects = root.join("projects");
    Workspace {
        stackvo_version: read_env_value(&env_file, "STACKVO_VERSION"),
        env_file: env_file.exists().then(|| env_file.display().to_string()),
        projects_dir: projects.is_dir().then(|| projects.display().to_string()),
        valid: true,
        source,
        root: Some(root.display().to_string()),
    }
}

/// Does this project name match SAFE_NAME from `contracts/project.schema.json`?
///
/// `^[a-zA-Z0-9][a-zA-Z0-9._-]*$`, at most 128 characters. The contract records
/// why the pattern is this narrow: it is the regex StackVo's own `exec.js`
/// enforces to keep shell metacharacters and path traversal out of names that
/// end up in commands and paths.
///
/// Written out rather than pulled in as a regex — one dependency for one
/// pattern is a poor trade, and the character classes are the whole rule.
pub fn is_safe_name(name: &str) -> bool {
    // The pattern is ASCII-only, so every non-ASCII byte fails the class check
    // below and `len()` in bytes is the same bound as length in characters.
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        // A leading dot is what makes "." and ".." traversal; requiring the
        // first character to be alphanumeric rules both out at the source.
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The single way to turn a caller-supplied project name into a path.
///
/// Every command that touches `projects/<name>` must go through this. Joining
/// the name directly is not safe: `Path::join` keeps `..` as a literal
/// component and `is_dir()` then *resolves* it, so a name like `../elsewhere`
/// passes an existence check and points outside the workspace — which matters
/// most in `project_delete`, where the next call is `remove_dir_all`.
///
/// The directory is not required to exist; creation flows need the path before
/// there is anything at it.
pub fn project_dir(root: &Path, name: &str) -> Result<PathBuf> {
    if !is_safe_name(name) {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{name}\" is not a valid project name"),
        )
        .with_hint(
            "Names may contain letters, digits, dot, underscore and dash, and must start with a letter or digit.",
        ));
    }

    let projects = root.join("projects");
    let dir = projects.join(name);

    // Defence in depth. A name can be perfectly safe and still resolve outside
    // the workspace if `projects/<name>` is a symlink, so when the path already
    // exists, confirm where it really lands. Both sides are canonicalised
    // because the root itself may be reached through a link.
    if let (Ok(real), Ok(base)) = (dir.canonicalize(), projects.canonicalize()) {
        if !real.starts_with(&base) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("project \"{name}\" resolves outside the workspace"),
            )
            .with_hint("Refusing to operate on a path that leaves projects/."));
        }
    }

    Ok(dir)
}

/// Pull a single key out of a `.env` without loading the whole file into a map.
/// Uses the same naive first-`=` split the Bash loader and the Node parser use,
/// per `contracts/env.schema.json` → `parsing.rules`.
fn read_env_value(env_file: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(env_file).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Where the persisted choice lives. Not in the StackVo repo — this is app
/// state, and writing it into the managed checkout would pollute the user's
/// working tree.
fn state_file() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("dev.stackvo.desktop")
            .join("workspace.txt"),
    )
}

fn load_stored() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(state_file()?).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn save_stored(root: &Path) -> Result<()> {
    let file = state_file()
        .ok_or_else(|| Error::new(Code::IoError, "cannot determine the OS config directory"))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io("creating the config directory", e))?;
    }
    std::fs::write(&file, root.display().to_string())
        .map_err(|e| Error::io("saving the workspace path", e))
}

/// Candidate locations, in the order a developer is likely to have cloned into.
/// Deliberately conservative — a wrong guess is worse than no guess, so every
/// candidate still has to pass `looks_like_stackvo`.
fn discovery_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    // Walk up from the current directory: covers `cargo tauri dev` run from
    // inside a sibling checkout.
    if let Ok(cwd) = std::env::current_dir() {
        let mut cursor = Some(cwd.as_path());
        while let Some(dir) = cursor {
            out.push(dir.to_path_buf());
            out.push(dir.join("stackvo"));
            cursor = dir.parent();
        }
    }

    if let Some(home) = dirs::home_dir() {
        for rel in [
            "stackvo",
            "Desktop/stackvo",
            "Projects/stackvo",
            "Sites/stackvo",
            "src/stackvo",
        ] {
            out.push(home.join(rel));
        }
    }

    out
}

/// Resolve the workspace: stored choice, then `STACKVO_ROOT`, then discovery.
///
/// A stored path that no longer validates does NOT silently fall through to
/// discovery — it returns invalid with the stale root attached, so the UI can
/// say "this folder is gone" instead of quietly switching checkouts.
pub fn resolve() -> Workspace {
    if let Some(stored) = load_stored() {
        return if looks_like_stackvo(&stored) {
            describe(stored, Source::Stored)
        } else {
            Workspace {
                root: Some(stored.display().to_string()),
                valid: false,
                source: Source::Stored,
                stackvo_version: None,
                projects_dir: None,
                env_file: None,
            }
        };
    }

    if let Ok(from_env) = std::env::var("STACKVO_ROOT") {
        // Canonicalise: a relative STACKVO_ROOT would end up verbatim in the
        // generated compose file's bind mounts, and Docker resolves those
        // against its own working directory, not ours.
        let path = std::fs::canonicalize(&from_env).unwrap_or_else(|_| PathBuf::from(&from_env));
        if looks_like_stackvo(&path) {
            return describe(path, Source::Env);
        }
    }

    for candidate in discovery_candidates() {
        if looks_like_stackvo(&candidate) {
            return describe(candidate, Source::Discovered);
        }
    }

    Workspace::none()
}

/// Validate and persist an explicit choice.
pub fn set(path: impl AsRef<Path>) -> Result<Workspace> {
    let path = path.as_ref();

    let canonical = path
        .canonicalize()
        .map_err(|e| Error::io(format!("resolving {}", path.display()), e))?;

    if !looks_like_stackvo(&canonical) {
        return Err(Error::new(
            Code::NoWorkspace,
            format!(
                "{} does not look like a StackVo checkout",
                canonical.display()
            ),
        )
        .with_hint("Pick the folder that directly contains core/cli/stackvo.sh and projects/.")
        .with_details(serde_json::json!({
            "path": canonical.display().to_string(),
            "hasCli": canonical.join("core/cli/stackvo.sh").is_file(),
            "hasProjects": canonical.join("projects").is_dir(),
        })));
    }

    save_stored(&canonical)?;
    Ok(describe(canonical, Source::Stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names_follow_the_contract_pattern() {
        for ok in ["myshop", "api.oxoeashop", "a", "web-1", "some_thing", "X9"] {
            assert!(is_safe_name(ok), "{ok} should be accepted");
        }
        for bad in [
            "",             // no name at all
            ".",            // the directory itself
            "..",           // the parent — the traversal primitive
            "../elsewhere", // traversal with a payload
            "a/b",          // a nested path, not a name
            "a\\b",         // the same on Windows
            "-leading",     // pattern requires an alphanumeric first
            ".hidden",      // ditto, and hides the project from the CLI
            "with space",   // not in the character class
            "semi;colon",   // a shell metacharacter
            "nul\0byte",    // truncates the path at the syscall boundary
            "üñí",          // the pattern is ASCII-only
        ] {
            assert!(!is_safe_name(bad), "{bad:?} should be rejected");
        }
        assert!(is_safe_name(&"a".repeat(128)));
        assert!(!is_safe_name(&"a".repeat(129)), "128 is the contract bound");
    }

    /// The bug this guards against: `Path::join` keeps `..` as a literal
    /// component, and the `is_dir()` check that follows *resolves* it. Without
    /// the name check, `project_delete("../x", remove_files: true)` reaches
    /// `remove_dir_all` on a directory outside the workspace.
    #[test]
    fn a_traversing_name_never_yields_a_path() {
        let tmp = std::env::temp_dir().join("stackvo-ws-test-traversal");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("projects/real")).unwrap();
        std::fs::create_dir_all(tmp.join("outside")).unwrap();

        // Proof the escape is real if the name is not checked.
        let unchecked = tmp.join("projects").join("../outside");
        assert!(unchecked.is_dir(), "the traversal does resolve");

        assert!(project_dir(&tmp, "../outside").is_err());
        assert!(project_dir(&tmp, "..").is_err());
        assert_eq!(
            project_dir(&tmp, "real").unwrap(),
            tmp.join("projects").join("real")
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A name can pass the pattern and still leave the workspace when the entry
    /// is a symlink, so containment is checked separately.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_project_pointing_outside_is_refused() {
        let tmp = std::env::temp_dir().join("stackvo-ws-test-symlink");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("projects")).unwrap();
        std::fs::create_dir_all(tmp.join("outside")).unwrap();
        std::os::unix::fs::symlink(tmp.join("outside"), tmp.join("projects/escapee")).unwrap();

        assert!(project_dir(&tmp, "escapee").is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Creation needs the path before anything exists at it, so a missing
    /// directory is not an error — only an unsafe or escaping one is.
    #[test]
    fn a_path_is_returned_before_the_directory_exists() {
        let tmp = std::env::temp_dir().join("stackvo-ws-test-missing");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("projects")).unwrap();

        assert_eq!(
            project_dir(&tmp, "not-yet").unwrap(),
            tmp.join("projects").join("not-yet")
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_a_directory_missing_the_cli() {
        let tmp = std::env::temp_dir().join("stackvo-ws-test-empty");
        let _ = std::fs::create_dir_all(tmp.join("projects"));
        assert!(
            !looks_like_stackvo(&tmp),
            "projects/ alone must not qualify"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_relative_env_root_is_made_absolute() {
        // A relative root reaches the compose generator as-is and produces bind
        // mounts Docker resolves against its own cwd.
        let cwd = std::env::current_dir().unwrap();
        let resolved = std::fs::canonicalize(".").unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved, std::fs::canonicalize(&cwd).unwrap());
    }

    #[test]
    fn none_workspace_refuses_to_hand_out_a_root() {
        let ws = Workspace::none();
        assert!(ws.require_root().is_err());
    }
}
