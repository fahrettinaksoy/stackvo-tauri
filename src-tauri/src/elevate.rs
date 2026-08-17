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

/// Run a program as root through polkit.
///
/// `pkexec` puts up the polkit dialog, which is the Linux equivalent of the
/// authentication panel: a prompt the *desktop* owns, not one a child process
/// tries to read from a terminal this app does not have. It is not always
/// installed, and [`available`] is how a caller finds that out before offering
/// a button that cannot work.
///
/// The exit codes are polkit's own: 126 is "the dialog was dismissed" and 127
/// is "not authorised", and both are answers rather than faults — the same
/// `Ok(false)` the macOS branch returns when somebody presses Cancel.
#[cfg(target_os = "linux")]
pub fn run(argv: &[&str]) -> Result<bool> {
    if argv.is_empty() {
        return Err(Error::new(
            Code::InvalidInput,
            "an elevated command needs a program to run",
        ));
    }

    let output = std::process::Command::new("pkexec")
        .args(argv)
        .output()
        .map_err(|e| {
            Error::new(
                Code::PermissionDenied,
                format!("pkexec is unavailable: {e}"),
            )
            .with_hint(crate::hints::INSTALL_POLKIT)
        })?;

    polkit_outcome(
        output.status.code(),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// What polkit's exit code means, as a value rather than as a control flow.
///
/// Split out of [`run`] for the reason §3 #35 exists: the dialog needs a human,
/// and *this* does not. Inline, the one part of the Linux path that decides
/// whether a cancelled prompt is an error could only ever be exercised by
/// somebody sitting in front of a polkit agent — so it never was, on a platform
/// nobody here develops on.
///
/// 126 and 127 are polkit's own: "the dialog was dismissed" and "not
/// authorised". Both are answers, and both have to arrive as `Ok(false)` — the
/// same value macOS returns for Cancel — because a caller that treats them as
/// failures puts a red error on screen for somebody who simply changed their
/// mind. `None` is the process dying on a signal, which is neither an answer
/// nor a permission problem, and is reported rather than swallowed.
///
/// Compiled on every platform even though only Linux calls it, on the same
/// terms as [`base64_utf16`] below: a branch that is only compiled where nobody
/// runs it is a branch that is wrong in the release.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn polkit_outcome(code: Option<i32>, stderr: &str) -> Result<bool> {
    match code {
        Some(0) => Ok(true),
        Some(126) | Some(127) => Ok(false),
        _ => Err(Error::new(
            Code::PermissionDenied,
            stderr.trim().to_string(),
        )),
    }
}

/// Windows has no `argv` of the shape this app's callers build — every one of
/// them names a POSIX tool. What it has instead is [`run_powershell`].
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn run(_argv: &[&str]) -> Result<bool> {
    Err(Error::new(
        Code::Unsupported,
        "This platform has no way for a windowed app to ask for administrator rights.",
    ))
}

/// Run a PowerShell script as an administrator, through the UAC prompt.
///
/// ## Why the script is base64 and not a command line
///
/// `Start-Process -ArgumentList` joins its arguments with spaces and leaves the
/// quoting to whoever wrote them, which is the same trap the macOS branch above
/// was rewritten to escape — except worse, because there are three parsers in
/// the path: PowerShell's, `CreateProcess`'s, and the receiving PowerShell's.
/// `-EncodedCommand` has none of that. UTF-16 base64 contains letters, digits,
/// `+`, `/` and `=`, so there is no character left for any of the three to read
/// as syntax, and the script arrives as the bytes that went in.
///
/// A dismissed UAC prompt throws in the *outer* shell rather than returning an
/// exit code, so cancellation is read from the message. It is `Ok(false)` here
/// for the same reason it is on macOS: nothing was changed, so nothing needs
/// reporting as broken.
#[cfg(windows)]
pub fn run_powershell(script: &str) -> Result<bool> {
    if script.trim().is_empty() {
        return Err(Error::new(
            Code::InvalidInput,
            "an elevated command needs a script to run",
        ));
    }

    let outer = uac_script(&base64_utf16(script));

    let output = std::process::Command::new(powershell_program())
        .args(["-NoProfile", "-NonInteractive", "-Command", &outer])
        .output()
        .map_err(|e| Error::io("running powershell", e))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_cancellation(&stderr) {
        return Ok(false);
    }

    Err(Error::new(
        Code::PermissionDenied,
        format!("Elevation failed: {}", stderr.trim()),
    ))
}

/// The outer shell's line, built from an already-encoded script.
///
/// Named for the same reason [`JOIN_ARGV`] is: the test below asserts on the
/// shipped text rather than on a copy, and every flag in it is load-bearing.
/// `-Verb RunAs` is the UAC prompt; without `-Wait` this returns before the
/// elevated shell has done anything and the caller reads a hosts file that has
/// not been written yet; without `-PassThru` there is no process object and
/// `$p.ExitCode` is `$null`, so `exit` sends 0 and every failure reads as
/// success. `-WindowStyle Hidden` keeps a console window from flashing up in
/// front of a windowed app.
#[cfg_attr(not(windows), allow(dead_code))]
fn uac_script(encoded: &str) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; \
         $p = Start-Process powershell \
           -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{encoded}' \
           -Verb RunAs -Wait -WindowStyle Hidden -PassThru; \
         exit $p.ExitCode"
    )
}

