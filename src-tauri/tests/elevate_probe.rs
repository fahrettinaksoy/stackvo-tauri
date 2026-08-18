//! The privileged path, driven end to end — everything except the dialog.
//!
//! §3 #35's remaining half is written as "the elevation itself — a pkexec, UAC
//! or osascript dialog needs a human". Half of that sentence is permanent and
//! half of it was a description of how the code was arranged.
//!
//! What genuinely needs a person is the **panel**: somebody has to look at it
//! and type a password or press Cancel. Nothing else in the chain does.
//! `apply` deciding it cannot write the file, staging the new contents,
//! handing `cp <staged> <hosts>` to the elevator, and reading polkit's exit
//! code back as *cancelled* rather than *failed* — none of those are the
//! dialog, and all of them were unreachable from a test only because the
//! elevator was named by a literal in the middle of a function.
//!
//! `pkexec` is looked up on `PATH`. So this puts one there.
//!
//! ## What the stub is and is not
//!
//! It is a real program, spawned by the real `std::process::Command` in the
//! shipped `elevate::run`, receiving the real argv `hosts::elevated_copy`
//! built. It records what it was given and exits with the code the case is
//! about. What it does not do is ask for a password — which is the one part
//! that could not be automated and the one part that is not being claimed.
//!
//! This is `examples/tui_probe.rs`'s lesson applied to the other platform
//! branch: a coder tested against their own expectation only agrees with
//! themselves. A hand-written assertion that `elevate::run` "would pass the
//! right argv" is that agreement. A stub that prints what actually arrived is
//! not.
//!
//! ## Two platforms, two stubs, and two different seams
//!
//! Linux and Windows both get driven here; macOS does not and cannot, because
//! `osascript`'s authentication panel is the mechanism rather than a program
//! that could be replaced.
//!
//! The stubs reach the code by different routes, and the difference is not
//! taste. `pkexec` is resolved by `execvp`, which walks `PATH` in order — so a
//! stub is a file in a directory placed first, unambiguously. Windows
//! resolution is several rules at once (the loading directory, the current
//! directory, `System32`, `PATH`) and Rust's own search does not match
//! `CreateProcess`'s exactly. A stub that lost that race would not fail the
//! test: it would run **real** PowerShell, raise a real UAC prompt on a runner
//! with nobody at it, and hang the job. So Windows goes through the named seam
//! `STACKVO_POWERSHELL`, which cannot lose a race, and `elevate.rs` carries the
//! same reasoning beside the function.
//!
//! The Windows stub is a real program, compiled by `rustc` into a temporary
//! directory when the test runs. A `.bat` would not do — `CreateProcess` cannot
//! execute one — and adding a fourth `[[bin]]` to a crate that ships three
//! would put a fake PowerShell in every release build to serve one test.
//! `rustc` is on `PATH` by construction: `cargo test` put it there.
//!
//! **One test function per platform, not five.** Both seams are process-wide —
//! `PATH` on one side, an environment variable on the other — and `cargo test`
//! runs test functions in parallel threads. Cases run in sequence inside one
//! function so that no case can be reading what another one set: the same
//! argument `hosts_roundtrip.rs` makes for being a separate binary, one level
//! in.

#[cfg(target_os = "linux")]
use stackvo_desktop_lib::hosts;
#[allow(unused_imports)]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
/// A directory holding a `pkexec` that exits with `code`.
///
/// The script records its argv and, separately, the *contents* of the file it
/// was asked to copy. The contents are the interesting half: `apply` deletes
/// the staged file as soon as the elevator returns, so a test that only kept
/// the path would be holding the name of something that no longer exists —
/// and the bytes that were about to land in `/etc/hosts` are the thing worth
/// looking at.
fn stub_pkexec(case: &str, code: i32, message: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!("stackvo-elevate-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a fixture directory");

    let argv_log = dir.join("argv");
    let staged_log = dir.join("staged");
    let script = format!(
        "#!/bin/sh\n\
         printf '%s\\n' \"$@\" > {argv}\n\
         if [ -f \"$2\" ]; then cp \"$2\" {staged}; fi\n\
         [ -n '{message}' ] && printf '%s\\n' '{message}' >&2\n\
         exit {code}\n",
        argv = argv_log.display(),
        staged = staged_log.display(),
        message = message,
        code = code,
    );

    let path = dir.join("pkexec");
    std::fs::write(&path, script).expect("the stub is writable");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("the stub is executable");

    // Ahead of the real one rather than instead of it: a machine that has
    // polkit installed must still take this stub, and a machine that does not
    // must not start finding a real `pkexec` because this test emptied `PATH`.
    let existing = std::env::var("PATH").unwrap_or_default();
    // SAFETY: this binary runs one test function, and every case in it is
    // sequential. Nothing else in this process reads `PATH` concurrently.
    unsafe { std::env::set_var("PATH", format!("{}:{existing}", dir.display())) };

    dir
}

