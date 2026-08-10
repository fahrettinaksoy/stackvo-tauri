//! A record of the things that cannot be taken back.
//!
//! `logging.rs` already writes a diagnostic log, and this is deliberately not
//! that. The two differ in every property that matters:
//!
//! | | diagnostic log | this |
//! | --- | --- | --- |
//! | answers | "why did it fail?" | "what was done, and when?" |
//! | contents | operations, transitions, errors | privileged and irreversible acts only |
//! | rotation | daily, oldest of 7 deleted | **none** |
//! | audience | whoever is debugging | whoever has to account for the machine |
//!
//! The rotation row is the whole point, and it is why a second file exists
//! rather than a log level. A record that deletes itself after seven days
//! cannot answer "when was this host entry added", which is precisely the
//! question asked three weeks later. §13 of the readiness review asked for a
//! separate, unrotated trail; a filter over a rotating one would have been the
//! same file with a different name.
//!
//! ## What is in it, and what is deliberately not
//!
//! Only acts that change something outside this app and cannot be undone by
//! pressing the button again:
//!
//!   * a write to `/etc/hosts`, which needs elevation;
//!   * a change to certificate trust, which edits a system store;
//!   * deleting a project, which is `remove_dir_all` over somebody's source;
//!   * writing `.env`, which reconfigures every container in the stack;
//!   * restoring a database, which replaces data that was there;
//!   * loading an image bundle, which installs something from elsewhere.
//!   * moving a credential into the OS keystore, and taking it back out, which
//!     changes where a password lives and what can still read it.
//!   * importing a site from another tool, which copies somebody's source tree
//!     into the workspace and, when asked to move, removes the original;
//!   * registering the MCP server with an assistant, and unregistering it,
//!     which edits a file belonging to another application and — with
//!     `--allow-writes` — hands that assistant the ability to stop the stack.
//!
//! Starting a container is not here, and neither is reading anything. An audit
//! trail that records routine traffic is one nobody reads, and a trail nobody
//! reads is not evidence — it is a file. The bar is "would somebody have to
//! account for this?", not "did something happen?".
//!
//! ## No identity field, and that is honest rather than lazy
//!
//! This is a single-user desktop app running as the person at the keyboard. An
//! `actor` column would be the OS account repeated on every line, which reads
//! as an authorisation record and is not one — the app has no accounts, no
//! roles and nothing to authenticate against. The account that owns the file is
//! the answer, and the file lives in that account's own directory.
//!
//! ## Failures are recorded too
//!
//! An attempt that was refused is exactly as interesting as one that worked —
//! more so, in the cases anyone reviews these for. `outcome` carries which.

use std::path::{Path, PathBuf};

/// How an act ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// It happened.
    Ok,
    /// The app declined before trying — a locked setting, a failed validation.
    Refused,
    /// It was attempted and did not succeed, including a cancelled password
    /// prompt: the user was asked for elevation and said no, which is a thing
    /// somebody may need to know happened.
    Failed,
}

/// One line of the trail.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    /// RFC 3339, UTC. Sorts lexicographically, which is what makes `grep` and
    /// `sort` usable on this file without a parser.
    pub at: String,
    /// A stable verb. Same string for the same act for ever — this is the
    /// column somebody filters on, so renaming one silently splits a history.
    pub action: &'static str,
    /// What it was done to: a domain, a project name, a path, an image tag.
    pub subject: String,
    pub outcome: Outcome,
    /// Free text for the one detail that makes the line worth reading. Never a
    /// payload: the same rule `logging.rs` states, for the same reason — a
    /// trail carrying `.env` values is one nobody can hand to anybody.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Where the trail lives.
///
/// Beside the logs rather than in the config directory: it is a record, not a
/// setting, and somebody collecting evidence looks where the logs are. It is
/// **not** managed by the log rotation, which only ever touches the files it
/// wrote itself.
pub fn path() -> Option<PathBuf> {
    crate::appdir::logs().map(|dir| dir.join("audit.jsonl"))
}

