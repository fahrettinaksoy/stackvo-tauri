//! Reading Xdebug's profiler output.
//!
//! P3-17 asked for a profiler UI and named Blackfire and SPX. Both were
//! checked and both are the wrong door:
//!
//! * **Blackfire** ships a template already, and needs an account. A signup
//!   wall in a local development tool is a strange thing to build towards.
//! * **SPX, XHProf, Excimer** are not in `contracts/php-extensions.json` —
//!   only `xdebug` is. Adding one is a change to a contract shared with the
//!   upstream repository, the same class of decision as the Mailpit swap, and
//!   not something to make unilaterally from this side.
//!
//! **Xdebug is already a profiler.** `xdebug.mode=profile` writes cachegrind
//! files, the extension is in the catalog, and the compose overlay that sets
//! `XDEBUG_MODE` already belongs to this app. That is the one route with no
//! contract change attached, so that is the route.
//!
//! ## What the format actually is
//!
//! Read off real output rather than from a specification — `xdebug 3.4.0 (PHP
//! 8.4.23)`, generated in one of this checkout's own containers. Four things
//! about it decide how this module is written:
//!
//! 1. **Names are compressed, and the ids are not in order.** `fl=(2) Command
//!    line code` can appear before `fl=(1) php:internal`, and `cfn=(1)` may
//!    reference a name defined further down. Names are therefore collected into
//!    a table and resolved at the end; resolving as you read produces blanks.
//! 2. **Every call gets its own block.** A trivial 200,000-iteration loop
//!    produced **200,004 `fn=` blocks across 1.6 million lines**. Aggregating by
//!    name is not a nicety — a viewer that showed blocks would show a quarter of
//!    a million rows of the same three functions.
//! 3. **A cost line's meaning depends on what preceded it.** After `fn=` it is
//!    the function's *self* cost; after `calls=` it is the *inclusive* cost of
//!    that one call, attributed to the callee.
//! 4. **The units are declared in the file**, not fixed: `events: Time_(10ns)
//!    Memory_(bytes)`. They are read, because assuming microseconds would be
//!    wrong by two orders of magnitude on exactly this build.
//!
//! And one that decides the *overlay*: Xdebug 3.4 writes **gzipped** output by
//! default (`xdebug.use_compression`). The overlay turns that off rather than
//! this module growing a decompressor — but a file compressed by somebody
//! else's ini still has to produce a sentence rather than a parse error, so the
//! magic bytes are recognised and reported.

use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::io::BufRead;

/// Where the overlay tells Xdebug to write, inside the container.
///
/// Under `/var/log`, which the generated compose already mounts at
/// `logs/projects/<name>` — so the files are on the host and readable with the
/// engine down, exactly like [`crate::applog`]'s.
pub const CONTAINER_DIR: &str = "/var/log/xdebug";

/// The host directory those land in, relative to the workspace root.
pub fn host_dir(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    root.join("logs").join("projects").join(name).join("xdebug")
}

/// Ceiling on how much of one file is read.
///
/// A profile of a real Laravel request runs to tens of megabytes and a loop
/// like the one above to hundreds. Reading is streaming and cheap, but an
/// unbounded read is an unbounded allocation on a machine that is already
/// running the whole stack. What is dropped is *reported*, never silently
/// trimmed — a truncated profile with no warning is a performance conclusion
/// drawn from half the data.
pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

/// How many functions the report carries.
///
/// The tail of a profile is thousands of functions with a rounding error each.
pub const TOP_N: usize = 60;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCost {
    pub name: String,
    pub file: String,
    /// Cost of this function's own code, in the file's own time unit.
    pub self_time: u64,
    pub self_memory: u64,
    /// Cost of this function including everything it called.
    pub inclusive_time: u64,
    /// How many times it was called. Zero for an entry point nothing calls.
    pub calls: u64,
    /// Share of the total self cost, 0–100.
    pub percent: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `xdebug 3.4.0 (PHP 8.4.23)`, verbatim.
    pub creator: String,
    /// What was being run — a URL for a web request, `Command line code` for
    /// the CLI.
    pub cmd: String,
    /// The event names the file declares, with their units.
    pub events: Vec<String>,
    /// The file's own `summary:` line. Not the sum of the self costs below —
    /// see `self_total`.
    pub summary: Vec<u64>,
    /// The sum of every function's self cost. This is what `percent` is a share
    /// of, because that denominator is the one that makes "how much of the work
    /// happened inside this function" true and adds to 100.
    pub self_total: u64,
    pub functions: Vec<FunctionCost>,
    /// How many distinct functions the file held, before the top-N cut.
    pub function_count: usize,
    /// True when the file was longer than `MAX_BYTES` and the tail was not read.
    pub truncated: bool,
}

