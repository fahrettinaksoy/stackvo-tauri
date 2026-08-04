//! Catching debug signals out of a running container, without restarting it.
//!
//! ## What was wrong with the version this replaces
//!
//! [`crate::dumps`] points Symfony's `VarDumper` at Symfony's own
//! `var-dump-server`, started with `docker exec` inside the project container.
//! It works, and three things about it cannot be fixed from where it stands:
//!
//! 1. **Turning it on costs a container.** The two variables arrive through a
//!    compose overlay, and a container's environment is fixed when it is
//!    created — so "start catching dumps" means recreating the container and
//!    waiting for the stack. Herd and Lerd both toggle theirs live; Lerd sells
//!    that explicitly ("no FPM container restart, no worker cascade").
//! 2. **It only sees `dump()`.** Queries, jobs, rendered views and outgoing
//!    HTTP calls are the signals people actually spend the day on, and none of
//!    them reaches `VarDumper`.
//! 3. **The output is text.** Symfony's CLI renderer produces a formatted
//!    block; the app streams it into a `<pre>`. Nothing can be searched,
//!    grouped by request, or linked back to the line that produced it, because
//!    by the time it arrives it is no longer data.
//!
//! ## The mechanism
//!
//! A PHP file, mounted into the container and loaded through
//! `auto_prepend_file`, plus a sentinel file the host creates and removes.
//!
//! `auto_prepend_file` runs **before the application's autoloader**, and that
//! is what makes this work at all: Symfony guards its own helpers with
//! `function_exists('dump')`, so whichever definition arrives first wins, and
//! the prepend file is always first. Lerd's bridge does the same thing for the
//! same reason. Nothing is hooked, patched or observed — the functions are
//! simply declared.
//!
//! It follows that the bridge must declare them **only while capture is on**.
//! With the sentinel absent it declares nothing and returns after a single
//! `is_file`, so Symfony's own `dump()` loads and behaves exactly as it does
//! without this app. That is also why toggling is free: the next request reads
//! the flag again, and a flag is a file in a directory that is already mounted.
//!
//! ## Why a file and not a socket
//!
//! The obvious design is a receiver on the host and a socket from the
//! container, which is what both competitors do — and what the Xdebug overlay
//! already proves is reachable, since it dials `host.docker.internal`. It was
//! rejected on two counts. A host-side listener has to bind somewhere, and the
//! only address every platform agrees a container can reach is not loopback —
//! a dump stream carries request bodies and query bindings, and putting that on
//! a listening port is a bigger decision than the feature is worth. And the
//! events have to survive a page reload to be worth grouping, which means
//! writing them down somewhere regardless.
//!
//! So the bridge appends newline-delimited JSON to a file in a writable mount,
//! and the app tails it. No port, no `host.docker.internal`, no difference
//! between platforms, and "keep the last N events across restarts" — a paid
//! feature elsewhere — is what the design already does.
//!
//! ## Verified in a container, not reasoned about
//!
//! Every claim above was run against `php 8.4` in one of this checkout's own
//! project containers before the module was finished:
//!
//! * with the sentinel absent the bridge declares nothing, so `dump()` is
//!   still Symfony's;
//! * with it present, `dump()` captures and returns its argument, and `dd()`
//!   captures and exits 1;
//! * the emitted line parses as the [`Event`] below, with file, line, request
//!   and SAPI filled in.
//!
//! One thing that only shows up by running it: **`php -r` does not process
//! `auto_prepend_file` at all.** `ini_get` reports the setting and the file is
//! never loaded. It costs nothing here — web requests, `artisan` and queue
//! workers all execute a script — but a `-r` one-liner is not a way to test
//! this, and half an hour went into finding that out.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the read-only half is mounted: the bridge and the sentinel.
pub const CONF_DIR: &str = "/usr/local/etc/stackvo";

/// Where the writable half is mounted. Deliberately *not* nested inside
/// [`CONF_DIR`]: a bind mount inside another bind mount is ordering-sensitive
/// on some engines, and there is nothing to gain by finding out which.
pub const EVENTS_DIR: &str = "/usr/local/etc/stackvo-events";

