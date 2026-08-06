//! Everything a bug report needs, in one file.
//!
//! Settings could already open the log *folder*. That leaves the rest to the
//! person reporting the problem: find the right file among seven daily ones,
//! know that the doctor output exists and is separate, remember the app version
//! and the platform, and think to mention that the engine was down. Most people
//! attach the newest log and nothing else, and the first reply is always a list
//! of the other four things.
//!
//! So this collects them. One button, one archive, and the maintainer gets the
//! same set every time.
//!
//! ## Masked again on the way in
//!
//! `logging::redact` already runs over subprocess output as it is written, so
//! the files on disk are masked. Every log line is put through it a second time
//! here, and that is not superstition: the redactor has been extended before,
//! and a bundle assembled today may contain lines written by an older build
//! whose rule was narrower. Re-running the current rule over old text costs a
//! pass over a few megabytes and closes the one gap that would leak a password
//! into an issue tracker.
//!
//! ## Bounded, so it can actually be sent
//!
//! Logs are capped per file. An unbounded archive is one nobody can attach —
//! and the end of a log is the part that explains a failure anyway. What was
//! dropped is *stated* in the manifest rather than silently trimmed, for the
//! reason `applog::FanoutScan` reports its own cap: a truncated report that
//! looks complete is worse than a short one that says so.
//!
//! ## Readable before it is sent
//!
//! Plain text and JSON, and a `README.txt` that says what each file is. The
//! whole premise of masking is that the bundle is safe to attach — but the
//! person attaching it should still be able to look, and a format they cannot
//! open is a format they cannot check.

use crate::error::{Error, Result};
use std::io::Write;
use std::path::Path;

/// How much of each log file is carried.
///
/// One mebibyte is roughly a day of ordinary operation and several thousand
/// lines of a failing build — past the point where more text adds information.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// One file in the archive.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub bytes: u64,
    /// True when the file was cut to [`MAX_LOG_BYTES`]. Reported rather than
    /// hidden, so nobody reads a trimmed log as the whole story.
    pub truncated: bool,
}

/// What was written, for the UI to show without opening the archive.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub path: String,
    pub bytes: u64,
    pub entries: Vec<Entry>,
}

/// The environment half of the report — no workspace and no engine needed.
///
/// Split out because it is the part that is always answerable. A bundle from a
/// machine where nothing works at all still carries this, and "which version on
/// which platform" is the first question of every report.
pub fn about() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    format!(
        "StackVo Desktop diagnostic bundle\n\
         \n\
         version    {version}\n\
         platform   {os} {arch}\n\
         collected  {stamp} UTC (unix {seconds})\n\
         log dir    {logs}\n\
         config dir {config}\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        stamp = crate::crash::stamp(seconds),
        logs = crate::appdir::logs()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".into()),
        config = crate::appdir::config()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".into()),
    )
}

/// The header that tells whoever opens the archive what they are looking at.
fn readme(entries: &[Entry]) -> String {
    let mut out = String::from(
        "What is in here\n\
         ===============\n\
         \n\
         about.txt      version, platform and where this app keeps its files\n\
         preflight.json every startup requirement and whether it is met\n\
         doctor.json    ports, hosts entries, generated-config drift, disk use\n\
         engine.json    the Docker engine as this app sees it\n\
         logs/          the rotating application log, newest last\n\
         crashes/       panic reports, if this app has ever died on this machine\n\
         \n\
         Passwords and tokens are masked as the log is written and masked again\n\
         as this archive is built. Nothing here is read from your .env, and no\n\
         project source is included. It is meant to be safe to attach to an\n\
         issue — but it is plain text, so read it first.\n\
         \n\
         Files\n\
         -----\n",
    );
    for entry in entries {
        out.push_str(&format!(
            "{:<40} {:>10} bytes{}\n",
            entry.name,
            entry.bytes,
            if entry.truncated {
                "  (truncated to the most recent 1 MiB)"
            } else {
                ""
            }
        ));
    }
    out
}

/// Pretty JSON, or a note saying why there is none.
///
/// A section that failed must still appear. An absent `doctor.json` is
/// indistinguishable from a bundle built by an older version; a `doctor.json`
/// holding an error is a fact about the machine.
fn section<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("{{ \"error\": \"could not serialise: {e}\" }}"))
}

