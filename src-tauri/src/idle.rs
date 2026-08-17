//! Which projects have not been asked for anything, and stopping them.
//!
//! I-2. A workspace with nine projects runs nine containers, and on most days
//! one of them is the one being worked on. The other eight hold memory for a
//! machine that has other uses for it.
//!
//! ## Container CPU is not the signal, and that is the whole difficulty
//!
//! The obvious measurement is wrong in both directions. `php-fpm` sits near
//! zero percent whether it is serving a request or asleep, so a busy project
//! looks idle; and network counters move for health checks, for DNS, for the
//! proxy's own connection handling, so an untouched project looks busy. Either
//! mistake is expensive: one stops something somebody is using, the other never
//! stops anything.
//!
//! The honest answer is the proxy's. Traefik knows exactly when it last routed
//! something to a router, because it wrote it down — so the generator turns its
//! access log on with **two fields kept and everything else dropped**, and this
//! reads the tail of it. A router that has served nothing for an hour is a fact
//! rather than an inference.
//!
//! ## Two fields, and the reason it is not more
//!
//! `RouterName` and `StartUTC`. An access log kept to answer one question must
//! not also become a record of every URL somebody visited on their own machine
//! — the default log carries the path, the referrer and the user agent, and
//! none of them is needed here.
//!
//! ## Never automatic without being asked, and never silent
//!
//! A project that stops behind somebody's back and then answers 502 is worse
//! than one that stays up: the failure arrives as a broken site with no
//! explanation. So the threshold is a setting that starts at "off", the sweep
//! reports what it stopped, and a suspended project is *visibly* stopped in
//! every list the app already draws — it is a stopped container, not a third
//! state nothing else knows about.
//!
//! There is no wake-on-request, and this is why: waking would need something in
//! the request path that can hold a connection open while a container starts,
//! and the only thing in that path is Traefik, which cannot. Adding a service
//! that can is a real design and not a detail — so what exists is honest
//! suspension, and starting it again is a click in the list, the tray or the
//! command palette.

use crate::error::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How much of the log to read.
///
/// Read from the end, because the answer is always in the last few lines and
/// the file has no bound — Traefik rotates nothing by itself. 256 KiB is far
/// more than enough for "when did each router last serve something" on a
/// machine with a handful of projects, and it is a fixed cost rather than one
/// that grows with how long the stack has been up.
const TAIL_BYTES: u64 = 256 * 1024;

pub fn log_path(root: &Path) -> PathBuf {
    root.join("generated/traefik/log/access.log")
}

/// A project, and how long since anything asked it for something.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Idle {
    pub project: String,
    pub router: String,
    /// Seconds since the last request, or `None` when the log has never
    /// mentioned this router.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds: Option<u64>,
    /// Whether it is past the threshold and could be stopped.
    pub suspendable: bool,
}

/// The last request time per router, from the tail of the access log.
///
/// A line that cannot be parsed is skipped rather than failing the read. This
/// file is written by another program and read while it is being written, so
/// the last line is routinely half a line — treating that as an error would
/// mean the feature stops working exactly while the stack is busy.
pub fn last_seen(root: &Path) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    let path = log_path(root);

    let Ok(mut file) = std::fs::File::open(&path) else {
        return out;
    };
    let Ok(meta) = file.metadata() else {
        return out;
    };

    use std::io::{Read, Seek, SeekFrom};
    let from = meta.len().saturating_sub(TAIL_BYTES);
    if file.seek(SeekFrom::Start(from)).is_err() {
        return out;
    }
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        return out;
    }

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(router), Some(at)) = (
            value.get("RouterName").and_then(|v| v.as_str()),
            value.get("StartUTC").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Some(epoch) = crate::mail::epoch_of(at) else {
            continue;
        };
        let epoch = epoch as i64;
        // Kept as the maximum rather than the last line: the log is appended in
        // arrival order, and a request that started earlier can be logged later
        // because Traefik writes at the end of the response.
        out.entry(router_key(router))
            .and_modify(|seen| {
                if epoch > *seen {
                    *seen = epoch
                }
            })
            .or_insert(epoch);
    }
    out
}

/// The router name as the log spells it, without Traefik's provider suffix.
///
/// Traefik reports `shop@file` or `shop@docker` — the router plus which
/// provider defined it. Matching on the whole string would mean a project
/// silently never matching if it were ever defined by the other provider, which
/// is a change nobody would connect to this.
fn router_key(name: &str) -> String {
    name.split('@').next().unwrap_or(name).to_string()
}

/// Decide, for each running project, whether it is past the threshold.
///
/// `threshold` of zero is off, and off means every project reports
/// `suspendable: false` while still reporting its idle time — the number is
/// worth seeing before somebody decides what to set the threshold to.
pub fn assess(
    root: &Path,
    projects: &[(String, bool)],
    threshold_seconds: u64,
    now: i64,
) -> Vec<Idle> {
    let seen = last_seen(root);

    projects
        .iter()
        .filter(|(_, running)| *running)
        .map(|(project, _)| {
            let router = crate::generator::traefik_name(project);
            let seconds = seen
                .get(&router)
                .map(|at| now.saturating_sub(*at).max(0) as u64);

            Idle {
                // A project the log has never mentioned is **not** suspendable.
                // It is the state of a stack that has just come up, and of one
                // whose access log was only enabled at the last regenerate —
                // stopping everything the first time somebody turns this on
                // would be the worst possible introduction to it.
                suspendable: threshold_seconds > 0
                    && seconds.is_some_and(|s| s >= threshold_seconds),
                seconds,
                project: project.clone(),
                router,
            }
        })
        .collect()
}

