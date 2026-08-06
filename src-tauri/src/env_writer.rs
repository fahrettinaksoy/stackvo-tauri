//! Editing `.env` without destroying it.
//!
//! StackVo's `.env` is 159 keys with section banners, explanatory comments and
//! deliberate blank lines. Serialising a parsed map back out would produce a
//! valid file that is worthless to the human who maintains it — so this module
//! never rewrites the file, it patches lines in place.
//!
//! Rules:
//!   - An existing key keeps its line number, its surrounding whitespace and
//!     any trailing comment on that line.
//!   - A new key is appended under a generated section, never interleaved.
//!   - Everything else in the file is byte-identical afterwards.
//!   - The previous contents are saved to `.env.bak` before the first write.

use crate::error::{Code, Error, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

/// Serialises the read-modify-write in `apply`.
///
/// The lock lives here rather than at the command layer because `.env` has
/// several writers — `env_set`, and `service_enable`/`service_disable` via
/// `set_service_enabled` — and those commands hold *different* in-flight keys.
/// Enabling Redis while enabling Postgres would have two callers read the same
/// file and write back two different descendants of it, and the second write
/// would silently drop the first. Atomic replacement does not help: each write
/// is individually whole, and one of them is still lost.
///
/// Held only across the synchronous body of `apply`, so it never crosses an
/// await and cannot participate in a cycle with the in-flight registry.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Apply `patch` to the text of a `.env`, returning the new text.
///
/// Exposed separately from the file I/O so the behaviour is testable without
/// touching a real checkout.
pub fn patch_text(original: &str, patch: &BTreeMap<String, String>) -> String {
    if patch.is_empty() {
        return original.to_string();
    }

    let mut remaining = patch.clone();
    // Preserve the file's line ending style rather than forcing \n.
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let mut out: Vec<String> = Vec::new();

    for raw in original.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();

        // Comments and blanks pass through untouched.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push(line.to_string());
            continue;
        }

        let Some((key_part, value_part)) = line.split_once('=') else {
            out.push(line.to_string());
            continue;
        };

        let key = key_part.trim();
        let Some(new_value) = remaining.remove(key) else {
            out.push(line.to_string());
            continue;
        };

        // Keep the original indentation and any trailing comment. StackVo's
        // own file uses `KEY=value` with no padding, but a hand-edited file
        // may not, and losing a `# why this is set` note is a real loss.
        let indent: String = key_part.chars().take_while(|c| c.is_whitespace()).collect();
        let comment = value_part
            .find(" #")
            .map(|i| value_part[i..].to_string())
            .unwrap_or_default();

        out.push(format!("{indent}{key}={new_value}{comment}"));
    }

    // Anything left in the patch is a key the file did not have.
    if !remaining.is_empty() {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
            out.push(String::new());
        }
        out.push("# >>> added by StackVo Desktop >>>".to_string());
        for (key, value) in &remaining {
            out.push(format!("{key}={value}"));
        }
        out.push("# <<< added by StackVo Desktop <<<".to_string());
    }

    out.join(newline)
}

/// Patch `<root>/.env` on disk, backing it up first.
pub fn apply(root: &Path, patch: &BTreeMap<String, String>) -> Result<()> {
    validate(patch)?;

    // Poisoning means a previous writer panicked mid-patch. The file itself is
    // fine — the write is atomic — so recovering the guard is correct here.
    let _serialised = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let path = root.join(".env");
    // Absent means nothing has been overridden yet, so the patch starts from
    // an empty file and this write is what brings it into existence.
    let original = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::io(format!("reading {}", path.display()), e)),
    };

    let updated = patch_text(&original, patch);
    if updated == original {
        return Ok(());
    }

    // Back up before the first modification so a bad patch is recoverable.
    let backup = root.join(".env.bak");
    if !backup.exists() {
        std::fs::write(&backup, &original).map_err(|e| Error::io("writing .env.bak", e))?;
    }

    // A half-written .env would be read by `docker compose --env-file` and fail
    // in a way that is hard to trace back here.
    crate::atomic::write(&path, &updated)
}

