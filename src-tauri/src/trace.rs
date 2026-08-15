//! Real stacks, and therefore a real flame graph (F-3).
//!
//! ## Why this module exists at all
//!
//! [`crate::profile`] reads cachegrind, and cachegrind holds **edges**: the
//! summed cost of "A called B", over every place in the program A called B. A
//! flame graph is built from **stacks**: each measurement carries the whole path
//! from the root, so a function called from two places is two boxes with their
//! own widths. `profile::call_tree` says all this in its own comment and is
//! honest about the consequence — it draws a call *tree*, and the screen calls
//! it one. F-3 stayed amber for that reason and could not be closed by
//! arranging the same numbers more cleverly. No arrangement of edges recovers
//! information the file does not contain.
//!
//! The input had to change, and Xdebug already writes the other kind:
//! `xdebug.mode=trace` with `xdebug.trace_format=1` records one line per
//! function **entry and exit**, each with its depth and a timestamp. That is a
//! stack, sampled at every call boundary rather than on a timer — so the widths
//! here are not estimates, they are the measured time each exact path was on the
//! stack.
//!
//! ## What a box means
//!
//! Time is attributed to the stack that is current when it elapses. Walking the
//! records in order and adding each gap to whatever is on the stack gives, for
//! every distinct path, the time spent *in that function with that path beneath
//! it* — self time, folded by stack, which is exactly what a flame graph's
//! widths are. A parent's width is the sum of its own self time and its
//! children's, which comes out of the tree rather than being computed a second
//! way.
//!
//! ## The cost, stated rather than discovered
//!
//! A trace is far heavier than a profile: Xdebug writes two lines per function
//! call, so a Laravel request runs to hundreds of thousands of lines and tens of
//! megabytes. That is why the mode is a trigger like profiling, why the file is
//! read with a ceiling, and why the tree is pruned before it crosses the IPC
//! boundary — a hundred thousand nodes of JSON is not a picture anybody can
//! read, and the pruning is reported rather than silent.

use crate::error::{Code, Error, Result};
use crate::profile::{Frame, MAX_DEPTH};
use serde::Serialize;
use std::collections::HashMap;

/// What Xdebug names a trace file, with `xdebug.trace_output_name` left alone.
pub const PREFIX: &str = "trace.";

/// Ceiling on how much of one file is read.
///
/// The same reasoning as `profile::MAX_BYTES` and a larger number, because a
/// trace of the same request is bigger: two lines per call rather than one
/// aggregate per function. A file longer than this is read up to here and says
/// so — a truncated trace is still a picture of the beginning of the request,
/// which is where a page's problem usually is.
const MAX_BYTES: u64 = 192 * 1024 * 1024;

/// Paths thinner than this share of the total are folded away.
///
/// One ten-thousandth. A flame graph is read by eye and a box that is a
/// thousandth of a pixel wide is not information — but the count of what went
/// is reported, because "this is everything" and "this is everything worth
/// drawing" are different claims.
const MIN_SHARE: f64 = 0.0001;

/// One recorded trace, and what came of reading it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Flame {
    /// The stacks, as a tree. This *is* a flame graph — see the module header
    /// for why the call tree next to it is not.
    pub frames: Vec<Frame>,
    /// Microseconds accounted for, which is the width of the root row.
    pub total: u64,
    /// How many entry/exit records were read.
    pub records: usize,
    /// How many distinct stacks the file held.
    pub stacks: usize,
    /// True when the file was longer than [`MAX_BYTES`] and the tail was not
    /// read — the picture is of the start of the request, not the whole of it.
    pub truncated: bool,
    /// Frames dropped for being too thin to draw.
    pub pruned: usize,
    /// True when the stack went deeper than [`MAX_DEPTH`] and was measured at
    /// that depth instead.
    pub depth_capped: bool,
}

/// One line of a format-1 trace, as far as this needs to read it.
#[derive(Debug, PartialEq)]
enum Record<'a> {
    /// Depth (1 = outermost), and the function's name.
    Enter(usize, &'a str),
    /// Depth of the function that is finishing.
    Exit(usize),
}

