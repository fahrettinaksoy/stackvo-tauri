//! What the database was actually asked, and where the same question was asked
//! a hundred times.
//!
//! F-1 in `docs/durum.md`, listed there as the largest product gap and as the
//! most-cited feature of the three competitors that sell it. That row also said
//! it "needs a collector inside the container", which is why it kept being
//! deferred — and that turned out to be wrong for the databases this stack runs
//! most. MySQL and MariaDB have a collector: their own general query log, which
//! can be switched to a **table** and turned on at runtime with two `SET GLOBAL`
//! statements. No image change, no restart, no agent, no code in anybody's
//! application.
//!
//! Measured before it was written: on a stock `mysql:8.0`, `SET GLOBAL
//! log_output='TABLE'` and `SET GLOBAL general_log=ON` both take effect
//! immediately, and `mysql.general_log` then holds every statement with a
//! timestamp — including the N+1 shape this module exists to find.
//!
//! ## Why it is off by default and cannot be left on
//!
//! The general log records **everything**, unfiltered and unsampled, and it
//! costs write throughput on every statement. That is fine for the minute
//! somebody spends looking at a slow page and is not fine as a default: it is a
//! debugging instrument, not telemetry. So this is a thing you switch on, look
//! at, and switch off — [`Session`] carries the wording for that on screen.
//!
//! It also holds **statement text**, which for a development database is the
//! data itself: an `INSERT` carries the row. It is never written to disk by this
//! app, never included in a diagnostics bundle, and cleared with the session.
//!
//! One database breaks the middle clause and the pane says so: on Postgres the
//! recording *is* the server writing to its own log file, so switching it on
//! puts every statement on disk inside the container. Nothing here can take that
//! back out — see [`clear`] for what "cleared with the session" means when the
//! log belongs to somebody else.
//!
//! ## Postgres and Mongo joined, and the note that said they could not
//!
//! This header used to end with a paragraph explaining that Postgres and Mongo
//! were out of reach: a stream whose format `log_line_prefix` changes, and a
//! profiler that is per-database. Both facts were true and neither was a reason.
//! `log_line_prefix` is a setting **this app can set**, and per-database is a
//! loop. What the note actually recorded was a shape of mistake worth keeping:
//! the difference between "cannot" and "differently" is one measurement, and
//! twice here the measurement went the other way.
//!
//! The Postgres half then failed a third time, for a reason none of the
//! reasoning above would have found. It read the container's log stream, which
//! is right for a stock image and wrong for every Postgres this app installs:
//! the packaged `postgresql.conf` sets `logging_collector = on`, and a collector
//! takes stderr out of the stream and puts it in a file. Recording worked, the
//! parser worked, every unit test passed, and the pane showed nothing. The fix
//! is to stop assuming and ask the server — [`pg_log_path_sql`] — and the lesson
//! is the one `examples/querylog_probe.rs` exists to enforce: a fixture proves
//! the parser reads what its author believed the format to be.

use crate::db::Kind;
use crate::error::{Code, Error, Result};
use serde::Serialize;

/// One statement, as the log recorded it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Seconds since the epoch, as the **server** computed them.
    ///
    /// Not the formatted `event_time`, and the difference is a bug avoided
    /// rather than a preference. `event_time` is written in the server's own
    /// time zone, which is a setting: a container started with `TZ=Europe/
    /// Istanbul` writes local time, this process reads it as if it were UTC,
    /// and every query lands three hours from where it belongs on a timeline
    /// beside dumps that came from `microtime`. Asking the server for
    /// `UNIX_TIMESTAMP` makes the server do its own conversion, which is the
    /// only party that knows what its clock means.
    ///
    /// Measured: a stock `mysql:8.0` runs UTC and its `UNIX_TIMESTAMP(NOW())`
    /// matches the host's `date +%s` exactly — so the two clocks already agree
    /// and this is about the workspace that changes one of them.
    pub at: f64,
    /// The statement text, verbatim.
    pub sql: String,
    /// The same statement with its literals replaced — what makes two calls
    /// that differ only in an id count as the same question.
    pub shape: String,
}

/// A repeated shape, and what it looked like.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repeat {
    pub shape: String,
    pub count: usize,
    /// One of the real statements, so the reader can see what was substituted.
    pub example: String,
}

/// What the log holds right now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Is the log on for this instance?
    pub recording: bool,
    /// True when this kind of database can be asked at all.
    ///
    /// All four this app runs answer true; anything else in the workspace —
    /// Redis, RabbitMQ, Memcached — keeps no statement log to switch on, and
    /// answers false rather than erroring, because the screen asks every
    /// database and "this kind cannot" must not read as "something broke".
    pub supported: bool,
    pub entries: Vec<Entry>,
    /// Shapes seen more times than [`N_PLUS_ONE`], most repeated first.
    pub repeats: Vec<Repeat>,
}

/// How many times one shape has to repeat before it is worth naming.
///
/// Three, not two. Two identical queries is a page reading a row and then
/// reading it again — common, usually harmless, and reporting it would bury the
/// real finding in noise. Three is where a loop starts to look like a loop, and
/// the ones that matter in practice are twenty and two hundred.
pub const N_PLUS_ONE: usize = 3;

// ------------------------------------------------------------- pure logic

