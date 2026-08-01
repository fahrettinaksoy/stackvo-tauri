//! The log files a project writes, as opposed to what its container prints.
//!
//! `container_logs_open` streams stdout and stderr from Docker, which is what
//! the entrypoint and the web server say. It is not where an application
//! records anything: a Laravel exception goes to `storage/logs/laravel.log`, an
//! nginx 502 goes to the mounted `error.log`, and a queue worker that died goes
//! to its own file under supervisord. None of those reach the container's
//! stdout, so none of them were visible anywhere in the app.
//!
//! Every one of these files is already on the host — the generated compose
//! mounts `projects/<name>` at `/var/www/html` and `logs/projects/<name>` at
//! `/var/log`. So this reads them directly rather than through `docker exec`:
//! no engine required, which matters because a container that crashed on boot
//! is exactly when its log is worth reading.
//!
//! Two roots, kept apart because they answer different questions:
//!
//!   * **application** — files the code wrote, under the project directory;
//!   * **server** — files the stack wrote, under `logs/projects/<name>`.
//!
//! Paths never cross the IPC boundary as paths. The UI is given an opaque id
//! (`app:storage/logs/laravel.log`) and hands it back; this module resolves it
//! against one of the two roots and refuses anything that lands outside. A
//! log viewer that accepts an absolute path from its own frontend is a file
//! reader for the whole disk.

use crate::error::{Code, Error, Result};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// How deep to look inside a log directory.
///
/// Laravel channels nest (`storage/logs/parser/parser-2026-07-28.log`), so a
/// flat listing misses most of what a real project writes. Three is enough for
/// every layout observed and shallow enough that a stray `node_modules` under
/// one of these directories cannot turn discovery into a full-disk walk.
const MAX_DEPTH: usize = 3;

/// Ceiling on how many files are offered.
///
/// A daily-rotating channel accumulates without limit, and a picker with a
/// thousand entries is not a picker. The newest survive the cut, which is the
/// end anybody reads from.
const MAX_FILES: usize = 60;

/// Directories under a project that hold logs the application wrote.
///
/// A fixed list rather than a search for `*.log`: the project directory is the
/// user's source tree, and walking it would descend into `vendor/` and
/// `node_modules/`, which between them hold more files than everything else on
/// the machine.
const APP_LOG_DIRS: [&str; 4] = ["storage/logs", "var/log", "log", "logs"];

/// WordPress writes one known file and puts it inside a directory that must not
/// be walked — `wp-content` also holds every plugin and upload.
const WORDPRESS_LOG: &str = "wp-content/debug.log";

/// Extensions worth offering. `.log` plus the rotated forms observed on disk.
fn is_log_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.starts_with('.') {
        return false;
    }
    name.ends_with(".log") || name.ends_with(".log.1") || name.ends_with(".out")
}

/// Which root an id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// `projects/<name>` — what the code wrote.
    App,
    /// `logs/projects/<name>` — what the stack wrote.
    Server,
}

impl Root {
    fn prefix(self) -> &'static str {
        match self {
            Root::App => "app",
            Root::Server => "server",
        }
    }

    fn group(self) -> &'static str {
        match self {
            Root::App => "application",
            Root::Server => "server",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    /// `<root>:<relative path>` — the handle the UI sends back, never a path.
    pub id: String,
    /// The relative path, for display.
    pub label: String,
    /// `application` or `server`.
    pub group: String,
    pub bytes: u64,
    /// Unix seconds, so the frontend can format it in the user's locale.
    pub modified: Option<i64>,
}

/// Split an id into its root and relative path, rejecting anything else.
///
/// The traversal check is on the *components*, before touching the filesystem:
/// a `..` that never resolves because the file does not exist would otherwise
/// slip past a canonicalising check.
fn parse_id(id: &str) -> Result<(Root, PathBuf)> {
    let (prefix, rest) = id
        .split_once(':')
        .ok_or_else(|| Error::new(Code::InvalidInput, format!("\"{id}\" is not a log id")))?;

    let root = match prefix {
        "app" => Root::App,
        "server" => Root::Server,
        _ => {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{prefix}\" is not a known log root"),
            ))
        }
    };

    let relative = PathBuf::from(rest);
    let ordinary = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if rest.is_empty() || !ordinary {
        return Err(Error::new(
            Code::InvalidInput,
            format!("\"{rest}\" is not a path inside the project"),
        )
        .with_hint("Log ids are relative, with no parent or root segments."));
    }

    Ok((root, relative))
}

