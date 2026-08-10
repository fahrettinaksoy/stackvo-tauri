//! Named database snapshots, and the schedule that takes them unattended.
//!
//! `db.rs` could already dump a database to a path the user chose in a save
//! dialog, and that is raw material rather than a feature: a file called
//! `mysql-2026-08-11T09-14-02.sql` in Downloads is not something anybody comes
//! back to. `ddev snapshot` and `lerd db:snapshot` name a point in time and
//! restore it by that name; Laragon and ServBay take one on a timer. Both
//! halves are gaps G-1 and G-2 of the competitive review, and both are this
//! module.
//!
//! ## The registry is the directory
//!
//! There is no index file. A snapshot is a file under
//! `<root>/backups/<service>/`, its name is the file's stem, and when it was
//! taken is the file's modification time. An index would be a second answer to
//! "what snapshots exist" that drifts the first time somebody deletes a file in
//! Finder — and the recovery from that drift is worse than the feature: a
//! restore that names a file which is not there.
//!
//! ## Retention never deletes a snapshot a person named
//!
//! The one rule worth stating twice. A schedule with a retention window keeps
//! N and removes the rest, and that is right for the copies it took itself.
//! Applying it to `before-the-migration` — typed by somebody at the moment they
//! were most worried — would be the worst failure this feature could have. So
//! automatic snapshots are named with a prefix ([`AUTO_PREFIX`]), and
//! [`prune`] only ever looks at those.
//!
//! ## What a schedule is, and what it is not
//!
//! It is an interval and a retention count, checked by a background loop that
//! asks "when was the last automatic snapshot" and compares it with the clock.
//! It is deliberately **not** a cron expression: this is a laptop, it sleeps,
//! and a missed 03:00 is a backup that never happened. An interval measured
//! from the last successful snapshot survives a closed lid.

use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What an automatic snapshot's name begins with.
///
/// The whole of the retention rule rests on this: a name a person typed can
/// never start with it, because [`safe_name`] refuses it.
pub const AUTO_PREFIX: &str = "auto-";

/// How often the scheduler takes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Schedule {
    #[default]
    Off,
    Hourly,
    Daily,
    Weekly,
}

impl Schedule {
    pub fn parse(value: &str) -> Self {
        match value {
            "hourly" => Schedule::Hourly,
            "daily" => Schedule::Daily,
            "weekly" => Schedule::Weekly,
            _ => Schedule::Off,
        }
    }

    /// `None` when nothing is scheduled.
    pub fn every(self) -> Option<Duration> {
        match self {
            Schedule::Off => None,
            Schedule::Hourly => Some(Duration::from_secs(3_600)),
            Schedule::Daily => Some(Duration::from_secs(86_400)),
            Schedule::Weekly => Some(Duration::from_secs(7 * 86_400)),
        }
    }
}

/// One snapshot on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub service: String,
    /// The file's stem — what the user restores by.
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// RFC 3339, from the file's modification time.
    pub taken_at: String,
    /// Written by the schedule rather than by a person, and therefore the only
    /// kind [`prune`] may remove.
    pub automatic: bool,
}

// -------------------------------------------------------------- pure logic

/// A snapshot name that is safe as a filename and cannot impersonate one the
/// scheduler took.
///
/// Refused rather than sanitised. Silently turning `../../etc/passwd` into
/// `etcpasswd` gives somebody a snapshot under a name they did not choose and
/// will not find again; the point of naming one is that the name is the handle.
pub fn safe_name(name: &str) -> Result<String> {
    let trimmed = name.trim();

    let refuse = |message: &str| {
        Err(Error::new(Code::InvalidInput, message.to_string())
            .with_hint(crate::hints::SNAPSHOT_NAME_CHARSET))
    };

    if trimmed.is_empty() || trimmed.len() > 64 {
        return refuse("a snapshot name is between 1 and 64 characters");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return refuse("a snapshot name may hold letters, digits, dot, dash and underscore");
    }
    // `.` and `..` are directories, and a leading dot hides the file from the
    // listing that is supposed to be the registry.
    if trimmed.starts_with('.') {
        return refuse("a snapshot name may not start with a dot");
    }
    if trimmed.starts_with(AUTO_PREFIX) {
        return refuse("`auto-` is reserved for scheduled snapshots, which are pruned by age");
    }

    Ok(trimmed.to_string())
}

/// The name the schedule gives the snapshot it is about to take.
pub fn auto_name(stamp: &str) -> String {
    format!("{AUTO_PREFIX}{stamp}")
}

/// Is another automatic snapshot due?
///
/// `last` is when the most recent automatic one was taken, `None` when there
/// has never been one — which is due immediately, because a schedule that
/// waits a full week before its first backup is a schedule somebody switched on
/// and cannot tell is working.
pub fn is_due(schedule: Schedule, last: Option<SystemTime>, now: SystemTime) -> bool {
    let Some(every) = schedule.every() else {
        return false;
    };
    let Some(last) = last else {
        return true;
    };

    // `duration_since` fails when `last` is in the future, which happens after
    // a clock correction. Treated as due: the alternative is a backup that
    // silently stops until the clock catches up with a wrong timestamp.
    now.duration_since(last).map(|d| d >= every).unwrap_or(true)
}