#[cfg(target_os = "linux")]
/// A hosts path this process cannot write, whoever it is running as.
///
/// A directory, not a `chmod 0444` file. Read-only modes are advisory to root,
/// and a container that runs `cargo test` as root would write straight through
/// one — at which point `apply` never reaches the elevator and this test
/// passes having proved nothing at all. `OpenOptions::write` on a directory
/// fails with `EISDIR` for every uid there is.
fn unwritable_hosts(case: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("stackvo-hosts-dir-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a fixture directory");
    // SAFETY: see `stub_pkexec`.
    unsafe { std::env::set_var("STACKVO_HOSTS_PATH", &path) };
    path
}

#[cfg(target_os = "linux")]
fn recorded_argv(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("argv"))
        .unwrap_or_else(|e| panic!("the stub never ran: {e}"))
        .lines()
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn the_polkit_path_runs_without_anybody_at_the_dialog() {
    // ---- it gets there, and with the right command -----------------------
    //
    // The claim is not "elevation was attempted". It is that the unprivileged
    // write was tried first and failed, and that what reached the elevator is
    // a copy of a staged file — never a shell line with somebody's domain
    // interpolated into it, which is what `hosts.rs` says it is avoiding.
    let dir = stub_pkexec("granted", 0, "");
    let hosts = unwritable_hosts("granted");

    let plan = hosts::apply(&["shop.loc".into()], &[]).expect("the stub grants it");
    assert!(plan.changed, "adding a domain is a change");

    let argv = recorded_argv(&dir);
    assert_eq!(argv.len(), 3, "pkexec got {argv:?}");
    assert_eq!(argv[0], "cp");
    assert_eq!(
        argv[2],
        hosts.display().to_string(),
        "the copy did not target the hosts path"
    );

    // The bytes that were on their way into the file. This is the assertion
    // that makes the rest mean something: `apply` could have handed the
    // elevator a correct-looking command over an empty or stale staging file
    // and every check above would still pass.
    let staged = std::fs::read_to_string(dir.join("staged")).expect("the staged file was copied");
    assert!(staged.contains("shop.loc"), "{staged}");
    assert!(
        staged.contains("StackVo"),
        "the marker block did not travel: {staged}"
    );

    // ---- somebody pressed Cancel ----------------------------------------
    //
    // 126 is polkit's "dialog dismissed". The whole point of reading it as an
    // answer is that the caller must not be told the machine is broken — but
    // `apply` still has to report that the file was not updated, because it
    // was not.
    let _dir = stub_pkexec("dismissed", 126, "");
    unwritable_hosts("dismissed");

    let error = hosts::apply(&["shop.loc".into()], &[]).expect_err("nothing was written");
    assert_eq!(
        error.code,
        stackvo_desktop_lib::error::Code::PermissionDenied
    );
    assert_eq!(
        error.hint_key,
        Some(stackvo_desktop_lib::hints::HOSTS_NEEDS_ADMIN.key),
        "a dismissed prompt must still tell the user what would fix it"
    );

    // ---- and a real failure is not a cancellation ------------------------
    //
    // The distinction this pair exists for. A `cp` that failed carries a
    // reason, and that reason is the only thing on screen that says what went
    // wrong — folding it into the same "not updated" message as Cancel would
    // lose it.
    let _dir = stub_pkexec("refused", 1, "cp: cannot create regular file");
    unwritable_hosts("refused");

    let error = hosts::apply(&["shop.loc".into()], &[]).expect_err("the copy failed");
    assert_eq!(
        error.code,
        stackvo_desktop_lib::error::Code::PermissionDenied
    );
    assert!(
        error.message.contains("cannot create regular file"),
        "the tool's own reason did not survive: {}",
        error.message
    );
}

// ---------------------------------------------------------------- Windows