/// JSON Lines, one object per line.
///
/// Not a JSON array: an array has to be rewritten to be appended to, so a
/// crash mid-write costs the whole history rather than one line. Append-only
/// also means no read-modify-write, so two writers cannot lose each other's
/// entry. `atomic::write` is the right tool for a file that is replaced and
/// exactly the wrong one for a file that grows.
pub fn append_to(path: &Path, entry: &Entry) {
    use std::io::Write;

    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let Ok(mut line) = serde_json::to_string(entry) else {
        return;
    };
    line.push('\n');

    // Errors are dropped, and this is the one place in the app where that is
    // the right answer rather than a shortcut. The alternative is failing a
    // `/etc/hosts` write because its audit line could not be stored, which
    // turns a record-keeping problem into an outage. The trail is evidence
    // about the app, not a participant in it.
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Record an act, wherever the trail happens to live.
pub fn record(action: &'static str, subject: impl Into<String>, outcome: Outcome) {
    record_with(action, subject, outcome, None);
}

/// As [`record`], with the one detail worth keeping.
pub fn record_with(
    action: &'static str,
    subject: impl Into<String>,
    outcome: Outcome,
    detail: Option<String>,
) {
    let Some(path) = path() else { return };
    append_to(
        &path,
        &Entry {
            at: now_rfc3339(),
            action,
            subject: subject.into(),
            outcome,
            detail,
        },
    )
}

/// UTC, to the second, RFC 3339.
///
/// The calendar arithmetic is `crash::civil_from_days`, reused rather than
/// written again — it is the algorithm every date library uses and there is no
/// second correct version of it. The *format* differs from `crash::stamp` on
/// purpose: that one names files, where colons are illegal on Windows, and this
/// one is read by whoever reviews the trail. Both sort lexicographically, which
/// is what makes `sort` work on this file without a parser.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    rfc3339(secs)
}

fn rfc3339(unix_seconds: i64) -> String {
    let (year, month, day) = crate::crash::civil_from_days(unix_seconds.div_euclid(86_400));
    let time_of_day = unix_seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-audit-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("audit.jsonl")
    }

    fn entry(action: &'static str, subject: &str, outcome: Outcome) -> Entry {
        Entry {
            at: "2026-08-10T09:00:00Z".into(),
            action,
            subject: subject.into(),
            outcome,
            detail: None,
        }
    }

    /// The property that makes this a trail rather than a status file.
    #[test]
    fn entries_accumulate_instead_of_replacing_each_other() {
        let path = temp("append");

        append_to(&path, &entry("hosts_apply", "shop.loc", Outcome::Ok));
        append_to(&path, &entry("project_delete", "blog", Outcome::Ok));
        append_to(&path, &entry("hosts_apply", "api.loc", Outcome::Failed));

        let lines: Vec<_> = std::fs::read_to_string(&path)
            .expect("readable")
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("each line is one object"))
            .collect();

        assert_eq!(lines.len(), 3, "nothing overwrote anything");
        assert_eq!(lines[0]["action"], "hosts_apply");
        assert_eq!(lines[1]["subject"], "blog");
        assert_eq!(
            lines[2]["outcome"], "failed",
            "a refused or failed act is recorded, not only a successful one"
        );
    }

    /// Every line stands alone, so a torn write costs one entry.
    #[test]
    fn the_file_is_json_lines_rather_than_a_json_array() {
        let path = temp("jsonl");
        append_to(&path, &entry("env_set", "DEFAULT_PHP_VERSION", Outcome::Ok));

        let text = std::fs::read_to_string(&path).expect("readable");
        assert!(text.ends_with('\n'), "each entry is a complete line");
        assert!(
            !text.trim_start().starts_with('['),
            "an array would have to be rewritten to be appended to"
        );
        assert_eq!(text.lines().count(), 1);
    }

    /// A detail is optional and absent rather than null when there is none.
    #[test]
    fn an_absent_detail_does_not_appear_in_the_line() {
        let path = temp("detail");
        append_to(&path, &entry("cert_apply", "shop.loc", Outcome::Ok));
        append_to(
            &path,
            &Entry {
                detail: Some("user cancelled the prompt".into()),
                ..entry("hosts_apply", "shop.loc", Outcome::Failed)
            },
        );

        let lines: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .expect("readable")
            .lines()
            .map(|l| serde_json::from_str(l).expect("valid"))
            .collect();

        assert!(lines[0].get("detail").is_none());
        assert_eq!(lines[1]["detail"], "user cancelled the prompt");
    }

    /// An unwritable trail must not become an outage.
    #[test]
    fn a_trail_that_cannot_be_written_is_silent_rather_than_fatal() {
        // A path whose parent is a file, not a directory: `create_dir_all`
        // fails and so does the open.
        let dir = std::env::temp_dir().join("stackvo-audit-unwritable");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let blocker = dir.join("not-a-dir");
        std::fs::write(&blocker, "").expect("writable");

        // Returns rather than panicking. The act it was recording — writing
        // `/etc/hosts` — must not fail because its receipt could not be filed.
        append_to(
            &blocker.join("audit.jsonl"),
            &entry("hosts_apply", "shop.loc", Outcome::Ok),
        );
    }
}