fn root_dir(root: &Path, which: Root, name: &str) -> Result<PathBuf> {
    match which {
        Root::App => crate::workspace::project_dir(root, name),
        Root::Server => {
            // Validated through the same helper, so an unsafe name is rejected
            // before it is used to build any path at all.
            crate::workspace::project_dir(root, name)?;
            Ok(root.join("logs").join("projects").join(name))
        }
    }
}

/// Turn an id into a file on disk, or refuse.
///
/// Confinement is checked twice on purpose: `parse_id` rejects the components,
/// and this rejects a path that resolves outside its root anyway — which a
/// symlink inside the project can do with no `..` in sight.
pub fn resolve(root: &Path, name: &str, id: &str) -> Result<PathBuf> {
    let (which, relative) = parse_id(id)?;
    let base = root_dir(root, which, name)?;
    let path = base.join(&relative);

    if let (Ok(real), Ok(real_base)) = (path.canonicalize(), base.canonicalize()) {
        if !real.starts_with(&real_base) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("\"{id}\" resolves outside the project"),
            ));
        }
    }

    if !path.is_file() {
        return Err(Error::not_found(format!("log file {id}")));
    }
    Ok(path)
}

fn modified_epoch(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Collect log files under `dir`, recording each as a path relative to `base`.
fn collect(base: &Path, dir: &Path, which: Root, depth: usize, out: &mut Vec<LogFile>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES * 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };

        if meta.is_dir() {
            // `symlink_metadata` would be needed to spot a link; `metadata`
            // follows it, and a link pointing back up would loop. Depth caps
            // that, and `resolve` refuses to open anything outside the root.
            collect(base, &path, which, depth + 1, out);
            continue;
        }
        if !meta.is_file() || !is_log_file(&path) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let label = relative.to_string_lossy().replace('\\', "/");

        out.push(LogFile {
            id: format!("{}:{}", which.prefix(), label),
            label,
            group: which.group().to_string(),
            bytes: meta.len(),
            modified: modified_epoch(&meta),
        });
    }
}

/// Every log file this project has, newest first.
///
/// Needs no engine. A container that died during boot wrote its reason to one
/// of these files and is no longer around to be asked.
pub fn candidates(root: &Path, name: &str) -> Result<Vec<LogFile>> {
    let project = crate::workspace::project_dir(root, name)?;
    let mut out = Vec::new();

    for dir in APP_LOG_DIRS {
        collect(&project, &project.join(dir), Root::App, 1, &mut out);
    }

    let wordpress = project.join(WORDPRESS_LOG);
    if let Ok(meta) = std::fs::metadata(&wordpress) {
        if meta.is_file() {
            out.push(LogFile {
                id: format!("app:{WORDPRESS_LOG}"),
                label: WORDPRESS_LOG.to_string(),
                group: Root::App.group().to_string(),
                bytes: meta.len(),
                modified: modified_epoch(&meta),
            });
        }
    }

    let server = root_dir(root, Root::Server, name)?;
    collect(&server, &server, Root::Server, 1, &mut out);

    // Newest first: the file somebody wants is almost always the one that just
    // changed. Ties break on the id so the order does not wobble between calls.
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
    out.truncate(MAX_FILES);
    Ok(out)
}