/// A PowerShell that is not PowerShell.
///
/// Compiled by `rustc` into a temporary directory, and pointed at through
/// `STACKVO_POWERSHELL` rather than `PATH` — the module comment says why the
/// two platforms differ here, and the short version is that a stub which lost a
/// `PATH` race on Windows would raise a real UAC prompt on a runner with nobody
/// at it.
///
/// One binary serves all three cases; what it does is read out of the
/// environment at run time. Compiling three would triple the slowest part of
/// this test for no more coverage.
#[cfg(windows)]
fn build_fake_powershell(dir: &Path) -> PathBuf {
    const SOURCE: &str = r#"
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(path) = std::env::var("STACKVO_STUB_ARGV") {
        // One argument per line. The outer PowerShell line contains spaces and
        // quotes; anything that joined them would make the test's job harder
        // than reading them back.
        let _ = std::fs::write(path, args.join("\n"));
    }
    if let Ok(message) = std::env::var("STACKVO_STUB_STDERR") {
        if !message.is_empty() {
            eprintln!("{message}");
        }
    }
    let code: i32 = std::env::var("STACKVO_STUB_EXIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    std::process::exit(code);
}
"#;

    let source = dir.join("fake_powershell.rs");
    std::fs::write(&source, SOURCE).expect("the stub source is writable");

    let exe = dir.join("powershell.exe");
    let out = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("rustc is on PATH — cargo test put it there");

    assert!(
        out.status.success(),
        "the stub did not compile:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// UTF-16LE base64, decoded — PowerShell's `-EncodedCommand` in reverse.
///
/// Written out rather than pulled in, on the same terms as the encoder it is
/// checking. And deliberately *not* by calling that encoder: a decoder built
/// from the encoder's own idea of base64 agrees with it whatever either does,
/// which is the mistake `elevate.rs` says a QR encoder already cost this
/// repository once.
///
/// **Not gated to Windows**, though only the Windows test calls it, and the
/// reason is the same one the WebDriver client is split for: this repository is
/// developed on macOS, and `cargo check --target x86_64-pc-windows-msvc` does
/// not work from here — `aws-lc-sys` wants the Windows SDK headers to build its
/// C. So the Windows probe below is code whose first compile is on CI. Leaving
/// the one piece of *reasoning* in it — a base64 and UTF-16 decoder — outside
/// the gate means it is compiled and run on every platform, against the vectors
/// PowerShell itself produced.
fn decode_utf16_base64(encoded: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut bits: Vec<u8> = Vec::new();
    let mut acc: u32 = 0;
    let mut have = 0;
    for byte in encoded.bytes().filter(|b| *b != b'=') {
        let index = ALPHABET
            .iter()
            .position(|c| *c == byte)
            .unwrap_or_else(|| panic!("{byte:?} is not base64 — the encoder emitted it"));
        acc = (acc << 6) | index as u32;
        have += 6;
        if have >= 8 {
            have -= 8;
            bits.push((acc >> have) as u8);
        }
    }

    let units: Vec<u16> = bits
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).expect("the encoder produced valid UTF-16")
}

#[cfg(windows)]
#[test]
fn the_uac_path_runs_without_anybody_at_the_prompt() {
    let dir = std::env::temp_dir().join(format!("stackvo-uac-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a fixture directory");

    let exe = build_fake_powershell(&dir);
    let argv_log = dir.join("argv");

    // SAFETY: this binary runs one test function per platform and every case in
    // it is sequential. Nothing else in this process reads these.
    unsafe {
        std::env::set_var("STACKVO_POWERSHELL", &exe);
        std::env::set_var("STACKVO_STUB_ARGV", &argv_log);
    }

    // ---- granted ---------------------------------------------------------
    //
    // The script this app would really run: `dns::write_nrpt`'s, quotes and
    // semicolons and all. A payload with no metacharacters in it would prove
    // the encoding survives the easy case, which is not the case the encoding
    // is for.
    let script = "Add-DnsClientNrptRule -Namespace '.loc' -NameServers '127.0.0.1'; \
                  ipconfig /flushdns | Out-Null";

    unsafe {
        std::env::set_var("STACKVO_STUB_EXIT", "0");
        std::env::set_var("STACKVO_STUB_STDERR", "");
    }
    assert!(
        stackvo_desktop_lib::elevate::run_powershell(script).expect("the stub grants it"),
        "a zero exit is a granted prompt"
    );

    let argv: Vec<String> = std::fs::read_to_string(&argv_log)
        .expect("the stub never ran — is STACKVO_POWERSHELL reaching elevate.rs?")
        .lines()
        .map(str::to_string)
        .collect();

    assert_eq!(
        &argv[..3],
        &["-NoProfile", "-NonInteractive", "-Command"],
        "the outer shell was invoked differently: {argv:?}"
    );

    let outer = &argv[3];
    assert!(outer.contains("-Verb RunAs"), "no UAC prompt: {outer}");
    assert!(outer.contains("-Wait"), "{outer}");
    assert!(outer.contains("-PassThru"), "{outer}");
    assert!(outer.contains("exit $p.ExitCode"), "{outer}");

    // The assertion this whole file exists for on this platform: the script
    // survived the encoder, the outer PowerShell line, `CreateProcess`'s
    // argument quoting and the receiving argv — and came out as the bytes that
    // went in. Every one of those is a parser, and `elevate.rs` chose
    // `-EncodedCommand` precisely so none of them can read the payload as
    // syntax. Reading the flags above proves the shape; only this proves it
    // works.
    let encoded = outer
        .split("'-EncodedCommand','")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .unwrap_or_else(|| panic!("no encoded command in the outer line: {outer}"));
    assert_eq!(
        decode_utf16_base64(encoded),
        script,
        "the script did not survive the trip"
    );

    // ---- dismissed -------------------------------------------------------
    //
    // A cancelled UAC prompt throws in the outer shell instead of returning a
    // code, so this is the one decision in the module that rests on a message.
    unsafe {
        std::env::set_var("STACKVO_STUB_EXIT", "1");
        std::env::set_var(
            "STACKVO_STUB_STDERR",
            "Start-Process : The operation was canceled by the user.",
        );
    }
    assert!(
        !stackvo_desktop_lib::elevate::run_powershell(script).expect("cancelling is not an error"),
        "a dismissed prompt must be Ok(false), not a failure"
    );

    // ---- refused ---------------------------------------------------------
    //
    // The other half of that pair. A real failure carries a reason, and losing
    // it would leave a user with "elevation failed" and nothing to act on.
    unsafe {
        std::env::set_var("STACKVO_STUB_EXIT", "1");
        std::env::set_var("STACKVO_STUB_STDERR", "Access is denied");
    }
    let error = stackvo_desktop_lib::elevate::run_powershell(script)
        .expect_err("a non-cancellation failure is an error");
    assert_eq!(
        error.code,
        stackvo_desktop_lib::error::Code::PermissionDenied
    );
    assert!(
        error.message.contains("Access is denied"),
        "the shell's own reason did not survive: {}",
        error.message
    );

    // ---- and an empty script never reaches a prompt -----------------------
    let error = stackvo_desktop_lib::elevate::run_powershell("   ")
        .expect_err("an empty script must not raise a prompt");
    assert_eq!(error.code, stackvo_desktop_lib::error::Code::InvalidInput);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The decoder agrees with PowerShell, not with this repository's encoder.
///
/// Runs everywhere, including the platform that cannot compile the Windows
/// probe above. The vectors are the ones `elevate::encoding_tests` uses and
/// they came from `[Convert]::ToBase64String([Text.Encoding]::Unicode
/// .GetBytes($s))` — PowerShell's own definition of the encoding this has to be
/// the inverse of. A decoder checked only by round-tripping the encoder would
/// agree with it however wrong both were.
#[test]
fn the_decoder_is_powershells_inverse_and_not_the_encoders_mirror() {
    assert_eq!(decode_utf16_base64(""), "");
    assert_eq!(decode_utf16_base64("QQA="), "A");
    assert_eq!(decode_utf16_base64("QQBCAA=="), "AB");
    assert_eq!(decode_utf16_base64("QQBCAEMA"), "ABC");
    assert_eq!(
        decode_utf16_base64("aQBwAGMAbwBuAGYAaQBnACAALwBmAGwAdQBzAGgAZABuAHMA"),
        "ipconfig /flushdns"
    );

    // And the round trip, which is what the probe actually asserts. Second
    // rather than first: passing this alone would only mean the two halves of
    // one opinion agree.
    for original in [
        "Add-DnsClientNrptRule -Namespace '.loc' -NameServers '127.0.0.1'",
        "& { rm -rf \"$HOME\" } `id` ; echo pwned",
        "türkçe karakterler ve emoji 🚀",
    ] {
        let encoded = stackvo_desktop_lib::elevate::base64_utf16(original);
        assert_eq!(decode_utf16_base64(&encoded), original, "{original}");
    }
}
