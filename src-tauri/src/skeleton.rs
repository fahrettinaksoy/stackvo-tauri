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

use crate::error::{Error, Result};
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

/// Directories every workspace has, whether or not anything is in them yet.
const DIRECTORIES: [&str; 5] = [
    "projects",
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
    if path.join("core/templates").is_dir() && path.join("projects").is_dir() {
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

/// Write the skeleton into `root`, creating what is missing and touching what
/// is already there as little as possible.
///
/// **Never overwrites.** A template the user edited is the reason the
/// workspace copy wins over the compiled-in one at read time; overwriting it
/// on the next launch would undo that quietly. Returns what it actually
/// wrote, so the caller can say so rather than claiming a fresh install.
pub fn install(root: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();

    for dir in DIRECTORIES {
        std::fs::create_dir_all(root.join(dir))
            .map_err(|e| Error::io(format!("creating {dir}"), e))?;
    }

    for file in files_of(&SKELETON) {
        let rel = file.path();
        // No `.env` is written. Every setting has a default in the binary, so
        // a fresh workspace has nothing to override and the file only appears
        // when Settings writes the first key somebody changed. The README
        // explains the skeleton to a reader of *this* repository and has no
        // business in the user's workspace.
        let target = match rel.to_string_lossy().as_ref() {
            "README.md" => continue,
            other => root.join(other),
        };

        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io(format!("creating {}", parent.display()), e))?;
        }
        std::fs::write(&target, file.contents())
            .map_err(|e| Error::io(format!("writing {}", target.display()), e))?;
        written.push(
            target
                .strip_prefix(root)
                .unwrap_or(&target)
                .display()
                .to_string(),
        );
    }

    Ok(written)
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
pub fn all_service_templates(root: &Path) -> Vec<String> {
    let dir = root.join("core/templates");
    if dir.is_dir() {
        let mut out = Vec::new();
        collect_tpl(&dir, &mut out);
        return out;
    }

    files_of(&SKELETON)
        .into_iter()
        .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("tpl"))
        .filter_map(|f| f.contents_utf8().map(str::to_string))
        .collect()
}

fn collect_tpl(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_tpl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("tpl") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push(text);
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
    fn installing_creates_a_workspace_and_never_overwrites() {
        let root = scratch("install");
        assert_eq!(fitness(&root), Fitness::Installable);

        let written = install(&root).unwrap();
        // No settings file. A fresh workspace overrides nothing, so there is
        // nothing to write; the file appears when Settings saves the first
        // change, and until then its absence is the state.
        assert!(
            !root.join(".env").exists(),
            "a workspace should start with no overrides: {written:?}"
        );
        assert!(root.join("core/templates/services/redis").is_dir());
        assert!(root.join("projects").is_dir());
        assert!(root.join("logs/services").is_dir());
        // The README explains the skeleton to a reader of this repository; it
        // has no business in somebody's workspace.
        assert!(!root.join("README.md").exists());
        assert_eq!(fitness(&root), Fitness::Existing);

        // An edited template survives a second install — which is what makes
        // "the workspace copy wins" a promise rather than a race.
        let edited = root.join("core/compose/base.yml");
        std::fs::write(&edited, "# mine\n").unwrap();
        let second = install(&root).unwrap();
        assert!(second.is_empty(), "reinstall rewrote {second:?}");
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "# mine\n");

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
