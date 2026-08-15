//! One request, from the code's side and the database's side, on one axis.
//!
//! F-2 in `docs/durum.md`, whose note read: "dump/mail/log three separate
//! screens, no correlation". Each of those answers a different half of the same
//! question — `dd($user)` says what the code thought it had, the query log says
//! what it actually asked for, and the two were readable only by looking at two
//! screens and comparing clocks by eye.
//!
//! ## Why this is possible now and was not before
//!
//! Two things had to be true. The dump bridge already wrote a `request` field —
//! `GET /api/health`, or the artisan command for a CLI run — and its `Event`
//! already carried a `kind` whose doc comment said, before any of this existed,
//! that it was there "so queries and jobs do not need a second file and a
//! second reader when they arrive". They have arrived. The second is that F-1
//! made the query log readable at all, and made it report seconds since the
//! epoch **as the server computed them** rather than a formatted local time —
//! which is what lets a query sit beside a dump on one axis instead of three
//! hours away from it.
//!
//! ## What correlates, and what only sorts
//!
//! Dumps carry the request they happened in, so several dumps from one page
//! load group together and the group is named. Queries do not: MySQL's general
//! log records the statement and the connection, and nothing in it says which
//! HTTP request caused it. Guessing — "the queries between two dumps belong to
//! the request those dumps belong to" — would be wrong on the first concurrent
//! request and wrong silently, which is the worst kind. So queries are placed
//! on the axis by time and left ungrouped, and the screen says which is which.
//!
//! That is a real limitation and it is the honest one. Attributing a query to a
//! request needs the *application* to say so — a header, a comment appended to
//! the SQL, a Telescope-style collector — and every one of those is code inside
//! somebody's project, which is the thing this feature was built to avoid
//! needing.

use serde::Serialize;

/// What a moment on the axis came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `dd()` / `dump()`, through the debug bridge.
    Dump,
    /// A statement from the database's own query log.
    Query,
    /// A message the catcher took, which the application believed it sent.
    Mail,
}

/// One thing that happened, whatever produced it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    /// Seconds since the epoch. Both sources report this the same way — see
    /// the module header for why that took a change to make true.
    pub at: f64,
    pub source: Source,
    /// A one-line summary: the dump's label and file, or the statement.
    pub summary: String,
    /// The request this belongs to, where the producer knew one. Always `None`
    /// for a query, and that absence is the point rather than a gap.
    pub request: Option<String>,
    /// For a query, the shape — so a group of them reads as one question.
    pub shape: Option<String>,
}

/// Everything in one window, and what repeated inside it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    /// Oldest first: a timeline read downward is a request read forward.
    pub moments: Vec<Moment>,
    /// Requests the dumps named, in the order they first appear.
    pub requests: Vec<String>,
    /// True when the query log was not recording, so the reader knows the
    /// database half is absent rather than empty.
    pub queries_recording: bool,
}

/// How far back a timeline reaches, in seconds.
///
/// Five minutes. Long enough to hold the page load somebody just did and the
/// one before it; short enough that a session left recording does not turn the
/// screen into a log file. The window is applied to the newest event rather
/// than to the wall clock, so a timeline read ten minutes after the request
/// still shows it.
pub const WINDOW: f64 = 300.0;

