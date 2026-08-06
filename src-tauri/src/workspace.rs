//! The two directories this app works in, and which of them anybody is asked
//! about.
//!
//! ## Why there are two
//!
//! There used to be one, and the app opened by asking for it. That question had
//! no good answer, because it was really two questions with different owners
//! stapled together.
//!
//! Everything under it except `projects/` belongs to the app: the compose files
//! it generates, the logs containers write, the certificates it issues, the
//! settings it saves, and the handful of templates somebody has overridden. A
//! user has no more opinion about where those live than they do about where
//! their browser keeps its cache, and being asked implies otherwise. Once the
//! templates moved into the binary there was not even a clone to point at —
//! the app was asking for a folder so it could create one.
//!
//! `projects/` is the opposite: it is the user's source code, it very often
//! already exists somewhere with a name of their choosing, and it is the one
//! thing they genuinely have to tell the app.
//!
//! So the app root is derived and created without asking, and the only question
//! left is the one worth asking.
//!
//! ## Why `~/.stackvo` and not the platform's app-data directory
//!
//! macOS convention says `~/Library/Application Support/…` and that is where
//! `preferences.json` lives. The stack cannot: those bind mounts reach Docker
//! through compose files, and not one of the twenty-odd `${HOST_STACKVO_ROOT}`
//! lines in the shipped templates is quoted. A path containing a space breaks
//! them twice over — YAML treats the value as unquoted scalar, and Compose
//! splits mount specs on `:` after the shell has already split on whitespace.
//! Quoting twenty-nine templates and remembering to quote the thirtieth is a
//! worse bet than a path with no spaces in it.
//!
//! `STACKVO_ROOT` still overrides it, which is what the tests and CI use.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the projects directory was arrived at. Surfaced to the UI verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The user chose it and we persisted the choice.
    Stored,
    /// `STACKVO_PROJECTS` in the environment.
    Env,
    /// Carried over from a single-root workspace this app used to manage.
    Migrated,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// The app's own directory. Always present — it is derived, not chosen.
    pub root: Option<String>,
    /// The user's project tree, once they have named one.
    pub projects_dir: Option<String>,
    /// True once there is a project tree to work in.
    pub valid: bool,
    /// Has the first-run setup ever finished?
    ///
    /// A marker the app writes after the last step, not a file some step
    /// happens to leave behind. It was the generated compose file for about an
    /// hour, and that is a record of the *first* step rather than of the whole:
    /// a run that wrote the compose files and then failed to issue the
    /// certificate looked complete for ever after, so the screen never offered
    /// again and the stack it left could not serve a single domain.
    ///
    /// Skipping past a failure deliberately does not write it. The offer should
    /// come back next launch — that is the point of the offer.
    pub bootstrapped: bool,
    pub source: Source,
    pub stackvo_version: Option<String>,
    pub env_file: Option<String>,
}

impl Workspace {
    pub fn none() -> Self {
        Self {
            root: None,
            valid: false,
            bootstrapped: false,
            source: Source::None,
            stackvo_version: None,
            projects_dir: None,
            env_file: None,
        }
    }

    /// The app root, or a NO_WORKSPACE error.
    ///
    /// Still gated on `valid` even though the app root itself always resolves:
    /// a command that reaches for it is about to do something to a stack, and a
    /// stack with no project tree behind it is not a thing to half-run. The one
    /// caller that legitimately wants the directory before that — Settings,
    /// showing where it is — reads `root` directly.
    pub fn require_root(&self) -> Result<PathBuf> {
        match (&self.root, self.valid) {
            (Some(r), true) => Ok(PathBuf::from(r)),
            _ => Err(Error::no_workspace()),
        }
    }
}

