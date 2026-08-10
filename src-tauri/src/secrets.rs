//! Database passwords in the OS keystore instead of in a text file.
//!
//! `.env` holds the stack's configuration and, today, its credentials. On a
//! company machine that file is backed up, synced to a laptop, and scanned by
//! whatever the organisation runs — which is the readiness review's §5.2, and
//! it is a fair complaint about a file whose whole purpose is to be copied
//! around.
//!
//! A key that has been moved reads like this:
//!
//! ```text
//! SERVICE_MYSQL_ROOT_PASSWORD=keychain:SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4
//! ```
//!
//! and the value lives in Keychain (macOS), Credential Manager (Windows) or the
//! Secret Service (Linux), under the service name `stackvo`.
//!
//! ## What this removes, and what it does not
//!
//! It takes the password out of `.env`. It does **not** take it off the disk,
//! and pretending otherwise would be the same mistake ADR 0009 refused to make
//! about the policy file.
//!
//! `generated/docker-compose.dynamic.yml` is rendered from
//! `{{ SERVICE_MYSQL_ROOT_PASSWORD }}`, so the real value is substituted into
//! it — as it always has been. The secret was never in one file; it was in two,
//! and §5.2 only counted the first. What changes is *which* file: `.env` is
//! hand-maintained, quoted in support threads, and the thing a backup tool is
//! pointed at, while `generated/` is output that ADR 0002 says is rewritten from
//! scratch on every run. Moving the value from the first to the second is a real
//! reduction and a partial one, and the honest thing is to say which.
//!
//! Getting it out of `generated/` as well means emitting `${VAR}` and feeding
//! the value to `docker compose` through its environment. That changes the
//! rendered bytes, which breaks the differential comparison against the Bash
//! generator, and it breaks `docker compose up` run by hand in that directory.
//! It is a v2 change and it is described in ADR 0010 rather than half-done here.
//!
//! ## The Bash CLI cannot read these, and that is the sharp edge
//!
//! `stackvo.sh` reads `.env` directly. Handed `keychain:…` it will use that
//! string as the password — so a fresh MySQL container comes up with a root
//! password nobody meant, and nothing announces it. This is why moving a key is
//! an explicit act with a warning attached and never something the app does on
//! its own, and why `doctor` reports a workspace that has both.
//!
//! ## The reference is data, not a derivation
//!
//! The entry name is written into `.env` and read back from there. It is
//! *generated* from the key and a digest of the workspace path — so two
//! workspaces do not fight over one Keychain entry — but it is never
//! *recomputed* to look a value up. Moving a workspace directory therefore does
//! not orphan its secrets, which recomputing would do silently and only for
//! people who move directories.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{Code, Error, Result};

/// The prefix that makes a `.env` value a reference rather than a password.
///
/// `keychain:` on every platform, including the two where the store is not
/// called that. The alternative — `credman:` on Windows, `secretservice:` on
/// Linux — would make a `.env` non-portable between two machines belonging to
/// the same person, for a word.
pub const SCHEME: &str = "keychain:";

/// The service name every entry is filed under, so they are findable together
/// in Keychain Access and its equivalents.
pub const SERVICE: &str = "stackvo";

/// Is this `.env` value a reference to the keystore?
pub fn is_reference(value: &str) -> bool {
    entry_of(value).is_some()
}

/// The entry name inside a reference, or `None` if this is a plain value.
pub fn entry_of(value: &str) -> Option<&str> {
    let entry = value.trim().strip_prefix(SCHEME)?;
    (!entry.is_empty()).then_some(entry)
}

/// Which keys may be moved.
///
/// Exactly the set [`crate::config::Env::is_secret`] already redacts, and
/// deliberately not a second list. Two lists of "what counts as a secret" is
/// how a key comes to be starred out on screen and stored in the clear, or the
/// reverse.
pub fn is_movable(key: &str) -> bool {
    crate::config::Env::is_secret(key)
}

/// The entry name for a key in a given workspace.
///
/// `SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4` — the key first because that is what
/// somebody scrolling through Keychain Access is looking for, and the digest
/// after it because two workspaces on one machine must not share one entry.
pub fn new_entry(key: &str, root: &Path) -> String {
    format!("{key}@{:08x}", digest(&root.display().to_string()))
}