/// Replace the parts of a statement that vary between iterations of a loop.
///
/// Numbers, quoted strings and `IN (…)` lists become placeholders, so
/// `WHERE id = 1` and `WHERE id = 2` are one shape. Deliberately a lexer over
/// characters rather than a SQL parser: this has to survive every dialect the
/// stack runs and every hand-written query in somebody's application, and a
/// parser that refuses an unfamiliar statement would drop exactly the queries
/// worth looking at.
///
/// Whitespace is collapsed and the result upper-cased for keywords only in the
/// sense that it is compared verbatim — two statements formatted differently
/// are two shapes, which is honest: they came from different code.
pub fn shape_of(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i] as char;

        // A quoted literal, single or double. Escaped quotes inside are stepped
        // over so a string containing one does not end it early.
        if c == '\'' || c == '"' {
            let quote = c;
            i += 1;
            while i < bytes.len() {
                let d = bytes[i] as char;
                if d == '\\' {
                    i += 2;
                    continue;
                }
                if d == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push('?');
            continue;
        }

        // A number, but only when it stands alone — `utf8mb4` and `sha2` are
        // identifiers with digits in them, not literals.
        if c.is_ascii_digit() && !out.ends_with(|p: char| p.is_alphanumeric() || p == '_') {
            while i < bytes.len() && ((bytes[i] as char).is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            out.push('?');
            continue;
        }

        if c.is_whitespace() {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }

    // `IN (?, ?, ?)` and `IN (?)` are the same question asked with different
    // batch sizes, and a page that grows its batch would otherwise produce a
    // new shape on every run.
    collapse_in_lists(out.trim())
}

fn collapse_in_lists(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut rest = sql;

    while let Some(pos) = rest.to_ascii_uppercase().find("IN (") {
        let (head, tail) = rest.split_at(pos + 4);
        out.push_str(head);
        match tail.find(')') {
            Some(end) => {
                let inside = &tail[..end];
                if inside
                    .split(',')
                    .all(|part| matches!(part.trim(), "?" | ""))
                    && inside.contains('?')
                {
                    out.push_str("?)");
                } else {
                    out.push_str(&tail[..=end]);
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push_str(tail);
                return out;
            }
        }
    }

    out.push_str(rest);
    out
}

/// Statements this module logged about itself.
///
/// Without it the first thing every session reports is its own `SELECT` against
/// `mysql.general_log`, repeated once per refresh — a finding the tool
/// manufactured.
fn is_own_traffic(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    // Postgres: exact, because every statement this module sends carries the
    // comment. The keyword list below stays for MySQL, whose general log records
    // the statement without a way to add one.
    lower.contains(OWN_MARKER)
        || lower.contains("general_log")
        || lower.contains("@@general_log")
        || lower.starts_with("set global")
        || lower.starts_with("select @@")
        // Kept as a floor under the comment marker: a workspace that was
        // recording before this version shipped has unmarked `ALTER SYSTEM`
        // lines already in its log, and they are still not findings.
        || lower.starts_with("alter system")
        || lower.contains("pg_reload_conf")
}

/// The shapes worth naming, most repeated first.
pub fn repeats(entries: &[Entry]) -> Vec<Repeat> {
    let mut counts: std::collections::HashMap<&str, (usize, &str)> =
        std::collections::HashMap::new();
    for entry in entries {
        let slot = counts.entry(&entry.shape).or_insert((0, &entry.sql));
        slot.0 += 1;
    }

    let mut out: Vec<Repeat> = counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= N_PLUS_ONE)
        .map(|(shape, (count, example))| Repeat {
            shape: shape.to_string(),
            count,
            example: example.to_string(),
        })
        .collect();

    // Most repeated first, then by shape so the order is stable between two
    // reads of the same log — a list that reshuffles is one nobody can compare.
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.shape.cmp(&b.shape)));
    out
}

/// Parse the tab-separated rows `mysql -N` produces.
///
/// `-N` drops the header and `--raw` is deliberately not used: the default
/// escaping turns a newline inside a statement into `\n`, which keeps one
/// statement on one line. Rows that do not split into two are skipped rather
/// than guessed at.
pub fn parse_rows(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let (at, sql) = line.split_once('\t')?;
            let sql = sql.trim();
            if sql.is_empty() || is_own_traffic(sql) {
                return None;
            }
            Some(Entry {
                // A row whose timestamp will not parse is dropped rather than
                // stamped with zero: on a timeline, 1970 is not a missing value,
                // it is a wrong one, and it drags the whole axis with it.
                at: at.trim().parse().ok()?,
                sql: sql.to_string(),
                shape: shape_of(sql),
            })
        })
        .collect()
}

// ------------------------------------------------------------- the database

/// Can this kind be asked for a query log without changing its image?
///
/// Three of the four. Postgres joined after the first version of this module
/// said it could not: that note argued the log is a stream whose format
/// `log_line_prefix` changes — true, and it missed that **this app can set that
/// prefix**. `ALTER SYSTEM` plus `pg_reload_conf()` takes effect without a
/// restart, and `%n` stamps each line with a Unix epoch, which is the same axis
/// `UNIX_TIMESTAMP` puts the MySQL rows on. Measured on a stock `postgres:17`.
///
/// Mongo still cannot: its profiler is per-database and writes a capped
/// collection, so there is no one place to switch on and no one place to read.
pub fn supports(kind: Kind) -> bool {
    // All four now. Mongo joined last and its note was wrong in the same way
    // Postgres's was: "the profiler is per-database" is true and is not a
    // reason it cannot be done — it is a loop. `setProfilingLevel(2)` on each
    // user database, and each one's `system.profile` read back; measured on a
    // stock `mongo:8`.
    matches!(
        kind,
        Kind::Mysql | Kind::Mariadb | Kind::Postgres | Kind::Mongo
    )
}

/// Databases the profiler is never switched on for.
///
/// `admin`, `config` and `local` are the server's own. Profiling them would
/// report this app's own bookkeeping — and `local` on a replica set is the
/// oplog, which is every write twice.
const MONGO_SYSTEM_DBS: [&str; 3] = ["admin", "config", "local"];

/// The database whose profiling flag *is* the session.
///
/// `admin` rather than one this app creates: a development tool that leaves a
/// database behind in somebody's Mongo to remember a checkbox is a tool that
/// has to be trusted to clean up. This one is already there, is already skipped
/// when reading, and its flag is the same thing the server would report anyway.
const MONGO_SESSION_DB: &str = "admin";