/// Gzip's magic bytes. Xdebug 3.4 compresses by default.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// One accumulating function.
#[derive(Default)]
struct Acc {
    file_id: Option<u32>,
    self_time: u64,
    self_memory: u64,
    inclusive_time: u64,
    calls: u64,
}

/// Parse `(id) optional name` from the tail of an `fl=`/`fn=`/`cfl=`/`cfn=`
/// line, recording the name when one is given.
///
/// Returns the id. A line with no parenthesised id at all — some producers
/// write `fn=name` uncompressed — gets a synthetic id derived from the name, so
/// an uncompressed file still aggregates correctly instead of collapsing into
/// one bucket.
fn read_ref(
    rest: &str,
    names: &mut std::collections::HashMap<u32, String>,
    synthetic: &mut std::collections::HashMap<String, u32>,
    next_synthetic: &mut u32,
) -> Option<u32> {
    let rest = rest.trim();

    if let Some(inner) = rest.strip_prefix('(') {
        let (digits, tail) = inner.split_once(')')?;
        let id: u32 = digits.trim().parse().ok()?;
        let name = tail.trim();
        if !name.is_empty() {
            names.insert(id, name.to_string());
        }
        return Some(id);
    }

    if rest.is_empty() {
        return None;
    }

    // Uncompressed. Give the name a stable id of its own.
    if let Some(id) = synthetic.get(rest) {
        return Some(*id);
    }
    // Synthetic ids count down from the top so they cannot collide with the
    // file's own, which count up from 1.
    *next_synthetic -= 1;
    let id = *next_synthetic;
    synthetic.insert(rest.to_string(), id);
    names.insert(id, rest.to_string());
    Some(id)
}

/// The numbers on a cost line, after the position.
fn costs(line: &str) -> (u64, u64) {
    let mut parts = line.split_whitespace();
    // The first field is the position (a line number, per `positions: line`).
    parts.next();
    let time = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let memory = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (time, memory)
}

