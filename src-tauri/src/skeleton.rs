//! The workspace skeleton, compiled into the binary.
//!
//! Until Sprint 23 the app could only manage a directory somebody had already
//! cloned: `looks_like_stackvo` demanded `core/templates` and `projects/`, so
//! the first thing a new user had to do was find and clone another repository.
//! The generator moved here in Sprint 17 and the Bash CLI was deleted in
//! Sprint 19; the templates it renders were the last input still living
//! somewhere else.
//!
//! They now ship inside the executable. A workspace stops being something the
//! user brings and becomes something the app can *create* — an empty folder is
//! a valid answer to "where should StackVo live".
//!
//! ## Why compiled in rather than bundled beside the app
//!
//! `bundle.resources` would copy the files next to the executable and need
//! `resolve_resource()` to find them, which resolves differently under
//! `tauri dev` than in a packaged app — a class of bug that only appears after
//! packaging, which is the worst time to find it. `include_dir!` has no path
//! to get wrong: the bytes are in the binary.
//!
//! ## What is NOT here
//!
//! `projects/`, `generated/` and `logs/` are created empty. They are the
//! user's code, this app's output, and the containers' output respectively —
//! none of them is a template, and shipping any of them would mean shipping
//! somebody else's data.
//!
//! ## Why nothing is copied to disk until somebody asks
//!
//! Installing used to write all thirty files into `core/`, and never overwrite
//! them again — the right rule for a file somebody has edited, applied to files
//! nobody had. Three things followed from that.
//!
//! A disk copy stopped meaning anything. `read_template` reads the workspace
//! first precisely so an edit wins, so "there is a file at `core/templates/…`"
//! is supposed to mean "the user changed this". Writing it for everyone made it
//! mean "the workspace was installed", which is not a question anyone asks.
//!
//! Template fixes could not ship. Improve the redis template, release, and
//! every existing workspace keeps the old bytes for good — with no way to tell
//! a stale pristine copy from a deliberate edit, so nothing could safely
//! rewrite it either.
//!
//! And the useful list — *which templates has this workspace changed* — was not
//! computable. It is now: [`overridden`] is the answer, and it is exactly the
//! set of files on disk.
//!
//! So `install` creates directories, and a file appears under `core/` only when
//! [`materialize`] puts it there because somebody chose to override it.
//! [`revert`] deletes it and the embedded copy takes over again.

use crate::error::{Code, Error, Result};
use include_dir::{include_dir, Dir};
use std::path::Path;

/// The `skeleton/` directory at the crate root.
///
/// Templates and compose fragments only. It carried a `.env.example` and — as
/// a measurement here found — a gitignored `skeleton/.env` too: `include_dir!`
/// copies whatever is on disk and does not read `.gitignore`, so the one file
/// that could hold a real password off a developer's machine was being
/// compiled into every build. Settings live in [`crate::config::EMBEDDED`]
/// now, and `no_env_file_is_compiled_in` keeps one from coming back.
static SKELETON: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../skeleton");

/// Directories the app's own root has, whether or not anything is in them yet.
///
/// `projects` is deliberately not here. Creating it would make
/// `<app root>/projects` exist on first launch, which is the fallback
/// `workspace::projects_root` uses — so the app would quietly answer "where do
/// you keep your code" with a hidden directory nobody chose, and the one
/// question worth asking would never get asked. `generated/projects` and
/// `logs/projects` are this app's output about projects, which is a different
/// thing and stays.
const DIRECTORIES: [&str; 4] = [
    "generated/projects",
    "generated/configs",
    "logs/projects",
    "logs/services",
];

/// Is this directory usable as a workspace — either already one, or empty
/// enough to become one?
///
/// The old question was "is this a StackVo checkout", which only an existing
/// clone could answer yes to. This one has three answers and the caller acts
/// on each differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fitness {
    /// Already has the templates and a projects directory.
    Existing,
    /// Empty, or holds nothing but hidden files — safe to install into.
    Installable,
    /// Has other content. Installing would scatter StackVo's files through
    /// somebody's unrelated folder, so it is refused rather than merged.
    Occupied,
}