/// The ini in `conf.d`, mounted as a single file.
///
/// `95-` so it lands before the user's `zz-stackvo.ini` and the profiler's
/// `zzz-stackvo-xdebug.ini`. Ordering does not matter for `auto_prepend_file`
/// itself, but a config file that sorts into the middle of somebody else's
/// numbering is a thing to reason about later for no reason.
pub const INI_CONTAINER_PATH: &str = "/usr/local/etc/php/conf.d/95-stackvo-debug.ini";

/// The events file, inside [`EVENTS_DIR`].
pub const EVENTS_FILE: &str = "events.ndjson";

/// Stop growing the file. Reached, it is rotated to `.1` and started again, so
/// the worst case on disk is two of these rather than one that never stops.
pub const MAX_EVENTS_BYTES: u64 = 8 * 1024 * 1024;

pub fn overlay_path(root: &Path) -> PathBuf {
    root.join("generated").join("docker-compose.debug.yml")
}

/// `<root>/generated/debug/<project>` — the per-project home for all of it.
///
/// Per project rather than one shared directory so the bridge can always look
/// at the same absolute path: it has no way to learn which project it is in
/// without an environment variable, and an environment variable would need the
/// container recreating, which is the whole thing this avoids.
pub fn project_dir(root: &Path, project: &str) -> PathBuf {
    root.join("generated").join("debug").join(project)
}

pub fn conf_dir(root: &Path, project: &str) -> PathBuf {
    project_dir(root, project).join("conf")
}

pub fn events_dir(root: &Path, project: &str) -> PathBuf {
    project_dir(root, project).join("events")
}

pub fn sentinel_path(root: &Path, project: &str) -> PathBuf {
    conf_dir(root, project).join("enabled.flag")
}

pub fn events_path(root: &Path, project: &str) -> PathBuf {
    events_dir(root, project).join(EVENTS_FILE)
}

/// One captured signal, as the bridge writes it and the UI reads it.
///
/// Every field is optional except the ones the bridge can always know, because
/// this is parsed from a file a container wrote: a line that is missing
/// something is worth showing without its label, not worth dropping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Seconds since the epoch, with fractions — the bridge's `microtime`.
    pub at: f64,
    /// `dump` today; the field exists so queries and jobs do not need a second
    /// file and a second reader when they arrive.
    pub kind: String,
    /// The variable's name where `dump($user)` makes one available.
    #[serde(default)]
    pub label: Option<String>,
    /// Relative to the project root where it could be made relative.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    /// `GET /api/health`, or the artisan command for a CLI run.
    #[serde(default)]
    pub request: Option<String>,
    /// `fpm-fcgi`, `cli`, … — what tells a queue worker from a web request.
    #[serde(default)]
    pub sapi: Option<String>,
    /// The rendered value. Bounded by the bridge, not here.
    pub value: String,
}

/// The ini that loads the bridge. Written once and never changed at runtime.
///
/// Mounted as a single file, and that is a constraint worth stating: a file
/// bind mount follows the inode, and this app writes through
/// [`crate::atomic`], which renames a new file over the old one. So an edit
/// here does not reach a running container — which is fine, because it never
/// gets edited. The sentinel does, and it lives in a *directory* mount, where
/// creating and removing files is seen immediately.
pub fn ini() -> String {
    format!(
        "; Generated by StackVo Desktop — do not edit.\n\
         ;\n\
         ; Loads the debug bridge before the application. The bridge does\n\
         ; nothing at all unless {CONF_DIR}/enabled.flag exists, so this costs\n\
         ; one stat per request while capture is off.\n\
         auto_prepend_file={CONF_DIR}/bridge.php\n"
    )
}