/// Aggregate a cachegrind stream into a report.
///
/// Streaming rather than read-to-string: the input is routinely tens of
/// megabytes and the output is sixty rows.
pub fn parse<R: BufRead>(reader: R, limit: u64) -> Result<Report> {
    use std::collections::HashMap;

    let mut report = Report::default();
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut files: HashMap<u32, String> = HashMap::new();
    let mut synthetic_fn: HashMap<String, u32> = HashMap::new();
    let mut synthetic_fl: HashMap<String, u32> = HashMap::new();
    let mut next_synthetic_fn = u32::MAX;
    let mut next_synthetic_fl = u32::MAX;

    let mut totals: HashMap<u32, Acc> = HashMap::new();

    let mut current_fl: Option<u32> = None;
    let mut current_fn: Option<u32> = None;
    // The callee a `calls=` line just announced, awaiting its cost line.
    let mut pending_callee: Option<(u32, u64)> = None;

    let mut read: u64 = 0;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        read += line.len() as u64 + 1;
        if read > limit {
            report.truncated = true;
            break;
        }
        let line = line.trim_end();

        if line.is_empty() {
            continue;
        }

        // Cost line: starts with a digit, or `*`/`+`/`-` for relative
        // positions, which Xdebug does not emit but the format allows.
        if line.starts_with(|c: char| c.is_ascii_digit() || c == '*' || c == '+' || c == '-') {
            let (time, memory) = costs(line);

            // After `calls=` the cost belongs to the callee, inclusively. This
            // is the whole reason the parser is stateful: the same shape of
            // line means two different things depending on what came before.
            if let Some((callee, count)) = pending_callee.take() {
                let acc = totals.entry(callee).or_default();
                acc.inclusive_time += time;
                acc.calls += count;
                continue;
            }

            if let Some(id) = current_fn {
                let acc = totals.entry(id).or_default();
                acc.self_time += time;
                acc.self_memory += memory;
                if acc.file_id.is_none() {
                    acc.file_id = current_fl;
                }
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("fn=") {
            current_fn = read_ref(rest, &mut names, &mut synthetic_fn, &mut next_synthetic_fn);
            if let Some(id) = current_fn {
                let acc = totals.entry(id).or_default();
                if acc.file_id.is_none() {
                    acc.file_id = current_fl;
                }
            }
            pending_callee = None;
        } else if let Some(rest) = line.strip_prefix("fl=") {
            current_fl = read_ref(rest, &mut files, &mut synthetic_fl, &mut next_synthetic_fl);
        } else if let Some(rest) = line.strip_prefix("cfn=") {
            let callee = read_ref(rest, &mut names, &mut synthetic_fn, &mut next_synthetic_fn);
            // Held until `calls=` gives the count and the next cost line gives
            // the cost.
            pending_callee = callee.map(|id| (id, 0));
        } else if let Some(rest) = line.strip_prefix("calls=") {
            let count = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            if let Some((id, _)) = pending_callee {
                pending_callee = Some((id, count));
            }
        } else if let Some(rest) = line.strip_prefix("cfl=") {
            let _ = read_ref(rest, &mut files, &mut synthetic_fl, &mut next_synthetic_fl);
        } else if let Some(rest) = line.strip_prefix("summary:") {
            report.summary = rest
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
        } else if let Some(rest) = line.strip_prefix("events:") {
            report.events = rest.split_whitespace().map(str::to_string).collect();
        } else if let Some(rest) = line.strip_prefix("creator:") {
            report.creator = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("cmd:") {
            report.cmd = rest.trim().to_string();
        }
    }

    // Names resolved here rather than as they were read: ids are not in file
    // order — a real file defined `fl=(2)` before `fl=(1)` — and a `cfn=(n)`
    // can reference a name that appears further down.
    let mut functions: Vec<FunctionCost> = totals
        .into_iter()
        .map(|(id, acc)| FunctionCost {
            name: names.get(&id).cloned().unwrap_or_else(|| format!("#{id}")),
            file: acc
                .file_id
                .and_then(|fid| files.get(&fid).cloned())
                .unwrap_or_default(),
            self_time: acc.self_time,
            self_memory: acc.self_memory,
            inclusive_time: acc.inclusive_time,
            calls: acc.calls,
            percent: 0.0,
        })
        .collect();

    report.function_count = functions.len();
    report.self_total = functions.iter().map(|f| f.self_time).sum();

    if report.self_total > 0 {
        for f in &mut functions {
            f.percent = (f.self_time as f64 / report.self_total as f64) * 100.0;
        }
    }

    functions.sort_by(|a, b| {
        b.self_time
            .cmp(&a.self_time)
            .then_with(|| a.name.cmp(&b.name))
    });
    functions.truncate(TOP_N);
    report.functions = functions;

    Ok(report)
}

// ------------------------------------------------------------------- I/O

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFile {
    /// The file name, which is also the handle. Never a path — same rule as
    /// `applog`: a reader that accepts an absolute path from its own frontend
    /// is a file reader for the whole disk.
    pub id: String,
    pub bytes: u64,
    pub modified: Option<i64>,
    /// True when the file is gzipped, so the UI can explain rather than the
    /// parser produce nonsense.
    pub compressed: bool,
}

/// A profile id is a bare file name Xdebug wrote. Anything else is refused
/// before it is joined to a path.
fn checked_id(id: &str) -> Result<&str> {
    let plain = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !id.starts_with('.')
        && id != ".."
        && id.starts_with("cachegrind.out.");

    if !plain {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{id}\" is not a profile file"),
        )
        .with_hint(crate::hints::PROFILE_IDS_FROM_LIST));
    }
    Ok(id)
}

fn is_gzip(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).is_ok() && magic == GZIP_MAGIC
}

