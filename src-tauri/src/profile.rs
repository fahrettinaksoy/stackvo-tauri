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
    /// Who called whom, and what it cost — the half of the file the top-N table
    /// throws away.
    ///
    /// F-3. A cost table answers "where did the time go"; it cannot answer
    /// "what called that", which is the question a flame graph exists for. The
    /// parser was already reading these edges — it needs them to attribute an
    /// inclusive cost to a callee — and was discarding the caller.
    pub edges: Vec<Edge>,
}

/// One caller→callee relationship, summed over every place it occurs.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub caller: String,
    pub callee: String,
    /// The callee's inclusive cost *when reached from this caller*.
    pub inclusive_time: u64,
    pub calls: u64,
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
    // Caller→callee, summed. Keyed by id pair because names resolve later.
    let mut edges: HashMap<(u32, u32), (u64, u64)> = HashMap::new();

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
                // The edge, which is what the top-N table has never kept. Only
                // when a caller is in scope: a `calls=` before any `fn=` is a
                // malformed file, and inventing a root for it would put a
                // fabricated node at the top of the tree.
                if let Some(caller) = current_fn {
                    let slot = edges.entry((caller, callee)).or_default();
                    slot.0 += time;
                    slot.1 += count;
                }
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

    // Resolved here for the reason the functions are: an id is not a name until
    // the whole file has been read. Sorted heaviest first so a caller's most
    // expensive branch is the first child a tree walk meets, and capped —
    // `EDGE_LIMIT` is the note on that constant.
    let mut resolved: Vec<Edge> = edges
        .into_iter()
        .map(|((caller, callee), (time, calls))| Edge {
            caller: names
                .get(&caller)
                .cloned()
                .unwrap_or_else(|| format!("#{caller}")),
            callee: names
                .get(&callee)
                .cloned()
                .unwrap_or_else(|| format!("#{callee}")),
            inclusive_time: time,
            calls,
        })
        .collect();
    resolved.sort_by(|a, b| {
        b.inclusive_time
            .cmp(&a.inclusive_time)
            .then_with(|| a.caller.cmp(&b.caller))
            .then_with(|| a.callee.cmp(&b.callee))
    });
    resolved.truncate(EDGE_LIMIT);
    report.edges = resolved;

    Ok(report)
}

/// How many caller→callee edges a report carries.
///
/// A real profile of a framework request holds tens of thousands, and the graph
/// is what a flame view walks — so this is not a display cap like `TOP_N`, it is
/// what crosses the boundary. Two thousand is enough to reach every branch that
/// costs anything: the edges are sorted heaviest first, so what falls off the
/// end is the tail that would render as a line one pixel wide.
pub const EDGE_LIMIT: usize = 2_000;

// ------------------------------------------------------- the call tree (F-3)

/// One box in the flame view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub name: String,
    /// The cost of this branch, in the file's own time unit.
    pub value: u64,
    pub children: Vec<Frame>,
    /// True when this function is already on the path above it — the recursion
    /// stopped here rather than the branch having no cost.
    pub recursive: bool,
}

/// How deep a branch is walked before it is cut.
///
/// A framework request nests forty or fifty deep in normal operation, so this
/// is generous rather than tight. It exists because the thing being walked is a
/// **graph**, not a tree: mutual recursion is a cycle, and a walk with no floor
/// would not return.
pub const MAX_DEPTH: usize = 64;

/// Turn the call graph into a tree a flame view can draw.
///
/// ## Why this is a call tree and not, strictly, a flame graph
///
/// A flame graph is built from sampled **stacks**: each sample is a full path
/// from the root, so the width of a box is the number of samples in which that
/// exact path appeared. Cachegrind holds no stacks. It holds *edges* — the
/// summed cost of "A called B", over every place in the program A called B —
/// and the two are not the same information. If A is called from two places, a
/// flame graph shows two boxes with their own widths; this shows one, with the
/// total.
///
/// So a branch here means "reaching B through A cost this much in total",
/// which is the question people actually bring to a profile, and it is
/// answerable from what Xdebug wrote. What is not answerable is "this specific
/// path was taken this often", and no amount of arranging the edges recovers
/// it. The screen calls this a call tree for that reason.
///
/// ## Recursion
///
/// A function already on the path above is not descended into: the cost has
/// been counted once at its first appearance, and following the cycle would add
/// it again on every lap. The frame is kept and marked, because a recursive
/// call is a fact about the program worth seeing rather than an edge to hide.
pub fn call_tree(report: &Report) -> Vec<Frame> {
    use std::collections::{BTreeSet, HashMap};

    let mut children: HashMap<&str, Vec<&Edge>> = HashMap::new();
    let mut called: BTreeSet<&str> = BTreeSet::new();
    for edge in &report.edges {
        children.entry(edge.caller.as_str()).or_default().push(edge);
        called.insert(edge.callee.as_str());
    }

    // A root is a function nothing calls. `{main}` is the usual one, but a
    // truncated file or an edge cap can leave several — showing all of them is
    // more honest than picking one and hiding the rest.
    let mut roots: Vec<&str> = children
        .keys()
        .copied()
        .filter(|name| !called.contains(name))
        .collect();
    roots.sort();

    // Nothing is a root when every caller is also a callee, which means the
    // edges that would have named one fell off `EDGE_LIMIT` or the file was
    // truncated. Falling back to the heaviest caller keeps the view useful and
    // does not pretend: the branch it draws is real, it is simply not the whole
    // program.
    if roots.is_empty() {
        if let Some(edge) = report.edges.first() {
            roots.push(edge.caller.as_str());
        }
    }

    let mut path: Vec<&str> = Vec::new();
    roots
        .into_iter()
        .map(|root| {
            let value = children
                .get(root)
                .map(|list| list.iter().map(|e| e.inclusive_time).sum())
                .unwrap_or(0);
            frame_of(root, value, &children, &mut path, 0)
        })
        .collect()
}