/// The bridge itself.
///
/// Written as one string rather than shipped as an asset because it is
/// generated *per project* — the paths are absolute inside the container and
/// identical for every project, but the file has to exist in each project's
/// own mounted directory, and one source of truth for its text beats a copy
/// that can drift.
pub fn bridge_php() -> String {
    format!(
        r#"<?php
// Generated by StackVo Desktop — do not edit; rewritten before every compose
// command.
//
// Loaded through auto_prepend_file, which runs before the application's
// autoloader. That is why dump() and dd() are DEFINED here rather than hooked:
// Symfony guards its own helpers with function_exists(), so whichever arrives
// first wins, and this file is always first.
//
// Nothing below runs unless the sentinel exists. With capture off this file
// declares no functions at all, so Symfony's own dump() loads and behaves
// exactly as it would without StackVo.

if (!@is_file('{CONF_DIR}/enabled.flag')) {{
    return;
}}

if (!function_exists('__stackvo_emit')) {{
    /**
     * Render a value with hard bounds.
     *
     * Not Symfony's cloner: it is not loaded yet and cannot be. The bounds are
     * the point — an Eloquent model graph or a container instance will happily
     * serialise to megabytes, and a debug pane that stalls the request it is
     * observing is worse than no debug pane.
     */
    function __stackvo_render($value, int $depth = 0): string
    {{
        if ($depth > 4) {{
            return '…';
        }}
        if ($value === null) {{
            return 'null';
        }}
        if (is_bool($value)) {{
            return $value ? 'true' : 'false';
        }}
        if (is_int($value) || is_float($value)) {{
            return (string) $value;
        }}
        if (is_string($value)) {{
            $cut = strlen($value) > 512;
            return '"' . ($cut ? substr($value, 0, 512) . '…' : $value) . '"';
        }}
        if (is_array($value)) {{
            $out = [];
            $n = 0;
            foreach ($value as $k => $v) {{
                if ($n++ >= 50) {{
                    $out[] = str_repeat('  ', $depth + 1) . '… ' . (count($value) - 50) . ' more';
                    break;
                }}
                $out[] = str_repeat('  ', $depth + 1) . var_export($k, true)
                    . ' => ' . __stackvo_render($v, $depth + 1);
            }}
            return $out ? "[\n" . implode(",\n", $out) . "\n" . str_repeat('  ', $depth) . ']' : '[]';
        }}
        if (is_object($value)) {{
            $class = get_class($value);
            // Closures and resources have nothing worth walking into.
            if ($value instanceof \Closure) {{
                return 'Closure';
            }}
            $props = [];
            $n = 0;
            foreach ((array) $value as $k => $v) {{
                if ($n++ >= 50) {{
                    $props[] = str_repeat('  ', $depth + 1) . '…';
                    break;
                }}
                // Private and protected keys arrive NUL-padded from the cast.
                $name = str_replace("\0", '·', (string) $k);
                $props[] = str_repeat('  ', $depth + 1) . $name . ': '
                    . __stackvo_render($v, $depth + 1);
            }}
            return $class . ($props ? " {{\n" . implode(",\n", $props) . "\n"
                . str_repeat('  ', $depth) . '}}' : ' {{}}');
        }}
        return gettype($value);
    }}

    /** Where the call came from, skipping this file's own frames. */
    function __stackvo_origin(): array
    {{
        $frames = @debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 8) ?: [];
        foreach ($frames as $frame) {{
            $file = $frame['file'] ?? '';
            if ($file !== '' && strpos($file, '{CONF_DIR}') !== 0) {{
                return [$file, $frame['line'] ?? null];
            }}
        }}
        return ['', null];
    }}

    function __stackvo_emit(string $kind, array $payload): void
    {{
        // Checked again rather than once at load: a long-running queue worker
        // outlives many toggles, and a worker that kept writing after the
        // switch was turned off is exactly the "worker cascade" this design
        // exists to avoid.
        if (!@is_file('{CONF_DIR}/enabled.flag')) {{
            return;
        }}

        [$file, $line] = __stackvo_origin();

        $request = null;
        if (PHP_SAPI === 'cli') {{
            $request = implode(' ', array_slice($_SERVER['argv'] ?? [], 0, 6));
        }} elseif (isset($_SERVER['REQUEST_URI'])) {{
            $request = trim(($_SERVER['REQUEST_METHOD'] ?? '') . ' ' . $_SERVER['REQUEST_URI']);
        }}

        $event = $payload + [
            'at' => microtime(true),
            'kind' => $kind,
            'file' => $file !== '' ? $file : null,
            'line' => $line,
            'request' => $request !== '' ? $request : null,
            'sapi' => PHP_SAPI,
        ];

        $line = @json_encode($event, JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE);
        if ($line === false) {{
            return;
        }}
        // LOCK_EX because several FPM workers share this file. One line per
        // write and an exclusive lock is enough; nothing here needs ordering
        // beyond what the filesystem already gives.
        @file_put_contents('{EVENTS_DIR}/{EVENTS_FILE}', $line . "\n", FILE_APPEND | LOCK_EX);
    }}
}}

if (!function_exists('dump')) {{
    function dump(...$vars)
    {{
        foreach ($vars as $key => $value) {{
            __stackvo_emit('dump', [
                'label' => is_string($key) ? $key : null,
                'value' => __stackvo_render($value),
            ]);
        }}
        return count($vars) === 1 ? $vars[array_key_first($vars)] : $vars;
    }}
}}

if (!function_exists('dd')) {{
    function dd(...$vars)
    {{
        // Symfony's own dd() sets this, and it is preserved deliberately: it
        // exists so a dd() left in code cannot be cached as a success, and
        // quietly disagreeing with the framework about what dd() means is a
        // worse surprise than the 500 itself.
        if (!in_array(PHP_SAPI, ['cli', 'phpdbg', 'embed'], true) && !headers_sent()) {{
            header('HTTP/1.1 500 Internal Server Error');
        }}

        foreach ($vars as $key => $value) {{
            __stackvo_emit('dump', [
                'label' => is_string($key) ? $key : null,
                'value' => __stackvo_render($value),
            ]);
        }}

        exit(1);
    }}
}}
"#
    )
}