/// Did the person dismiss the UAC prompt?
///
/// A dismissed prompt throws in the *outer* shell rather than returning an exit
/// code, so this is read out of a message — the one place in this module where
/// a decision rests on prose. Both spellings are matched because the string
/// comes from Windows and its own components disagree: the Win32 error is "The
/// operation was canceled by the user" with one `l`, and PowerShell's wrapper
/// has been seen to render it with two.
///
/// Lowercased first. `Start-Process` surfaces the message inside its own
/// `Exception calling …` envelope, and the capitalisation of what it quotes is
/// not this app's to predict.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_cancellation(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("canceled") || lower.contains("cancelled")
}

/// Can this machine put up an authentication prompt at all?
///
/// Asked before a switch is drawn rather than after it is pressed. On Linux the
/// answer is genuinely "sometimes": a machine with no polkit agent has no way
/// for a windowed app to ask, and the honest offer there is a command the user
/// runs themselves.
pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        polkit_on(std::env::var_os("PATH").as_deref())
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        false
    }
}

/// Which PowerShell to run, and the seam that makes the UAC path testable.
///
/// `STACKVO_POWERSHELL` is the same kind of seam `STACKVO_HOSTS_PATH` is, for
/// the same reason and with one extra: without it the only way to exercise this
/// function is to raise a real UAC prompt, and there is nobody at a CI runner
/// to answer one.
///
/// ## Why this is a variable and the Linux side is a `PATH` entry
///
/// The asymmetry is deliberate, and it is about what happens when the stub is
/// *not* found. `pkexec` is resolved by `execvp`, which walks `PATH` in order —
/// unambiguous, so `tests/elevate_probe.rs` puts a stub first and that is that.
/// Windows resolution is not one rule: the loading directory, the current
/// directory, `System32` and `PATH` all take part, and Rust's own search does
/// not match `CreateProcess`'s exactly. A stub that lost that race would not
/// fail the test — it would run **real** PowerShell, which would put a UAC
/// prompt on a machine with nobody at it and hang the job until it timed out.
///
/// A named variable cannot lose a race. It is read on every call rather than
/// cached, exactly as `hosts_path()` is: a test that set it after this module
/// had been touched once would otherwise be talking to the real shell.
#[cfg_attr(not(windows), allow(dead_code))]
fn powershell_program() -> String {
    match std::env::var("STACKVO_POWERSHELL") {
        Ok(from_env) if !from_env.trim().is_empty() => from_env.trim().to_string(),
        _ => "powershell".to_string(),
    }
}

