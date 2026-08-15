//! Reading the system hosts file.
//!
//! The web UI could only ever read this, and only through a read-only bind
//! mount at `/host/etc/hosts` — it could tell you a domain was missing but not
//! do anything about it, so the README's `sudo tee -a /etc/hosts` step stayed
//! manual. Phase 1 keeps parity (read-only); the elevated write lands in Phase 3
//! as `hosts_apply`, which is why this module already models the marker block.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Lines between these markers are ours to rewrite. Anything outside is the
/// user's and must survive untouched.
pub const BLOCK_START: &str = "# >>> stackvo >>>";
pub const BLOCK_END: &str = "# <<< stackvo <<<";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsEntry {
    pub ip: String,
    pub domain: String,
    pub configured: bool,
    pub managed_by_stackvo: bool,
}

pub fn hosts_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

fn read_raw() -> Option<String> {
    std::fs::read_to_string(hosts_path()).ok()
}

/// Every domain currently mapped, plus whether it sits inside our marker block.
///
/// A hosts line is `<ip> <name> [name…]`, comments start with `#`. Multiple
/// names per line are common (`127.0.0.1 localhost broadcasthost`) and the old
/// substring check in `isDomainConfigured` got those wrong — it looked for the
/// domain anywhere in the file, so `foo.loc` matched a commented-out
/// `#foo.loc` line and reported it configured.
pub fn mapped_domains() -> (HashSet<String>, HashSet<String>) {
    let mut all = HashSet::new();
    let mut managed = HashSet::new();

    let Some(text) = read_raw() else {
        return (all, managed);
    };

    let mut inside_block = false;
    for raw in text.lines() {
        let line = raw.trim();

        if line == BLOCK_START {
            inside_block = true;
            continue;
        }
        if line == BLOCK_END {
            inside_block = false;
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Strip a trailing comment before splitting into fields.
        let content = line.split('#').next().unwrap_or("").trim();
        let mut fields = content.split_whitespace();
        let Some(_ip) = fields.next() else { continue };

        for name in fields {
            all.insert(name.to_ascii_lowercase());
            if inside_block {
                managed.insert(name.to_ascii_lowercase());
            }
        }
    }

    (all, managed)
}

/// Resolve status for a set of domains in one pass.
pub fn status_for(domains: &[String]) -> Vec<HostsEntry> {
    let (all, managed) = mapped_domains();

    domains
        .iter()
        .map(|d| {
            let key = d.to_ascii_lowercase();
            HostsEntry {
                ip: "127.0.0.1".into(),
                configured: all.contains(&key),
                managed_by_stackvo: managed.contains(&key),
                domain: d.clone(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- writing

/// What a proposed change would do, for the user to approve before elevating.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsPlan {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    /// The exact text that would replace the file. Shown as a diff, and it is
    /// what actually gets written — no second computation that could differ.
    pub preview: String,
    /// The file as it stands, so the UI can diff against it rather than guess
    /// which lines are new.
    pub current: String,
    pub changed: bool,
    pub path: String,
}

/// Rewrite only the StackVo-managed block, leaving everything else byte-exact.
///
/// The user's hosts file routinely carries work entries, VPN overrides and
/// hand-written comments. Rewriting the whole file from a domain list would be
/// simple and would also, eventually, cost somebody an afternoon.
/// Is this a hostname we are willing to write into `/etc/hosts`?
///
/// The pattern is the one `contracts/project.schema.json` already specifies for
/// `domain`: labels of alphanumerics and hyphens, not starting or ending with a
/// hyphen, at least two of them, 3–253 characters. The contract knew; this code
/// was not asking.
///
/// Why it matters more here than anywhere else: `plan_text` interpolates the
/// value into a line of the hosts file, and `apply` then writes that file with
/// administrator rights. A domain containing a newline does not corrupt the
/// file — it adds a *valid extra entry*, so `evil.loc\n127.0.0.1\tgithub.com`
/// silently points github.com at localhost. Staging the text in a temp file and
/// copying it under elevation defends against shell injection, which is a
/// different problem from content injection.
pub fn is_valid_domain(domain: &str) -> bool {
    if domain.len() < 3 || domain.len() > 253 {
        return false;
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }

    labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// A hostname, or a wildcard covering exactly one label beneath it.
///
/// One definition, because there were nearly two: the manifest decides whether
/// an alias may be written, the certificate module decides whether it may be a
/// SAN, and the two agreeing by coincidence is how `*.shop.loc` comes to be
/// accepted in a file and dropped on the way to mkcert.
///
/// RFC 6125 puts the star in the leftmost label and nowhere else, which is also
/// exactly what [`crate::certs::san_covers`] matches on and what mkcert issues.
/// `*.*.shop.loc` and `api.*.shop.loc` are not wildcards, they are hostnames
/// with an asterisk in them.
pub fn is_valid_wildcard_or_domain(value: &str) -> bool {
    match value.strip_prefix("*.") {
        Some(rest) => is_valid_domain(rest),
        None => is_valid_domain(value),
    }
}

/// Reject the whole request if any domain is malformed.
///
/// Not "filter out the bad ones": a caller that asked for four domains and got
/// three has been silently given something it did not ask for, and that is the
/// failure mode this project keeps finding in the shell implementation.
fn check_domains(domains: &[String]) -> crate::error::Result<()> {
    use crate::error::{Code, Error};

    for domain in domains {
        if !is_valid_domain(domain) {
            return Err(Error::new(
                Code::InvalidInput,
                format!("{domain:?} is not a valid hostname"),
            )
            .with_hint(crate::hints::HOSTNAME_CHARSET));
        }
    }
    Ok(())
}

pub fn plan_text(original: &str, add: &[String], remove: &[String]) -> String {
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let mut before: Vec<&str> = Vec::new();
    let mut managed: Vec<String> = Vec::new();
    let mut after: Vec<&str> = Vec::new();

    let mut section = 0; // 0 = before block, 1 = inside, 2 = after
    for raw in original.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim();

        if trimmed == BLOCK_START {
            section = 1;
            continue;
        }
        if trimmed == BLOCK_END {
            section = 2;
            continue;
        }

        match section {
            0 => before.push(line),
            1 => managed.push(line.to_string()),
            _ => after.push(line),
        }
    }

    // Domains currently in our block.
    let mut domains: Vec<String> = managed
        .iter()
        .filter_map(|line| {
            let content = line.split('#').next()?.trim();
            let mut fields = content.split_whitespace();
            fields.next()?; // ip
            fields.next().map(|d| d.to_ascii_lowercase())
        })
        .collect();

    let removing: Vec<String> = remove.iter().map(|d| d.to_ascii_lowercase()).collect();
    domains.retain(|d| !removing.contains(d));

    for domain in add {
        let key = domain.to_ascii_lowercase();
        if !domains.contains(&key) {
            domains.push(key);
        }
    }
    domains.sort();
    domains.dedup();

    let mut out: Vec<String> = before.iter().map(|s| s.to_string()).collect();

    // Drop trailing blanks so repeated edits do not accumulate empty lines.
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }

    if !domains.is_empty() {
        out.push(String::new());
        out.push(BLOCK_START.to_string());
        out.push("# Managed by StackVo Desktop. Edits inside this block are overwritten.".into());
        for domain in &domains {
            out.push(format!("127.0.0.1\t{domain}"));
        }
        out.push(BLOCK_END.to_string());
    }

    for line in after {
        out.push(line.to_string());
    }

    let mut text = out.join(newline);
    if !text.ends_with(newline) {
        text.push_str(newline);
    }
    text
}

/// Build the plan without touching anything.
pub fn plan(add: &[String], remove: &[String]) -> crate::error::Result<HostsPlan> {
    check_domains(add)?;
    check_domains(remove)?;

    let path = hosts_path();
    let original = read_raw().unwrap_or_default();
    let preview = plan_text(&original, add, remove);

    Ok(HostsPlan {
        changed: preview != original,
        add: add.to_vec(),
        remove: remove.to_vec(),
        preview,
        current: original,
        path: path.display().to_string(),
    })
}

/// Apply the plan, prompting for administrator rights.
///
/// This is the one place the app asks for elevation, and it does exactly one
/// thing: replace `/etc/hosts` with text the user has already seen. It replaces
/// the README's `echo "127.0.0.1 x.loc" | sudo tee -a /etc/hosts` step.
pub fn apply(add: &[String], remove: &[String]) -> crate::error::Result<HostsPlan> {
    use crate::error::{Code, Error};

    let plan = plan(add, remove)?;
    if !plan.changed {
        return Ok(plan);
    }

    let path = hosts_path();

    // Write the new contents to a temp file first, so the elevated step is a
    // plain copy rather than a shell heredoc carrying user-supplied domains.
    let staged = std::env::temp_dir().join("stackvo-hosts-staged");
    std::fs::write(&staged, &plan.preview).map_err(|e| Error::io("staging the hosts file", e))?;

    let backup = std::env::temp_dir().join("stackvo-hosts-backup");
    if let Ok(original) = std::fs::read_to_string(&path) {
        let _ = std::fs::write(&backup, original);
    }

    let ok = elevated_copy(&staged, &path)?;
    let _ = std::fs::remove_file(&staged);

    if !ok {
        return Err(
            Error::new(Code::PermissionDenied, "The hosts file was not updated.")
                .with_hint(crate::hints::HOSTS_NEEDS_ADMIN),
        );
    }

    Ok(plan)
}

/// Copy `from` over `to` with administrator rights.
///
/// The domains live inside the staged file's contents, not in this command line.
/// The paths still do, though — `from` is under `TMPDIR` and `to` comes from
/// `hosts_path()` — so they are passed as separate arguments rather than
/// hand-quoted into one string. `elevate::run` explains why that stopped being
/// the caller's job.
#[cfg(target_os = "macos")]
fn elevated_copy(from: &Path, to: &Path) -> crate::error::Result<bool> {
    // The elevation itself lives in `elevate`, which is the one place in this
    // app that asks for a password — and the place where letting a child
    // process ask for one instead hung the whole app.
    crate::elevate::run(&[
        "/bin/cp",
        &from.display().to_string(),
        &to.display().to_string(),
    ])
    .map_err(|e| e.with_hint(crate::hints::HOSTS_NOT_REPLACED))
}

/// The polkit half used to be written out here, exit codes and all, and there
/// is now a second caller — `dns::install` writes a resolver drop-in the same
/// way. Two copies of "126 means the dialog was dismissed" is one copy too
/// many, so the pkexec call moved to `elevate::run` beside the macOS one and
/// this is the same three facts it always was.
#[cfg(target_os = "linux")]
fn elevated_copy(from: &Path, to: &Path) -> crate::error::Result<bool> {
    crate::elevate::run(&["cp", &from.display().to_string(), &to.display().to_string()])
        .map_err(|e| e.with_hint(crate::hints::HOSTS_NOT_REPLACED))
}

#[cfg(target_os = "windows")]
fn elevated_copy(from: &Path, to: &Path) -> crate::error::Result<bool> {
    use crate::error::{Code, Error};

    // PowerShell's -Verb RunAs raises the UAC prompt.
    let command = format!(
        "Start-Process -FilePath cmd -ArgumentList '/c copy /Y \"{}\" \"{}\"' -Verb RunAs -Wait",
        from.display(),
        to.display()
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &command])
        .output()
        .map_err(|e| Error::io("running powershell", e))?;

    if output.status.success() {
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("canceled") || stderr.contains("cancelled") {
            Ok(false)
        } else {
            Err(Error::new(Code::PermissionDenied, stderr.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {

    /// The bug this guards, verified before it was fixed: a domain carrying a
    /// newline does not corrupt the hosts file — it appends a second, perfectly
    /// valid entry. `apply` then writes that file with administrator rights, so
    /// `shop.loc\n127.0.0.1\tgithub.com` quietly points github.com at
    /// localhost.
    #[test]
    fn a_newline_in_a_domain_cannot_inject_a_hosts_entry() {
        let evil = "shop.loc\n127.0.0.1\tgithub.com".to_string();

        // It really would have been written, had planning not refused.
        let injected = plan_text("127.0.0.1\tlocalhost\n", std::slice::from_ref(&evil), &[]);
        assert!(
            injected.contains("github.com"),
            "the injection is real, so the guard has something to guard"
        );

        assert!(plan(std::slice::from_ref(&evil), &[]).is_err());
        assert!(plan(&[], &[evil]).is_err());
    }

    #[test]
    fn hostnames_follow_the_contract_pattern() {
        for ok in ["shop.loc", "api.oxoeashop.test", "a-b.c-d.loc", "x1.y2"] {
            assert!(is_valid_domain(ok), "{ok} should be accepted");
        }
        for bad in [
            "",                    // nothing
            "loc",                 // a single label is not a domain here
            "shop..loc",           // empty label
            "-shop.loc",           // leading hyphen
            "shop-.loc",           // trailing hyphen
            "shop.loc extra",      // whitespace — a second field on the line
            "shop.loc\t127.0.0.1", // a tab does the same
            "shop.loc\n127.0.0.1", // the injection primitive
            "shop.loc#comment",    // would comment out what follows
            "üzüm.loc",            // non-ASCII; punycode is the caller's job
        ] {
            assert!(!is_valid_domain(bad), "{bad:?} should be rejected");
        }
        assert!(
            !is_valid_domain(&format!("{}.loc", "a".repeat(64))),
            "label cap"
        );
        assert!(!is_valid_domain(&"a.".repeat(200)), "length cap");
    }

    /// A request with one bad entry fails whole, rather than quietly applying
    /// the rest — the caller would have no way to learn what was dropped.
    #[test]
    fn one_bad_domain_rejects_the_whole_request() {
        let result = plan(&["good.loc".into(), "bad domain".into()], &[]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, crate::error::Code::InvalidInput);
    }

    use super::*;

    /// Same parsing rules, against an in-memory file, so the assertions do not
    /// depend on the developer's real /etc/hosts.
    fn parse(text: &str) -> (HashSet<String>, HashSet<String>) {
        let mut all = HashSet::new();
        let mut managed = HashSet::new();
        let mut inside = false;

        for raw in text.lines() {
            let line = raw.trim();
            if line == BLOCK_START {
                inside = true;
                continue;
            }
            if line == BLOCK_END {
                inside = false;
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let content = line.split('#').next().unwrap_or("").trim();
            let mut fields = content.split_whitespace();
            if fields.next().is_none() {
                continue;
            }
            for name in fields {
                all.insert(name.to_ascii_lowercase());
                if inside {
                    managed.insert(name.to_ascii_lowercase());
                }
            }
        }
        (all, managed)
    }

    const SAMPLE: &str = r#"
127.0.0.1	localhost broadcasthost
# 127.0.0.1  commented-out.loc
127.0.0.1  live.loc   # trailing comment
# >>> stackvo >>>
127.0.0.1  managed.loc
# <<< stackvo <<<
"#;

    #[test]
    fn a_commented_line_is_not_a_mapping() {
        // The old substring check reported this as configured.
        let (all, _) = parse(SAMPLE);
        assert!(!all.contains("commented-out.loc"));
        assert!(all.contains("live.loc"));
    }

    #[test]
    fn multiple_names_on_one_line_all_count() {
        let (all, _) = parse(SAMPLE);
        assert!(all.contains("localhost"));
        assert!(all.contains("broadcasthost"));
    }

    #[test]
    fn trailing_comments_are_stripped() {
        let (all, _) = parse(SAMPLE);
        assert!(!all.contains("#"));
        assert!(!all.iter().any(|d| d.contains("trailing")));
    }

    #[test]
    fn marker_block_membership_is_tracked() {
        let (all, managed) = parse(SAMPLE);
        assert!(managed.contains("managed.loc"));
        assert!(!managed.contains("live.loc"));
        assert!(all.contains("managed.loc"));
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    const USER_FILE: &str = "\
##
# Host Database
##
127.0.0.1\tlocalhost
255.255.255.255\tbroadcasthost
10.0.0.5\tinternal.corp   # work VPN
";

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn adds_a_managed_block_without_touching_user_lines() {
        let out = plan_text(USER_FILE, &s(&["shop.loc"]), &[]);

        assert!(out.contains("127.0.0.1\tshop.loc"));
        assert!(out.contains(BLOCK_START) && out.contains(BLOCK_END));
        // Every original line survives, including the commented VPN note.
        for line in [
            "# Host Database",
            "127.0.0.1\tlocalhost",
            "10.0.0.5\tinternal.corp   # work VPN",
        ] {
            assert!(out.contains(line), "lost: {line}");
        }
    }

    #[test]
    fn removing_a_domain_leaves_the_others() {
        let with_two = plan_text(USER_FILE, &s(&["a.loc", "b.loc"]), &[]);
        let after = plan_text(&with_two, &[], &s(&["a.loc"]));

        assert!(!after.contains("a.loc"));
        assert!(after.contains("b.loc"));
        assert!(after.contains("127.0.0.1\tlocalhost"));
    }

    #[test]
    fn repeated_edits_do_not_accumulate_blank_lines_or_blocks() {
        let once = plan_text(USER_FILE, &s(&["a.loc"]), &[]);
        let twice = plan_text(&once, &s(&["a.loc"]), &[]);
        assert_eq!(once, twice, "applying the same plan twice must be a no-op");
        assert_eq!(twice.matches(BLOCK_START).count(), 1);
    }

    #[test]
    fn the_block_disappears_when_the_last_domain_is_removed() {
        let with_one = plan_text(USER_FILE, &s(&["only.loc"]), &[]);
        let emptied = plan_text(&with_one, &[], &s(&["only.loc"]));

        assert!(
            !emptied.contains(BLOCK_START),
            "an empty marker block is litter"
        );
        assert!(emptied.contains("127.0.0.1\tlocalhost"));
    }

    #[test]
    fn user_lines_after_the_block_are_preserved() {
        let seeded = plan_text(USER_FILE, &s(&["a.loc"]), &[]);
        let with_tail = format!("{seeded}192.168.1.9\tprinter.local\n");
        let out = plan_text(&with_tail, &s(&["b.loc"]), &[]);

        assert!(out.contains("192.168.1.9\tprinter.local"));
        assert!(out.contains("a.loc") && out.contains("b.loc"));
    }

    #[test]
    fn a_user_entry_outside_the_block_is_not_hijacked() {
        // manual.loc is the user's line; asking to remove it must not touch it,
        // because we only ever rewrite our own block.
        let manual = format!("{USER_FILE}127.0.0.1\tmanual.loc\n");
        let out = plan_text(&manual, &[], &s(&["manual.loc"]));
        assert!(out.contains("127.0.0.1\tmanual.loc"));
    }

    #[test]
    fn domains_are_normalised_and_deduplicated() {
        let out = plan_text(USER_FILE, &s(&["Shop.LOC", "shop.loc"]), &[]);
        assert_eq!(out.matches("shop.loc").count(), 1);
        assert!(!out.contains("Shop.LOC"));
    }
}