/// Read the last `max_bytes` of a file as text.
///
/// Reading the whole thing is not an option: these grow to hundreds of
/// megabytes, and the interesting end is the last screen. Returns the byte
/// offset the read started from, so a follower knows where to continue.
pub fn tail(path: &Path, max_bytes: u64) -> Result<(String, u64)> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)
        .map_err(|e| Error::io(format!("opening {}", path.display()), e))?;
    let len = file
        .metadata()
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?
        .len();

    let from = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(from))
        .map_err(|e| Error::io(format!("seeking {}", path.display()), e))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;

    // A log is not required to be valid UTF-8 — a truncated multi-byte
    // character at the seek point is guaranteed not to be. Lossy rather than an
    // error: a replacement character in one line beats refusing the file.
    let mut text = String::from_utf8_lossy(&buffer).into_owned();

    // Seeking by bytes lands mid-line unless the file starts there. Half a line
    // presented as a line is a log entry that never happened.
    if from > 0 {
        match text.find('\n') {
            Some(i) => text = text[i + 1..].to_string(),
            None => text.clear(),
        }
    }

    Ok((text, len))
}

/// What changed since `offset`, and where to continue from.
///
/// Handles the two ways a log file moves under a reader: it grows, or it is
/// replaced. Laravel's daily channel writes a new file and `> laravel.log`
/// truncates in place; in both cases the file is now shorter than where the
/// reader had got to, and continuing from that offset would read nothing for
/// ever while the app kept logging.
pub fn read_since(path: &Path, offset: u64) -> Result<(String, u64)> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)
        .map_err(|e| Error::io(format!("opening {}", path.display()), e))?;
    let len = file
        .metadata()
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?
        .len();

    if len < offset {
        // Truncated or rotated. Start again from the top of whatever is there
        // now rather than silently going quiet.
        return tail(path, len);
    }
    if len == offset {
        return Ok((String::new(), len));
    }

    file.seek(SeekFrom::Start(offset))
        .map_err(|e| Error::io(format!("seeking {}", path.display()), e))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| Error::io(format!("reading {}", path.display()), e))?;

    Ok((String::from_utf8_lossy(&buffer).into_owned(), len))
}

// ------------------------------------------------------- across every project

/// How many files the cross-project tail follows at once.
///
/// Eight projects at the per-project cap is 480 files, and a `stat` apiece
/// twice a second is not the problem — 480 concurrently interesting files is.
/// The cap is on attention, not on cost, and what it drops is *reported* rather
/// than silently trimmed: `FanoutScan` carries both numbers so the UI can say
/// "following 60 of 137" instead of implying it covers everything.
const MAX_FOLLOWED: usize = 60;

/// The managed projects, read from disk with no engine involved.
///
/// A directory counts when it holds a `stackvo.json`; an unadopted folder under
/// `projects/` is somebody's checkout, not a project this app has anything to
/// say about. Matches `list_projects`, minus the container lookup — this path
/// must keep working with the engine down, which is the whole premise of
/// reading logs from the host.
pub fn projects(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("projects");
    let entries =
        std::fs::read_dir(&dir).map_err(|e| Error::io(format!("reading {}", dir.display()), e))?;

    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || !crate::workspace::is_safe_name(name) {
            continue;
        }
        if !path.join("stackvo.json").is_file() {
            continue;
        }
        names.push(name.to_string());
    }
    names.sort();
    Ok(names)
}

/// One log file, told apart from the identically-named file in another project.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLogFile {
    pub project: String,
    #[serde(flatten)]
    pub file: LogFile,
}

/// Every log file every project has, newest first.
pub fn candidates_all(root: &Path) -> Result<Vec<ProjectLogFile>> {
    let mut out = Vec::new();
    for name in projects(root)? {
        // One unreadable project does not sink the list — a directory whose
        // permissions changed is a gap in coverage, not a failed call.
        let Ok(files) = candidates(root, &name) else {
            continue;
        };
        for file in files {
            out.push(ProjectLogFile {
                project: name.clone(),
                file,
            });
        }
    }
    out.sort_by(|a, b| {
        b.file
            .modified
            .cmp(&a.file.modified)
            .then_with(|| a.project.cmp(&b.project))
            .then_with(|| a.file.id.cmp(&b.file.id))
    });
    Ok(out)
}

/// A line, with enough identity to say where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanoutLine {
    pub project: String,
    /// The `LogFile.id` the line was read from — an opaque handle, not a path.
    pub id: String,
    pub text: String,
    /// True for the seed: lines that were already in the file when the tail
    /// started. Ordering across files is by file, not by time, and the UI says
    /// so rather than letting them pass as live output.
    pub historic: bool,
}