pub fn fitness(path: &Path) -> Fitness {
    // Recognised by the directories `install` creates. It used to look for
    // `core/templates`, which was true of every installed workspace back when
    // installing wrote thirty files into it; now that it writes none, that test
    // would call a perfectly good workspace `Occupied` on the second launch and
    // refuse to open it.
    //
    // `generated` and `logs` together, because either alone is a name a folder
    // can have for its own reasons — and because `projects` is no longer one of
    // the directories this app creates, so keying on it would have been the
    // same mistake in a new place. Both of these are also true of every
    // single-root workspace that came before the split, which is what lets one
    // be recognised and migrated.
    if path.join("generated").is_dir() && path.join("logs").is_dir() {
        return Fitness::Existing;
    }

    let visible = std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .count()
        })
        .unwrap_or(usize::MAX);

    if visible == 0 {
        Fitness::Installable
    } else {
        Fitness::Occupied
    }
}

/// Lay out `root`, creating what is missing and touching nothing else.
///
/// Directories only — see the module doc for why no template is copied. Returns
/// the directories it actually created, so a second call on an existing
/// workspace returns nothing and the caller can tell "set this up" from "this
/// was already set up".
pub fn install(root: &Path) -> Result<Vec<String>> {
    let mut created = Vec::new();

    for dir in DIRECTORIES {
        let path = root.join(dir);
        if path.is_dir() {
            continue;
        }
        std::fs::create_dir_all(&path).map_err(|e| Error::io(format!("creating {dir}"), e))?;
        created.push(dir.to_string());
    }

    Ok(created)
}

/// Every file a workspace may override, as paths relative to the root.
///
/// The README is not one of them: it explains the skeleton to a reader of *this*
/// repository and has no meaning in somebody's workspace.
pub fn overridable() -> Vec<String> {
    let mut out: Vec<String> = files_of(&SKELETON)
        .into_iter()
        .map(|f| f.path().display().to_string())
        .filter(|p| p != "README.md")
        .collect();
    out.sort();
    out
}

/// The ones this workspace has actually taken over.
///
/// Just the files on disk — which is the whole reason `install` writes none.
pub fn overridden(root: &Path) -> Vec<String> {
    overridable()
        .into_iter()
        .filter(|rel| root.join(rel).is_file())
        .collect()
}

/// A path this app is willing to write under `core/`.
///
/// The relative path arrives from the front end, so it is a string a caller
/// chose, and it is about to be joined onto the workspace root and written to.
/// Membership in `overridable()` is the check: it is an exact match against a
/// fixed list compiled into the binary, which no amount of `..` can talk its
/// way into.
fn checked(relative: &str) -> Result<()> {
    if overridable().iter().any(|p| p == relative) {
        return Ok(());
    }
    Err(Error::new(
        Code::InvalidInput,
        format!("{relative} is not a file this workspace can override"),
    )
    .with_hint("Only the templates the app ships can be overridden."))
}

/// Copy the embedded file into the workspace so it can be edited.
///
/// Refuses when there is already one there: that file is the user's, and this
/// is the call that would silently replace it with the shipped version.
/// Returns the text, so a caller opening an editor does not have to read it
/// back.
pub fn materialize(root: &Path, relative: &str) -> Result<String> {
    checked(relative)?;

    let target = root.join(relative);
    if target.exists() {
        return Err(Error::new(
            Code::InvalidInput,
            format!("{relative} is already overridden in this workspace"),
        )
        .with_hint("Revert it first if you want the shipped version back."));
    }

    let text = SKELETON
        .get_file(relative)
        .and_then(|f| f.contents_utf8())
        .ok_or_else(|| Error::new(Code::NotFound, format!("{relative} is not in the binary")))?;

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
    }
    std::fs::write(&target, text)
        .map_err(|e| Error::io(format!("writing {}", target.display()), e))?;

    Ok(text.to_string())
}

