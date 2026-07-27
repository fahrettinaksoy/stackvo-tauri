//! Watching `projects/*/stackvo.json` for edits.
//!
//! StackVo's config flow is manual: edit a manifest, remember to run
//! `stackvo generate`, remember to restart. Forgetting either leaves the
//! containers running against a stale Dockerfile with nothing to indicate it.
//!
//! A host process can just watch the files. This module does not regenerate
//! anything on its own — it emits `manifest:changed` and lets the UI offer the
//! action, because silently rebuilding a container underneath someone who is
//! mid-edit is worse than the problem it solves.

use crate::events;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Holds the live watcher so it can be replaced.
///
/// Dropping a `notify` watcher stops the watch, so the handle has to be kept —
/// and kept somewhere replaceable, or changing the workspace would leave the
/// app watching a directory the user has moved away from. That was the case
/// until this existed: the new workspace only got a watcher on the next launch.
#[derive(Default)]
pub struct Handle(Mutex<Option<notify::RecommendedWatcher>>);

impl Handle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Point the watcher at `root`, replacing whatever it was watching.
    ///
    /// The old watcher is dropped first: two watchers on overlapping trees
    /// would emit every change twice, and the debounce is per-watcher.
    pub fn retarget(&self, app: &AppHandle, root: Option<PathBuf>) {
        let Ok(mut slot) = self.0.lock() else { return };
        *slot = None;

        if let Some(root) = root {
            *slot = start(app.clone(), root).ok();
        }
    }
}

/// Editors write a file several times in a burst (temp file, rename, chmod).
/// Anything inside this window after the first event is the same logical edit.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// Which project a changed path belongs to, if any.
///
/// Accepts only `<root>/projects/<name>/stackvo.json` — deliberately narrow, so
/// an editor's `stackvo.json~` backup or a file deeper in the tree does not
/// trigger a regenerate prompt.
fn project_for(root: &Path, changed: &Path) -> Option<String> {
    if changed.file_name()? != "stackvo.json" {
        return None;
    }

    let projects = root.join("projects");
    let relative = changed.strip_prefix(&projects).ok()?;
    let mut parts = relative.components();
    let name = parts.next()?.as_os_str().to_str()?.to_string();

    // Exactly one directory level between projects/ and the file.
    (parts.next().is_some() && parts.next().is_none()).then_some(name)
}

/// Start watching. The returned watcher must be kept alive — dropping it stops
/// the watch, which is why it is parked in Tauri's managed state.
pub fn start(app: AppHandle, root: PathBuf) -> notify::Result<notify::RecommendedWatcher> {
    let projects_dir = root.join("projects");
    let (tx, rx) = mpsc::channel::<Event>();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;

    watcher.watch(&projects_dir, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        let mut last: Vec<(String, Instant)> = Vec::new();

        for event in rx {
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                continue;
            }

            for path in &event.paths {
                let Some(project) = project_for(&root, path) else {
                    continue;
                };

                let now = Instant::now();
                last.retain(|(_, at)| now.duration_since(*at) < DEBOUNCE);
                if last.iter().any(|(p, _)| p == &project) {
                    continue;
                }
                last.push((project.clone(), now));

                events::emit(
                    &app,
                    "manifest:changed",
                    serde_json::json!({
                        "project": project,
                        "path": path.display().to_string(),
                    }),
                );
            }
        }
    });

    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/w/stackvo";

    #[test]
    fn matches_a_project_manifest() {
        assert_eq!(
            project_for(
                Path::new(ROOT),
                Path::new("/w/stackvo/projects/shop/stackvo.json")
            ),
            Some("shop".to_string())
        );
    }

    #[test]
    fn ignores_editor_backups_and_other_files() {
        for path in [
            "/w/stackvo/projects/shop/stackvo.json~",
            "/w/stackvo/projects/shop/composer.json",
            "/w/stackvo/projects/shop/.stackvo/Dockerfile",
        ] {
            assert_eq!(
                project_for(Path::new(ROOT), Path::new(path)),
                None,
                "{path}"
            );
        }
    }

    #[test]
    fn ignores_manifests_outside_the_one_level_layout() {
        // Too shallow, and too deep — neither is a project manifest.
        assert_eq!(
            project_for(
                Path::new(ROOT),
                Path::new("/w/stackvo/projects/stackvo.json")
            ),
            None
        );
        assert_eq!(
            project_for(
                Path::new(ROOT),
                Path::new("/w/stackvo/projects/a/b/stackvo.json")
            ),
            None
        );
    }

    #[test]
    fn ignores_paths_outside_the_projects_directory() {
        assert_eq!(
            project_for(Path::new(ROOT), Path::new("/w/stackvo/core/stackvo.json")),
            None
        );
    }
}