/// The JavaScript that switches profiling on for every user database.
///
/// One `--eval`, not one per database: each is a `docker exec` and a shell
/// round-trip, and a workspace with six databases would pay six of them for a
/// button press.
///
/// ## Two limits this used to carry, and how each was closed
///
/// Profiling in Mongo is **per database**, so this used to set it on the
/// databases that existed when the switch was pressed — and a freshly started
/// Mongo lists only `admin`, `config` and `local`. Measured against the running
/// stack rather than reasoned about: the switch turned on nothing, `status` then
/// honestly reported off, and on screen that is a toggle that bounces back.
/// Worse, the everyday case is the one it missed — an application creates its
/// database on the first write, which is *after* somebody presses record.
///
/// So there are two changes, and neither of them is the server's own
/// `--profile 2` (a start-up argument, and a container recreate every time
/// somebody wants to look at a page):
///
/// * **`admin` carries the session.** Profiling it is what makes "on" a fact
///   the server holds rather than one this app would have to remember, so the
///   switch stays on with no user database in sight. Its own statements are
///   still skipped when reading — see [`MONGO_SYSTEM_DBS`] — so what it records
///   is never shown to anybody.
/// * **Reading re-applies it.** Every read, while the session is on, switches
///   profiling on for any user database that has appeared since. The window
///   that remains is one refresh wide, and it is the honest one: statements
///   made against a database in the instant it was created cannot be recorded
///   by a profiler that is per database.
fn mongo_enable_js() -> String {
    format!(
        "const skip={skip};\
         db.adminCommand({{listDatabases:1}}).databases.map(d=>d.name)\
           .filter(n=>!skip.includes(n))\
           .forEach(n=>db.getSiblingDB(n).setProfilingLevel(2));\
         db.getSiblingDB('{marker}').setProfilingLevel(2);print('ok')",
        skip = mongo_skip_list(),
        marker = MONGO_SESSION_DB
    )
}

/// Off, and the collection dropped.
///
/// Dropped rather than left: `system.profile` is capped but it is still the
/// statements somebody's application ran, and this module's rule everywhere
/// else is that stopping clears. It has to be dropped while profiling is off —
/// Mongo refuses to drop it otherwise.
fn mongo_disable_js() -> String {
    format!(
        "const skip={skip};\
         const off=n=>{{const d=db.getSiblingDB(n);d.setProfilingLevel(0);\
           try{{d.system.profile.drop()}}catch(e){{}}}};\
         db.adminCommand({{listDatabases:1}}).databases.map(d=>d.name)\
           .filter(n=>!skip.includes(n)).forEach(off);\
         off('{marker}');print('ok')",
        skip = mongo_skip_list(),
        marker = MONGO_SESSION_DB
    )
}

/// Is it on anywhere? One database being profiled is the session being on.
fn mongo_status_js() -> String {
    format!(
        "const skip={skip};\
         print(db.getSiblingDB('{marker}').getProfilingStatus().was>0 ||\
           db.adminCommand({{listDatabases:1}}).databases.map(d=>d.name)\
             .filter(n=>!skip.includes(n))\
             .some(n=>db.getSiblingDB(n).getProfilingStatus().was>0))",
        skip = mongo_skip_list(),
        marker = MONGO_SESSION_DB
    )
}

/// Every profiled statement, newest first, as one JSON line each.
///
/// `ts` is a `Date`; `getTime()` makes it the same epoch milliseconds every
/// other source on the timeline reports, divided to seconds on the way out.
fn mongo_read_js(limit: usize) -> String {
    format!(
        "const skip={skip};\
         const on=db.getSiblingDB('{marker}').getProfilingStatus().was>0;\
         const names=db.adminCommand({{listDatabases:1}}).databases.map(d=>d.name)\
           .filter(n=>!skip.includes(n));\
         if(on){{names.forEach(n=>{{const d=db.getSiblingDB(n);\
           if(d.getProfilingStatus().was===0)d.setProfilingLevel(2)}})}}\
         names.flatMap(n=>db.getSiblingDB(n).system.profile.find({{}})\
           .sort({{ts:-1}}).limit({limit}).toArray())\
           .forEach(r=>print(JSON.stringify({{\
             at:r.ts?r.ts.getTime()/1000:0,ns:r.ns||'',op:r.op||'',\
             command:r.command||{{}}}})))",
        skip = mongo_skip_list(),
        marker = MONGO_SESSION_DB
    )
}

fn mongo_skip_list() -> String {
    let quoted: Vec<String> = MONGO_SYSTEM_DBS.iter().map(|n| format!("'{n}'")).collect();
    format!("[{}]", quoted.join(","))
}

/// The marker every line this module asks Postgres to write begins with.
///
/// The container's log holds the server's own chatter — checkpoints, autovacuum,
/// connection notices — and a reader that took every line would report them as
/// statements. A prefix this app chose is the only reliable way to tell the
/// lines it asked for from the lines the server writes anyway.
const PG_MARKER: &str = "STACKVO";

/// The comment every statement **this module** sends to Postgres carries.
///
/// `log_statement = 'all'` logs this module's own statements too, comment and
/// all, so a session's first finding used to be the tool asking where its log
/// was. [`is_own_traffic`] matched those by keyword — `alter system`, `show `,
/// `pg_reload_conf` — which is a list that has to be extended every time a
/// statement is added and which hides a *user's* `SHOW` along the way. A comment
/// this app writes is exact: it marks the statements it sent and nothing else.
const OWN_MARKER: &str = "stackvo:querylog";

/// What [`clear`] writes into a Postgres log to mean "everything above this is
/// the previous session".
///
/// See [`clear`] for why a watermark rather than a deletion.
const PG_CLEAR_MARKER: &str = "stackvo:querylog:clear";

/// The SQL that turns recording on, off, or reports it.
///
/// Kept as functions rather than inlined so the statements are visible in one
/// place — this is the part that touches somebody's running database, and it
/// should be readable without following a call.
fn sql_enable() -> &'static str {
    // TABLE rather than FILE: a file needs a path inside the container, a
    // volume to read it back through and a parser for a format that changes
    // with the server's own settings. A table is queryable with the connection
    // that is already open.
    "SET GLOBAL log_output='TABLE'; SET GLOBAL general_log=ON;"
}

fn sql_disable() -> &'static str {
    // The log is truncated on the way out. Leaving it would leave statement
    // text — which is the data, in a development database — sitting in a table
    // nobody remembers switching on.
    "SET GLOBAL general_log=OFF; TRUNCATE TABLE mysql.general_log;"
}

fn sql_status() -> &'static str {
    "SELECT @@general_log;"
}