/// One project's worth of overlay input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub service: String,
    pub conf_host: String,
    pub events_host: String,
    pub ini_host: String,
}

/// Render the overlay, or None when no project is eligible.
///
/// Three mounts and not one: the configuration is read-only because the
/// container has no business rewriting what this app generated for it, the
/// events directory has to be writable because that is the whole channel, and
/// the ini has to land in `conf.d` by itself — mounting the directory would
/// shadow every other ini in it.
pub fn overlay_yaml(entries: &[Entry]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# Generated by StackVo Desktop — do not edit.\n\
         #\n\
         # Mounts the debug bridge. The bridge does nothing until the sentinel\n\
         # file appears in the conf directory, which is why turning capture on\n\
         # and off needs no container at all — only these mounts do.\n\
         #\n\
         # NOTE: `stackvo up` from the Bash CLI does not layer this file.\n\
         services:\n",
    );

    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.service.cmp(&b.service));

    for entry in sorted {
        out.push_str(&format!("  {}:\n", entry.service));
        out.push_str("    volumes:\n");
        // Quoted because a Windows host path contains a colon, which is the
        // separator compose splits these on.
        out.push_str(&format!("      - \"{}:{CONF_DIR}:ro\"\n", entry.conf_host));
        out.push_str(&format!("      - \"{}:{EVENTS_DIR}\"\n", entry.events_host));
        out.push_str(&format!(
            "      - \"{}:{INI_CONTAINER_PATH}:ro\"\n",
            entry.ini_host
        ));
    }

    Some(out)
}

// ------------------------------------------------------------------- I/O

/// Every PHP project with a compose service to mount the bridge on.
///
/// Eligibility is deliberately wide — every PHP project, whether or not anyone
/// has switched capture on. The mounts are what needs a container, so they go
/// in once, for everybody, and the switch afterwards is free. A project that
/// never uses this pays three bind mounts and one `is_file` per request.
///
/// The service check is not belt-and-braces: naming a service the generator did
/// not emit declares one with neither an image nor a build context, and compose
/// then refuses every command against the stack.
fn entries(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();

    let generated =
        std::fs::read_to_string(root.join("generated").join("docker-compose.projects.yml"))
            .unwrap_or_default();
    let services = crate::xdebug::generated_services(&generated);

    let Some(projects) = crate::workspace::projects_root(root) else {
        return out;
    };
    let Ok(dirs) = std::fs::read_dir(&projects) else {
        return out;
    };

    for dir in dirs.flatten() {
        let path = dir.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !services.iter().any(|s| s == name) {
            continue;
        }
        let manifest = path.join("stackvo.json");
        if !manifest.is_file() {
            continue;
        }
        let Ok(m) = crate::manifest::read(&manifest, name) else {
            continue;
        };
        if m.runtime != "php" {
            continue;
        }

        let conf = conf_dir(root, name);
        let events = events_dir(root, name);
        if std::fs::create_dir_all(&conf).is_err() || std::fs::create_dir_all(&events).is_err() {
            continue;
        }

        // Rewritten every time, like every other generated file: an overlay
        // that is a pure function of the tree cannot be stale.
        let ini_file = conf.join("stackvo-debug.ini");
        if crate::atomic::write(&conf.join("bridge.php"), &bridge_php()).is_err()
            || crate::atomic::write(&ini_file, &ini()).is_err()
        {
            continue;
        }

        out.push(Entry {
            service: name.to_string(),
            conf_host: crate::paths::to_docker_mount(&conf.display().to_string()),
            events_host: crate::paths::to_docker_mount(&events.display().to_string()),
            ini_host: crate::paths::to_docker_mount(&ini_file.display().to_string()),
        });
    }

    out.sort_by(|a, b| a.service.cmp(&b.service));
    out
}