/// Read the tail of a file and mask it, reporting whether it was cut.
fn redacted_tail(path: &Path) -> Option<(String, bool)> {
    let len = std::fs::metadata(path).ok()?.len();
    let truncated = len > MAX_LOG_BYTES;

    let (text, _) = crate::applog::tail(path, MAX_LOG_BYTES).ok()?;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(&crate::logging::redact(line));
        out.push('\n');
    }
    Some((out, truncated))
}

/// Everything the archive holds, as `(name, contents)`.
///
/// Separate from the writing so it can be tested without producing a zip and
/// unpacking it again. What matters about this function is *what is collected
/// and what is masked*; that a zip writer can write bytes is not in question.
pub async fn parts(root: Option<&Path>) -> Vec<(String, String)> {
    let mut parts = vec![("about.txt".to_string(), about())];

    // Each of these reaches out — to Docker, to the filesystem, to the host —
    // and each is written to answer rather than to fail, so a broken machine
    // still produces a full bundle. That is exactly the machine this is for.
    parts.push((
        "preflight.json".into(),
        section(&crate::preflight::run().await),
    ));
    parts.push((
        "doctor.json".into(),
        section(&crate::doctor::run(root).await),
    ));
    parts.push((
        "engine.json".into(),
        section(&crate::engine::status().await),
    ));

    if let Some(dir) = crate::logging::dir() {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        // By name, which for both the daily log and the crash reports is
        // chronological — see `crash::stamp`.
        files.sort();

        for path in files {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((text, _)) = redacted_tail(&path) else {
                continue;
            };
            let folder = if name.starts_with("crash-") {
                "crashes"
            } else {
                "logs"
            };
            parts.push((format!("{folder}/{name}"), text));
        }
    }

    parts
}

