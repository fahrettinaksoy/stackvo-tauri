//! Asking for administrator rights from a window.
//!
//! One thing in this app needs them — replacing `/etc/hosts` — and it is here
//! rather than beside its caller because of what the *other* candidate taught:
//! **a windowed app must never let a child process ask for a password.**
//!
//! `mkcert -install` does exactly that. It shells out to
//! `sudo --prompt=Sudo password: -- security add-trusted-cert …`, and `sudo`
//! reads the password from the terminal. A GUI app has no terminal, so the
//! prompt goes nowhere and the process waits — forever, with no output, no
//! error and nothing on screen. That is what it looked like:
//!
//! ```text
//! root  33845  sudo --prompt=Sudo password: -- security add-trusted-cert …
//!       33836  mkcert -install
//! ```
//!
//! The first-run screen sat on "Issuing the certificate" until it was killed.
//! A failure would have been fine; the app has a retry button and an error
//! area. Hanging is the one outcome nothing recovers from.
//!
//! So elevation happens here, through the mechanism the platform gives a
//! windowed app: `osascript`'s `with administrator privileges`, which puts up
//! the standard authentication panel. And every helper this app spawns gets its
//! stdin closed, so one that decides to prompt anyway fails instead of stopping.
//!
//! The certificate authority is not a caller. Root through an AppleScript was
//! tried and refused — `SecTrustSettingsSetTrustSettings: the authorization was
//! denied since no user interaction was possible` — because writing the admin
//! trust domain needs the Security framework's own confirmation, which it
//! cannot show from there. `certs::trust_ca` writes the user trust domain
//! instead and needs no elevation at all.

use crate::error::{Code, Error, Result};

/// Turn `argv` into one shell command, quoting every item.
///
/// A named handler rather than inline code because the test below runs this
/// exact text. A copy would drift, and the thing being tested is a quoting rule
/// — the class of thing that is only ever wrong in the copy nobody ran.
///
/// `quoted form of` is AppleScript's own POSIX-shell quoter: it wraps the value
/// in single quotes and rewrites an embedded quote as `'\''`. Whatever the
/// string holds — a space, a `;`, a `$(…)`, a backtick — comes out as one
/// literal argument.
#[cfg(target_os = "macos")]
const JOIN_ARGV: &str = r#"on join(argv)
    set cmd to ""
    repeat with i from 1 to (count of argv)
        if i > 1 then set cmd to cmd & " "
        set cmd to cmd & quoted form of (item i of argv)
    end repeat
    return cmd
end join"#;

/// Run a program as an administrator, one argument per element.
///
/// `Ok(false)` means the person dismissed the prompt, which is an answer rather
/// than a fault — nothing was changed and nothing needs reporting as broken.
///
/// ## Why this takes a vector and not a command line
///
/// It used to take a string and interpolate it:
///
/// ```text
/// format!(r#"do shell script "{command}" with administrator privileges"#)
/// ```
///
/// which made every caller responsible for its own escaping, and the only thing
/// enforcing that was this comment. The caller built paths out of the user's
/// home directory and `STACKVO_ROOT` — values the user controls and can put a
/// quote in — so the single defence against a path ending the AppleScript string
/// early was that nobody had tried. In a function whose entire job is to run
/// something as root.
///
/// Nothing is interpolated now. The script is a constant, the paths travel as
/// process arguments to `osascript` and arrive in `argv`, and [`JOIN_ARGV`]
/// quotes them on the other side. There is no string for a caller to break out
/// of, which is a stronger statement than "no caller does".
///
/// This is the same argv-only discipline `runner` and `quickcmd` already apply
/// to every unprivileged subprocess. It should always have applied hardest here.
#[cfg(target_os = "macos")]
pub fn run(argv: &[&str]) -> Result<bool> {
    if argv.is_empty() {
        return Err(Error::new(
            Code::InvalidInput,
            "an elevated command needs a program to run",
        ));
    }

    let script = format!(
        "{JOIN_ARGV}\n\
         on run argv\n\
         \x20   do shell script join(argv) with administrator privileges\n\
         end run"
    );

    // `--` first: an argument that begins with a dash would otherwise be read as
    // an option by `osascript` itself rather than reaching `argv`.
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .arg("--")
        .args(argv)
        .output()
        .map_err(|e| Error::io("running osascript", e))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    // -128 is the user cancelling the prompt.
    if stderr.contains("-128") || stderr.contains("User canceled") {
        return Ok(false);
    }

    Err(Error::new(
        Code::PermissionDenied,
        format!("Elevation failed: {}", stderr.trim()),
    ))
}