/// Newest first, capped: a page that ran ten thousand statements is a page
/// whose problem is visible in the first hundred.
fn sql_read(limit: usize) -> String {
    format!(
        "SELECT UNIX_TIMESTAMP(event_time), CONVERT(argument USING utf8mb4) \
         FROM mysql.general_log WHERE command_type='Query' \
         ORDER BY event_time DESC LIMIT {limit};"
    )
}

/// How many statements one read returns.
pub const READ_LIMIT: usize = 500;

/// How many log lines are pulled from a Postgres container.
///
/// Larger than `READ_LIMIT` because the stream is not only statements: the
/// server's own notices share it, so the marked lines are a fraction of what
/// comes back.
pub const PG_TAIL: u32 = 4_000;

/// One profiled Mongo operation, from the JSON `mongo_read_js` prints.
///
/// The shape is built from the command's **keys**, not its values: a Mongo
/// query is a document, and two lookups that differ only in an `_id` are the
/// same question in exactly the way `WHERE id = 1` and `WHERE id = 2` are. So
/// `{find:"users",filter:{_id:3}}` becomes `find users filter{_id}`, which is
/// what makes an N+1 in Mongo countable at all.
///
/// The `sql` field carries the command as it was, because the shape has thrown
/// away the half a reader needs to recognise their own code.
pub fn mongo_parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
            let at = row.get("at")?.as_f64()?;
            if at <= 0.0 {
                return None;
            }
            let command = without_envelope(row.get("command")?);
            let ns = row.get("ns").and_then(|v| v.as_str()).unwrap_or("");

            let sql = format!("{ns} {}", compact(&command));
            if is_own_mongo_traffic(&sql) {
                return None;
            }
            Some(Entry {
                at,
                shape: format!("{ns} {}", mongo_shape(&command)),
                sql,
            })
        })
        .collect()
}

/// The command as one line, values and all.
fn compact(command: &serde_json::Value) -> String {
    serde_json::to_string(command).unwrap_or_else(|_| "{}".into())
}

/// The keys a driver adds that are about the *connection*, not the question.
///
/// `mongo_shape` skipped four of these so that two runs of one query would not
/// count as two shapes. What nobody looked at until the profiler was run
/// against a live Mongo is the other half — the text on screen, which kept all
/// of them: every entry was around five hundred characters of `$clusterTime`,
/// a `signature.hash`, an `lsid` and a `$readPreference`, with the `find` and
/// the `filter` somewhere in the middle. A query log nobody can read at a
/// glance is a query log that does not do its job.
///
/// One list rather than two, used by both, so a key that is noise in the shape
/// cannot still be noise on the screen.
const MONGO_ENVELOPE: [&str; 8] = [
    "lsid",
    "$db",
    "$clusterTime",
    "$readPreference",
    "$audit",
    "signature",
    "txnNumber",
    "apiVersion",
];

/// The command with the driver's bookkeeping taken out, recursively.
fn without_envelope(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| !MONGO_ENVELOPE.contains(&key.as_str()))
                .map(|(key, inner)| (key.clone(), without_envelope(inner)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(without_envelope).collect())
        }
        other => other.clone(),
    }
}

/// The command with its values replaced by their key names.
///
/// Recursive over objects and arrays; a scalar becomes nothing at all, so what
/// is left is the *structure* of the question. Keys are sorted, because a
/// driver is free to serialise a document in any order and two orderings of one
/// query must not count as two questions.
fn mongo_shape(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut parts: Vec<String> = map
                .iter()
                // The driver's own bookkeeping, which changes per connection
                // and would make every statement its own shape. Filtered here
                // as well as in `without_envelope` because `mongo_shape` is
                // public surface and a caller may hand it a raw command.
                .filter(|(key, _)| !MONGO_ENVELOPE.contains(&key.as_str()))
                .map(|(key, inner)| {
                    let nested = mongo_shape(inner);
                    if nested.is_empty() {
                        key.clone()
                    } else {
                        format!("{key}{{{nested}}}")
                    }
                })
                .collect();
            parts.sort();
            parts.join(",")
        }
        serde_json::Value::Array(items) => items
            .iter()
            .map(mongo_shape)
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// The statements this module's own switch produces.
fn is_own_mongo_traffic(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("setprofilinglevel")
        || lower.contains("getprofilingstatus")
        || lower.contains("listdatabases")
        || lower.contains("system.profile")
}

// ------------------------------------------------------------- postgres

/// Turn logging on, and pin the format **and the place** so it can be read back.
///
/// Three `ALTER SYSTEM` statements and a reload — no restart. Each pins one
/// thing a reader would otherwise have to guess:
///
/// * `log_statement = 'all'` is the switch itself.
/// * `log_line_prefix` carries `%n`, a Unix epoch, so a Postgres statement lands
///   on the same axis as a MySQL one and as a dump, and the marker separates the
///   lines this asked for from the server's own chatter. `%n` was the one thing
///   here that looked like a version risk, so it was measured rather than
///   assumed: `postgres:12` — the oldest version this app's catalogue offers —
///   and `postgres:14` both write it. An escape a server did not understand
///   would be dropped silently, leaving lines with no timestamp and a pane that
///   says "recording" over an empty list.
/// * `log_destination = 'stderr'` is the format of the file. A workspace
///   configured for `csvlog` or `jsonlog` writes the same statements in a shape
///   [`pg_parse`] cannot read, and unlike the prefix that is not visible in
///   anything the pane shows.
///
/// `ALTER SYSTEM` writes `postgresql.auto.conf`, which survives a restart —
/// which is exactly why [`pg_disable_sql`] resets every key rather than only
/// turning the switch off. A workspace left with a rewritten `log_line_prefix`
/// would be one this app changed and never changed back.
fn pg_enable_sql() -> [&'static str; 4] {
    [
        "ALTER SYSTEM SET log_statement = 'all' /* stackvo:querylog */",
        // Cannot be one statement with the above: `ALTER SYSTEM` refuses to run
        // inside a transaction block, and psql wraps a multi-statement `-c` in
        // one. Measured, as an error message, on the first attempt.
        "ALTER SYSTEM SET log_line_prefix = 'STACKVO %n ' /* stackvo:querylog */",
        "ALTER SYSTEM SET log_destination = 'stderr' /* stackvo:querylog */",
        "SELECT pg_reload_conf() /* stackvo:querylog */",
    ]
}