/// Remove the copies an older install left behind, and only those.
///
/// Every workspace created before this module stopped copying holds all thirty
/// files. Nothing distinguishes them from edits by their presence — which is
/// the whole problem — but the bytes do: a file identical to the one in the
/// binary was written by `install`, not by a person. Deleting it changes what
/// no reader sees (`read_template` falls back to the same bytes) and buys back
/// two things it had lost: template fixes reach the workspace again, and
/// "overridden" starts meaning what it says.
///
/// A file that differs is untouched, whatever it differs by. The cost of being
/// wrong here is somebody's edit, so the test is equality and nothing cleverer.
///
/// Best-effort by design: a read that fails leaves the file alone, and the
/// caller gets the count rather than an error, because an unreadable template
/// in an otherwise fine workspace is not a reason to refuse to open it.
pub fn prune_pristine(root: &Path) -> usize {
    let mut removed = 0;

    for file in files_of(&SKELETON) {
        let rel = file.path().display().to_string();
        if rel == "README.md" {
            continue;
        }
        let target = root.join(&rel);
        let Ok(on_disk) = std::fs::read(&target) else {
            continue;
        };
        if on_disk != file.contents() {
            continue;
        }
        if std::fs::remove_file(&target).is_ok() {
            removed += 1;
        }
    }

    // The directories those files were in are now empty and say nothing.
    // `remove_dir` refuses a non-empty one, which is exactly the guard wanted:
    // a directory holding a surviving edit stays.
    prune_empty_dirs(&root.join("core"));

    removed
}

/// Depth-first, so a parent is tried after its children have had their turn.
fn prune_empty_dirs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            prune_empty_dirs(&path);
        }
    }
    let _ = std::fs::remove_dir(dir);
}

/// Delete the workspace's copy; the embedded one takes over on the next read.
///
/// Deliberately not "restore the shipped bytes into the file". Leaving a
/// pristine copy on disk is what made an override indistinguishable from an
/// install in the first place, and it is the state this whole module exists to
/// stop producing.
pub fn revert(root: &Path, relative: &str) -> Result<()> {
    checked(relative)?;

    let target = root.join(relative);
    match std::fs::remove_file(&target) {
        Ok(()) => Ok(()),
        // Already back to the shipped version. The user asked for a state, not
        // for an operation, and they are in it.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(format!("removing {}", target.display()), e)),
    }
}

/// Every file in the tree, at any depth.
///
/// Hand-unrolling the levels was tried first and shipped a workspace with no
/// service templates in it: `services/redis/docker-compose.redis.tpl` is four
/// deep and the chain stopped at three. Depth is not something to count by
/// hand — a test caught it, but only because it asserted on a real path
/// rather than on a file count.
fn files_of<'a>(dir: &'a Dir<'a>) -> Vec<&'a include_dir::File<'a>> {
    let mut out: Vec<&include_dir::File> = dir.files().collect();
    for child in dir.dirs() {
        out.extend(files_of(child));
    }
    out
}

/// A template's bytes: the workspace's copy when it has one, the compiled-in
/// copy otherwise.
///
/// The order is the whole point. Shipping templates must not take away the
/// ability to change them — a user who edits `core/templates/services/redis/…`
/// in their own workspace keeps that edit, and one who does not gets a file
/// they never had to fetch.
pub fn read_template(root: &Path, relative: &str) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(root.join(relative)) {
        return Some(text);
    }
    SKELETON
        .get_file(relative)
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}