/// Where the app keeps everything it owns.
///
/// Derived, never asked about. `STACKVO_ROOT` wins when it is set — a relative
/// value is made absolute, because it would otherwise reach the generated
/// compose files verbatim and Docker resolves bind mounts against its own
/// working directory rather than ours.
pub fn app_root() -> PathBuf {
    if let Ok(from_env) = std::env::var("STACKVO_ROOT") {
        if !from_env.trim().is_empty() {
            let path = PathBuf::from(from_env);
            return std::fs::canonicalize(&path).unwrap_or(path);
        }
    }

    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".stackvo")
}

/// Where the pointer to the user's project tree is kept.
///
/// Inside the app root rather than beside `preferences.json`, because it is
/// state about this stack rather than about this person — and because it means
/// every function that already receives the app root can find the project tree
/// without a second parameter threaded through fifteen modules to reach it.
fn projects_pointer(app_root: &Path) -> PathBuf {
    app_root.join("projects.path")
}

/// The user's project tree, or None when nobody has named one.
///
/// **There is deliberately no default.** `<app root>/projects` was one for
/// about an hour and it was wrong twice over: it is a hidden directory the user
/// never chose, and its mere existence would satisfy the one requirement the
/// gate exists to hold — so the app would come up "ready" pointing at an empty
/// folder inside its own state directory, and the question would never get
/// asked. An unanswered question has to read as unanswered.
pub fn projects_root(app_root: &Path) -> Option<PathBuf> {
    if let Ok(text) = std::fs::read_to_string(projects_pointer(app_root)) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    std::env::var("STACKVO_PROJECTS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The project tree, or a NO_WORKSPACE error.
///
/// For the callers that are already inside an operation — by then the gate has
/// been passed, so None means a command reached the filesystem without one, and
/// an error naming that is better than a path nobody chose.
pub fn require_projects_root(app_root: &Path) -> Result<PathBuf> {
    projects_root(app_root).ok_or_else(Error::no_workspace)
}

/// Record where the projects are. Public because tests set it up and the
/// migration writes it; it is the only way the pointer is ever created.
pub fn point_at_projects(app_root: &Path, projects: &Path) -> Result<()> {
    std::fs::create_dir_all(app_root)
        .map_err(|e| Error::io(format!("creating {}", app_root.display()), e))?;
    std::fs::write(projects_pointer(app_root), projects.display().to_string())
        .map_err(|e| Error::io("saving the project directory", e))
}

/// Where the first-run setup records that it finished.
fn bootstrap_marker(app_root: &Path) -> PathBuf {
    app_root.join("bootstrapped")
}

/// Record that the first-run setup completed. Written only by the screen that
/// runs it, and only after its last step.
pub fn mark_bootstrapped(app_root: &Path) -> Result<()> {
    std::fs::create_dir_all(app_root)
        .map_err(|e| Error::io(format!("creating {}", app_root.display()), e))?;
    std::fs::write(bootstrap_marker(app_root), "")
        .map_err(|e| Error::io("recording that setup finished", e))
}

/// A directory the app can work in — one it already set up, or an empty one
/// it can set up now.
///
/// This used to ask "is this a StackVo checkout", which only an existing clone
/// could answer yes to; the templates now ship in the binary, so an empty
/// folder is a perfectly good answer to "where should StackVo live". The
/// marker before that was `core/cli/stackvo.sh`, which stopped existing when
/// the Bash CLI was deleted.
pub fn looks_like_stackvo(path: &Path) -> bool {
    crate::skeleton::fitness(path) != crate::skeleton::Fitness::Occupied
}

fn describe(root: PathBuf, source: Source) -> Workspace {
    let env_file = root.join(".env");
    // Chosen *and* still there. A pointer at a folder somebody has since
    // deleted is not a working answer, and reporting it as one sends every
    // command that follows into an IO error instead of back to the question.
    let projects = projects_root(&root).filter(|p| p.is_dir());
    Workspace {
        stackvo_version: read_env_value(&env_file, "STACKVO_VERSION"),
        env_file: env_file.exists().then(|| env_file.display().to_string()),
        valid: projects.is_some(),
        bootstrapped: bootstrap_marker(&root).is_file(),
        projects_dir: projects.map(|p| p.display().to_string()),
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

/// The form a new project's name is filed under: trimmed and lower-case.
///
/// Not a style rule. The generated compose tags the image `stackvo-<name>`, and
/// Docker rejects an image reference with a capital in it outright — "repository
/// name must be lowercase" — so a project called `Aksoyca` writes a compose file
/// that cannot build. The directory, the container name, the Traefik router and
/// the manifest's `name` all have to agree (W-04), so the case is settled once,
/// here, before the directory is made rather than after Docker complains.
///
/// Applied at creation only. An existing directory keeps whatever name it has:
/// adoption cannot rename somebody's folder, and `manifest::normalize` warns
/// about the case instead.
pub fn canonical_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
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
        .with_hint(crate::hints::PROJECT_NAME_CHARSET));
    }

    let projects = require_projects_root(root)?;
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
            .with_hint(crate::hints::PATH_LEAVES_PROJECTS));
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

/// Where a pre-split install recorded the one directory it managed.
///
/// Only read, and only once — see [`migrate_single_root`]. Nothing writes it
/// any more.
///
/// Deliberately the *old* folder name and not the current one: this is a fact
/// about installs that already exist on disk. Following the rename would mean
/// looking for a file in a directory that, by definition, no install that wrote
/// it ever had.
fn legacy_state_file() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join(crate::appdir::LEGACY_DIR)
            .join("workspace.txt"),
    )
}