/// Read one line.
///
/// Format 1 is tab-separated and positional:
///
/// ```text
/// level  fn#  0  time  memory  function  user-defined  include  file  line …
/// level  fn#  1  time  memory
/// ```
///
/// Everything else in the file — the version banner, `TRACE START`, the
/// summary at the end, and the `R` return-value records — is not a stack
/// movement and is skipped. Skipped rather than refused: a trace is a log, its
/// tail is routinely half-written when it is read, and one unreadable line is
/// not a reason to throw away the request it belongs to.
fn parse(line: &str) -> Option<(f64, Record<'_>)> {
    let mut fields = line.split('\t');
    let level: usize = fields.next()?.trim().parse().ok()?;
    let _fn_number = fields.next()?;
    let kind = fields.next()?;
    let time: f64 = fields.next()?.trim().parse().ok()?;

    match kind {
        "0" => {
            let _memory = fields.next()?;
            let name = fields.next()?;
            (!name.is_empty()).then_some((time, Record::Enter(level.max(1), name)))
        }
        "1" => Some((time, Record::Exit(level.max(1)))),
        // `R` is a return *value*, recorded at the same depth and not a move.
        _ => None,
    }
}

/// Fold a trace into stacks, each with the microseconds spent in it.
///
/// The depth field is used to resynchronise rather than trusted only for
/// bookkeeping: entering at level 4 means the stack is three deep underneath,
/// whatever this reader thought. A trace whose first lines fell off the front
/// of a truncated read then produces a shallower tree instead of an
/// increasingly wrong one.
pub fn fold(text: &str) -> (HashMap<Vec<String>, u64>, usize, bool) {
    let mut folded: HashMap<Vec<String>, u64> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut last: Option<f64> = None;
    let mut records = 0usize;
    let mut depth_capped = false;

    for line in text.lines() {
        let Some((time, record)) = parse(line) else {
            continue;
        };
        records += 1;

        // Whatever was on the stack owns the time since the previous record.
        if let Some(previous) = last {
            let elapsed = ((time - previous) * 1_000_000.0).round();
            if elapsed > 0.0 && !stack.is_empty() {
                *folded.entry(stack.clone()).or_default() += elapsed as u64;
            }
        }
        last = Some(time);

        match record {
            Record::Enter(level, name) => {
                if level > MAX_DEPTH {
                    depth_capped = true;
                    continue;
                }
                stack.truncate(level.saturating_sub(1));
                stack.push(name.to_string());
            }
            Record::Exit(level) => {
                if level > MAX_DEPTH {
                    continue;
                }
                stack.truncate(level.saturating_sub(1));
            }
        }
    }

    (folded, records, depth_capped)
}

/// A node while the tree is being built.
#[derive(Default)]
struct Node {
    value: u64,
    children: HashMap<String, Node>,
}

impl Node {
    fn insert(&mut self, path: &[String], value: u64) {
        match path.split_first() {
            None => self.value += value,
            Some((head, rest)) => self
                .children
                .entry(head.clone())
                .or_default()
                .insert(rest, value),
        }
    }

    /// Total of this subtree.
    fn total(&self) -> u64 {
        self.value + self.children.values().map(Node::total).sum::<u64>()
    }
}