/// Which automatic snapshots to remove to keep `keep` of them, oldest first.
///
/// Named snapshots are not candidates and are not counted. Both halves matter:
/// counting them would let five hand-named snapshots stop the schedule from
/// ever pruning its own, and removing them is the failure this module opens by
/// promising not to.
pub fn expired(snapshots: &[Snapshot], keep: usize) -> Vec<String> {
    let mut automatic: Vec<&Snapshot> = snapshots.iter().filter(|s| s.automatic).collect();
    // Name as the tie-break, not just the timestamp: `taken_at` has
    // second resolution, and an hourly schedule catching up after a sleep can
    // write two in the same second. Without it the order — and therefore which
    // one is deleted — depends on the order `read_dir` happened to return.
    automatic.sort_by(|a, b| a.taken_at.cmp(&b.taken_at).then(a.name.cmp(&b.name)));

    if automatic.len() <= keep {
        return Vec::new();
    }
    automatic[..automatic.len() - keep]
        .iter()
        .map(|s| s.name.clone())
        .collect()
}

/// `2026-08-11T09-14-02` — the spelling `db::suggested_filename` uses, and for
/// the reason it gives: a colon is not a legal filename character on Windows.
pub fn stamp(now: SystemTime) -> String {
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = crate::crash::civil_from_days(secs.div_euclid(86_400));
    let time_of_day = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    )
}

fn rfc3339(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = crate::crash::civil_from_days(secs.div_euclid(86_400));
    let time_of_day = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    )
}

// ------------------------------------------------------------------- I/O

/// Where one service's snapshots live.
pub fn dir(root: &Path, service: &str) -> PathBuf {
    root.join("backups").join(service)
}

/// The file a named snapshot is written to.
pub fn path_for(root: &Path, service: &str, name: &str) -> Result<PathBuf> {
    let kind = crate::db::Kind::from_service(service).ok_or_else(|| {
        Error::new(
            Code::Unsupported,
            format!("{service} is not a database this app can snapshot"),
        )
        .with_hint(crate::hints::SUPPORTED_DATABASES)
    })?;

    Ok(dir(root, service).join(format!("{name}.{}", kind.extension())))
}

/// Every snapshot in the workspace, newest first.
///
/// Read off the filesystem each time. A directory that does not exist is no
/// snapshots rather than an error — it is what a workspace looks like before
/// the first one is taken.
pub fn list(root: &Path) -> Vec<Snapshot> {
    let mut out = Vec::new();

    for kind in crate::db::KINDS {
        let service = kind.as_str();
        let Ok(entries) = std::fs::read_dir(dir(root, service)) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // Only this engine's own extension: a stray `.txt` beside the
            // dumps is not a snapshot, and offering to restore one would run
            // it through `mysql` as if it were SQL.
            if path.extension().and_then(|e| e.to_str()) != Some(kind.extension()) {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(meta) = entry.metadata() else { continue };

            out.push(Snapshot {
                service: service.to_string(),
                name: name.to_string(),
                path: path.display().to_string(),
                bytes: meta.len(),
                taken_at: rfc3339(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
                automatic: name.starts_with(AUTO_PREFIX),
            });
        }
    }

    out.sort_by(|a, b| b.taken_at.cmp(&a.taken_at).then(a.name.cmp(&b.name)));
    out
}

/// When the most recent automatic snapshot of this service was taken.
pub fn last_automatic(root: &Path, service: &str) -> Option<SystemTime> {
    std::fs::read_dir(dir(root, service))
        .ok()?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(AUTO_PREFIX))
        })
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Delete one snapshot. Missing is success — a second click is not an error.
pub fn remove(root: &Path, service: &str, name: &str) -> Result<()> {
    // Through `safe_name` even on the way out: this is the one argument that
    // becomes a path, and `remove` is the one direction where a traversal
    // deletes rather than merely reads.
    let name = if name.starts_with(AUTO_PREFIX) {
        auto_checked(name)?
    } else {
        safe_name(name)?
    };

    let path = path_for(root, service, &name)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(format!("removing {}", path.display()), e)),
    }
}

