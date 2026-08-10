//! The per-container sample history, kept across restarts.
//!
//! `sample_container_stats` has filled a `Mutex<HashMap>` on a timer since the
//! port, and the comment above it says what it is for: so that opening a
//! project's detail view shows a sparkline rather than a single point. That was
//! only ever true of a *long-running* window. Quit the app and the series went
//! with the process, so the first two hours after every launch showed the chart
//! this exists to avoid.
//!
//! ## The hard part is not writing it, it is reading it back
//!
//! A time series reloaded verbatim is a lie about time. An app closed on Friday
//! and opened on Monday would hand the chart three-day-old samples, and a
//! sparkline draws whatever it is given as if it were continuous — so a flat
//! line from the weekend would read as a container that sat idle, not as one
//! that was not running at all.
//!
//! So loading is filtered by age rather than trusted. Anything older than
//! [`RETENTION`] is dropped on the way in, which makes the worst case "the
//! series is shorter than it was" instead of "the series is wrong". The bound
//! is the same two hours the in-memory cap already meant: 120 samples at the
//! 60-second interval.
//!
//! ## Corruption is a fresh start, not a failure
//!
//! Same stance as `preferences.json` (§18.2 of the readiness report), and for a
//! weaker reason, which makes it easier: this file is a cache. A prefs file
//! that fails to parse costs the user their settings and is worth preserving
//! for them to look at; this one costs a sparkline its first hour. Nothing here
//! may keep the app from starting.

use crate::atomic;
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// container name -> (unix seconds, cpu %, memory %).
pub type StatsHistory = HashMap<String, Vec<(u64, f64, f64)>>;

/// The shape written today. Stamped so a later change has somewhere to branch.
const SCHEMA_VERSION: u64 = 1;

/// How far back a reloaded sample is still worth drawing.
///
/// Two hours: 120 samples at the 60-second interval, which is the cap
/// `sample_container_stats` already applies in memory. Making this longer would
/// mean the file could hold more than the running app ever will, and the extra
/// would be discarded on the first write anyway.
pub const RETENTION: u64 = 2 * 60 * 60;

/// Where the history lives. `None` when the OS config directory is unknown —
/// the same condition under which preferences cannot be stored either.
pub fn path() -> Option<PathBuf> {
    crate::appdir::config().map(|dir| dir.join("stats-history.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    series: StatsHistory,
}

/// Read the history back, dropping whatever has gone stale.
///
/// `now` is passed in rather than read here so the retention rule can be tested
/// without waiting two hours or freezing the clock.
pub fn load_from(path: &Path, now: u64) -> StatsHistory {
    // No file is a first run, which is the one case that must stay silent.
    let Ok(text) = std::fs::read_to_string(path) else {
        return StatsHistory::new();
    };

    let Ok(stored) = serde_json::from_str::<Stored>(&text) else {
        return StatsHistory::new();
    };

    // An unknown version is not readable by definition — the field exists to
    // say so. Guessing that a newer shape is close enough is how a half-read
    // file becomes a series nobody can explain.
    if stored.schema_version != SCHEMA_VERSION {
        return StatsHistory::new();
    }

    let cutoff = now.saturating_sub(RETENTION);
    let mut series = stored.series;
    for samples in series.values_mut() {
        samples.retain(|(at, _, _)| *at >= cutoff);
    }
    // A container whose every sample expired is not an empty container, it is
    // one this file has nothing to say about. Keeping the key would put an
    // empty series in front of the chart, which draws as a flat nothing rather
    // than as absence.
    series.retain(|_, samples| !samples.is_empty());
    series
}

/// Write the history out, atomically.
///
/// Errors are returned rather than swallowed so the caller can decide; the
/// background sampler ignores them deliberately, because a cache that cannot be
/// written is not a reason to stop sampling.
pub fn save_to(path: &Path, series: &StatsHistory) -> Result<()> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let stored = Stored {
        schema_version: SCHEMA_VERSION,
        series: series.clone(),
    };
    let text = serde_json::to_string(&stored)
        .map_err(|e| crate::error::Error::new(crate::error::Code::IoError, e.to_string()))?;
    atomic::write(path, &text)
}

/// The current wall clock in unix seconds, the way the sampler stamps a sample.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-stats-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("stats-history.json")
    }

    /// One sample as the sampler stamps it.
    type Sample = (u64, f64, f64);

    fn history(pairs: &[(&str, &[Sample])]) -> StatsHistory {
        pairs
            .iter()
            .map(|(name, samples)| (name.to_string(), samples.to_vec()))
            .collect()
    }

    #[test]
    fn a_series_survives_a_round_trip() {
        let path = temp("roundtrip");
        let now = 1_000_000;
        let written = history(&[("stackvo-shop", &[(now - 60, 12.5, 30.0), (now, 13.0, 31.0)])]);

        save_to(&path, &written).expect("the cache is writable");
        assert_eq!(load_from(&path, now), written);
    }

    /// The assertion this module exists for.
    #[test]
    fn samples_older_than_the_window_do_not_come_back() {
        let path = temp("stale");
        let now = 1_000_000;

        // A weekend's worth, then one recent sample.
        let written = history(&[(
            "stackvo-shop",
            &[
                (now - 3 * 24 * 60 * 60, 90.0, 90.0),
                (now - RETENTION - 1, 80.0, 80.0),
                (now - 30, 12.0, 30.0),
            ],
        )]);
        save_to(&path, &written).expect("writable");

        let read = load_from(&path, now);
        assert_eq!(
            read["stackvo-shop"],
            vec![(now - 30, 12.0, 30.0)],
            "only the sample inside the window is drawn; the older ones would \
             render as a continuous line across a gap the app was not running in"
        );
    }

    /// A container with nothing recent is absent, not empty.
    #[test]
    fn a_container_whose_samples_all_expired_is_dropped_entirely() {
        let path = temp("expired");
        let now = 1_000_000;
        let written = history(&[("stackvo-old", &[(now - RETENTION - 5, 50.0, 50.0)])]);
        save_to(&path, &written).expect("writable");

        assert!(
            load_from(&path, now).is_empty(),
            "an empty series draws as a flat line, which is a claim rather than a gap"
        );
    }

    #[test]
    fn a_missing_file_is_a_first_run() {
        let path = temp("missing");
        assert!(load_from(&path, now()).is_empty());
    }

    /// Nothing in this file may stop the app from starting.
    #[test]
    fn a_corrupt_or_foreign_file_reads_as_empty() {
        let path = temp("corrupt");

        for contents in [
            "{ not json",
            "3",
            "\"a string\"",
            "[]",
            // Valid JSON of the right shape, but a version this build cannot
            // claim to understand.
            r#"{"schemaVersion":99,"series":{"x":[[1,2.0,3.0]]}}"#,
        ] {
            std::fs::write(&path, contents).expect("writable");
            assert!(
                load_from(&path, now()).is_empty(),
                "{contents} should have read as empty"
            );
        }
    }
}