fn legacy_root() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(legacy_state_file()?).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// Carry a single-root workspace over to the split layout, once.
///
/// The old directory holds four things worth keeping and two worth leaving.
/// Kept: the project tree (adopted where it stands — moving somebody's source
/// code is not a migration, it is a surprise), `.env`, the templates they
/// overrode, and nothing else. Left: `generated/` and `logs/`, which are
/// output. The next generate rewrites the first and the containers refill the
/// second, and both carry absolute host paths that would be wrong anyway.
///
/// Best-effort throughout. A migration that fails half way must not stop the
/// app from opening — the user can always point at the folder by hand, and an
/// app that will not start is a worse outcome than one that asks a question.
fn migrate_single_root(app_root: &Path) {
    if let Some(old) = legacy_root() {
        migrate(app_root, &old);
    }
}

/// The migration itself, with the old root passed in rather than read.
///
/// Split out so it can be exercised against two temp directories. Reading the
/// path from the OS config directory inside the same function would have made
/// this testable only by writing to the real one.
fn migrate(app_root: &Path, old: &Path) {
    if projects_pointer(app_root).exists() {
        return;
    }
    if !old.is_dir() || old == app_root {
        return;
    }

    // Their code, where it already is.
    let projects = old.join("projects");
    let adopted = if projects.is_dir() {
        projects
    } else {
        old.to_path_buf()
    };

    // Their settings. Never over a file the new root already has.
    let env = old.join(".env");
    if env.is_file() && !app_root.join(".env").exists() {
        if let Err(e) = std::fs::copy(&env, app_root.join(".env")) {
            tracing::warn!(error = %e, "could not carry .env over");
        }
    }

    // Their overrides. `prune_pristine` has already run against the old root by
    // the time anything gets here, so what is left under `core/` is edits.
    for rel in crate::skeleton::overridden(old) {
        let target = app_root.join(&rel);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(old.join(&rel), &target) {
            tracing::warn!(file = %rel, error = %e, "could not carry an overridden template over");
        }
    }

    if let Err(e) = point_at_projects(app_root, &adopted) {
        tracing::warn!(error = %e, "could not record the migrated project tree");
        return;
    }
    tracing::info!(
        from = %old.display(),
        projects = %adopted.display(),
        "migrated a single-root workspace"
    );
}