/// The same check for a name that is *supposed* to carry the prefix.
fn auto_checked(name: &str) -> Result<String> {
    let rest = name.strip_prefix(AUTO_PREFIX).unwrap_or_default();
    if rest.is_empty()
        || !rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || rest.starts_with('.')
    {
        return Err(Error::new(
            Code::InvalidInput,
            "not a scheduled snapshot name".to_string(),
        )
        .with_hint(crate::hints::SNAPSHOT_NAME_CHARSET));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(name: &str, taken_at: &str) -> Snapshot {
        Snapshot {
            service: "mysql".into(),
            name: name.into(),
            path: format!("/ws/backups/mysql/{name}.sql"),
            bytes: 10,
            taken_at: taken_at.into(),
            automatic: name.starts_with(AUTO_PREFIX),
        }
    }

    #[test]
    fn a_name_that_would_leave_the_directory_is_refused_not_repaired() {
        for bad in ["../escape", "a/b", "..", ".hidden", "", "  ", "a b"] {
            assert!(safe_name(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(
            safe_name("  before-migration  ").unwrap(),
            "before-migration"
        );
        assert_eq!(safe_name("v1.2_final").unwrap(), "v1.2_final");
    }

    /// The prefix is what the retention rule rests on. If a person could take a
    /// snapshot called `auto-keepme`, the scheduler would eventually delete it.
    #[test]
    fn a_person_cannot_name_a_snapshot_the_way_the_scheduler_does() {
        assert!(safe_name("auto-keepme").is_err());
        assert!(safe_name("auto").is_ok(), "only the prefix is reserved");
    }

    #[test]
    fn retention_removes_the_oldest_automatic_ones_and_nothing_else() {
        let snapshots = [
            snap("auto-2026-08-01T00-00-00", "2026-08-01T00:00:00Z"),
            snap("auto-2026-08-02T00-00-00", "2026-08-02T00:00:00Z"),
            snap("auto-2026-08-03T00-00-00", "2026-08-03T00:00:00Z"),
            snap("before-migration", "2026-07-01T00:00:00Z"),
        ];

        // Oldest first, and the hand-named one is not a candidate even though
        // it is by far the oldest.
        assert_eq!(expired(&snapshots, 2), ["auto-2026-08-01T00-00-00"]);
        assert_eq!(
            expired(&snapshots, 1),
            ["auto-2026-08-01T00-00-00", "auto-2026-08-02T00-00-00"]
        );
        assert!(expired(&snapshots, 3).is_empty());
    }

    /// And it is not counted either: three hand-named snapshots must not stop
    /// the schedule pruning the copies it took itself.
    #[test]
    fn named_snapshots_do_not_fill_the_retention_window() {
        let snapshots = [
            snap("keep-a", "2026-01-01T00:00:00Z"),
            snap("keep-b", "2026-01-02T00:00:00Z"),
            snap("keep-c", "2026-01-03T00:00:00Z"),
            snap("auto-1", "2026-08-01T00:00:00Z"),
            snap("auto-2", "2026-08-02T00:00:00Z"),
        ];
        assert_eq!(expired(&snapshots, 1), ["auto-1"]);
    }

    #[test]
    fn nothing_is_due_when_nothing_is_scheduled() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(!is_due(Schedule::Off, None, now));
    }

    /// A schedule switched on has to do something soon enough to be believed.
    #[test]
    fn the_first_one_is_due_immediately() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(is_due(Schedule::Weekly, None, now));
    }

    #[test]
    fn the_interval_is_measured_from_the_last_snapshot() {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let almost = last + Duration::from_secs(86_399);
        let just = last + Duration::from_secs(86_400);

        assert!(!is_due(Schedule::Daily, Some(last), almost));
        assert!(is_due(Schedule::Daily, Some(last), just));
        // Hourly is due long before daily is.
        assert!(is_due(
            Schedule::Hourly,
            Some(last),
            last + Duration::from_secs(3_600)
        ));
    }

    /// A laptop that was asleep for three days does not owe three snapshots; it
    /// owes one. Measuring from the last one rather than from a wall-clock time
    /// is what makes that true, and it is why this is not cron.
    #[test]
    fn a_long_gap_is_one_snapshot_and_not_a_backlog() {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let much_later = last + Duration::from_secs(30 * 86_400);
        assert!(is_due(Schedule::Daily, Some(last), much_later));
    }

    /// A clock correction can put the last snapshot in the future. Treating
    /// that as "not due" would stop backups until the clock caught up.
    #[test]
    fn a_timestamp_in_the_future_does_not_stop_the_schedule() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let last = now + Duration::from_secs(86_400);
        assert!(is_due(Schedule::Daily, Some(last), now));
    }

    #[test]
    fn the_stamp_has_no_character_windows_refuses_in_a_filename() {
        let text = stamp(SystemTime::UNIX_EPOCH + Duration::from_secs(1_754_899_442));
        assert!(!text.contains(':'), "{text}");
        assert_eq!(text.len(), "2026-08-11T09-24-02".len());
        assert!(
            safe_name(&text).is_ok(),
            "a stamp must be usable as a name: {text}"
        );
    }

    #[test]
    fn a_schedule_round_trips_through_its_own_spelling() {
        for (text, value) in [
            ("off", Schedule::Off),
            ("hourly", Schedule::Hourly),
            ("daily", Schedule::Daily),
            ("weekly", Schedule::Weekly),
            ("nonsense", Schedule::Off),
        ] {
            assert_eq!(Schedule::parse(text), value);
        }
    }
}