/// What one discovery pass found.
#[derive(Debug, Clone, Copy, Default)]
pub struct FanoutScan {
    /// Files now being followed.
    pub followed: usize,
    /// Files that exist. Larger than `followed` means the cap bit.
    pub total: usize,
    pub projects: usize,
}

struct Tracked {
    path: PathBuf,
    offset: u64,
}

/// How much of each file the first pass rewinds over. Small on purpose: this
/// is "where does this file end", not "what happened today" — that question
/// belongs to the per-project viewer, which reads one file and can answer it.
const SEED_BYTES: u64 = 2_048;

/// A live tail across every project at once.
///
/// **Live, over a labelled seed.** The ordering problem is real — nothing here
/// parses a timestamp (Laravel, nginx and supervisord do not agree on a
/// format), so across files the only chronology available is the order bytes
/// arrive in: true for new output, fiction for old. Adopting every file at its
/// current end solved that honestly and produced a page that is *empty* on any
/// stack that has been quiet for an hour — which reads as broken, not as calm.
///
/// So the first pass seeds a small tail per file and marks those lines
/// `historic`. They are grouped by file, never interleaved, and the UI draws
/// the live boundary after them — the claim is "here is where each file
/// currently ends", which is true, rather than "here is what happened", which
/// would not be. Everything after the boundary is genuinely live.
///
/// Re-discovery is the other half. A daily channel rolls over at midnight into
/// a filename that did not exist when the tail started, so a fixed file set
/// goes quiet exactly when the day's log begins. Files found by a later scan
/// are adopted at offset zero, because everything in them was written after the
/// tail was already watching.
pub struct Fanout {
    root: PathBuf,
    tracked: HashMap<(String, String), Tracked>,
    /// The first scan seeds a tail; later scans adopt at the top.
    seeded: bool,
    /// Has the seed been delivered? Only the first poll is historic.
    drained: bool,
}