/// Every service template's text, workspace-first.
///
/// Used by the volume harvest, which needs *all* of them rather than one by
/// name — including services that are switched off, because that is what the
/// Bash generator did and the volumes section is compared byte-for-byte.
///
/// ## One resolution rule, not two
///
/// This used to read the disk *instead of* the binary whenever
/// `core/templates` existed, while `read_template` reads the disk *before* the
/// binary, file by file. Two rules for the same question, and they disagree in
/// a case that costs data: delete one `.tpl` from a workspace that has the
/// directory, and the harvest stops seeing that service's volumes while every
/// other reader still renders it from the embedded copy. The compose file then
/// declares a service whose named volume is not in the `volumes:` section, and
/// compose refuses the whole file.
///
/// So the set is the union — what the binary ships, plus anything extra the
/// workspace added — and every member resolves through `read_template`, which
/// is the one rule.
pub fn all_service_templates(root: &Path) -> Vec<String> {
    let mut paths: std::collections::BTreeSet<String> = files_of(&SKELETON)
        .into_iter()
        .map(|f| f.path().display().to_string())
        .filter(|p| p.ends_with(".tpl"))
        .collect();

    // A template the workspace has and the binary does not. Nothing renders
    // such a service — `DYNAMIC_SERVICES` is a fixed list — but its volumes
    // were harvested before this change, and quietly dropping them would be a
    // behaviour change nobody asked for.
    collect_tpl_paths(root, &root.join("core/templates"), &mut paths);

    paths
        .into_iter()
        .filter_map(|rel| read_template(root, &rel))
        .collect()
}