/// No equivalent that a windowed app can rely on.
///
/// `pkexec` is the closest thing and it is not always installed or running;
/// callers fall back to telling the user what to run themselves, which is
/// honest and does not hang.
#[cfg(not(target_os = "macos"))]
pub fn run(_argv: &[&str]) -> Result<bool> {
    Err(Error::new(
        Code::Unsupported,
        "This platform has no way for a windowed app to ask for administrator rights.",
    ))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Run [`JOIN_ARGV`] — the shipped text, not a copy — and return what it
    /// makes of `argv`.
    ///
    /// [`run`] itself cannot be called from a test: it would put a password
    /// panel on whoever is running `cargo test`. So the privileged line is the
    /// one thing swapped out, and everything the quoting depends on is the same
    /// constant the real script is built from.
    fn joined(argv: &[&str]) -> String {
        let script = format!("{JOIN_ARGV}\non run argv\n    return join(argv)\nend run");
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .arg("--")
            .args(argv)
            .output()
            .expect("osascript is present on macOS");

        assert!(
            output.status.success(),
            "osascript refused the script: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string()
    }

    /// The finding this rewrite closed. Under the old string-interpolating
    /// version, a path holding a `"` ended the AppleScript string and the rest
    /// was parsed as script — in the one function in this codebase that runs
    /// its argument as root.
    ///
    /// The paths are not hypothetical: they are built from the user's home
    /// directory and `STACKVO_ROOT`, both of which the user can put a quote in.
    #[test]
    fn a_path_cannot_escape_into_the_command() {
        let hostile = r#"/tmp/a"; rm -rf /; echo ""#;
        let joined = joined(&["/bin/cp", hostile, "/etc/hosts"]);

        // Every metacharacter is inside a quoted run, so the shell sees one
        // argument. `rm` is text, not a command.
        assert_eq!(
            joined,
            r#"'/bin/cp' '/tmp/a"; rm -rf /; echo "' '/etc/hosts'"#
        );
    }

    /// A quote in the *value* is the case a naive quoter gets wrong: closing the
    /// single-quoted run and reopening it is the only correct answer, and it is
    /// what `quoted form of` produces.
    #[test]
    fn an_embedded_single_quote_is_reopened_rather_than_dropped() {
        assert_eq!(
            joined(&["/bin/cp", "/Users/me/Ali's Files/hosts"]),
            r#"'/bin/cp' '/Users/me/Ali'\''s Files/hosts'"#
        );
    }

    /// The everyday case that the old shape only survived because both callers
    /// remembered to single-quote by hand.
    #[test]
    fn spaces_need_nothing_from_the_caller() {
        assert_eq!(
            joined(&["/bin/cp", "/var/folders/T/staged hosts", "/etc/hosts"]),
            "'/bin/cp' '/var/folders/T/staged hosts' '/etc/hosts'"
        );
    }

    /// `osascript` parses its own options first, so an argument starting with a
    /// dash has to be fenced off with `--` or it never reaches `argv`.
    #[test]
    fn a_leading_dash_reaches_the_command_instead_of_osascript() {
        assert_eq!(
            joined(&["/bin/ls", "-la", "/etc"]),
            "'/bin/ls' '-la' '/etc'"
        );
    }

    /// End to end through `do shell script`, minus the elevation: the joined
    /// command has to survive the shell as the literal argument it went in as.
    /// This is the assertion that would have failed under the old version.
    #[test]
    fn the_shell_receives_exactly_one_argument() {
        let payload = r#"a b'c"d $(whoami) `id` ; echo pwned"#;
        let script = format!("{JOIN_ARGV}\non run argv\n    do shell script join(argv)\nend run");

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .arg("--")
            .args(["/bin/echo", payload])
            .output()
            .expect("osascript is present on macOS");

        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            payload,
            "the shell expanded something it should have treated as text"
        );
    }

    /// An empty vector would build `do shell script ""`, which is a prompt for
    /// a password to run nothing.
    #[test]
    fn an_empty_command_is_refused_before_a_panel_appears() {
        let error = run(&[]).expect_err("an empty argv must not reach osascript");
        assert_eq!(error.code, Code::InvalidInput);
    }
}