/// One frame and everything under it.
///
/// The lifetime is spelled out rather than elided so `path` can hold borrows of
/// the same strings the edge map does — a stack, pushed and popped, rather than
/// a set cloned per branch: the depth is bounded and the width is not, so
/// cloning would be a copy per edge.
fn frame_of<'a>(
    name: &'a str,
    value: u64,
    children: &std::collections::HashMap<&'a str, Vec<&'a Edge>>,
    path: &mut Vec<&'a str>,
    depth: usize,
) -> Frame {
    if path.contains(&name) {
        return Frame {
            name: name.to_string(),
            value,
            children: Vec::new(),
            recursive: true,
        };
    }
    if depth >= MAX_DEPTH {
        return Frame {
            name: name.to_string(),
            value,
            children: Vec::new(),
            recursive: false,
        };
    }

    path.push(name);
    let kids = children
        .get(name)
        .map(|list| {
            list.iter()
                .map(|edge| {
                    frame_of(
                        edge.callee.as_str(),
                        edge.inclusive_time,
                        children,
                        path,
                        depth + 1,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    path.pop();

    Frame {
        name: name.to_string(),
        value,
        children: kids,
        recursive: false,
    }
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
    // ------------------------------------------------- the call tree (F-3)

    fn edge(caller: &str, callee: &str, time: u64) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            inclusive_time: time,
            calls: 1,
        }
    }

    fn report_with(edges: Vec<Edge>) -> Report {
        Report {
            edges,
            ..Default::default()
        }
    }

    /// The shape the whole feature is for: a root, its callees, and theirs.
    #[test]
    fn the_tree_hangs_off_the_function_nothing_calls() {
        let tree = call_tree(&report_with(vec![
            edge("{main}", "handle", 100),
            edge("handle", "query", 60),
            edge("handle", "render", 30),
        ]));

        assert_eq!(tree.len(), 1, "one root: {tree:?}");
        assert_eq!(tree[0].name, "{main}");
        assert_eq!(tree[0].value, 100);

        let top: Vec<&str> = tree[0].children.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(top, vec!["handle"], "the root calls one thing");

        let under: Vec<&str> = tree[0].children[0]
            .children
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(under, vec!["query", "render"], "heaviest branch first");
        assert_eq!(tree[0].children[0].children[0].value, 60);
    }

    /// A cycle must not be walked forever, and the frame that closes it is
    /// kept — a recursive call is a fact about the program, not an edge to
    /// hide.
    #[test]
    fn recursion_stops_and_says_that_it_did() {
        let tree = call_tree(&report_with(vec![
            edge("{main}", "walk", 100),
            edge("walk", "walk", 90),
        ]));

        let walk = &tree[0].children[0];
        assert_eq!(walk.name, "walk");
        let again = &walk.children[0];
        assert_eq!(again.name, "walk");
        assert!(again.recursive, "the second appearance is marked");
        assert!(again.children.is_empty(), "and is not descended into");
    }

    /// Mutual recursion is the case a `HashSet` of seen names would get wrong
    /// in the other direction — it would refuse to show a function twice on two
    /// different branches. The stack is per path, so this terminates and the
    /// two branches are independent.
    #[test]
    fn mutual_recursion_terminates() {
        let tree = call_tree(&report_with(vec![
            edge("{main}", "a", 100),
            edge("a", "b", 90),
            edge("b", "a", 80),
        ]));
        // Reaching this at all is the assertion; a cycle without the path check
        // would not return.
        assert_eq!(tree[0].children[0].children[0].children[0].name, "a");
        assert!(tree[0].children[0].children[0].children[0].recursive);
    }

    /// A file whose root edge fell off the cap still draws something, and what
    /// it draws is real — see the comment in `call_tree`.
    #[test]
    fn a_graph_with_no_root_falls_back_to_its_heaviest_caller() {
        let tree = call_tree(&report_with(vec![edge("a", "b", 50), edge("b", "a", 40)]));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a", "the heaviest edge names the fallback");
    }

    /// Two entry points is a real state — a truncated file, or an edge cap —
    /// and showing one of them would hide the other.
    #[test]
    fn several_roots_are_all_shown() {
        let tree = call_tree(&report_with(vec![
            edge("{main}", "a", 10),
            edge("worker", "b", 20),
        ]));
        let roots: Vec<&str> = tree.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(roots, vec!["worker", "{main}"], "sorted, both present");
    }

    #[test]
    fn a_profile_with_no_edges_produces_no_tree() {
        assert!(call_tree(&report_with(vec![])).is_empty());
    }

    /// The parser keeps the caller, which it used to throw away.
    #[test]
    fn parsing_records_who_called_whom() {
        let file = "\
version: 1
creator: xdebug 3.4.0
cmd: /shop
events: Time Memory

fl=(1) /app/index.php
fn=(1) {main}
5 10 5
cfn=(2) handle
calls=1 0 0
12 90 20

fl=(1)
fn=(2) handle
20 90 20
";
        let report = parse(std::io::Cursor::new(file), MAX_BYTES).unwrap();
        assert_eq!(report.edges.len(), 1, "{:?}", report.edges);
        assert_eq!(report.edges[0].caller, "{main}");
        assert_eq!(report.edges[0].callee, "handle");
        assert_eq!(report.edges[0].inclusive_time, 90);
    }

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