/// Every profile this project has written, newest first.
pub fn list(root: &std::path::Path, name: &str) -> Result<Vec<ProfileFile>> {
    crate::workspace::project_dir(root, name)?;
    let dir = host_dir(root, name);

    let Ok(entries) = std::fs::read_dir(&dir) else {
        // No directory means nothing has been profiled yet, which is the normal
        // state and not a failure.
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file_name.starts_with("cachegrind.out.") {
            continue;
        }

        out.push(ProfileFile {
            id: file_name.to_string(),
            bytes: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            compressed: is_gzip(&path),
        });
    }

    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

pub fn read(root: &std::path::Path, name: &str, id: &str) -> Result<Report> {
    crate::workspace::project_dir(root, name)?;
    let path = host_dir(root, name).join(checked_id(id)?);

    if !path.is_file() {
        return Err(Error::not_found(format!("profile {id}")));
    }

    // Said plainly rather than parsed into gibberish. The overlay disables
    // compression, but a file written under somebody else's ini — or before
    // profiling was turned on here — can still be gzipped.
    if is_gzip(&path) {
        return Err(
            Error::new(Code::Unsupported, format!("{id} is gzip-compressed"))
                .with_hint(crate::hints::PROFILE_IS_COMPRESSED),
        );
    }

    let file = std::fs::File::open(&path)
        .map_err(|e| Error::io(format!("opening {}", path.display()), e))?;
    parse(std::io::BufReader::new(file), MAX_BYTES)
}

pub fn delete(root: &std::path::Path, name: &str, id: &str) -> Result<()> {
    crate::workspace::project_dir(root, name)?;
    let path = host_dir(root, name).join(checked_id(id)?);
    if path.is_file() {
        std::fs::remove_file(&path)
            .map_err(|e| Error::io(format!("removing {}", path.display()), e))?;
    }
    Ok(())
}

/// Remove every profile this project has written, returning how many and how
/// much.
///
/// Profiling fills a disk fast — the 200,000-iteration loop that shaped this
/// module produced a 20 MB file from one run — so "clear these" has to be one
/// button, not sixty.
pub fn clear(root: &std::path::Path, name: &str) -> Result<(usize, u64)> {
    let files = list(root, name)?;
    let dir = host_dir(root, name);

    let mut removed = 0usize;
    let mut freed = 0u64;
    for file in files {
        if std::fs::remove_file(dir.join(&file.id)).is_ok() {
            removed += 1;
            freed += file.bytes;
        }
    }
    Ok((removed, freed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Xdebug output, not a hand-written approximation.
    ///
    /// Generated in one of this checkout's own containers — `xdebug 3.4.0 (PHP
    /// 8.4.23)` — by profiling `outer() -> slow(3)`. Every number below is what
    /// Xdebug actually wrote, which is the point: a fixture invented here would
    /// only prove the fixture.
    const REAL: &str = "version: 1
creator: xdebug 3.4.0 (PHP 8.4.23)
cmd: Command line code
part: 1
positions: line

events: Time_(10ns) Memory_(bytes)

fl=(2) Command line code
fn=(1) slow
1 2846 0

fl=(2)
fn=(2) outer
1 350 32
cfl=(2)
cfn=(1)
calls=1 0 0
1 2846 0

fl=(2)
fn=(3) {main}
1 1667 32
cfl=(2)
cfn=(2)
calls=1 0 0
1 3196 32

fl=(1) php:internal
fn=(4) php::swoole_internal_call_user_shutdown_begin
1 58 0

summary: 13563 1913512
";

    fn parsed() -> Report {
        parse(REAL.as_bytes(), MAX_BYTES).unwrap()
    }

    #[test]
    fn the_header_is_read_rather_than_assumed() {
        let report = parsed();
        assert_eq!(report.creator, "xdebug 3.4.0 (PHP 8.4.23)");
        assert_eq!(report.cmd, "Command line code");
        // The unit is in the file. Assuming microseconds would be wrong by two
        // orders of magnitude on exactly this build.
        assert_eq!(report.events, ["Time_(10ns)", "Memory_(bytes)"]);
        assert_eq!(report.summary, [13563, 1913512]);
    }

    #[test]
    fn self_cost_comes_from_the_lines_that_follow_fn() {
        let report = parsed();
        let by = |name: &str| {
            report
                .functions
                .iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("{name} missing from {:?}", report.functions))
        };

        assert_eq!(by("slow").self_time, 2846);
        assert_eq!(by("outer").self_time, 350);
        assert_eq!(by("{main}").self_time, 1667);
        assert_eq!(
            by("php::swoole_internal_call_user_shutdown_begin").self_time,
            58
        );
    }

    /// The stateful half: an identical-looking cost line means self cost after
    /// `fn=` and inclusive cost after `calls=`. Getting this wrong doubles
    /// every caller's self time and is invisible in the output.
    #[test]
    fn cost_after_calls_is_the_callee_s_inclusive_time_not_the_caller_s_self_time() {
        let report = parsed();
        let by = |name: &str| report.functions.iter().find(|f| f.name == name).unwrap();

        // `outer` calls `slow` once, inclusively 2846.
        assert_eq!(by("slow").inclusive_time, 2846);
        assert_eq!(by("slow").calls, 1);
        // `{main}` calls `outer` once, inclusively 3196.
        assert_eq!(by("outer").inclusive_time, 3196);
        assert_eq!(by("outer").calls, 1);
        // And `outer`'s own self time is untouched by the call line under it.
        assert_eq!(by("outer").self_time, 350);
        // Nothing calls {main}.
        assert_eq!(by("{main}").calls, 0);
    }

    /// `fl=(2)` is defined before `fl=(1)` in real output, and `cfn=(1)` refers
    /// to a name by id. Resolving as you read produces blanks.
    #[test]
    fn compressed_names_resolve_even_when_the_ids_are_out_of_order() {
        let report = parsed();
        let by = |name: &str| report.functions.iter().find(|f| f.name == name).unwrap();

        assert_eq!(by("slow").file, "Command line code");
        assert_eq!(by("outer").file, "Command line code");
        assert_eq!(
            by("php::swoole_internal_call_user_shutdown_begin").file,
            "php:internal"
        );
        assert!(
            report.functions.iter().all(|f| !f.name.starts_with('#')),
            "an id went unresolved: {:?}",
            report.functions
        );
    }

    /// Percentages are a share of the summed self cost, which is the
    /// denominator that makes "how much of the work happened in here" true and
    /// adds to 100. The file's own `summary:` is larger and is reported
    /// separately rather than used for this.
    #[test]
    fn percentages_are_a_share_of_the_self_total_and_add_up() {
        let report = parsed();
        assert_eq!(report.self_total, 2846 + 350 + 1667 + 58);
        assert_ne!(
            report.self_total, report.summary[0],
            "the summary is not the sum of self costs; conflating them would mislead"
        );

        let sum: f64 = report.functions.iter().map(|f| f.percent).sum();
        assert!((sum - 100.0).abs() < 0.001, "{sum}");
        // The heaviest is first.
        assert_eq!(report.functions[0].name, "slow");
        assert!(report.functions[0].percent > 55.0);
    }

    /// The scaling fact: every call gets its own block. A 200,000-iteration
    /// loop produced 200,004 of them across 1.6M lines, and a viewer that did
    /// not aggregate would show a quarter of a million rows of three functions.
    #[test]
    fn repeated_blocks_for_one_function_aggregate_into_one_row() {
        let mut text = String::from("events: Time\n\nfl=(1) x.php\nfn=(1) hot\n1 10 0\n\n");
        for _ in 0..999 {
            text.push_str("fl=(1)\nfn=(1)\n1 10 0\n\n");
        }
        let report = parse(text.as_bytes(), MAX_BYTES).unwrap();

        assert_eq!(report.function_count, 1);
        assert_eq!(report.functions[0].name, "hot");
        assert_eq!(report.functions[0].self_time, 10_000);
    }

    /// A truncated profile with no warning is a performance conclusion drawn
    /// from half the data.
    #[test]
    fn a_file_past_the_cap_says_so() {
        let mut text = String::from("events: Time\nfl=(1) x.php\nfn=(1) hot\n");
        for _ in 0..5000 {
            text.push_str("1 10 0\n");
        }
        let report = parse(text.as_bytes(), 1024).unwrap();
        assert!(report.truncated);
        assert!(report.functions[0].self_time < 50_000);
    }

    /// An id is a handle, not a path — the same rule the log viewer runs on.
    #[test]
    fn only_a_cachegrind_file_name_is_accepted_as_an_id() {
        assert!(checked_id("cachegrind.out.7636").is_ok());
        assert!(checked_id("cachegrind.out.7636.gz").is_ok());

        assert!(checked_id("../../etc/passwd").is_err());
        assert!(checked_id("/etc/passwd").is_err());
        assert!(checked_id("laravel.log").is_err(), "not a profile");
        assert!(checked_id("").is_err());
        assert!(checked_id("..").is_err());
        assert!(checked_id(".cachegrind.out.1").is_err());
    }

    /// Some producers write names uncompressed. Without synthetic ids every one
    /// of them would land in the same bucket.
    #[test]
    fn an_uncompressed_file_still_aggregates_per_function() {
        let text = "events: Time\nfl=a.php\nfn=alpha\n1 100 0\nfl=a.php\nfn=beta\n1 50 0\n";
        let report = parse(text.as_bytes(), MAX_BYTES).unwrap();

        assert_eq!(report.function_count, 2);
        assert_eq!(report.functions[0].name, "alpha");
        assert_eq!(report.functions[0].self_time, 100);
        assert_eq!(report.functions[1].name, "beta");
        assert_eq!(report.functions[0].file, "a.php");
    }

    #[test]
    fn an_empty_file_is_an_empty_report_not_an_error() {
        let report = parse("".as_bytes(), MAX_BYTES).unwrap();
        assert!(report.functions.is_empty());
        assert_eq!(report.self_total, 0);
    }
}
