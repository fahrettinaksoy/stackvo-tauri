//! The four rules the keystore layer would be dangerous without.
//!
//! Every one of them is invisible on the machine somebody would break it on: a
//! developer with an unlocked keychain and no `stackvo.sh` in their workflow
//! sees a feature that works, whichever way these are written.
//!
//!   1. `apply_verbatim` has **one** caller. A second one puts a password back
//!      in `.env` on somebody pressing Save.
//!   2. The ordinary write path consults the current line, so a moved key stays
//!      moved.
//!   3. The generator refuses on an unresolved reference, rather than rendering
//!      the template default and starting a database on a password nobody chose.
//!   4. The documentation keeps saying the value is still in `generated/`.
//!      A keystore feature is read as "the secret is off the disk"; this one is
//!      not, and the sentence saying so is the difference between a partial win
//!      and a false one.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn read_up(relative: &str) -> String {
    let path = repo_root().join("..").join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// A source file with its own test modules cut out.
///
/// The lesson from `policy_claims.rs`: a gate satisfied by the fixtures in the
/// file it is checking is satisfied by nothing.
///
/// Cutting at the *first* `#[cfg(test)]` was the obvious version and it is
/// wrong here — `commands.rs` has nine of them, interleaved with production
/// code, and the first sits two thousand lines above everything this file
/// checks. That version reported `render_generated` as missing and the caller
/// count as one, both of which were the scanner's failure and not the code's.
/// So the test modules are removed where they occur and the rest is joined.
fn production(relative: &str, first_line: &str) -> String {
    let source = read(relative);
    let start = source
        .find(first_line)
        .unwrap_or_else(|| panic!("{relative} no longer starts with `{first_line}`"));

    let mut out = String::with_capacity(source.len());
    let mut depth = 0usize;
    let mut in_tests = false;

    for line in source[start..].lines() {
        if !in_tests && line.trim_start().starts_with("#[cfg(test)]") {
            in_tests = true;
            depth = 0;
            continue;
        }
        if in_tests {
            depth += line.matches('{').count();
            let closes = line.matches('}').count();
            // The module's own opening brace is on the `mod tests {` line, so
            // depth returns to zero exactly at its closing one.
            if depth > 0 && depth <= closes {
                in_tests = false;
            }
            depth = depth.saturating_sub(closes);
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The escape hatch is an escape hatch, not a second front door.
#[test]
fn only_secret_restore_writes_a_password_back_to_the_file() {
    let sources = [
        ("commands.rs", production("src/commands.rs", "use crate::")),
        (
            "env_writer.rs",
            production("src/env_writer.rs", "use crate::error::"),
        ),
    ];

    let callers: usize = sources
        .iter()
        .map(|(_, text)| text.matches("apply_verbatim(").count())
        .sum();

    // One call site in `commands.rs`, one definition in `env_writer.rs`.
    assert_eq!(
        callers, 2,
        "`apply_verbatim` must have exactly one caller — every other write path \
         has to go through `apply`, or a save from any Settings pane silently \
         puts a moved password back into .env"
    );

    let commands = &sources[0].1;
    let before = commands
        .split("apply_verbatim(")
        .next()
        .expect("the call exists");
    assert!(
        before
            .rfind("pub fn secret_restore")
            .is_some_and(|restore| {
                before[restore..]
                    .find("pub fn ")
                    .map(|n| n == 0)
                    .unwrap_or(true)
            }),
        "the one caller must be `secret_restore`; undoing the redirection is \
         the only thing it is for"
    );
}

/// The rule that survives a Settings save.
#[test]
fn the_ordinary_write_path_looks_at_the_line_that_is_there() {
    let writer = production("src/env_writer.rs", "use crate::error::");

    assert!(
        writer.contains("redirect_moved_keys(&original, patch)"),
        "`apply` no longer asks whether the key is already in the keystore, so \
         the next save that touches it writes the password back into the file"
    );
    assert!(
        writer.contains("crate::config::Env::parse(original)"),
        "the check has to read the .env *text*: a loaded Env has already \
         replaced the reference with the value behind it, and by then there is \
         nothing left to tell a moved key from one that never was"
    );
}

/// The refusal that stops a locked keychain from becoming a default password.
#[test]
fn the_generator_refuses_rather_than_rendering_a_hole() {
    let commands = production("src/commands.rs", "use crate::");

    let Some(render) = commands.find("pub fn render_generated") else {
        panic!("render_generated moved; this gate is pointing at nothing");
    };
    let body = &commands[render..];

    let check = body
        .find("unresolved_secrets()")
        .expect("render_generated must ask whether every reference resolved");
    let renders = body
        .find("files.push")
        .expect("render_generated pushes files");

    assert!(
        check < renders,
        "the check has to come before anything is rendered — a key the keystore \
         would not produce is *absent*, so `{{{{ SERVICE_MYSQL_ROOT_PASSWORD | \
         default('root') }}}}` renders `root` and a container comes up on a \
         password the user does not know is in force"
    );
}

/// Every place a reader could form the wrong impression.
#[test]
fn the_documentation_says_the_value_is_still_in_the_generated_file() {
    let places = [
        ("secrets.rs", read("src/secrets.rs")),
        (
            "ADR 0010",
            read_up("docs/adr/0010-secrets-move-out-of-env-not-off-the-disk.md"),
        ),
        ("contracts/ipc.json", read_up("contracts/ipc.json")),
        (
            "SecretsPane.vue",
            read_up("src/components/settings/SecretsPane.vue"),
        ),
        ("PRIVACY.md", read_up("PRIVACY.md")),
        ("en.js", read_up("src/i18n/locales/en.js")),
    ];

    for (name, text) in places {
        assert!(
            text.contains("docker-compose.dynamic.yml"),
            "{name} does not say the password is still rendered into \
             generated/docker-compose.dynamic.yml. A keystore feature is read as \
             meaning the secret left the disk; this one does not, and the \
             sentence that says so is the difference between a partial win and a \
             false claim"
        );
    }
}

/// One definition of "this is a credential", not two.
#[test]
fn the_movable_set_is_the_redacted_set() {
    let secrets = production("src/secrets.rs", "use std::collections");

    assert!(
        secrets.contains("crate::config::Env::is_secret(key)"),
        "`is_movable` must defer to the redaction rule. A second list is how a \
         key comes to be starred out on screen and stored in the clear, or the \
         reverse"
    );
    assert!(
        !secrets.contains("PASSWORD\""),
        "and it must not restate the suffixes here"
    );
}