fn pg_disable_sql() -> [&'static str; 4] {
    [
        "ALTER SYSTEM RESET log_statement /* stackvo:querylog */",
        "ALTER SYSTEM RESET log_line_prefix /* stackvo:querylog */",
        "ALTER SYSTEM RESET log_destination /* stackvo:querylog */",
        "SELECT pg_reload_conf() /* stackvo:querylog */",
    ]
}

/// Is it on? Asked of the server rather than remembered by this app.
fn pg_status_sql() -> &'static str {
    "SHOW log_statement /* stackvo:querylog */"
}

/// Where the server is writing, in the server's own words.
///
/// The one question the first version of this never asked, and the reason the
/// Postgres half read nothing on a real workspace. It assumed a container's log
/// stream — true of the stock image, false of every StackVo Postgres package,
/// which ships a `postgresql.conf` with `logging_collector = on`. A collector
/// takes stderr away from the container's stdout and writes it to a file under
/// the data directory, so `docker logs` holds the startup banner, one line
/// saying "redirecting log output to logging collector process", and then
/// nothing for the rest of the container's life. Measured against this
/// workspace's own `postgres:14`: recording on, statements in the file, and
/// `logs_tail` returning a banner from four hours earlier.
///
/// `pg_current_logfile('stderr')` is the server answering it directly, and it
/// answers rotation too — a `log_filename` with a `%Y-%m-%d` in it names a
/// different file every day and this always names today's. It is `NULL` when
/// there is no collector, which is the signal to read the stream instead.
///
/// Relative paths are resolved here rather than in Rust because
/// `data_directory` is another thing only the server knows.
fn pg_log_path_sql() -> &'static str {
    "SELECT COALESCE(CASE WHEN pg_current_logfile('stderr') LIKE '/%' \
       THEN pg_current_logfile('stderr') \
       ELSE current_setting('data_directory') || '/' || pg_current_logfile('stderr') END, '') \
     /* stackvo:querylog */"
}

/// The statement whose presence in the log means "everything above me is over".
fn pg_clear_sql() -> String {
    format!("SELECT '{PG_CLEAR_MARKER}'")
}

/// Everything the server has written that this app can still reach.
///
/// Two sources, and the server picks: the file when a collector owns it, the
/// container's stream when nothing does. The fallback is not a guess — a
/// collector's file that cannot be read from here (a `tail` the image does not
/// ship, a path outside the container's own filesystem) is a reason to try the
/// other source rather than to report a session with nothing in it.
async fn pg_log_text(root: &std::path::Path, service: &str) -> Result<String> {
    let container = crate::db::container_of(root, service);
    let path = crate::db::run_sql(root, service, pg_log_path_sql())
        .await?
        .trim()
        .to_string();

    if path.is_empty() {
        return crate::engine::logs_tail(&container, PG_TAIL).await;
    }
    match crate::db::read_tail(&container, &path, PG_TAIL).await {
        Ok(text) => Ok(text),
        Err(_) => crate::engine::logs_tail(&container, PG_TAIL).await,
    }
}

/// Pull the statements out of a Postgres log.
///
/// Only the marked lines, and only the ones that are statements: the same log
/// carries the `LOG:  parameter … changed` line this module's own `ALTER SYSTEM`
/// produced, which would otherwise be the first thing every session reported.
///
/// The log is append-only from this app's side, so [`clear`] is a **watermark**
/// rather than a deletion: a statement carrying [`PG_CLEAR_MARKER`] means
/// everything above it belongs to a session somebody already finished with, and
/// what has been collected so far is dropped on the spot.
pub fn pg_parse(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();

    for line in text.lines() {
        // A statement that spans lines is written across lines, and only the
        // first carries the prefix — the rest arrive indented with a tab.
        // Measured, after a first version that read only the first line and
        // reported a `CREATE TABLE` as the empty string.
        if line.starts_with('\t') || line.starts_with("    ") {
            if let Some(last) = out.last_mut() {
                last.sql.push(' ');
                last.sql.push_str(line.trim());
                last.shape = shape_of(&last.sql);
            }
            continue;
        }

        let Some(rest) = line.trim_start().strip_prefix(PG_MARKER) else {
            continue;
        };
        let Some((stamp, tail)) = rest.trim_start().split_once(' ') else {
            continue;
        };
        let Some(after) = tail.trim().strip_prefix("LOG:") else {
            continue;
        };
        let Some(sql) = after.trim().strip_prefix("statement:") else {
            continue;
        };
        let sql = sql.trim();
        // The watermark, checked before anything filters it: what came before
        // it is a session the reader has already thrown away.
        if sql.contains(PG_CLEAR_MARKER) {
            out.clear();
            continue;
        }
        // An empty statement is the opening line of a multi-line one whose text
        // is entirely on the continuations; keep it so they have somewhere to
        // land, and drop it at the end if nothing followed.
        if is_own_traffic(sql) {
            continue;
        }
        let Ok(at) = stamp.parse::<f64>() else {
            continue;
        };
        out.push(Entry {
            at,
            sql: sql.to_string(),
            shape: shape_of(sql),
        });
    }

    // The tool's own statements can only be recognised once the continuations
    // have been joined — `ALTER SYSTEM SET …` fits on one line, but a hand-run
    // one might not.
    out.retain(|e| !e.sql.is_empty() && !is_own_traffic(&e.sql));
    out
}

/// Turn recording on for one instance.
pub async fn enable(root: &std::path::Path, service: &str) -> Result<()> {
    let kind = guard(service)?;
    if kind == Kind::Mongo {
        crate::db::run_sql(root, service, &mongo_enable_js()).await?;
        return Ok(());
    }
    if kind == Kind::Postgres {
        for statement in pg_enable_sql() {
            crate::db::run_sql(root, service, statement).await?;
        }
        return Ok(());
    }
    crate::db::run_sql(root, service, sql_enable())
        .await
        .map(|_| ())
}