/// Read the threshold out of `.env`. Absent or unparseable is off.
pub const THRESHOLD_KEY: &str = "IDLE_SUSPEND_MINUTES";

pub fn threshold_seconds(root: &Path) -> Result<u64> {
    let env = crate::config::Env::load(root)?;
    Ok(env
        .get(THRESHOLD_KEY)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|minutes| minutes * 60)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace with an access log in it, in a directory named after the
    /// test that asked for one.
    ///
    /// **Named, not timestamped**, and that is a bug fix rather than a style
    /// preference. This used to build the path from
    /// `SystemTime::now().as_nanos()`, on the reasonable-sounding argument that
    /// a nanosecond clock cannot hand out the same value twice. It can: the
    /// value is quantised to a microsecond on macOS — every reading ends in
    /// `000` — and `cargo test` starts these nine tests on parallel threads
    /// inside the same one. Two of them got the same directory, the second
    /// `fs::write` replaced the first's log, and whichever test read afterwards
    /// asserted against the other's fixture.
    ///
    /// It presented as a flake that only ever failed in a full run: alone, or
    /// filtered to this module, the tests are far enough apart to get distinct
    /// readings. A clock is not an identity.
    ///
    /// `remove_dir_all` first, as `market::tests::scratch` does, so a run does
    /// not inherit whatever a killed one left behind. The name makes a stray
    /// directory say which test made it, which a timestamp never could.
    fn workspace(name: &str, lines: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-idle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("generated/traefik/log")).unwrap();
        std::fs::write(log_path(&dir), lines.join("\n") + "\n").unwrap();
        dir
    }

    fn entry(router: &str, at: &str) -> String {
        format!(r#"{{"RouterName":"{router}","StartUTC":"{at}"}}"#)
    }

    #[test]
    fn the_last_request_per_router_is_read_out_of_the_log() {
        let dir = workspace(
            "last-seen",
            &[
                &entry("shop@docker", "2026-08-15T10:00:00Z"),
                &entry("blog@docker", "2026-08-15T10:05:00Z"),
                &entry("shop@docker", "2026-08-15T10:10:00Z"),
            ],
        );
        let seen = last_seen(&dir);
        assert_eq!(seen.len(), 2);
        assert!(seen["shop"] > seen["blog"], "{seen:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Traefik writes at the end of a response, so a request that *started*
    /// earlier can be logged later. Taking the last line would move a router's
    /// last-seen time backwards.
    #[test]
    fn an_out_of_order_line_does_not_move_the_answer_backwards() {
        let dir = workspace(
            "out-of-order",
            &[
                &entry("shop@docker", "2026-08-15T10:10:00Z"),
                &entry("shop@docker", "2026-08-15T10:00:00Z"),
            ],
        );
        let seen = last_seen(&dir);
        assert_eq!(seen["shop"], 1786788600, "should be the 10:10 entry");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The file is read while another program appends to it, so the last line
    /// is routinely half a line.
    #[test]
    fn a_torn_or_unparseable_line_is_skipped_rather_than_failing_the_read() {
        let dir = workspace(
            "torn-line",
            &[
                &entry("shop@docker", "2026-08-15T10:00:00Z"),
                "{\"RouterName\":\"blog@doc",
                "not json at all",
                r#"{"RouterName":"blog@docker"}"#,
            ],
        );
        let seen = last_seen(&dir);
        assert_eq!(seen.len(), 1, "only the complete line counts: {seen:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A router defined by the other provider must not silently never match.
    #[test]
    fn the_provider_suffix_is_stripped() {
        assert_eq!(router_key("shop@docker"), "shop");
        assert_eq!(router_key("shop@file"), "shop");
        assert_eq!(router_key("shop"), "shop");
    }

    #[test]
    fn an_absent_log_is_no_answers_rather_than_an_error() {
        let dir = std::env::temp_dir().join("stackvo-idle-absent");
        assert!(last_seen(&dir).is_empty());
    }

    // ---- the decision -----------------------------------------------------

    /// The number is worth seeing before somebody chooses a threshold, so it is
    /// reported with the feature off.
    #[test]
    fn with_the_threshold_off_the_idle_time_is_still_reported() {
        let dir = workspace(
            "threshold-off",
            &[&entry("shop@docker", "2026-08-15T10:00:00Z")],
        );
        // 10:00 plus ten minutes.
        let out = assess(&dir, &[("shop".into(), true)], 0, 1786788600);
        assert_eq!(out[0].seconds, Some(600));
        assert!(!out[0].suspendable, "off means off");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_past_the_threshold_is_suspendable() {
        let dir = workspace(
            "past-threshold",
            &[&entry("shop@docker", "2026-08-15T10:00:00Z")],
        );
        let out = assess(&dir, &[("shop".into(), true)], 300, 1786788600);
        assert!(out[0].suspendable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The worst possible introduction to this feature would be turning it on
    /// and having everything stop at once.
    #[test]
    fn a_project_the_log_has_never_mentioned_is_not_suspendable() {
        let dir = workspace(
            "never-mentioned",
            &[&entry("blog@docker", "2026-08-15T10:00:00Z")],
        );
        let out = assess(&dir, &[("shop".into(), true)], 60, 1786788600);
        assert_eq!(out[0].seconds, None);
        assert!(!out[0].suspendable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stopped_project_is_not_in_the_answer_at_all() {
        let dir = workspace(
            "stopped-project",
            &[&entry("shop@docker", "2026-08-15T10:00:00Z")],
        );
        let out = assess(&dir, &[("shop".into(), false)], 60, 1786788600);
        assert!(out.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