impl Fanout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            tracked: HashMap::new(),
            seeded: false,
            drained: false,
        }
    }

    /// Re-discover, adopting files that appeared and forgetting files that did
    /// not survive. Emits nothing: what a newly adopted file holds is delivered
    /// by the next `poll`, through the same delta path as everything else.
    pub fn scan(&mut self, only: &[String]) -> FanoutScan {
        let Ok(names) = projects(&self.root) else {
            return FanoutScan::default();
        };
        let names: Vec<String> = if only.is_empty() {
            names
        } else {
            names.into_iter().filter(|n| only.contains(n)).collect()
        };

        let mut found: Vec<(String, LogFile)> = Vec::new();
        for name in &names {
            let Ok(files) = candidates(&self.root, name) else {
                continue;
            };
            for file in files {
                found.push((name.clone(), file));
            }
        }
        let total = found.len();

        // Newest first, so what the cap drops is what nobody has written to in
        // the longest — and the tie-break is stable, or the followed set would
        // churn between scans and re-adopt files at their end, losing lines.
        found.sort_by(|a, b| {
            b.1.modified
                .cmp(&a.1.modified)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        found.truncate(MAX_FOLLOWED);

        let mut keep = HashMap::new();
        for (project, file) in found {
            let key = (project.clone(), file.id.clone());
            if let Some(existing) = self.tracked.remove(&key) {
                keep.insert(key, existing);
                continue;
            }
            let Ok(path) = resolve(&self.root, &project, &file.id) else {
                continue;
            };
            // First scan: start at the end, because everything already in the
            // file predates the request to watch. Later scans: start at zero,
            // because the file itself postdates it.
            // First pass: rewind a little so each file shows where it
            // currently ends. Later passes adopt at zero — a file that
            // appeared after the tail started was written while we watched.
            let offset = if self.seeded {
                0
            } else {
                file.bytes.saturating_sub(SEED_BYTES)
            };
            keep.insert(key, Tracked { path, offset });
        }

        // Whatever is left in `tracked` was not found this time — rotated away
        // or deleted. Dropping it is what stops a vanished file from erroring
        // on every poll for as long as the pane stays open.
        self.tracked = keep;
        self.seeded = true;

        FanoutScan {
            followed: self.tracked.len(),
            total,
            projects: names.len(),
        }
    }

    /// Everything written since the last call, tagged with its origin.
    ///
    /// The first call after a scan returns the seed, marked `historic`; every
    /// call after that is live output.
    pub fn poll(&mut self) -> Vec<FanoutLine> {
        let historic = !self.drained;
        self.drained = true;

        let mut out = Vec::new();
        let mut gone = Vec::new();

        for (key, tracked) in self.tracked.iter_mut() {
            match read_since(&tracked.path, tracked.offset) {
                Ok((chunk, next)) => {
                    tracked.offset = next;
                    for line in chunk.lines() {
                        out.push(FanoutLine {
                            project: key.0.clone(),
                            id: key.1.clone(),
                            text: line.to_string(),
                            historic,
                        });
                    }
                }
                Err(_) => gone.push(key.clone()),
            }
        }

        for key in gone {
            self.tracked.remove(&key);
        }

        // Grouped by file as read, which within one file is true order. Across
        // files the map's iteration order is arbitrary, so sorting by project
        // at least makes the same batch render the same way twice.
        out.sort_by(|a, b| a.project.cmp(&b.project).then_with(|| a.id.cmp(&b.id)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-applog-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole point of the id: the frontend never names a path, so it cannot
    /// name one outside the project.
    #[test]
    fn traversal_is_refused_before_the_filesystem_is_touched() {
        assert!(parse_id("app:../../../../etc/passwd").is_err());
        assert!(parse_id("app:/etc/passwd").is_err());
        assert!(parse_id("app:").is_err());
        assert!(parse_id("etc/passwd").is_err(), "no root prefix");
        assert!(
            parse_id("shell:storage/logs/a.log").is_err(),
            "unknown root"
        );
    }

    #[test]
    fn the_two_roots_are_told_apart() {
        let (root, path) = parse_id("app:storage/logs/laravel.log").unwrap();
        assert_eq!(root, Root::App);
        assert_eq!(path, PathBuf::from("storage/logs/laravel.log"));

        let (root, path) = parse_id("server:nginx/error.log").unwrap();
        assert_eq!(root, Root::Server);
        assert_eq!(path, PathBuf::from("nginx/error.log"));
    }

    #[test]
    fn only_log_files_are_offered() {
        assert!(is_log_file(Path::new("a/laravel.log")));
        assert!(is_log_file(Path::new("a/error.log.1")));
        assert!(is_log_file(Path::new("a/worker.out")));
        assert!(!is_log_file(Path::new("a/index.php")));
        assert!(!is_log_file(Path::new("a/.gitignore")));
        // A dotfile that ends in .log is still a dotfile — `.DS_Store` taught
        // this lesson once already.
        assert!(!is_log_file(Path::new("a/.hidden.log")));
    }

    /// Seeking by bytes lands mid-line. Presenting that fragment as a line
    /// invents a log entry.
    #[test]
    fn a_partial_first_line_is_dropped() {
        let dir = scratch("partial");
        let file = dir.join("a.log");
        std::fs::write(&file, "first line\nsecond line\nthird line\n").unwrap();

        let (text, len) = tail(&file, 18).unwrap();
        assert_eq!(len, 34);
        assert!(!text.contains("first"), "got {text:?}");
        assert!(text.ends_with("third line\n"));
        // Whatever survived starts at a line boundary.
        assert!(
            text.starts_with("second") || text.starts_with("third"),
            "got {text:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reading from the start needs no trimming — there is no partial line.
    #[test]
    fn a_short_file_is_returned_whole() {
        let dir = scratch("whole");
        let file = dir.join("a.log");
        std::fs::write(&file, "only line\n").unwrap();
        let (text, _) = tail(&file, 4096).unwrap();
        assert_eq!(text, "only line\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn following_returns_only_what_is_new() {
        let dir = scratch("follow");
        let file = dir.join("a.log");
        std::fs::write(&file, "one\n").unwrap();
        let (_, offset) = tail(&file, 4096).unwrap();

        let (text, offset) = read_since(&file, offset).unwrap();
        assert_eq!(text, "", "nothing was appended");

        std::fs::write(&file, "one\ntwo\n").unwrap();
        let (text, _) = read_since(&file, offset).unwrap();
        assert_eq!(text, "two\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `> laravel.log` in a terminal, or a daily channel rolling over. The
    /// reader is now past the end of the file; continuing from there reads
    /// nothing for ever while the application keeps logging.
    #[test]
    fn truncation_restarts_instead_of_going_silent() {
        let dir = scratch("truncate");
        let file = dir.join("a.log");
        std::fs::write(&file, "old line one\nold line two\n").unwrap();
        let (_, offset) = tail(&file, 4096).unwrap();

        std::fs::write(&file, "fresh\n").unwrap();
        let (text, new_offset) = read_since(&file, offset).unwrap();
        assert_eq!(text, "fresh\n");
        assert_eq!(new_offset, 6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Laravel channels nest, so a flat listing misses most of what a real
    /// project writes — `storage/logs/parser/parser-2026-07-28.log` is a real
    /// path from the checkout this was written against.
    #[test]
    fn nested_channel_directories_are_found() {
        let dir = scratch("nested");
        let logs = dir.join("storage/logs/parser");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(dir.join("storage/logs/laravel.log"), "a\n").unwrap();
        std::fs::write(logs.join("parser-2026-07-28.log"), "b\n").unwrap();
        std::fs::write(dir.join("storage/logs/.gitignore"), "*\n").unwrap();

        let mut out = Vec::new();
        collect(&dir, &dir.join("storage/logs"), Root::App, 1, &mut out);

        let mut labels: Vec<&str> = out.iter().map(|f| f.label.as_str()).collect();
        labels.sort();
        assert_eq!(
            labels,
            [
                "storage/logs/laravel.log",
                "storage/logs/parser/parser-2026-07-28.log"
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A workspace with `n` projects, each holding one Laravel-shaped log.
    fn workspace(name: &str, projects: &[&str]) -> PathBuf {
        let root = scratch(name);
        for project in projects {
            let logs = root.join("projects").join(project).join("storage/logs");
            std::fs::create_dir_all(&logs).unwrap();
            std::fs::write(
                root.join("projects").join(project).join("stackvo.json"),
                "{}",
            )
            .unwrap();
            std::fs::write(logs.join("laravel.log"), "old line\n").unwrap();
        }
        root
    }

    /// An unadopted checkout under `projects/` is somebody's folder, not a
    /// project this app follows.
    #[test]
    fn only_projects_with_a_manifest_are_listed() {
        let root = workspace("projects-list", &["alpha", "beta"]);
        std::fs::create_dir_all(root.join("projects/stray/storage/logs")).unwrap();
        std::fs::create_dir_all(root.join("projects/.hidden")).unwrap();

        assert_eq!(projects(&root).unwrap(), vec!["alpha", "beta"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The premise of the whole module: no engine is consulted, so a file in a
    /// project whose container never started is still listed.
    #[test]
    fn candidates_span_every_project() {
        let root = workspace("candidates-all", &["alpha", "beta"]);
        let all = candidates_all(&root).unwrap();

        assert_eq!(all.len(), 2);
        let mut seen: Vec<&str> = all.iter().map(|f| f.project.as_str()).collect();
        seen.sort();
        assert_eq!(seen, ["alpha", "beta"]);
        assert!(all
            .iter()
            .all(|f| f.file.label == "storage/logs/laravel.log"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The seed comes first and says so; everything after it is live.
    ///
    /// Adopting strictly at end-of-file was honest and produced a blank page on
    /// any stack that had been quiet for an hour — indistinguishable from
    /// broken. The seed is small, grouped by file, and flagged, so the UI can
    /// draw the boundary rather than pass old lines off as new output.
    #[test]
    fn the_fanout_seeds_a_labelled_tail_then_goes_live() {
        let root = workspace("fanout-live", &["alpha", "beta"]);
        let mut fanout = Fanout::new(&root);

        let scan = fanout.scan(&[]);
        assert_eq!(scan.followed, 2);
        assert_eq!(scan.projects, 2);

        let seed = fanout.poll();
        assert!(!seed.is_empty(), "the page would open blank");
        assert!(seed.iter().all(|l| l.historic), "the seed must be flagged");
        // A second poll with nothing written is silent — the seed is delivered
        // once, not replayed on every tick.
        assert!(fanout.poll().is_empty());

        std::fs::write(
            root.join("projects/alpha/storage/logs/laravel.log"),
            "old line\nfresh\n",
        )
        .unwrap();

        let lines = fanout.poll();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].project, "alpha");
        assert_eq!(lines[0].text, "fresh");
        assert_eq!(lines[0].id, "app:storage/logs/laravel.log");
        assert!(!lines[0].historic, "output after the seed is live");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A daily channel rolls over into a filename that did not exist when the
    /// tail started. A fixed file set goes quiet exactly at midnight, which is
    /// exactly when the day's log begins — so rediscovery adopts at the top,
    /// since everything in a file that new was written while we were watching.
    #[test]
    fn a_file_created_after_the_tail_started_is_read_from_the_top() {
        let root = workspace("fanout-rollover", &["alpha"]);
        let mut fanout = Fanout::new(&root);
        fanout.scan(&[]);
        fanout.poll(); // drain the seed

        std::fs::write(
            root.join("projects/alpha/storage/logs/laravel-2026-07-31.log"),
            "first line of the new day\n",
        )
        .unwrap();

        let scan = fanout.scan(&[]);
        assert_eq!(scan.followed, 2, "the rolled-over file was not adopted");

        let lines = fanout.poll();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "first line of the new day");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Re-scanning must not re-adopt a file already being followed: adoption on
    /// a later scan starts at zero, so a churning followed-set would replay the
    /// whole file every thirty seconds.
    #[test]
    fn rescanning_does_not_replay_a_file_already_followed() {
        let root = workspace("fanout-stable", &["alpha"]);
        let mut fanout = Fanout::new(&root);
        fanout.scan(&[]);
        fanout.poll();

        std::fs::write(
            root.join("projects/alpha/storage/logs/laravel.log"),
            "old line\nsecond\n",
        )
        .unwrap();
        fanout.scan(&[]);

        let lines = fanout.poll();
        assert_eq!(lines.len(), 1, "got {lines:?}");
        assert_eq!(lines[0].text, "second");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file that rotated away by rename errors on every read. Dropping it is
    /// what stops the poll spinning on it for as long as the pane stays open.
    #[test]
    fn a_vanished_file_is_dropped_rather_than_polled_for_ever() {
        let root = workspace("fanout-gone", &["alpha"]);
        let mut fanout = Fanout::new(&root);
        assert_eq!(fanout.scan(&[]).followed, 1);

        std::fs::remove_file(root.join("projects/alpha/storage/logs/laravel.log")).unwrap();
        assert!(fanout.poll().is_empty());
        assert!(fanout.tracked.is_empty(), "the dead file is still tracked");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_fanout_can_be_narrowed_to_chosen_projects() {
        let root = workspace("fanout-filter", &["alpha", "beta"]);
        let mut fanout = Fanout::new(&root);

        let scan = fanout.scan(&["beta".to_string()]);
        assert_eq!(scan.followed, 1);
        assert_eq!(scan.projects, 1);

        // The seed only ever covers the selected project.
        assert!(fanout.poll().iter().all(|l| l.project == "beta"));

        std::fs::write(
            root.join("projects/alpha/storage/logs/laravel.log"),
            "old line\nignored\n",
        )
        .unwrap();
        assert!(fanout.poll().is_empty(), "an unselected project leaked in");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn depth_is_bounded() {
        let dir = scratch("deep");
        let deep = dir.join("logs/a/b/c/d/e");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.log"), "x\n").unwrap();

        let mut out = Vec::new();
        collect(&dir, &dir.join("logs"), Root::App, 1, &mut out);
        assert!(out.is_empty(), "walked past the depth cap: {out:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