/// Turn it off, and clear what it collected.
pub async fn disable(root: &std::path::Path, service: &str) -> Result<()> {
    let kind = guard(service)?;
    if kind == Kind::Mongo {
        crate::db::run_sql(root, service, &mongo_disable_js()).await?;
        return Ok(());
    }
    if kind == Kind::Postgres {
        // The watermark first, while the log is still recording — otherwise
        // stopping would leave this session's statements as the first thing the
        // next one shows. Every other kind here deletes what it collected on the
        // way out, and this is the nearest true thing on a log the app does not
        // own: the statements stay in the server's file, and nothing this app
        // shows reaches back past the line.
        crate::db::run_sql(root, service, &pg_clear_sql()).await?;
        for statement in pg_disable_sql() {
            crate::db::run_sql(root, service, statement).await?;
        }
        return Ok(());
    }
    crate::db::run_sql(root, service, sql_disable())
        .await
        .map(|_| ())
}

/// Throw away what has been collected without turning recording off — the
/// "start again from here" a person reaches for before reloading a page.
///
/// Postgres cannot truncate, and that has not changed: what it wrote is a log
/// this app does not own and must not rewrite. What *did* change is the
/// conclusion drawn from it. This used to be a no-op there — the button worked
/// on three databases and quietly did nothing on the fourth — and "cannot
/// delete" is not the same as "cannot start again". So it writes a **watermark**
/// instead: one statement whose text says where the previous session ended, and
/// [`pg_parse`] drops everything above it on the next read. The reader gets what
/// they pressed the button for; the server's log is left exactly as it was.
pub async fn clear(root: &std::path::Path, service: &str) -> Result<()> {
    let kind = guard(service)?;
    // Mongo drops its collection on the way off; "start again" mid-session is
    // the same drop, and the profiler keeps writing into a fresh one.
    if kind == Kind::Mongo {
        crate::db::run_sql(
            root,
            service,
            "db.adminCommand({listDatabases:1}).databases.map(d=>d.name)             .filter(n=>!['admin','config','local'].includes(n))             .forEach(n=>{const d=db.getSiblingDB(n);const l=d.getProfilingStatus().was;             d.setProfilingLevel(0);try{d.system.profile.drop()}catch(e){};             d.setProfilingLevel(l)});print('ok')",
        )
        .await?;
        return Ok(());
    }
    if kind == Kind::Postgres {
        return crate::db::run_sql(root, service, &pg_clear_sql())
            .await
            .map(|_| ());
    }
    crate::db::run_sql(root, service, "TRUNCATE TABLE mysql.general_log;")
        .await
        .map(|_| ())
}

/// What the log holds, and what repeats in it.
pub async fn read(root: &std::path::Path, service: &str) -> Result<Session> {
    let Some(kind) = Kind::from_service(service) else {
        return Err(Error::new(
            Code::Unsupported,
            format!("{service} is not a database"),
        ));
    };
    if !supports(kind) {
        // Not an error: the screen asks every database and shows the ones that
        // can answer. A refusal here would make "this kind cannot" look like
        // "something went wrong".
        return Ok(Session {
            recording: false,
            supported: false,
            entries: Vec::new(),
            repeats: Vec::new(),
        });
    }

    let (recording, entries) = if kind == Kind::Mongo {
        let on = crate::db::run_sql(root, service, &mongo_status_js())
            .await?
            .trim()
            .eq_ignore_ascii_case("true");
        let entries = if on {
            mongo_parse(&crate::db::run_sql(root, service, &mongo_read_js(READ_LIMIT)).await?)
        } else {
            Vec::new()
        };
        (on, entries)
    } else if kind == Kind::Postgres {
        let on = crate::db::run_sql(root, service, pg_status_sql())
            .await?
            .trim()
            .eq_ignore_ascii_case("all");
        let entries = if on {
            pg_parse(&pg_log_text(root, service).await?)
        } else {
            Vec::new()
        };
        (on, entries)
    } else {
        let on = crate::db::run_sql(root, service, sql_status())
            .await?
            .trim()
            .starts_with('1');
        let entries = if on {
            parse_rows(&crate::db::run_sql(root, service, &sql_read(READ_LIMIT)).await?)
        } else {
            Vec::new()
        };
        (on, entries)
    };
    let repeats = repeats(&entries);

    Ok(Session {
        recording,
        supported: true,
        entries,
        repeats,
    })
}

