//! Writing a file so that an interrupted write cannot destroy the old one.
//!
//! Every file this app writes belongs to the user: `stackvo.json` is part of
//! their source tree, `.env` configures their whole stack. `fs::write`
//! truncates first and fills after, so a crash, a power loss or a full disk
//! between those two steps leaves a zero-length or half-written file and the
//! previous contents are gone. Writing to a sibling and renaming makes the
//! replacement a single atomic step: either the old file or the new one, never
//! a torn one.
//!
//! `.env` already did this by hand; `stackvo.json` did not. One helper rather
//! than two conventions, because the second file is the one in the user's
//! repository.

use crate::error::{Error, Result};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes concurrent writers within this process; the pid distinguishes
/// them across processes. A fixed `.tmp` name would let two writers clobber
/// each other's staging file and rename a mix of both into place.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Replace `path` with `contents`, atomically.
///
/// The temporary lives in the same directory as the target, because `rename` is
/// only atomic within a filesystem and the OS temp directory is routinely on a
/// different one.
pub fn write(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| {
        Error::io(
            format!("writing {}", path.display()),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path has no parent directory",
            ),
        )
    })?;

    let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let temp = dir.join(format!(
        ".{stem}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    // Scoped so the handle is closed before the rename: Windows refuses to
    // rename over — or from — a file that still has an open handle.
    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        // Without this the rename can land while the data is still in the page
        // cache, which is exactly the crash window this function exists to
        // close.
        file.sync_all()
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(Error::io(format!("writing {}", temp.display()), e));
    }

    if let Err(e) = std::fs::rename(&temp, path) {
        // Leaving the staging file behind would litter the user's project
        // directory with something that looks like a StackVo artefact.
        let _ = std::fs::remove_file(&temp);
        return Err(Error::io(format!("replacing {}", path.display()), e));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("stackvo-atomic-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_a_file_that_did_not_exist() {
        let dir = sandbox("create");
        let path = dir.join("new.json");

        write(&path, "{\"a\":1}").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replaces_existing_contents_completely() {
        let dir = sandbox("replace");
        let path = dir.join("existing.json");
        std::fs::write(&path, "a much longer previous body").unwrap();

        write(&path, "short").unwrap();

        // Not "short" followed by the tail of the old file — the rename swaps
        // the whole inode rather than overwriting in place.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "short");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaves_no_staging_file_behind() {
        let dir = sandbox("clean");
        write(&dir.join("f.json"), "{}").unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files remained: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The failure this guards: a write into a directory that does not exist
    /// must not leave the caller thinking the file was replaced.
    #[test]
    fn reports_an_error_when_the_directory_is_missing() {
        let dir = sandbox("missing");
        let path = dir.join("nope").join("f.json");

        assert!(write(&path, "{}").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two writers must not stage into the same temporary path.
    #[test]
    fn concurrent_writers_get_distinct_staging_paths() {
        let dir = sandbox("concurrent");
        let path = dir.join("shared.json");

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = path.clone();
                std::thread::spawn(move || write(&p, &format!("{{\"writer\":{i}}}")))
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }

        // Whichever writer landed last, the file is one of theirs in full.
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(
            (0..8).any(|i| body == format!("{{\"writer\":{i}}}")),
            "torn or interleaved result: {body}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