/// A full `.env` value for a freshly moved key.
pub fn reference_for(key: &str, root: &Path) -> String {
    format!("{SCHEME}{}", new_entry(key, root))
}

/// FNV-1a, 32-bit.
///
/// Written out rather than reached for, and not `DefaultHasher`: this value
/// lands in a file that outlives the process, and `DefaultHasher`'s output is
/// explicitly not guaranteed between Rust releases. Nothing would break — a
/// reference is stored, never recomputed — but a persisted identifier whose
/// definition is "whatever the standard library did that year" is not one to
/// write down. Six lines and it is fixed for ever.
fn digest(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

// ---------------------------------------------------------------- the store

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| unreachable_store(name, e))
}

/// The keystore said no in a way the user can do something about.
///
/// [`Code::PermissionDenied`] rather than [`Code::Forbidden`]: a locked
/// keychain, a Secret Service that is not running, a prompt somebody dismissed
/// — every one of them is answered by the user doing something and trying
/// again, which is exactly the distinction ADR 0009 drew between the two codes.
fn unreachable_store(name: &str, err: keyring::Error) -> Error {
    Error::new(
        Code::PermissionDenied,
        format!("the keystore did not answer for `{name}`: {err}"),
    )
    .with_hint(crate::hints::UNLOCK_THE_KEYSTORE)
}

/// Read one entry. `Ok(None)` means the store answered and has nothing.
///
/// The distinction is the whole reason this is not `Option`-flavoured
/// everywhere: "there is no such password" and "the keychain is locked" lead to
/// completely different things happening, and collapsing them is how a locked
/// keychain becomes an empty root password.
pub fn read(name: &str) -> Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(unreachable_store(name, e)),
    }
}

pub fn write(name: &str, value: &str) -> Result<()> {
    entry(name)?
        .set_password(value)
        .map_err(|e| unreachable_store(name, e))
}

/// Remove an entry, treating an absent one as success.
///
/// The caller is restoring a value to `.env` or dropping a key; either way the
/// end state it wants is "not in the keystore", and an entry that was already
/// gone is that state.
pub fn delete(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(unreachable_store(name, e)),
    }
}

/// Can this machine store a secret at all?
///
/// A read of a name nothing ever writes. `NoEntry` is the store answering
/// correctly and is the success case; anything else means there is no usable
/// store — a headless Linux box with no Secret Service, most likely. Probing by
/// reading rather than by writing so that asking the question leaves nothing
/// behind.
pub fn available() -> bool {
    matches!(read("stackvo-probe@availability"), Ok(None) | Ok(Some(_)))
}

// ------------------------------------------------------------- resolution