/// The two kinds this works on, refused early so the error names the reason
/// rather than arriving as a MySQL syntax error from a Postgres client.
fn guard(service: &str) -> Result<Kind> {
    match Kind::from_service(service) {
        Some(kind) if supports(kind) => Ok(kind),
        _ => Err(Error::new(
            Code::Unsupported,
            format!("{service} keeps no query log this app can switch on"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of a shape: two iterations of a loop are one question.
    #[test]
    fn literals_become_placeholders_so_a_loop_reads_as_one_shape() {
        let a = shape_of("SELECT * FROM users WHERE id = 1");
        let b = shape_of("SELECT * FROM users WHERE id = 4711");
        assert_eq!(a, b);
        assert_eq!(a, "SELECT * FROM users WHERE id = ?");

        assert_eq!(
            shape_of("SELECT * FROM u WHERE name = 'bob' AND age > 30"),
            "SELECT * FROM u WHERE name = ? AND age > ?"
        );
    }

    /// An identifier with digits in it is not a literal. `utf8mb4` and `sha2`
    /// are the two this repository trips over constantly.
    #[test]
    fn digits_inside_an_identifier_are_left_alone() {
        assert_eq!(
            shape_of("SET NAMES utf8mb4"),
            "SET NAMES utf8mb4",
            "an identifier was mangled into a placeholder"
        );
    }

    /// A quote inside a string must not end it early, or everything after it
    /// shifts and two identical statements produce different shapes.
    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert_eq!(
            shape_of(r"SELECT * FROM t WHERE s = 'it\'s' AND id = 2"),
            "SELECT * FROM t WHERE s = ? AND id = ?"
        );
    }

    /// A page that batches its lookups grows the list as it goes, and each
    /// size would otherwise be a shape of its own.
    #[test]
    fn an_in_list_collapses_whatever_its_length() {
        let two = shape_of("SELECT * FROM users WHERE id IN (1, 2)");
        let five = shape_of("SELECT * FROM users WHERE id IN (1, 2, 3, 4, 5)");
        assert_eq!(two, five);
        assert_eq!(two, "SELECT * FROM users WHERE id IN (?)");
    }

    /// Three, not two — see `N_PLUS_ONE`.
    #[test]
    fn a_shape_is_named_only_once_it_looks_like_a_loop() {
        let rows = |n: usize| {
            (1..=n)
                .map(|i| format!("178679770{i}\tSELECT * FROM users WHERE id = {i}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        assert!(
            repeats(&parse_rows(&rows(2))).is_empty(),
            "two is not a loop"
        );

        let found = repeats(&parse_rows(&rows(4)));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].count, 4);
        assert_eq!(found[0].shape, "SELECT * FROM users WHERE id = ?");
        // The example is a real statement, so the reader can see what varied.
        assert!(found[0].example.contains("WHERE id ="));
    }

    /// The N+1 shape itself: one query for the list, then one per row.
    #[test]
    fn the_n_plus_one_shape_is_what_gets_reported() {
        let log = "\
1786797700\tSELECT * FROM posts
1786797701\tSELECT * FROM users WHERE id = 1
1786797702\tSELECT * FROM users WHERE id = 2
1786797703\tSELECT * FROM users WHERE id = 3";

        let found = repeats(&parse_rows(log));
        assert_eq!(found.len(), 1, "the list query is not a repeat: {found:?}");
        assert_eq!(found[0].count, 3);
        assert!(found[0].shape.contains("users"));
    }

    /// Without this the first finding every session reports is the tool
    /// reading its own log.
    #[test]
    fn the_tools_own_statements_are_not_findings() {
        let log = "\
1786797700\tSELECT event_time, argument FROM mysql.general_log WHERE command_type='Query'
1786797701\tSET GLOBAL general_log=ON
1786797702\tSELECT @@general_log
1786797703\tSELECT * FROM posts";

        let entries = parse_rows(log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT * FROM posts");
    }

    /// Rows the server did not format as expected are skipped, not guessed at.
    #[test]
    fn a_row_without_a_tab_is_dropped_rather_than_half_read() {
        assert!(parse_rows("no tab here\n\n").is_empty());
    }

    // ------------------------------------------------------ postgres

    /// The line shape `log_line_prefix = 'STACKVO %n '` produces, parsed.
    ///
    /// `%n` is a Unix epoch, which is what puts a Postgres statement on the
    /// same axis as a MySQL one and as a dump — the whole reason the prefix is
    /// pinned rather than read as the server happens to have it.
    #[test]
    fn a_marked_postgres_line_yields_a_statement_with_an_epoch() {
        let log = "\
STACKVO 1786801017.193 LOG:  statement: SELECT * FROM users WHERE id = 7
";
        let entries = pg_parse(log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT * FROM users WHERE id = 7");
        assert_eq!(entries[0].shape, "SELECT * FROM users WHERE id = ?");
        assert!((entries[0].at - 1_786_801_017.193).abs() < 0.001);
    }

    /// The container's log is the server's too. Checkpoints, connection
    /// notices and autovacuum share it, and a reader that took every line would
    /// report them as statements somebody's code ran.
    #[test]
    fn the_servers_own_chatter_is_not_a_statement() {
        let log = "\
2026-08-15 12:00:00 UTC [1] LOG:  database system is ready to accept connections
STACKVO 1786801015.084 LOG:  parameter \"log_line_prefix\" changed to \"STACKVO %n \"
STACKVO 1786801016.000 LOG:  statement: ALTER SYSTEM SET log_statement = 'all'
STACKVO 1786801017.193 LOG:  statement: SELECT 42
LOG:  checkpoint starting: time
";
        let entries = pg_parse(log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT 42");
    }

    /// A statement that spans lines is written across lines, and only the first
    /// carries the prefix — the rest arrive indented. The first version of this
    /// read only the first line and reported a `CREATE TABLE` as the empty
    /// string, which is how the continuation rule got measured.
    #[test]
    fn a_multi_line_statement_is_joined_rather_than_truncated() {
        let log = "STACKVO 1786801312.948 LOG:  statement: SELECT 1,\n\t2 FROM t\n";
        let entries = pg_parse(log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT 1, 2 FROM t");
        // And the shape is recomputed over the whole thing, not over the head.
        assert_eq!(entries[0].shape, "SELECT ?, ? FROM t");
    }

    /// An opening line whose text is entirely on its continuations must not be
    /// reported as an empty statement when nothing followed it.
    #[test]
    fn an_empty_statement_with_no_continuation_is_dropped() {
        assert!(pg_parse("STACKVO 1786801312.948 LOG:  statement: \n").is_empty());
    }

    /// Every statement this module sends carries a comment, and none of them is
    /// a finding — including the one that asks where the log file is, which is
    /// sent on **every read** and would otherwise be the most repeated shape in
    /// any session long enough to matter.
    #[test]
    fn the_statements_this_module_sends_are_not_findings() {
        let mut log = String::new();
        for statement in pg_enable_sql()
            .iter()
            .chain(pg_disable_sql().iter())
            .chain([pg_status_sql(), pg_log_path_sql()].iter())
        {
            log.push_str(&format!(
                "STACKVO 1786801017.193 LOG:  statement: {statement}\n"
            ));
        }
        log.push_str("STACKVO 1786801018.000 LOG:  statement: SELECT * FROM posts\n");

        let entries = pg_parse(&log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT * FROM posts");
    }

    /// The keyword list this replaced hid `SHOW ` — every `SHOW`, including a
    /// user's own. A query log that drops the reader's statements to hide the
    /// tool's is one that lies in the direction nobody would check.
    #[test]
    fn a_users_own_show_is_still_a_finding() {
        let entries = pg_parse("STACKVO 1786801017.193 LOG:  statement: SHOW search_path\n");
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SHOW search_path");
    }

    /// "Start again from here" on a log that cannot be truncated: the watermark
    /// is a statement, and everything above it goes.
    #[test]
    fn the_clear_watermark_drops_what_came_before_it() {
        let log = format!(
            "STACKVO 1786801010.000 LOG:  statement: SELECT * FROM old_page\n\
             STACKVO 1786801011.000 LOG:  statement: {}\n\
             STACKVO 1786801012.000 LOG:  statement: SELECT * FROM new_page\n",
            pg_clear_sql()
        );

        let entries = pg_parse(&log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].sql, "SELECT * FROM new_page");
        // And the watermark itself is not shown as a statement somebody ran.
        assert!(!entries[0].sql.contains(PG_CLEAR_MARKER));
    }

    /// A malformed or unmarked line is dropped rather than half-read — the same
    /// rule the MySQL parser follows.
    #[test]
    fn an_unmarked_or_broken_line_is_dropped() {
        assert!(
            pg_parse("no marker here\nSTACKVO\nSTACKVO x LOG:  statement: SELECT 1\n").is_empty()
        );
    }

    /// All four, and the last two arrived by the same route: a note saying
    /// "cannot" that turned out to say "differently".
    #[test]
    fn every_database_this_app_runs_has_a_readable_log() {
        for kind in [Kind::Mysql, Kind::Mariadb, Kind::Postgres, Kind::Mongo] {
            assert!(supports(kind), "{kind:?}");
        }

        // The refusal still exists for anything that is not a database at all,
        // and still says so by name rather than arriving as a syntax error.
        let err = guard("redis").unwrap_err();
        assert_eq!(err.code, Code::Unsupported);
    }

    // ------------------------------------------------------------- mongo

    /// A line as `mongo:8` actually writes it, captured by
    /// `examples/querylog_probe.rs` from the live stack.
    ///
    /// Every unit test around it was written against a hand-typed fixture that
    /// carried the command and nothing else, so nothing ever saw what the real
    /// thing is mostly made of: a cluster time, a signature, a session id and a
    /// read preference, with the question in the middle. On screen that was
    /// five hundred characters per row.
    #[test]
    fn the_drivers_envelope_is_not_what_gets_shown() {
        let line = r#"{"at":1786825736.418,"ns":"shop.users","op":"query","command":{"$clusterTime":{"clusterTime":{"$timestamp":"7674358091181195266"},"signature":{"hash":"jhLUQOcMoxyqaywiv4S/ozn6cB8=","keyId":{"high":1786771222,"low":6,"unsigned":false}}},"$db":"shop","$readPreference":{"mode":"primaryPreferred"},"filter":{"probe":4},"find":"users","lsid":{"id":"8077f3f5-cc4c-4949-9124-88ac2ae31bfa"}}}"#;

        let entries = mongo_parse(line);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];

        for noise in [
            "$clusterTime",
            "signature",
            "lsid",
            "$readPreference",
            "$db",
        ] {
            assert!(
                !entry.sql.contains(noise),
                "{noise} is still shown: {}",
                entry.sql
            );
            assert!(
                !entry.shape.contains(noise),
                "{noise} is still in the shape"
            );
        }
        // What is left is the question.
        assert!(entry.sql.contains("\"find\":\"users\""), "{}", entry.sql);
        assert!(
            entry.sql.contains("\"filter\":{\"probe\":4}"),
            "{}",
            entry.sql
        );
        assert!(entry.sql.len() < 80, "still {} characters", entry.sql.len());
    }

    /// A Mongo query is a document, and two lookups differing only in an `_id`
    /// are the same question — exactly as `WHERE id = 1` and `WHERE id = 2`
    /// are. The shape is the command's keys; the values are what varies.
    #[test]
    fn two_mongo_lookups_differing_only_in_a_value_are_one_shape() {
        let line = |id: u32| {
            format!(
                r#"{{"at":1786802016.5,"ns":"shop.users","op":"query","command":{{"find":"users","filter":{{"_id":{id}}}}}}}"#
            )
        };
        let entries = mongo_parse(&format!("{}\n{}", line(1), line(2)));
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].shape, entries[1].shape);
        assert_eq!(entries[0].shape, "shop.users filter{_id},find");
        // And the statement itself survives, because the shape threw away the
        // half a reader needs to recognise their own code.
        assert!(entries[0].sql.contains("\"_id\":1"));
    }

    /// A driver may serialise a document in any order, and two orderings of one
    /// query must not count as two questions.
    #[test]
    fn key_order_does_not_make_a_second_shape() {
        let a =
            mongo_parse(r#"{"at":1.0,"ns":"s.u","command":{"find":"u","filter":{"a":1,"b":2}}}"#);
        let b =
            mongo_parse(r#"{"at":1.0,"ns":"s.u","command":{"filter":{"b":9,"a":8},"find":"u"}}"#);
        assert_eq!(a[0].shape, b[0].shape);
    }

    /// `lsid` changes per connection, so leaving it in would make every single
    /// statement its own shape and the whole count meaningless.
    #[test]
    fn per_connection_bookkeeping_is_not_part_of_the_shape() {
        let with = mongo_parse(
            r#"{"at":1.0,"ns":"s.u","command":{"find":"u","lsid":{"id":"abc"},"$db":"s"}}"#,
        );
        let without = mongo_parse(r#"{"at":1.0,"ns":"s.u","command":{"find":"u"}}"#);
        assert_eq!(with[0].shape, without[0].shape);
    }

    /// The switch profiles itself otherwise, and the first finding of every
    /// session would be this module reading its own collection.
    #[test]
    fn the_profilers_own_traffic_is_not_a_finding() {
        let log = concat!(
            r#"{"at":1.0,"ns":"shop.system.profile","command":{"find":"system.profile"}}"#,
            "\n",
            r#"{"at":2.0,"ns":"shop.users","command":{"find":"users"}}"#
        );
        let entries = mongo_parse(log);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert!(entries[0].sql.contains("users"));
    }

    /// A row with no usable timestamp is dropped rather than placed at the
    /// epoch — the same rule the other two parsers follow.
    #[test]
    fn a_mongo_row_without_a_timestamp_is_dropped() {
        assert!(mongo_parse(r#"{"at":0,"ns":"s.u","command":{"find":"u"}}"#).is_empty());
        assert!(mongo_parse("not json").is_empty());
    }
}