/// Is `pkexec` on this `PATH`?
///
/// Takes the value rather than reading the environment, and that is the whole
/// difference between a function this repository can test and one it cannot:
/// `std::env::set_var` is process-global, so a test that pointed the real
/// `PATH` at a fixture would be a test that could change what a parallel test
/// sees. Given the string, both answers are reachable on any platform.
///
/// `exists` rather than an executable-bit check: a `pkexec` on `PATH` that
/// cannot be executed is a broken installation, and [`run`] reports that with
/// the error the failed spawn actually produced. Deciding it here would mean
/// drawing no button and explaining nothing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn polkit_on(path: Option<&std::ffi::OsStr>) -> bool {
    path.map(|path| std::env::split_paths(path).any(|dir| dir.join("pkexec").exists()))
        .unwrap_or(false)
}

/// UTF-16LE, then base64 — what PowerShell's `-EncodedCommand` expects.
///
/// Written out rather than pulled in: this is the only base64 in the app, and a
/// dependency's worth of encoder for one call site is a dependency to audit,
/// license and update for twenty lines. It is compiled everywhere and tested
/// everywhere even though only Windows calls it, because an encoder that is
/// only exercised on the platform nobody develops on is one that is wrong for a
/// release.
///
/// `pub` for that last sentence rather than because anything outside calls it.
/// `tests/elevate_probe.rs` decodes what this produces with an independently
/// written decoder, and it does that on **every** platform — which is the only
/// coverage the Windows elevation path gets from the machine it is developed
/// on, since `cargo check --target x86_64-pc-windows-msvc` cannot run here
/// (`aws-lc-sys` wants the Windows SDK to build its C).
pub fn base64_utf16(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// The parts of the two branches this machine cannot show a dialog for.
///
/// §3 #35 says the remaining half of the Windows and Linux paths "needs a
/// human", and for the *prompt* that is true and permanent. It was never true
/// of everything around the prompt: which exit code means "cancelled", whether
/// the outer PowerShell line still carries `-Wait`, whether a machine without
/// polkit is detected before a button is drawn. Every one of those is a
/// decision made before or after the panel appears, and every one of them used
/// to be unreachable from a test only because it was written inline.
///
/// The macOS half of this module has had exactly this arrangement since it was
/// written — `joined()` runs the shipped `JOIN_ARGV` through a real `osascript`
/// with the privileged line swapped out. This is the same idea for the other
/// two.
#[cfg(test)]
mod branch_tests {
    use super::*;

    #[test]
    fn a_dismissed_polkit_dialog_is_an_answer_and_not_a_fault() {
        // 126 is dismissed, 127 is not authorised. A caller that treated either
        // as an error would put a red banner in front of somebody who changed
        // their mind — and on this platform nobody here can watch that happen.
        assert!(!polkit_outcome(Some(126), "").expect("dismissed is not an error"));
        assert!(!polkit_outcome(Some(127), "").expect("unauthorised is not an error"));
    }

    #[test]
    fn polkit_success_is_the_only_success() {
        assert!(polkit_outcome(Some(0), "").expect("zero is success"));

        let error = polkit_outcome(Some(1), "cp: /etc/hosts: Read-only file system")
            .expect_err("a non-zero, non-polkit code is a failure");
        assert_eq!(error.code, Code::PermissionDenied);
        // The message the tool produced, not one this module made up: it is the
        // only thing that says *why*, and it goes on screen.
        assert!(error.message.contains("Read-only file system"), "{error}");
    }

    #[test]
    fn a_signal_is_reported_rather_than_read_as_cancellation() {
        // `None` is the process killed rather than exited. Folding it into the
        // 126/127 arm would turn an OOM-killed elevation into a silent "the
        // user said no", and the hosts file would be left half written with
        // nothing on screen.
        let error = polkit_outcome(None, "").expect_err("a signal is not an answer");
        assert_eq!(error.code, Code::PermissionDenied);
    }

    #[test]
    fn the_uac_line_keeps_the_four_flags_that_make_it_correct() {
        let script = uac_script("QQA=");

        assert!(script.contains("-Verb RunAs"), "no UAC prompt: {script}");
        // Without `-Wait` the caller reads a hosts file that has not been
        // written yet; without `-PassThru` there is no `$p`, so `$p.ExitCode`
        // is `$null`, `exit` sends 0, and every failure reads as success.
        assert!(script.contains("-Wait"), "{script}");
        assert!(script.contains("-PassThru"), "{script}");
        assert!(script.contains("exit $p.ExitCode"), "{script}");
        // The encoded form is the whole defence against the three parsers
        // between here and the elevated shell.
        assert!(script.contains("-EncodedCommand"), "{script}");
        assert!(
            script.contains("QQA="),
            "the script did not travel: {script}"
        );
    }

    #[test]
    fn both_spellings_of_a_cancelled_uac_prompt_are_understood() {
        // Windows spells it with one `l`; PowerShell's wrapper has been seen to
        // render it with two. A branch that knew only one spelling would report
        // a dismissed prompt as a permission failure half the time.
        assert!(is_cancellation("The operation was canceled by the user."));
        assert!(is_cancellation("The operation was cancelled by the user."));
        // Case comes out of an `Exception calling …` envelope this app does not
        // control.
        assert!(is_cancellation("Operation was CANCELED by the user"));
        assert!(!is_cancellation("Access is denied"));
        assert!(!is_cancellation(""));
    }

    /// The seam defaults to the real shell, and a blank value is not a program.
    ///
    /// Asserted because the failure is silent in the worst direction: an empty
    /// `STACKVO_POWERSHELL` left in an environment would make this app try to
    /// spawn `""`, and the error a user would see is about a program with no
    /// name rather than about a variable somebody exported.
    #[test]
    fn the_powershell_seam_falls_back_to_the_real_one() {
        // Read rather than set: this runs beside every other test in the
        // process and `set_var` is process-wide. The default branch is the one
        // every machine takes, and it is the one worth pinning.
        assert_eq!(powershell_program(), "powershell");
    }

    #[test]
    fn a_machine_without_polkit_is_told_apart_from_one_with_it() {
        let dir = std::env::temp_dir().join(format!("stackvo-polkit-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("a fixture directory");

        let empty = std::ffi::OsString::from(bin.as_os_str());
        assert!(
            !polkit_on(Some(&empty)),
            "an empty directory answered yes, so the check answers yes to anything"
        );

        std::fs::write(bin.join("pkexec"), "#!/bin/sh\n").expect("a fixture pkexec");
        assert!(polkit_on(Some(&empty)));

        // No `PATH` at all is a machine that cannot be asked, not one that can.
        assert!(!polkit_on(None));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::base64_utf16;

    /// The expected values are what `[Convert]::ToBase64String(
    /// [Text.Encoding]::Unicode.GetBytes($s))` produces — PowerShell's own
    /// definition of the thing this has to match.
    #[test]
    fn a_script_encodes_the_way_powershell_decodes_it() {
        assert_eq!(base64_utf16(""), "");
        assert_eq!(base64_utf16("A"), "QQA=");
        assert_eq!(base64_utf16("AB"), "QQBCAA==");
        assert_eq!(base64_utf16("ABC"), "QQBCAEMA");
        assert_eq!(
            base64_utf16("ipconfig /flushdns"),
            "aQBwAGMAbwBuAGYAaQBnACAALwBmAGwAdQBzAGgAZABuAHMA"
        );
    }

    /// Nothing in the output can be read as syntax by any of the three parsers
    /// between here and the elevated shell. That is the whole reason for it.
    #[test]
    fn the_encoding_has_no_character_a_shell_could_read() {
        let hostile = "Add-DnsClientNrptRule -Namespace '.loc'; & { rm -rf \"$HOME\" } `id`";
        let encoded = base64_utf16(hostile);
        assert!(encoded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='));
    }
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