/// Replace every reference in `vars` with its stored value.
///
/// Returns the keys that could **not** be resolved, and removes them from the
/// map rather than leaving an empty string behind. That choice is the important
/// one in this file: `template::render` prefers a present-but-empty value over
/// the template's own `| default(…)`, so a value of `""` would render
/// `MYSQL_ROOT_PASSWORD: ""` — a database with no root password, silently,
/// because a keychain was locked. Absent at least falls back to the default,
/// and the returned list is what makes the generator refuse outright.
pub fn resolve(vars: &mut BTreeMap<String, String>) -> Vec<String> {
    let referenced: Vec<(String, String)> = vars
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), entry_of(value)?.to_string())))
        .collect();

    let mut unresolved = Vec::new();
    for (key, name) in referenced {
        match read(&name) {
            Ok(Some(value)) => {
                vars.insert(key, value);
            }
            // Both arms are failures, and deliberately not distinguished here:
            // an entry somebody deleted from Keychain Access and a store that
            // will not open leave the same hole in the same file. The
            // difference is in the message `read` already produced, and in
            // `secrets_status`, which reports it per key.
            Ok(None) | Err(_) => {
                vars.remove(&key);
                unresolved.push(key);
            }
        }
    }
    unresolved.sort();
    unresolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_recognised_and_a_plain_value_is_not() {
        assert_eq!(
            entry_of("keychain:SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4"),
            Some("SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4")
        );
        assert!(is_reference("  keychain:x  "), "a padded line still parses");

        for plain in ["hunter2", "", "keychain:", "keychain", "chain:x"] {
            assert!(
                !is_reference(plain),
                "`{plain}` is a password, and treating it as a reference would \
                 replace it with nothing"
            );
        }
    }

    /// Two workspaces on one machine must not share one Keychain entry.
    #[test]
    fn the_entry_name_is_scoped_to_the_workspace() {
        let a = new_entry(
            "SERVICE_MYSQL_ROOT_PASSWORD",
            Path::new("/Users/x/.stackvo"),
        );
        let b = new_entry("SERVICE_MYSQL_ROOT_PASSWORD", Path::new("/Users/x/work"));

        assert_ne!(a, b);
        assert!(
            a.starts_with("SERVICE_MYSQL_ROOT_PASSWORD@"),
            "the key comes first so the entry is findable by eye: {a}"
        );
    }

    #[test]
    fn the_same_workspace_and_key_always_name_the_same_entry() {
        let root = Path::new("/Users/x/.stackvo");
        assert_eq!(
            new_entry("SERVICE_MYSQL_ROOT_PASSWORD", root),
            new_entry("SERVICE_MYSQL_ROOT_PASSWORD", root)
        );
    }

    /// One list of what a secret is, not two.
    #[test]
    fn movable_is_exactly_what_the_ui_already_redacts() {
        for key in [
            "SERVICE_MYSQL_ROOT_PASSWORD",
            "SERVICE_GRAFANA_ADMIN_PASSWORD",
            "SOMETHING_TOKEN",
            "SERVICE_BLACKFIRE_SERVER_ID",
        ] {
            assert!(is_movable(key), "{key} is starred out on screen already");
        }
        for key in ["DEFAULT_TLD_SUFFIX", "SERVICE_MYSQL_VERSION"] {
            assert!(!is_movable(key));
        }
    }

    /// The FNV constants, against a value anybody can check.
    #[test]
    fn the_digest_is_the_published_fnv1a() {
        // The reference vector for FNV-1a 32-bit.
        assert_eq!(digest("a"), 0xe40c_292c);
        assert_eq!(digest(""), 0x811c_9dc5);
    }

    /// The choice that stops a locked keychain from becoming an empty password.
    ///
    /// `template::render` prefers a present-but-empty value over the template's
    /// own default, so leaving `""` behind here would render
    /// `MYSQL_ROOT_PASSWORD: ""` and start a database anybody can open.
    ///
    /// This is the one test here that touches the real store, and it asserts
    /// the same thing on a machine that has one and a machine that does not: a
    /// developer's Keychain answers "no entry" and a CI container has nothing
    /// listening at all, and both are holes of exactly the same shape.
    #[test]
    fn an_unresolvable_reference_leaves_no_key_rather_than_an_empty_one() {
        let mut vars = BTreeMap::from([
            (
                "SERVICE_MYSQL_ROOT_PASSWORD".to_string(),
                // A name nothing has ever written, so the store answers "no
                // entry" — the same hole a deleted Keychain item leaves.
                format!("{SCHEME}stackvo-test-absent@00000000"),
            ),
            ("SERVICE_MYSQL_VERSION".to_string(), "8.0".to_string()),
        ]);

        let unresolved = resolve(&mut vars);

        assert_eq!(unresolved, ["SERVICE_MYSQL_ROOT_PASSWORD"]);
        assert!(
            !vars.contains_key("SERVICE_MYSQL_ROOT_PASSWORD"),
            "an empty value would beat the template's `| default('root')` and \
             render a database with no password at all"
        );
        assert_eq!(
            vars.get("SERVICE_MYSQL_VERSION").map(String::as_str),
            Some("8.0"),
            "everything that is not a reference is left alone"
        );
    }

    #[test]
    fn a_map_with_no_references_is_untouched() {
        let original = BTreeMap::from([
            (
                "SERVICE_MYSQL_ROOT_PASSWORD".to_string(),
                "root".to_string(),
            ),
            ("DEFAULT_TLD_SUFFIX".to_string(), "stackvo.loc".to_string()),
        ]);
        let mut vars = original.clone();

        assert!(resolve(&mut vars).is_empty());
        assert_eq!(vars, original, "the ordinary workspace pays nothing");
    }
}