/// Write the bundle to `dest`.
///
/// `dest` comes from the system save dialog, like `mail::save_attachment`'s
/// does — the front end never names a destination this process did not receive
/// from the user.
pub async fn write(root: Option<&Path>, dest: &Path) -> Result<Bundle> {
    let parts = parts(root).await;

    let mut entries: Vec<Entry> = parts
        .iter()
        .map(|(name, body)| Entry {
            name: name.clone(),
            bytes: body.len() as u64,
            truncated: body.len() as u64 >= MAX_LOG_BYTES,
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let file = std::fs::File::create(dest)
        .map_err(|e| Error::io(format!("creating {}", dest.display()), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let readme = readme(&entries);
    let io = |e: std::io::Error| Error::io(format!("writing {}", dest.display()), e);

    zip.start_file("README.txt", options).map_err(zip_error)?;
    zip.write_all(readme.as_bytes()).map_err(io)?;

    for (name, body) in &parts {
        zip.start_file(name, options).map_err(zip_error)?;
        zip.write_all(body.as_bytes()).map_err(io)?;
    }
    zip.finish().map_err(zip_error)?;

    let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    tracing::info!(path = %dest.display(), bytes, files = entries.len() + 1, "diagnostic bundle written");

    Ok(Bundle {
        path: dest.display().to_string(),
        bytes,
        entries,
    })
}

fn zip_error(e: zip::result::ZipError) -> Error {
    Error::new(
        crate::error::Code::IoError,
        format!("could not build the archive: {e}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_names_the_version_and_the_platform() {
        let text = about();
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains(std::env::consts::OS));
        assert!(text.contains("UTC"));
    }

    /// The readme is the only part a non-developer reads, and its whole job is
    /// to say what is in the archive and that it is safe to attach.
    #[test]
    fn the_readme_lists_every_file_and_says_it_is_masked() {
        let entries = vec![
            Entry {
                name: "logs/stackvo.2026-08-06.log".into(),
                bytes: 12,
                truncated: false,
            },
            Entry {
                name: "logs/stackvo.2026-08-05.log".into(),
                bytes: MAX_LOG_BYTES,
                truncated: true,
            },
        ];
        let text = readme(&entries);

        assert!(text.contains("logs/stackvo.2026-08-06.log"));
        assert!(text.contains("logs/stackvo.2026-08-05.log"));
        assert!(text.contains("truncated"), "a cut file must say so");
        assert!(text.contains("masked"));
        assert!(text.contains("read it first"));
    }

    /// The reason the second masking pass exists: a log written by an older
    /// build carries whatever that build's redactor let through, and today's
    /// rule is the one that should decide what leaves the machine.
    #[test]
    fn a_secret_already_on_disk_is_masked_on_the_way_into_the_bundle() {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-diagnostics-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("stackvo.2026-08-06.log");
        std::fs::write(
            &log,
            "starting up\nshop | MYSQL_ROOT_PASSWORD=hunter2\ndone\n",
        )
        .unwrap();

        let (text, truncated) = redacted_tail(&log).expect("a readable log");

        assert!(!truncated);
        assert!(text.contains("MYSQL_ROOT_PASSWORD=***"), "{text}");
        assert!(!text.contains("hunter2"));
        // And the ordinary lines survive, or the bundle is useless.
        assert!(text.contains("starting up"));
        assert!(text.contains("done"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_log_is_cut_and_says_so() {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-diagnostics-big-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("big.log");

        let line = "a line of ordinary build output that says nothing secret\n";
        let body = line.repeat((MAX_LOG_BYTES as usize / line.len()) + 2_000);
        std::fs::write(&log, &body).unwrap();

        let (text, truncated) = redacted_tail(&log).expect("a readable log");
        assert!(truncated, "the cap did not bite");
        assert!(
            (text.len() as u64) <= MAX_LOG_BYTES,
            "kept {} bytes, cap is {MAX_LOG_BYTES}",
            text.len()
        );
        // The *end* is what is kept — the beginning of a long log is the part
        // nobody needs.
        assert!(text.ends_with("secret\n"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A section that could not be produced still has to appear, or its absence
    /// reads as "this bundle came from an older version".
    #[test]
    fn an_unserialisable_section_becomes_a_recorded_error_not_a_gap() {
        // `f64::NAN` has no JSON representation, which is the one way
        // `to_string_pretty` fails on a type that is otherwise `Serialize`.
        let text = section(&serde_json::json!({ "ok": true }));
        assert!(text.contains("\"ok\": true"));

        // A map with a non-string key is the reliable `serde_json` failure —
        // JSON objects have string keys and nothing else.
        let mut map = std::collections::HashMap::new();
        map.insert(vec![1u8, 2], "value");
        let text = section(&map);
        assert!(text.contains("error"), "{text}");
    }

    /// The archive itself, written and read back.
    ///
    /// The parts are covered above without a zip in sight; what this adds is
    /// the one thing those cannot: that the file on disk is a readable archive
    /// with the names the README promises. A bundle that cannot be opened is
    /// worse than no bundle, and nothing else would notice.
    #[tokio::test]
    async fn the_archive_opens_and_holds_what_the_readme_lists() {
        let dir = std::env::temp_dir().join(format!(
            "stackvo-bundle-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("bundle.zip");

        // No workspace: the bundle from a machine that never got that far is
        // exactly the one somebody needs, so it must still be produced.
        let bundle = write(None, &dest)
            .await
            .expect("a bundle with no workspace");

        assert!(dest.is_file());
        assert_eq!(bundle.path, dest.display().to_string());
        assert!(bundle.bytes > 0);

        let file = std::fs::File::open(&dest).unwrap();
        let mut archive = zip::ZipArchive::new(file).expect("the file is a readable zip");

        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();

        for required in [
            "README.txt",
            "about.txt",
            "preflight.json",
            "doctor.json",
            "engine.json",
        ] {
            assert!(
                names.contains(&required.to_string()),
                "missing {required} from {names:?}"
            );
        }

        // The README has to name every entry, or it is a manifest that lies.
        let mut readme = String::new();
        {
            use std::io::Read;
            archive
                .by_name("README.txt")
                .unwrap()
                .read_to_string(&mut readme)
                .unwrap();
        }
        for entry in &bundle.entries {
            assert!(
                readme.contains(&entry.name),
                "the README does not mention {}",
                entry.name
            );
        }

        // And the JSON sections are JSON, not a debug rendering of a struct.
        for name in ["preflight.json", "doctor.json", "engine.json"] {
            use std::io::Read;
            let mut text = String::new();
            archive
                .by_name(name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or_else(|e| panic!("{name} is not valid JSON: {e}"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