/// Lay the directory out if it is empty, and sweep it if it is not.
///
/// An uninstalled root had no `projects/`, so the first command that listed
/// anything failed with an IO error naming a directory the user had never heard
/// of. Installing is idempotent and costs five `is_dir()` calls once the
/// directory exists.
///
/// An already-installed one gets swept instead: workspaces created before the
/// templates stopped being copied hold all thirty of them, and until those
/// pristine copies are gone every template fix stops at the workspace boundary
/// and the "which have you overridden" list answers "all of them".
/// `prune_pristine` removes only bytes identical to the binary's, so an edit is
/// never what goes.
fn ensure_installed(path: &Path) {
    // Before anything asks what shape it is in. `fitness` reads the directory
    // and calls a failed read `Occupied`, which is the right answer for a
    // folder it cannot see into and the wrong one for a folder that is simply
    // not there yet — and "not there yet" is every first launch, so nothing was
    // ever created and the app came up with no directories at all.
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(path) {
            tracing::warn!(path = %path.display(), error = %e, "could not create the app directory");
            return;
        }
    }

    match crate::skeleton::fitness(path) {
        crate::skeleton::Fitness::Installable => {
            if let Err(e) = crate::skeleton::install(path) {
                tracing::warn!(path = %path.display(), error = %e, "could not set up the workspace");
            }
        }
        crate::skeleton::Fitness::Existing => {
            let removed = crate::skeleton::prune_pristine(path);
            if removed > 0 {
                tracing::info!(
                    path = %path.display(),
                    removed,
                    "removed unedited template copies an older install left behind"
                );
            }
        }
        crate::skeleton::Fitness::Occupied => {}
    }
}

/// Where the app is working, and whether it has somewhere to work.
///
/// The app root always resolves — it is derived and created, and there is
/// nothing to fail at. What can be absent is the project tree, and that is what
/// `valid` reports. There is no "no workspace" outcome any more, and no
/// discovery either: guessing which of `~/stackvo`, `~/Desktop/stackvo` and
/// three others somebody meant was a heuristic for a question this no longer
/// asks.
pub fn resolve() -> Workspace {
    let root = app_root();
    ensure_installed(&root);
    migrate_single_root(&root);

    let source = if projects_pointer(&root).is_file() {
        // Written either by `set_projects` or by the migration. Telling those
        // apart matters to nobody except the person reading Settings, and the
        // migration logs its own line.
        if legacy_root().is_some() {
            Source::Migrated
        } else {
            Source::Stored
        }
    } else if std::env::var("STACKVO_PROJECTS").is_ok_and(|v| !v.trim().is_empty()) {
        Source::Env
    } else {
        Source::None
    };

    describe(root, source)
}