/// Reject anything that would corrupt the file's line-oriented format.
fn validate(patch: &BTreeMap<String, String>) -> Result<()> {
    for (key, value) in patch {
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            || key.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Err(Error::new(
                Code::InvalidInput,
                format!("`{key}` is not a valid .env key"),
            )
            .with_hint(crate::hints::ENV_KEY_CHARSET));
        }

        if value.contains('\n') || value.contains('\r') {
            return Err(Error::new(
                Code::InvalidInput,
                format!("the value for `{key}` contains a line break"),
            )
            .with_hint(crate::hints::ENV_IS_ONE_KEY_PER_LINE));
        }
    }

    Ok(())
}

/// Flip `SERVICE_<NAME>_ENABLE`.
///
/// The service id comes from the contract catalog, never from reversing an env
/// key — see CONFLICTS.md C-09, where reversing the transform is exactly what
/// makes `mongo-express` unstartable.
pub fn set_service_enabled(root: &Path, service_id: &str, enabled: bool) -> Result<()> {
    let key = format!("{}ENABLE", crate::config::Env::service_prefix(service_id));
    let mut patch = BTreeMap::new();
    patch.insert(key, if enabled { "true" } else { "false" }.to_string());
    apply(root, &patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
###################################################################
# SERVICES
###################################################################

# MySQL — the default database
SERVICE_MYSQL_ENABLE=false
SERVICE_MYSQL_VERSION=8.0

SERVICE_REDIS_ENABLE=true
";

    fn patch_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn changes_only_the_targeted_line() {
        let out = patch_text(SAMPLE, &patch_of(&[("SERVICE_MYSQL_ENABLE", "true")]));

        assert!(out.contains("SERVICE_MYSQL_ENABLE=true"));
        // Banners, comments, blank lines and neighbouring keys all survive.
        assert!(out.contains("# SERVICES"));
        assert!(out.contains("# MySQL — the default database"));
        assert!(out.contains("SERVICE_MYSQL_VERSION=8.0"));
        assert!(out.contains("SERVICE_REDIS_ENABLE=true"));
        assert_eq!(out.lines().count(), SAMPLE.lines().count());
    }

    #[test]
    fn an_empty_patch_is_byte_identical() {
        assert_eq!(patch_text(SAMPLE, &BTreeMap::new()), SAMPLE);
    }

    #[test]
    fn trailing_comments_survive_a_value_change() {
        let input = "SERVICE_KAFKA_ENABLE=false # needs 2GB RAM\n";
        let out = patch_text(input, &patch_of(&[("SERVICE_KAFKA_ENABLE", "true")]));
        assert_eq!(out.trim_end(), "SERVICE_KAFKA_ENABLE=true # needs 2GB RAM");
    }

    #[test]
    fn unknown_keys_are_appended_in_a_marked_block() {
        let out = patch_text(SAMPLE, &patch_of(&[("BRAND_NEW_KEY", "1")]));
        assert!(out.contains("# >>> added by StackVo Desktop >>>"));
        assert!(out.contains("BRAND_NEW_KEY=1"));
        // Existing content is untouched.
        assert!(out.contains("SERVICE_MYSQL_ENABLE=false"));
    }

    #[test]
    fn a_commented_out_key_is_not_treated_as_a_definition() {
        let input = "# SERVICE_MYSQL_ENABLE=true\nSERVICE_MYSQL_ENABLE=false\n";
        let out = patch_text(input, &patch_of(&[("SERVICE_MYSQL_ENABLE", "true")]));
        // The comment stays a comment; only the real line changes.
        assert!(out.starts_with("# SERVICE_MYSQL_ENABLE=true\n"));
        assert!(out.contains("\nSERVICE_MYSQL_ENABLE=true"));
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let input = "A=1\r\nB=2\r\n";
        let out = patch_text(input, &patch_of(&[("A", "9")]));
        assert!(
            out.contains("A=9\r\n"),
            "expected CRLF preserved, got {out:?}"
        );
        assert!(!out.contains("A=9\n\r"), "line endings were mangled");
    }

    #[test]
    fn values_containing_equals_are_replaced_whole() {
        let input = "DATABASE_URL=postgres://u:p@h:5432/db?a=1\n";
        let out = patch_text(input, &patch_of(&[("DATABASE_URL", "mysql://x")]));
        assert_eq!(out.trim_end(), "DATABASE_URL=mysql://x");
    }

    #[test]
    fn rejects_keys_and_values_that_would_corrupt_the_format() {
        assert!(validate(&patch_of(&[("lower_case", "1")])).is_err());
        assert!(validate(&patch_of(&[("HAS-DASH", "1")])).is_err());
        assert!(validate(&patch_of(&[("9LEADING", "1")])).is_err());
        assert!(validate(&patch_of(&[("OK_KEY", "line1\nline2")])).is_err());
        assert!(validate(&patch_of(&[("OK_KEY", "fine")])).is_ok());
    }

    /// The on-disk path — everything above tests `patch_text` in memory, but
    /// `apply` is where the backup, the atomic replace and the real file meet.
    /// Two services enabled at the same moment must both survive.
    ///
    /// This is the failure the command-layer locks cannot catch: `service_enable`
    /// holds `service:redis` and `service:memcached` respectively, so neither
    /// blocks the other, yet both read the same .env and write back a different
    /// descendant of it. Without the lock in `apply`, one of the two keys is
    /// simply gone afterwards.
    #[test]
    fn concurrent_patches_do_not_lose_each_other() {
        let dir = std::env::temp_dir().join("stackvo-envw-lost-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".env"),
            "SERVICE_REDIS_ENABLE=false\nSERVICE_MEMCACHED_ENABLE=false\nSERVICE_MONGO_ENABLE=false\n",
        )
        .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let handles: Vec<_> = ["REDIS", "MEMCACHED", "MONGO"]
            .into_iter()
            .map(|service| {
                let dir = dir.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    apply(
                        &dir,
                        &patch_of(&[(&format!("SERVICE_{service}_ENABLE"), "true")]),
                    )
                    .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let after = std::fs::read_to_string(dir.join(".env")).unwrap();
        for service in ["REDIS", "MEMCACHED", "MONGO"] {
            assert!(
                after.contains(&format!("SERVICE_{service}_ENABLE=true")),
                "{service} was lost:\n{after}"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_backs_up_the_original_and_replaces_the_file_whole() {
        let dir = std::env::temp_dir().join("stackvo-envw-apply");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = "# header\nSERVICE_REDIS_ENABLE=false\nDEFAULT_TLD_SUFFIX=stackvo.loc\n";
        std::fs::write(dir.join(".env"), original).unwrap();

        apply(&dir, &patch_of(&[("SERVICE_REDIS_ENABLE", "true")])).unwrap();

        let after = std::fs::read_to_string(dir.join(".env")).unwrap();
        assert!(after.contains("SERVICE_REDIS_ENABLE=true"));
        assert!(after.contains("# header"), "comments must survive");
        assert!(after.contains("DEFAULT_TLD_SUFFIX=stackvo.loc"));

        // The backup is the file as it was, so a bad patch is recoverable.
        assert_eq!(
            std::fs::read_to_string(dir.join(".env.bak")).unwrap(),
            original
        );

        // The staging file must not survive next to the user's .env.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files remained: {leftovers:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A second patch must not overwrite the backup — the first one is the
    /// state the user actually had before this app touched anything.
    #[test]
    fn the_backup_is_written_once_not_on_every_patch() {
        let dir = std::env::temp_dir().join("stackvo-envw-backup-once");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pristine = "SERVICE_REDIS_ENABLE=false\n";
        std::fs::write(dir.join(".env"), pristine).unwrap();

        apply(&dir, &patch_of(&[("SERVICE_REDIS_ENABLE", "true")])).unwrap();
        apply(&dir, &patch_of(&[("SERVICE_REDIS_ENABLE", "false")])).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join(".env.bak")).unwrap(),
            pristine,
            "the backup must still be the pre-StackVo state"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn service_enable_key_uses_the_contract_transform() {
        // mongo-express -> SERVICE_MONGO_EXPRESS_ENABLE (C-09 direction).
        let key = format!(
            "{}ENABLE",
            crate::config::Env::service_prefix("mongo-express")
        );
        assert_eq!(key, "SERVICE_MONGO_EXPRESS_ENABLE");
    }
}