/// Every `.tpl` under `dir`, as a path relative to `root`.
fn collect_tpl_paths(root: &Path, dir: &Path, out: &mut std::collections::BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_tpl_paths(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("tpl") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.insert(rel.display().to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-skel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_templates_the_generator_needs_are_all_compiled_in() {
        // Not a count — the actual list the renderer reads. A template that
        // ships in the repo but not in the binary is a service that silently
        // stops being generated once the app is packaged.
        for (_, path) in crate::template::DYNAMIC_SERVICES {
            assert!(
                SKELETON
                    .get_file(format!("core/templates/{path}"))
                    .is_some(),
                "{path} is missing from the compiled-in skeleton"
            );
        }
        assert!(SKELETON.get_file("core/compose/base.yml").is_some());
    }

    /// `include_dir!` does not respect `.gitignore`.
    ///
    /// This was assumed rather than checked, and the assumption was written
    /// into the module doc as if it were a safeguard. It is not: a
    /// `skeleton/.env` sat in the binary, ahead of `.env.example` in the file
    /// order, so it was the file a new workspace was actually seeded from.
    #[test]
    fn no_env_file_is_compiled_in() {
        let leaked: Vec<String> = files_of(&SKELETON)
            .iter()
            .map(|f| f.path().display().to_string())
            .filter(|p| p.rsplit('/').next().unwrap_or(p).starts_with(".env"))
            .collect();
        assert!(leaked.is_empty(), "env files in the binary: {leaked:?}");
    }

    #[test]
    fn no_real_credential_is_compiled_into_the_binary() {
        // Both places a value can ship from. `.env.example` is committed, so a
        // live value in it would be baked into every build ever made — the
        // copy this skeleton came from carried a 64-hex Blackfire token. The
        // starting credentials now travel in `EMBEDDED` instead, which is the
        // same exposure through a different file, so the guard has to look at
        // both or it protects the half that no longer holds anything.
        let from_file = SKELETON
            .get_file(".env.example")
            .and_then(|f| f.contents_utf8())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_string(), v.trim().to_string()))
            .collect::<Vec<_>>();

        let embedded = crate::config::EMBEDDED
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()));

        let mut checked = 0usize;
        for (key, value) in from_file.into_iter().chain(embedded) {
            if !crate::config::Env::is_secret(&key) {
                continue;
            }
            checked += 1;
            // Placeholders are short words like `root`. Anything long enough
            // to be generated is something that leaked off a real machine.
            assert!(
                value.len() < 24,
                "{key} looks like a real credential ({} chars)",
                value.len()
            );
        }

        // A guard over an empty set passes for the wrong reason. This one has
        // already had exactly that failure mode once, when the keys moved out
        // of the file it was reading.
        assert!(
            checked >= 10,
            "expected to be checking credentials, saw {checked}"
        );
    }

    /// Every setting shipped is one something reads.
    ///
    /// The file arrived with 162 keys and 26 of them had no consumer at all —
    /// flags of the deleted Bash CLI, a Let's Encrypt integration that was
    /// never written, `DOCKER_REMOVE_ORPHANS` when the code passes
    /// `--remove-orphans` literally. A setting nothing reads is worse than a
    /// missing one: it invites a change and then ignores it silently. Two of
    /// them were exactly that failure — `HOST_PORT_ADMINER` looked like the
    /// port knob while the template read `SERVICE_ADMINER_HOST_PORT`.
    ///
    /// Dynamic families are assembled with `format!()`, so the fragment is
    /// what to look for rather than the whole key.
    #[test]
    fn every_shipped_setting_has_a_consumer() {
        // Reads `EMBEDDED` now that the settings live there rather than in a
        // shipped file. Pointing it at a file that no longer exists would have
        // been the same vacuous pass described below, arrived at differently.
        let shipped: Vec<(String, String)> = crate::config::EMBEDDED
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        // Everything that could read one: this crate, and the templates it
        // renders. The schema is deliberately NOT in here — it describes a
        // key, it does not read it, and counting it was what made a first pass
        // report zero.
        //
        // The settings file used to be in the corpus, which was worse: every
        // key matched its own definition, so this test passed for anything at
        // all. It was checked by feeding it a key named `TOTALLY_BOGUS_KEY_XYZ`
        // — which it waved through. A guard that cannot fail is not a guard.
        let mut code = String::new();
        for entry in walkdir(std::path::Path::new("src")) {
            if entry.extension().and_then(|e| e.to_str()) == Some("rs") {
                let text = std::fs::read_to_string(&entry).unwrap_or_default();
                code.push_str(&without_comments(&text));
            }
        }
        for file in files_of(&SKELETON) {
            if let Some(text) = file.contents_utf8() {
                code.push_str(&without_comments(text));
            }
        }

        let dynamic = [
            ("SUPPORTED_LANGUAGES_", "SUPPORTED_LANGUAGES_{"),
            ("SERVICE_", "_ENABLE"),
            ("SERVICE_", "_VERSION"),
            ("SERVICE_", "_URL"),
        ];

        let mut dead = Vec::new();
        for (key, _) in &shipped {
            let key = key.as_str();
            if mentions(&code, key) {
                continue;
            }
            let assembled = dynamic.iter().any(|(prefix, fragment)| {
                key.starts_with(prefix) && key.ends_with(fragment.trim_start_matches('_'))
                    || (key.starts_with(prefix) && code.contains(fragment))
            });
            if !assembled {
                dead.push(key.to_string());
            }
        }

        assert!(dead.is_empty(), "settings nothing reads: {dead:?}");
    }

    /// Prose is not a consumer.
    ///
    /// Comments discuss keys by name, and a mention in one reads exactly like
    /// a use to a text search — which is how a dead key survived this test by
    /// being named in the very comment explaining why it looked alive. Erring
    /// this way is deliberate: a truncated line can only ever make the test
    /// report too much, and that fails loudly instead of passing quietly.
    fn without_comments(text: &str) -> String {
        text.lines()
            .map(|line| {
                let cut = line.find("//").into_iter();
                let cut = cut.chain(
                    line.find('#')
                        .filter(|_| line.trim_start().starts_with('#')),
                );
                match cut.min() {
                    Some(at) => &line[..at],
                    None => line,
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Does `code` reference `key` as a whole name?
    ///
    /// A plain substring search is not enough: a short key can sit inside a
    /// longer one, borrow its mentions and look alive. Env names are
    /// `[A-Za-z0-9_]`, so a match counts only when neither neighbour could be
    /// part of the same name.
    fn mentions(code: &str, key: &str) -> bool {
        let boundary =
            |c: Option<char>| !matches!(c, Some(c) if c.is_ascii_alphanumeric() || c == '_');
        code.match_indices(key).any(|(at, _)| {
            boundary(code[..at].chars().next_back())
                && boundary(code[at + key.len()..].chars().next())
        })
    }

    fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn installing_creates_directories_and_copies_nothing() {
        let root = scratch("install");
        assert_eq!(fitness(&root), Fitness::Installable);

        let created = install(&root).unwrap();
        assert!(root.join("generated/configs").is_dir());
        assert!(root.join("logs/services").is_dir());
        assert_eq!(created.len(), DIRECTORIES.len());

        // Not `projects`. Creating it would answer "where do you keep your
        // code" with a directory nobody chose — `workspace::projects_root`
        // falls back to exactly this path, so its absence is what makes the app
        // ask.
        assert!(
            !root.join("projects").exists(),
            "installing claimed the user's project directory"
        );

        // No settings file. A fresh workspace overrides nothing, so there is
        // nothing to write; the file appears when Settings saves the first
        // change, and until then its absence is the state.
        assert!(
            !root.join(".env").exists(),
            "a workspace should start with no overrides"
        );

        // And no templates. The binary has them; a copy on disk is what an
        // override *is*, and nobody has made one yet.
        assert!(
            !root.join("core").exists(),
            "installing wrote template copies nobody asked for"
        );
        assert!(!root.join("README.md").exists());

        // Recognisable as a workspace on the next launch — the thing that
        // breaks if `fitness` keeps looking for the files install stopped
        // writing.
        assert_eq!(fitness(&root), Fitness::Existing);

        // A second install is a no-op rather than a repair.
        assert!(install(&root).unwrap().is_empty(), "reinstall wrote again");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn overriding_is_a_file_appearing_and_reverting_is_it_going_away() {
        let root = scratch("override-cycle");
        install(&root).unwrap();

        const TARGET: &str = "core/templates/services/redis/docker-compose.redis.tpl";
        assert!(
            overridden(&root).is_empty(),
            "a fresh workspace owns nothing"
        );
        assert!(overridable().iter().any(|p| p == TARGET));

        let text = materialize(&root, TARGET).unwrap();
        assert!(text.contains("stackvo-redis"));
        assert_eq!(std::fs::read_to_string(root.join(TARGET)).unwrap(), text);
        assert_eq!(overridden(&root), vec![TARGET.to_string()]);

        // The second call is the dangerous one: it is the path that would
        // replace an edit with the shipped bytes.
        std::fs::write(root.join(TARGET), "# mine\n").unwrap();
        assert!(
            materialize(&root, TARGET).is_err(),
            "an edit was overwritten"
        );
        assert_eq!(
            std::fs::read_to_string(root.join(TARGET)).unwrap(),
            "# mine\n"
        );

        revert(&root, TARGET).unwrap();
        assert!(!root.join(TARGET).exists());
        assert!(overridden(&root).is_empty());
        // Reverting twice is not an error — the caller asked for a state.
        revert(&root, TARGET).unwrap();
        // And the embedded copy is serving again.
        assert!(read_template(&root, TARGET)
            .unwrap()
            .contains("stackvo-redis"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The relative path comes from the front end and is joined onto the
    /// workspace root before being written to.
    #[test]
    fn only_files_the_binary_ships_can_be_written() {
        let root = scratch("override-guard");
        install(&root).unwrap();

        for bad in [
            "../../../etc/passwd",
            "core/../../escape.yml",
            ".env",
            "projects/mine/stackvo.json",
            "README.md",
        ] {
            assert!(materialize(&root, bad).is_err(), "{bad} was accepted");
            assert!(
                revert(&root, bad).is_err(),
                "{bad} was accepted for removal"
            );
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The migration every workspace made before this change needs.
    #[test]
    fn an_older_installs_untouched_copies_are_swept_and_edits_are_not() {
        let root = scratch("prune");
        install(&root).unwrap();

        // Recreate what the old install produced: every shipped file on disk,
        // byte-identical to the binary's.
        let mut wrote = 0;
        for rel in overridable() {
            let target = root.join(&rel);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, read_template(&root, &rel).unwrap()).unwrap();
            wrote += 1;
        }
        assert!(wrote >= 30, "expected the whole skeleton, wrote {wrote}");
        assert_eq!(
            overridden(&root).len(),
            wrote,
            "the setup is not the old state"
        );

        // One of them is a real edit, and one is a file the app never shipped.
        const EDITED: &str = "core/compose/base.yml";
        std::fs::write(root.join(EDITED), "# mine\n").unwrap();
        std::fs::write(root.join("core/mine.txt"), "keep me\n").unwrap();

        let removed = prune_pristine(&root);
        assert_eq!(removed, wrote - 1, "swept the wrong number of files");
        assert_eq!(overridden(&root), vec![EDITED.to_string()]);
        assert_eq!(
            std::fs::read_to_string(root.join(EDITED)).unwrap(),
            "# mine\n"
        );

        // A directory holding something that survived is not empty and stays;
        // the ones that emptied out are gone.
        assert!(root.join("core/mine.txt").is_file());
        assert!(
            !root.join("core/templates").exists(),
            "an empty tree was left behind"
        );

        // Idempotent, and the render is unchanged by any of it.
        assert_eq!(prune_pristine(&root), 0);
        assert!(read_template(
            &root,
            "core/templates/services/redis/docker-compose.redis.tpl"
        )
        .unwrap()
        .contains("stackvo-redis"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bug the union in `all_service_templates` exists for.
    #[test]
    fn a_deleted_template_still_contributes_its_volumes() {
        let root = scratch("harvest");
        install(&root).unwrap();

        let all = all_service_templates(&root);
        let baseline = all.len();
        assert!(
            baseline >= 20,
            "expected the whole catalogue, got {baseline}"
        );

        // Materialise one and delete it: the workspace now *has* a
        // `core/templates` directory, which is what used to switch the harvest
        // to disk-only and lose every template that was not in it.
        const TARGET: &str = "core/templates/services/mysql/docker-compose.mysql.tpl";
        materialize(&root, TARGET).unwrap();
        std::fs::remove_file(root.join(TARGET)).unwrap();
        assert!(root.join("core/templates").is_dir());

        let after = all_service_templates(&root);
        assert_eq!(after.len(), baseline, "the harvest lost templates");
        assert!(
            after.iter().any(|t| t.contains("stackvo-mysql-data")),
            "mysql's volume is missing from the harvest"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_with_unrelated_content_is_refused_rather_than_merged() {
        let root = scratch("occupied");
        std::fs::write(root.join("thesis.pdf"), "…").unwrap();
        assert_eq!(fitness(&root), Fitness::Occupied);

        // A dotfile is not content: a fresh `git init` or a stray .DS_Store
        // must not make an empty folder un-installable.
        let dotted = scratch("dotted");
        std::fs::write(dotted.join(".DS_Store"), "").unwrap();
        assert_eq!(fitness(&dotted), Fitness::Installable);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dotted);
    }

    #[test]
    fn a_workspace_template_wins_over_the_compiled_in_one() {
        let root = scratch("override");
        install(&root).unwrap();

        let shipped = read_template(&root, "core/compose/base.yml").unwrap();
        assert!(shipped.contains("traefik"));

        std::fs::create_dir_all(root.join("core/compose")).unwrap();
        std::fs::write(root.join("core/compose/base.yml"), "# edited\n").unwrap();
        assert_eq!(
            read_template(&root, "core/compose/base.yml").unwrap(),
            "# edited\n"
        );

        // And a file the workspace does not have still resolves.
        let _ = std::fs::remove_file(root.join("core/compose/base.yml"));
        assert!(read_template(&root, "core/compose/base.yml")
            .unwrap()
            .contains("traefik"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
