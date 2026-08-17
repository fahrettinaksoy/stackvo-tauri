//! The hosts file, planned and actually written — on whichever platform is
//! running this.
//!
//! §3 #35 said the privilege paths never ran on Windows or Linux. CI has run
//! `cargo test` on three operating systems for a long time, so the *pure* half
//! of `hosts.rs` — the parser, the marker block, `plan_text` — has been covered
//! everywhere all along. What had never run anywhere is the half that touches a
//! file: `apply`. There was no way to run it, because it wrote to `/etc/hosts`
//! and asked for a password to do it, and neither of those belongs in a test.
//!
//! Two changes made this possible and both are in `hosts.rs`:
//!
//! * `hosts_path()` honours `STACKVO_HOSTS_PATH`, the same seam `STACKVO_ROOT`
//!   already is;
//! * `apply` writes the file directly when it already may, and only elevates
//!   when it may not — which is a fix in its own right, because the app used to
//!   raise a password prompt on machines where the file was already writable.
//!
//! **An integration test rather than a unit test**, deliberately: the seam is an
//! environment variable, and a `#[cfg(test)]` module shares its process with
//! every other test in the crate. One of them reading the hosts file in
//! parallel would read this one's temporary file instead. A separate binary has
//! its own environment and cannot do that to anybody.
//!
//! What this still does **not** cover is the elevation itself — pkexec, UAC,
//! osascript. Those need a human at a dialog, and the row in `docs/durum.md`
//! says so rather than claiming the whole item.

use stackvo_desktop_lib::hosts;
use std::path::PathBuf;

/// A hosts file as a machine ships one, with entries the app must not disturb.
const EXISTING: &str = "\
127.0.0.1\tlocalhost
255.255.255.255\tbroadcasthost
::1\t\tlocalhost
# a comment somebody left
10.0.0.5\tinternal.example
";

fn staged(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("stackvo-hosts-test-{name}-{}", std::process::id()));
    std::fs::write(&path, EXISTING).expect("the fixture is writable");
    // SAFETY: this binary runs one test and sets this before touching `hosts`.
    unsafe { std::env::set_var("STACKVO_HOSTS_PATH", &path) };
    path
}

#[test]
fn a_domain_is_added_read_back_and_removed_without_disturbing_the_rest() {
    let path = staged("roundtrip");

    // ---- add ------------------------------------------------------------
    let plan = hosts::apply(&["shop.loc".into(), "blog.loc".into()], &[]).expect("apply");
    assert!(plan.changed, "adding two domains is a change");

    let written = std::fs::read_to_string(&path).expect("the file is still there");
    assert!(written.contains("shop.loc"), "{written}");
    assert!(written.contains("blog.loc"), "{written}");

    // The lines that were there before are still there, in order. This is the
    // assertion that matters: the app replaces the whole file, so "it added our
    // domain" and "it kept everybody else's" are two different claims.
    for line in EXISTING.lines() {
        assert!(written.contains(line), "{line:?} was lost:\n{written}");
    }

    // ---- what the app now reports ---------------------------------------
    let seen = hosts::status_for(&["shop.loc".into(), "unmapped.loc".into()]);
    let shop = seen
        .iter()
        .find(|e| e.domain == "shop.loc")
        .expect("shop is listed");
    assert!(
        shop.configured,
        "the domain this app just wrote reads back as configured"
    );
    assert!(
        shop.managed_by_stackvo,
        "and as one of ours, so removing it is offered"
    );
    let other = seen
        .iter()
        .find(|e| e.domain == "unmapped.loc")
        .expect("listed");
    assert!(!other.configured, "a domain nobody wrote is not configured");

    // ---- idempotent ------------------------------------------------------
    let again = hosts::apply(&["shop.loc".into(), "blog.loc".into()], &[]).expect("apply");
    assert!(
        !again.changed,
        "applying the same domains twice is not a second change — a plan that \
         always reports a change is one the UI can never call clean"
    );

    // ---- remove ----------------------------------------------------------
    hosts::apply(&[], &["shop.loc".into()]).expect("apply");
    let after = std::fs::read_to_string(&path).expect("read");
    assert!(!after.contains("shop.loc"), "{after}");
    assert!(after.contains("blog.loc"), "the other one stays: {after}");
    for line in EXISTING.lines() {
        assert!(
            after.contains(line),
            "{line:?} was lost on removal:\n{after}"
        );
    }

    // ---- and the file is left as a hosts file, not as a fragment ---------
    hosts::apply(&[], &["blog.loc".into()]).expect("apply");
    let empty = std::fs::read_to_string(&path).expect("read");
    for line in EXISTING.lines() {
        assert!(
            empty.contains(line),
            "{line:?} was lost when ours all went:\n{empty}"
        );
    }

    let _ = std::fs::remove_file(&path);
}