/// Re-render the overlay. True when it exists and should be layered.
pub fn sync(root: &Path) -> bool {
    let path = overlay_path(root);

    match overlay_yaml(&entries(root)) {
        Some(yaml) => {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
            // A write failure must not take compose down with it. The honest
            // degradation is "the bridge is not mounted", which `status` then
            // reports, rather than "no container can be started".
            match crate::atomic::write(&path, &yaml) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not write the debug overlay");
                    let _ = std::fs::remove_file(&path);
                    false
                }
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

/// What the pane needs to know.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// PHP, and the generator emitted a service for it.
    pub supported: bool,
    /// The sentinel exists — capture is on, with no container involved.
    pub enabled: bool,
    /// The mounts are in the container that is actually running.
    ///
    /// Read from the container, never inferred: `stackvo up` from the Bash CLI
    /// layers three compose files, not eight, and will recreate a container
    /// without them.
    pub mounted: bool,
    pub running: bool,
    /// How many events are waiting in the file.
    pub events: usize,
}

pub async fn status(root: &Path, name: &str) -> Result<Status> {
    let dir = crate::workspace::project_dir(root, name)?;
    let manifest_file = dir.join("stackvo.json");
    if !manifest_file.is_file() {
        return Err(Error::not_found(format!("project {name}")));
    }
    let manifest = crate::manifest::read(&manifest_file, name)?;

    let details = crate::engine::inspect(name).await.ok();
    let mounted = details.as_ref().is_some_and(|d| {
        d.mounts
            .iter()
            .any(|m| m.destination == EVENTS_DIR || m.destination == CONF_DIR)
    });

    Ok(Status {
        supported: manifest.runtime == "php",
        enabled: sentinel_path(root, name).is_file(),
        mounted,
        running: details.as_ref().is_some_and(|d| d.running),
        events: read_events(root, name).len(),
    })
}

/// Turn capture on or off. No container is touched — that is the feature.
pub fn set_enabled(root: &Path, name: &str, on: bool) -> Result<()> {
    crate::workspace::project_dir(root, name)?;
    let flag = sentinel_path(root, name);

    if on {
        if let Some(parent) = flag.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::io("creating the debug directory", e))?;
        }
        std::fs::write(&flag, "")
            .map_err(|e| Error::io(format!("writing {}", flag.display()), e))?;
    } else {
        match std::fs::remove_file(&flag) {
            Ok(()) => {}
            // Already off. The caller asked for a state, not an operation.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(format!("removing {}", flag.display()), e)),
        }
    }
    Ok(())
}

/// Parse the events file, newest last.
///
/// A malformed line is skipped rather than failing the read: the file is
/// appended to by several processes, and a partial write during a crash must
/// not cost the reader every event before it.
pub fn read_events(root: &Path, name: &str) -> Vec<Event> {
    let Ok(text) = std::fs::read_to_string(events_path(root, name)) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Event>(l).ok())
        .collect()
}

/// Drop everything recorded so far.
///
/// Removed rather than truncated: the file is written by the container, which
/// may be running as a different user, and a host process can always delete an
/// entry from a directory it owns even when it cannot write the file itself.
pub fn clear(root: &Path, name: &str) -> Result<()> {
    crate::workspace::project_dir(root, name)?;
    let path = events_path(root, name);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::new(
            Code::IoError,
            format!("could not clear {}: {e}", path.display()),
        )),
    }
}