/// Build one axis out of the two sources.
///
/// Pure, and takes what it needs rather than reading it: the two producers live
/// behind a filesystem and a database respectively, and a function that reached
/// for both would be untestable in the way that matters — the ordering, the
/// window and the grouping are the logic, and none of them need either.
pub fn build(
    dumps: &[crate::debugbridge::Event],
    queries: &[crate::querylog::Entry],
    mail: &[crate::mail::MailMessage],
    queries_recording: bool,
) -> Timeline {
    let mut moments: Vec<Moment> = Vec::new();

    for event in dumps {
        moments.push(Moment {
            at: event.at,
            source: Source::Dump,
            summary: summarise_dump(event),
            request: event.request.clone(),
            shape: None,
        });
    }

    for entry in queries {
        moments.push(Moment {
            at: entry.at,
            source: Source::Query,
            summary: entry.sql.clone(),
            request: None,
            shape: Some(entry.shape.clone()),
        });
    }

    for message in mail {
        // Only the ones whose date could be read. A message the catcher dated
        // in a spelling neither parser knows is left off rather than placed at
        // the epoch — see `mail::epoch_of`, and the note on `at` above.
        let Some(at) = message.at else { continue };
        moments.push(Moment {
            at,
            source: Source::Mail,
            summary: summarise_mail(message),
            // A catcher records the envelope, not the request that produced it,
            // so mail correlates no better than a query does — for the same
            // reason and with the same refusal to guess.
            request: None,
            shape: None,
        });
    }

    // Oldest first. `total_cmp` rather than `partial_cmp().unwrap()`: a NaN
    // reaching here from a malformed file would panic the sort, and a timeline
    // is not worth a crash.
    moments.sort_by(|a, b| a.at.total_cmp(&b.at));

    // The window, measured from the newest thing there is — not from now.
    // Reading a timeline is something somebody does after the request, and a
    // window anchored to the clock would empty itself while they read.
    if let Some(newest) = moments.last().map(|m| m.at) {
        let floor = newest - WINDOW;
        moments.retain(|m| m.at >= floor);
    }

    let mut requests: Vec<String> = Vec::new();
    for moment in &moments {
        if let Some(request) = &moment.request {
            if !requests.contains(request) {
                requests.push(request.clone());
            }
        }
    }

    Timeline {
        moments,
        requests,
        queries_recording,
    }
}

/// One line for a message: who it went to and what it was about.
fn summarise_mail(message: &crate::mail::MailMessage) -> String {
    let to = message.to.join(", ");
    let subject = if message.subject.trim().is_empty() {
        "(no subject)"
    } else {
        message.subject.trim()
    };
    if to.is_empty() {
        subject.to_string()
    } else {
        format!("{subject} → {to}")
    }
}