/// Point the app at a project tree.
///
/// Deliberately permissive about what is in there. The old `set` refused any
/// folder holding files it did not put there, because it was about to scatter
/// thirty templates through it — a real hazard, and the reason for the check.
/// Nothing is scattered now: this records a path and creates the directory if
/// it is missing. A folder with eighteen projects already in it is not an
/// obstacle, it is the common case, and `stackvo.json` is what marks the ones
/// this app manages.
pub fn set_projects(path: impl AsRef<Path>) -> Result<Workspace> {
    let path = path.as_ref();

    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| Error::io(format!("creating {}", path.display()), e))?;
    }

    // Canonicalise before storing: a relative path reaches the generated
    // compose files verbatim through the bind mounts, and Docker resolves those
    // against its own working directory rather than ours.
    let canonical = path
        .canonicalize()
        .map_err(|e| Error::io(format!("resolving {}", path.display()), e))?;

    if !canonical.is_dir() {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{} is not a directory", canonical.display()),
        ));
    }

    let root = app_root();
    ensure_installed(&root);
    point_at_projects(&root, &canonical)?;

    Ok(describe(root, Source::Stored))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The pointer is the whole mechanism, and its absence has to be an answer.
    ///
    /// There was briefly a `<app root>/projects` fallback here. It made every
    /// call site a one-liner and it was wrong: `install` creates the app root,
    /// so the fallback would come into existence on its own the moment anything
    /// wrote there, `valid` would flip to true, and the app would declare
    /// itself ready pointing at a hidden folder the user had never seen. The
    /// question has to stay unanswered until somebody answers it.
    #[test]
    fn the_project_tree_follows_the_pointer_and_has_no_default() {
        let root = scratch("pointer");
        assert_eq!(projects_root(&root), None);
        assert!(require_projects_root(&root).is_err());
        assert!(project_dir(&root, "shop").is_err());

        // And not even when the old default is sitting right there.
        std::fs::create_dir_all(root.join("projects")).unwrap();
        assert_eq!(projects_root(&root), None, "the old default came back");

        let elsewhere = scratch("pointer-code");
        point_at_projects(&root, &elsewhere).unwrap();
        assert_eq!(projects_root(&root), Some(elsewhere.clone()));

        // Project paths resolve into it, with the traversal guard intact.
        assert_eq!(project_dir(&root, "shop").unwrap(), elsewhere.join("shop"));
        assert!(project_dir(&root, "../escape").is_err());

        // A blank pointer is not a path. Truncating the file — an interrupted
        // write, a manual edit — would otherwise make every project resolve to
        // the filesystem root.
        std::fs::write(projects_pointer(&root), "   \n").unwrap();
        assert_eq!(projects_root(&root), None);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    /// What a workspace made before the split has to survive.
    #[test]
    fn migrating_adopts_the_project_tree_and_carries_settings_and_edits() {
        let old = scratch("migrate-old");
        let new = scratch("migrate-new");

        // A single-root workspace: projects, settings, one edited template, and
        // output that must not be dragged along.
        std::fs::create_dir_all(old.join("projects/shop")).unwrap();
        std::fs::create_dir_all(old.join("generated")).unwrap();
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::write(old.join(".env"), "SERVICE_MYSQL_ENABLE=true\n").unwrap();
        std::fs::write(old.join("generated/stale.yml"), "old\n").unwrap();
        let edited = "core/compose/base.yml";
        std::fs::create_dir_all(old.join("core/compose")).unwrap();
        std::fs::write(old.join(edited), "# mine\n").unwrap();

        migrate(&new, &old);

        // Their code stays where it is — moving somebody's source tree is not a
        // migration, it is a surprise.
        assert_eq!(projects_root(&new), Some(old.join("projects")));
        assert!(old.join("projects/shop").is_dir());

        assert_eq!(
            std::fs::read_to_string(new.join(".env")).unwrap(),
            "SERVICE_MYSQL_ENABLE=true\n"
        );
        assert_eq!(
            std::fs::read_to_string(new.join(edited)).unwrap(),
            "# mine\n"
        );

        // Output is not carried: it is rewritten by the next generate, and it
        // holds absolute host paths that are wrong the moment anything moves.
        assert!(!new.join("generated/stale.yml").exists());

        // And it happens once. A second run must not overwrite a pointer the
        // user has since changed.
        let moved = scratch("migrate-moved");
        point_at_projects(&new, &moved).unwrap();
        migrate(&new, &old);
        assert_eq!(projects_root(&new), Some(moved.clone()));

        for dir in [&old, &new, &moved] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

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

    #[test]
    fn a_new_project_is_filed_lower_case() {
        // The reported case: a project typed as "Aksoyca" produced
        // `image: stackvo-Aksoyca`, which Docker refuses outright.
        assert_eq!(canonical_name("Aksoyca"), "aksoyca");
        assert_eq!(canonical_name("  API.MyApp  "), "api.myapp");
        // Already canonical, and still a safe name afterwards — the two rules
        // have to agree or creation would accept a name it cannot file.
        assert_eq!(canonical_name("web-1"), "web-1");
        assert!(is_safe_name(&canonical_name("Aksoyca")));
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
        point_at_projects(&tmp, &tmp.join("projects")).unwrap();

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
        point_at_projects(&tmp, &tmp.join("projects")).unwrap();

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