/// Keep the file from growing without limit.
///
/// Called on every read rather than on a timer: the only process that can grow
/// it is the container, and the only one that reads it is this app, so the
/// moment a read happens is the moment the size is known to be current.
pub fn rotate_if_large(root: &Path, name: &str) {
    let path = events_path(root, name);
    let Ok(meta) = std::fs::metadata(&path) else {
        return;
    };
    if meta.len() <= MAX_EVENTS_BYTES {
        return;
    }
    let _ = std::fs::rename(&path, path.with_extension("ndjson.1"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_declares_nothing_until_the_sentinel_exists() {
        let php = bridge_php();
        // The early return has to come before every declaration, or a request
        // with capture off pays for parsing and loses Symfony's own dump().
        let guard = php.find("enabled.flag").expect("no sentinel check");
        let first_fn = php.find("function ").expect("no functions at all");
        assert!(
            guard < first_fn,
            "the sentinel is checked after something is already declared"
        );
        assert!(php.contains("return;"));
    }

    /// The whole mechanism rests on this: Symfony guards its helpers with
    /// `function_exists`, so the bridge must guard its own the same way — or
    /// two definitions collide and the request fatals.
    #[test]
    fn the_helpers_are_declared_defensively() {
        let php = bridge_php();
        for name in ["dump", "dd", "__stackvo_emit"] {
            assert!(
                php.contains(&format!("if (!function_exists('{name}'))")),
                "{name} is declared without a guard"
            );
        }
    }

    /// `dd()` sets a 500 in Symfony's own implementation so a forgotten call
    /// cannot be cached as a success. Owning the function is not a licence to
    /// quietly disagree with the framework about what it means.
    #[test]
    fn dd_keeps_the_status_and_the_exit_symfony_gives_it() {
        let php = bridge_php();
        assert!(php.contains("500 Internal Server Error"));
        assert!(php.contains("exit(1);"));
    }

    #[test]
    fn the_overlay_mounts_all_three_and_only_the_events_are_writable() {
        let yaml = overlay_yaml(&[Entry {
            service: "shop".into(),
            conf_host: "/w/generated/debug/shop/conf".into(),
            events_host: "/w/generated/debug/shop/events".into(),
            ini_host: "/w/generated/debug/shop/conf/stackvo-debug.ini".into(),
        }])
        .expect("no overlay");

        assert!(yaml.contains(&format!("/w/generated/debug/shop/conf:{CONF_DIR}:ro")));
        assert!(yaml.contains(&format!("/w/generated/debug/shop/events:{EVENTS_DIR}\"")));
        assert!(yaml.contains(&format!("stackvo-debug.ini:{INI_CONTAINER_PATH}:ro")));
        // The events mount must not be read-only, or the bridge writes nothing
        // and the pane stays empty with everything else looking correct.
        assert!(!yaml.contains(&format!("{EVENTS_DIR}:ro")));
    }

    #[test]
    fn no_eligible_project_means_no_overlay_at_all() {
        assert!(overlay_yaml(&[]).is_none());
    }

    #[test]
    fn the_ini_points_at_the_bridge_the_overlay_mounts() {
        assert!(ini().contains(&format!("auto_prepend_file={CONF_DIR}/bridge.php")));
    }

    #[test]
    fn a_malformed_line_costs_only_itself() {
        let dir = std::env::temp_dir().join(format!("stackvo-bridge-{}", std::process::id()));
        let events = dir.join("generated/debug/shop/events");
        std::fs::create_dir_all(&events).unwrap();
        std::fs::write(
            events.join(EVENTS_FILE),
            "{\"at\":1.0,\"kind\":\"dump\",\"value\":\"one\"}\n\
             not json at all\n\
             \n\
             {\"at\":2.0,\"kind\":\"dump\",\"value\":\"two\"}\n",
        )
        .unwrap();

        let found = read_events(&dir, "shop");
        assert_eq!(found.len(), 2, "a bad line took a good one with it");
        assert_eq!(found[0].value, "one");
        assert_eq!(found[1].value, "two");

        std::fs::remove_dir_all(&dir).ok();
    }
}