/// One line for a dump: what it was called and where it was written.
fn summarise_dump(event: &crate::debugbridge::Event) -> String {
    let label = event.label.as_deref().unwrap_or("dump");
    match (&event.file, event.line) {
        (Some(file), Some(line)) => format!("{label} — {file}:{line}"),
        (Some(file), None) => format!("{label} — {file}"),
        _ => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debugbridge::Event;
    use crate::querylog::Entry;

    fn dump(at: f64, request: Option<&str>) -> Event {
        Event {
            at,
            kind: "dump".into(),
            label: Some("user".into()),
            file: Some("app/Http/Controllers/Home.php".into()),
            line: Some(42),
            request: request.map(str::to_string),
            sapi: Some("fpm-fcgi".into()),
            value: serde_json::Value::Null,
        }
    }

    fn mail(at: Option<f64>, subject: &str) -> crate::mail::MailMessage {
        crate::mail::MailMessage {
            id: "1".into(),
            from: "app@shop.loc".into(),
            to: vec!["buyer@example.test".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            subject: subject.into(),
            date: Some("2026-08-15T13:53:36.807Z".into()),
            at,
            snippet: None,
            read: false,
        }
    }

    fn query(at: f64, sql: &str) -> Entry {
        Entry {
            at,
            sql: sql.into(),
            shape: crate::querylog::shape_of(sql),
        }
    }

    /// The whole point: two producers, one axis, in the order things happened.
    #[test]
    fn both_sources_land_on_one_axis_oldest_first() {
        let line = build(
            &[dump(100.0, Some("GET /")), dump(102.0, Some("GET /"))],
            &[query(101.0, "SELECT * FROM users WHERE id = 1")],
            &[],
            true,
        );

        let ats: Vec<f64> = line.moments.iter().map(|m| m.at).collect();
        assert_eq!(ats, vec![100.0, 101.0, 102.0]);
        assert_eq!(line.moments[1].source, Source::Query);
    }

    /// A query is never attributed to a request, and the absence is deliberate
    /// — see the module header. Guessing would be wrong on the first concurrent
    /// request and wrong silently.
    #[test]
    fn a_query_carries_no_request_even_between_two_dumps_that_do() {
        let line = build(
            &[dump(100.0, Some("GET /")), dump(102.0, Some("GET /"))],
            &[query(101.0, "SELECT 1")],
            &[],
            true,
        );

        let q = line
            .moments
            .iter()
            .find(|m| m.source == Source::Query)
            .unwrap();
        assert_eq!(q.request, None);
        assert_eq!(line.requests, vec!["GET /"], "only the dumps name requests");
    }

    /// The window is anchored to the newest event, not to the clock — a
    /// timeline read ten minutes later still holds the request it was opened
    /// for.
    #[test]
    fn the_window_is_measured_from_the_newest_event() {
        let line = build(
            &[
                dump(1_000.0, Some("old")),
                dump(1_000.0 + WINDOW + 1.0, Some("new")),
            ],
            &[],
            &[],
            true,
        );

        assert_eq!(line.moments.len(), 1, "the older one is outside the window");
        assert_eq!(line.requests, vec!["new"]);
    }

    /// Requests appear in the order they first happen, so the list reads the
    /// way the page did.
    #[test]
    fn requests_are_listed_in_the_order_they_first_appear() {
        let line = build(
            &[
                dump(100.0, Some("GET /a")),
                dump(101.0, Some("GET /b")),
                dump(102.0, Some("GET /a")),
            ],
            &[],
            &[],
            true,
        );
        assert_eq!(line.requests, vec!["GET /a", "GET /b"]);
    }

    /// A CLI dump has no request, and that must not become an empty group.
    #[test]
    fn an_event_without_a_request_names_none() {
        let line = build(&[dump(100.0, None)], &[], &[], true);
        assert!(line.requests.is_empty());
        assert_eq!(line.moments.len(), 1);
    }

    /// "The database half is off" and "the database was asked nothing" are
    /// different states, and only one of them has an answer.
    #[test]
    fn not_recording_is_reported_rather_than_shown_as_no_queries() {
        let line = build(&[dump(100.0, None)], &[], &[], false);
        assert!(!line.queries_recording);
        assert!(line.moments.iter().all(|m| m.source == Source::Dump));
    }

    /// The third producer. A page that sent a mail while it ran should show it
    /// between the query that fetched the buyer and the dump that followed.
    #[test]
    fn a_message_lands_on_the_axis_with_the_others() {
        let line = build(
            &[dump(100.0, Some("POST /checkout"))],
            &[query(101.0, "SELECT * FROM users WHERE id = 1")],
            &[mail(Some(102.0), "Your order")],
            true,
        );

        let sources: Vec<Source> = line.moments.iter().map(|m| m.source).collect();
        assert_eq!(sources, vec![Source::Dump, Source::Query, Source::Mail]);
        assert!(line.moments[2].summary.contains("Your order"));
        assert!(line.moments[2].summary.contains("buyer@example.test"));
    }

    /// A date neither parser understood must not become 1970 — on an axis that
    /// is not a missing value, it is a wrong one, and it drags everything else
    /// into a corner.
    #[test]
    fn a_message_with_an_unreadable_date_is_left_off_rather_than_placed_at_zero() {
        let line = build(&[dump(100.0, None)], &[], &[mail(None, "Undated")], true);
        assert_eq!(line.moments.len(), 1);
        assert_eq!(line.moments[0].source, Source::Dump);
    }

    /// A catcher records the envelope, not the request that produced it — so
    /// mail correlates no better than a query, for the same reason.
    #[test]
    fn a_message_names_no_request() {
        let line = build(&[], &[], &[mail(Some(1.0), "x")], true);
        assert_eq!(line.moments[0].request, None);
        assert!(line.requests.is_empty());
    }

    #[test]
    fn a_dump_summarises_to_its_label_and_where_it_was_written() {
        let line = build(&[dump(1.0, None)], &[], &[], true);
        assert_eq!(
            line.moments[0].summary,
            "user — app/Http/Controllers/Home.php:42"
        );
    }
}
