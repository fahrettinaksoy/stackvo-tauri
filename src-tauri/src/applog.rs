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