/// Turn a node's children into frames, widest first, dropping the undrawable.
fn frames_of(
    node: &Node,
    floor: u64,
    ancestors: &mut Vec<String>,
    pruned: &mut usize,
) -> Vec<Frame> {
    let mut out: Vec<Frame> = Vec::new();

    for (name, child) in &node.children {
        let value = child.total();
        if value < floor {
            *pruned += 1;
            continue;
        }
        // A name already on the path is recursion — kept and marked, the same
        // way the call tree marks it, because it is a fact about the program.
        let recursive = ancestors.iter().any(|a| a == name);
        ancestors.push(name.clone());
        let children = frames_of(child, floor, ancestors, pruned);
        ancestors.pop();

        out.push(Frame {
            name: name.clone(),
            value,
            children,
            recursive,
        });
    }

    out.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Build the flame graph for one trace's text.
pub fn flame_of(text: &str, truncated: bool) -> Flame {
    let (folded, records, depth_capped) = fold(text);

    let mut root = Node::default();
    for (path, value) in &folded {
        root.insert(path, *value);
    }
    let total = root.total();
    let floor = ((total as f64) * MIN_SHARE) as u64;

    let mut pruned = 0;
    let mut ancestors = Vec::new();
    let frames = frames_of(&root, floor, &mut ancestors, &mut pruned);

    Flame {
        frames,
        total,
        records,
        stacks: folded.len(),
        truncated,
        pruned,
        depth_capped,
    }
}

/// Is this a file this module wrote and will read?
fn checked_id(id: &str) -> Result<&str> {
    let plain = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !id.contains("..")
        && id.starts_with(PREFIX);

    if !plain {
        return Err(
            Error::new(Code::InvalidInput, format!("\"{id}\" is not a trace file"))
                .with_hint(crate::hints::PROFILE_IDS_FROM_LIST),
        );
    }
    Ok(id)
}

/// Every trace this project has written, newest first.
///
/// The same directory the profiles land in, because it is the same
/// `xdebug.output_dir` — the two are told apart by the name Xdebug gives them.
pub fn list(root: &std::path::Path, name: &str) -> Result<Vec<crate::profile::ProfileFile>> {
    crate::workspace::project_dir(root, name)?;
    let dir = crate::profile::host_dir(root, name);

    let Ok(entries) = std::fs::read_dir(&dir) else {
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
        if !file_name.starts_with(PREFIX) {
            continue;
        }

        out.push(crate::profile::ProfileFile {
            id: file_name.to_string(),
            bytes: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            compressed: false,
        });
    }

    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// Read one trace and fold it.
pub fn read(root: &std::path::Path, name: &str, id: &str) -> Result<Flame> {
    use std::io::Read;

    crate::workspace::project_dir(root, name)?;
    let path = crate::profile::host_dir(root, name).join(checked_id(id)?);
    if !path.is_file() {
        return Err(Error::not_found(format!("trace {id}")));
    }

    let file = std::fs::File::open(&path).map_err(|e| Error::io(format!("reading {id}"), e))?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let truncated = size > MAX_BYTES;

    let mut text = String::new();
    file.take(MAX_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| Error::io(format!("reading {id}"), e))?;

    Ok(flame_of(&text, truncated))
}

pub fn delete(root: &std::path::Path, name: &str, id: &str) -> Result<()> {
    crate::workspace::project_dir(root, name)?;
    let path = crate::profile::host_dir(root, name).join(checked_id(id)?);
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| Error::io(format!("deleting {id}"), e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trace as Xdebug writes it, in the shape `trace_format=1` produces.
    ///
    /// `{main}` runs for 100µs and calls two things: `slow()` twice from
    /// different depths and `fast()` once. The point of the fixture is the pair
    /// of `slow()` calls under *different parents* — the case cachegrind cannot
    /// represent and this module exists for.
    const TRACE: &str = "\
Version: 3.4.0\n\
File format: 4\n\
TRACE START [2026-08-15 22:00:00]\n\
1\t0\t0\t0.000000\t100\t{main}\t1\t\t/app/index.php\t0\n\
2\t1\t0\t0.000100\t200\tController::handle\t1\t\t/app/c.php\t10\n\
3\t2\t0\t0.000200\t300\tslow\t1\t\t/app/db.php\t5\n\
3\t2\t1\t0.000700\t300\n\
2\t1\t1\t0.000800\t200\n\
2\t3\t0\t0.000900\t200\tView::render\t1\t\t/app/v.php\t3\n\
3\t4\t0\t0.001000\t300\tslow\t1\t\t/app/db.php\t5\n\
3\t4\t1\t0.001100\t300\n\
2\t3\t1\t0.001200\t200\n\
1\t0\t1\t0.001300\t100\n\
TRACE END [2026-08-15 22:00:01]\n";

    fn find<'a>(frames: &'a [Frame], name: &str) -> Option<&'a Frame> {
        frames.iter().find(|f| f.name == name)
    }

    #[test]
    fn entries_and_exits_are_read_and_everything_else_is_skipped() {
        let (folded, records, capped) = fold(TRACE);
        // Five entries and five exits; the banner, TRACE START and TRACE END
        // are not stack movements.
        assert_eq!(records, 10);
        assert!(!capped);
        assert!(!folded.is_empty());
    }

    /// The whole reason this module exists: one function, two callers, two
    /// boxes — with their own widths. Cachegrind sums them into one edge and
    /// cannot tell you which caller was expensive.
    #[test]
    fn the_same_function_under_two_parents_is_two_boxes() {
        let flame = flame_of(TRACE, false);
        let main = find(&flame.frames, "{main}").expect("a root");

        let handle = find(&main.children, "Controller::handle").expect("the controller");
        let render = find(&main.children, "View::render").expect("the view");

        let under_handle = find(&handle.children, "slow").expect("slow under the controller");
        let under_render = find(&render.children, "slow").expect("slow under the view");

        // 0.000200 → 0.000700 is 500µs; 0.001000 → 0.001100 is 100µs.
        assert_eq!(under_handle.value, 500);
        assert_eq!(under_render.value, 100);
        assert_ne!(
            under_handle.value, under_render.value,
            "if these were equal the two paths had been merged, which is the \
             bug this whole module exists to avoid"
        );
    }

    /// A parent is as wide as itself plus its children, and the root row is the
    /// whole measured time.
    #[test]
    fn a_parent_is_as_wide_as_what_it_contains() {
        let flame = flame_of(TRACE, false);
        let main = find(&flame.frames, "{main}").unwrap();

        let sum: u64 = main.children.iter().map(|c| c.value).sum();
        assert!(main.value >= sum, "a parent narrower than its children");
        assert_eq!(flame.total, main.value, "the root is the total");
        // The first record starts the clock and the last stops it: 0.000000 to
        // 0.001300, every microsecond of it with {main} on the stack.
        assert_eq!(flame.total, 1300);
    }

    /// Recursion is drawn rather than hidden, and marked so a reader knows why
    /// the same name is above itself.
    #[test]
    fn a_function_that_calls_itself_is_marked() {
        let recursive = "\
1\t0\t0\t0.000000\t100\t{main}\t1\t\t/app/i.php\t0\n\
2\t1\t0\t0.000100\t200\twalk\t1\t\t/app/w.php\t2\n\
3\t2\t0\t0.000200\t300\twalk\t1\t\t/app/w.php\t2\n\
3\t2\t1\t0.000900\t300\n\
2\t1\t1\t0.001000\t200\n\
1\t0\t1\t0.001100\t100\n";

        let flame = flame_of(recursive, false);
        let outer = find(&find(&flame.frames, "{main}").unwrap().children, "walk").unwrap();
        assert!(!outer.recursive, "the first call is not recursion");
        let inner = find(&outer.children, "walk").unwrap();
        assert!(inner.recursive, "the second is");
        assert_eq!(inner.value, 700);
    }

    /// A trace read from the middle — the first lines gone — produces a
    /// shallower tree rather than an increasingly wrong one, because the depth
    /// field is used to resynchronise.
    #[test]
    fn a_trace_missing_its_head_resynchronises_on_the_level() {
        let tail = "\
3\t2\t0\t0.000200\t300\tslow\t1\t\t/app/db.php\t5\n\
3\t2\t1\t0.000700\t300\n\
2\t3\t0\t0.000900\t200\tView::render\t1\t\t/app/v.php\t3\n\
2\t3\t1\t0.001200\t200\n";

        let flame = flame_of(tail, false);
        // `slow` entered at level 3 with nothing beneath it: two empty frames
        // would be a lie, so it simply sits at the depth it claims.
        assert_eq!(flame.records, 4);
        assert!(flame.total > 0);
        assert!(find(&flame.frames, "slow").is_some());
    }

    #[test]
    fn a_line_that_is_not_a_record_is_skipped_rather_than_guessed_at() {
        assert_eq!(parse("TRACE START [2026-08-15]"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("Version: 3.4.0"), None);
        // A return value, at the same depth and not a move.
        assert_eq!(parse("3\t2\tR\t\t\t1"), None);
        // Truncated mid-line, which is how the tail of a live trace reads.
        assert_eq!(parse("2\t1\t0\t0.0001\t200"), None);
    }

    /// The ids come from `list`; a path does not.
    #[test]
    fn only_a_trace_file_can_be_asked_for() {
        assert!(checked_id("trace.1786825736.xt").is_ok());
        for hostile in [
            "../../etc/passwd",
            "trace../../x",
            "cachegrind.out.1",
            "",
            "trace.$(id)",
        ] {
            assert!(checked_id(hostile).is_err(), "{hostile} was accepted");
        }
    }

    /// A box too thin to see is dropped, and the count of what went is
    /// reported rather than the picture quietly being partial.
    #[test]
    fn unreadably_thin_paths_are_dropped_and_counted() {
        let mut text = String::from("1\t0\t0\t0.000000\t100\t{main}\t1\t\t/app/i.php\t0\n");
        // One heavy call, then a hundred that each take a microsecond.
        text.push_str("2\t1\t0\t0.000001\t200\theavy\t1\t\t/app/h.php\t1\n");
        text.push_str("2\t1\t1\t1.000000\t200\n");
        for n in 0..100 {
            let at = 1.0 + (n as f64) * 0.000_001;
            text.push_str(&format!(
                "2\t{n}\t0\t{at:.6}\t200\ttiny{n}\t1\t\t/app/t.php\t1\n"
            ));
            text.push_str(&format!("2\t{n}\t1\t{:.6}\t200\n", at + 0.000_001));
        }

        let flame = flame_of(&text, false);
        let main = find(&flame.frames, "{main}").unwrap();
        assert!(
            find(&main.children, "heavy").is_some(),
            "the heavy one stays"
        );
        assert!(flame.pruned > 0, "nothing was pruned");
        assert!(
            main.children.len() < 50,
            "still {} children",
            main.children.len()
        );
    }
}
