//! `stackvo tui` against a real terminal (M-8).
//!
//!   cargo run --example tui_probe
//!
//! Every other probe in this directory exists because something could only be
//! wrong at run time — `mariadb-dump`'s missing client, the QR encoder that
//! agreed with itself, the profile directory nothing created. This one exists
//! for a sharper reason: **the failure mode is the user's terminal.**
//!
//! A screen that takes a terminal into raw mode and does not give it back
//! leaves somebody with no echo, no line editing and no working `Ctrl-C`,
//! having to type `reset` blind. No unit test can see that. It is a property
//! of a `Drop`, a panic hook and an escape sequence arriving in the right
//! order at a device — so this opens a pty, runs the real binary in it, drives
//! it with real keystrokes and reads the terminal's own settings back.
//!
//! What it checks, in order:
//!
//! 1. The screen comes up on the **alternate** screen with the cursor hidden.
//! 2. It draws the stack — a heading and at least one row.
//! 3. `j` moves the cursor, which is the whole difference between a screen and
//!    a report.
//! 4. `q` leaves, and the exit is clean: cursor shown, alternate screen left.
//! 5. **The terminal is back in line mode with echo on.** This is the one that
//!    matters; the other four are how it gets there.
//!
//! Unix only, and the `#[cfg]` is at the top rather than around the checks: a
//! probe that "passed" on Windows by doing nothing would be worse than one
//! that says it did not run.

#[cfg(not(unix))]
fn main() {
    eprintln!("tui_probe needs a Unix pty — nothing was checked on this platform.");
}

#[cfg(unix)]
fn main() {
    std::process::exit(unix::run());
}

#[cfg(unix)]
mod unix {
    use std::io::{Read, Write};
    use std::os::unix::io::RawFd;
    use std::time::{Duration, Instant};

    /// Where the binary this drives is, given a `cargo run --example` cwd.
    fn binary() -> std::path::PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for profile in ["debug", "release"] {
            let candidate = manifest.join("target").join(profile).join("stackvo");
            if candidate.is_file() {
                return candidate;
            }
        }
        eprintln!(
            "no `stackvo` binary under {}/target — run `cargo build --bin stackvo` first",
            manifest.display()
        );
        std::process::exit(2);
    }

    pub fn run() -> i32 {
        let program = binary();
        let (master, pid) = match spawn(&program) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("could not open a pty: {e}");
                return 2;
            }
        };

        let mut failures = 0;
        let mut seen = Vec::new();

        // Long enough for the first read of the stack, which lists containers
        // through the Docker socket.
        pump(master, &mut seen, Duration::from_secs(4));

        check(
            &mut failures,
            "enters the alternate screen",
            contains(&seen, b"\x1b[?1049h"),
        );
        check(
            &mut failures,
            "hides the cursor",
            contains(&seen, b"\x1b[?25l"),
        );
        check(
            &mut failures,
            "draws the stack",
            contains(&seen, b"PROJECTS") && contains(&seen, "▸".as_bytes()),
        );

        // Raw mode, read off the terminal rather than assumed.
        check(&mut failures, "turns echo off", !echo_on(master));
        check(&mut failures, "leaves line mode", !line_mode_on(master));

        // The cursor moves. Taken as "the marked row changed", because which
        // row is second depends on the machine this runs on.
        let before = marked_row(&seen);
        write(master, b"j");
        let mut after_key = Vec::new();
        pump(master, &mut after_key, Duration::from_millis(1500));
        let after = marked_row(&after_key).or(before.clone());
        check(
            &mut failures,
            "the cursor moves on a key",
            before.is_some() && after != before,
        );
        seen.extend_from_slice(&after_key);

        // And it leaves.
        write(master, b"q");
        pump(master, &mut seen, Duration::from_secs(3));

        check(
            &mut failures,
            "shows the cursor again",
            contains(&seen, b"\x1b[?25h"),
        );
        check(
            &mut failures,
            "leaves the alternate screen",
            contains(&seen, b"\x1b[?1049l"),
        );

        // The one that matters. Read after the process has gone, from the
        // terminal it was using.
        std::thread::sleep(Duration::from_millis(300));
        check(&mut failures, "gives echo back", echo_on(master));
        check(&mut failures, "gives line mode back", line_mode_on(master));

        let status = reap(pid);
        check(&mut failures, "exits 0", status == Some(0));

        unsafe { libc::close(master) };

        if failures == 0 {
            println!("\ntui_probe: everything held.");
            0
        } else {
            println!("\ntui_probe: {failures} check(s) failed.");
            1
        }
    }

    fn check(failures: &mut usize, what: &str, ok: bool) {
        println!("{}  {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            *failures += 1;
        }
    }

    /// The label on the row the cursor is on, if the screen drew one.
    ///
    /// Read out of the escape-laden output rather than modelled: this probe is
    /// checking what a terminal received, and a parser that agreed with the
    /// drawing code would be the QR encoder's mistake again.
    fn marked_row(bytes: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(bytes);
        // The last marked row wins: the output holds several frames and the
        // most recent one is the current state.
        text.lines()
            .filter(|line| line.contains('▸'))
            .map(|line| strip_escapes(line).trim().to_string())
            .next_back()
    }

    fn strip_escapes(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // ---- the pty ---------------------------------------------------------

    fn spawn(program: &std::path::Path) -> std::io::Result<(RawFd, libc::pid_t)> {
        unsafe {
            let mut master: RawFd = 0;
            let mut slave: RawFd = 0;
            if libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }

            let pid = libc::fork();
            if pid < 0 {
                return Err(std::io::Error::last_os_error());
            }

            if pid == 0 {
                // The child: the pty is its controlling terminal, and all
                // three descriptors are the slave end.
                libc::close(master);
                libc::setsid();
                libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
                if slave > 2 {
                    libc::close(slave);
                }

                let path = std::ffi::CString::new(program.as_os_str().as_encoded_bytes()).unwrap();
                let arg = std::ffi::CString::new("tui").unwrap();
                let columns = std::ffi::CString::new("COLUMNS=100").unwrap();
                libc::putenv(columns.into_raw());
                libc::execv(
                    path.as_ptr(),
                    [path.as_ptr(), arg.as_ptr(), std::ptr::null()].as_ptr(),
                );
                libc::_exit(127);
            }

            libc::close(slave);
            Ok((master, pid))
        }
    }

    /// Read whatever the screen has produced, for a while.
    fn pump(fd: RawFd, into: &mut Vec<u8>, how_long: Duration) {
        let deadline = Instant::now() + how_long;
        let mut file = unsafe { file_of(fd) };
        let mut buffer = [0u8; 8192];

        while Instant::now() < deadline {
            if !readable(fd, Duration::from_millis(100)) {
                continue;
            }
            match file.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => into.extend_from_slice(&buffer[..n]),
            }
        }
        std::mem::forget(file); // the fd is closed once, at the end
    }

    fn write(fd: RawFd, bytes: &[u8]) {
        let mut file = unsafe { file_of(fd) };
        let _ = file.write_all(bytes);
        let _ = file.flush();
        std::mem::forget(file);
    }

    unsafe fn file_of(fd: RawFd) -> std::fs::File {
        use std::os::unix::io::FromRawFd;
        unsafe { std::fs::File::from_raw_fd(fd) }
    }

    fn readable(fd: RawFd, within: Duration) -> bool {
        unsafe {
            let mut set: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut set);
            libc::FD_SET(fd, &mut set);
            let mut timeout = libc::timeval {
                tv_sec: within.as_secs() as _,
                tv_usec: within.subsec_micros() as _,
            };
            libc::select(
                fd + 1,
                &mut set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut timeout,
            ) > 0
        }
    }

    fn echo_on(fd: RawFd) -> bool {
        flags(fd).is_some_and(|f| f & libc::ECHO != 0)
    }

    fn line_mode_on(fd: RawFd) -> bool {
        flags(fd).is_some_and(|f| f & libc::ICANON != 0)
    }

    fn flags(fd: RawFd) -> Option<libc::tcflag_t> {
        unsafe {
            let mut settings: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut settings) != 0 {
                return None;
            }
            Some(settings.c_lflag)
        }
    }

    fn reap(pid: libc::pid_t) -> Option<i32> {
        unsafe {
            let mut status = 0;
            // The screen has been told to quit; give it a moment rather than
            // blocking forever if it did not.
            for _ in 0..30 {
                if libc::waitpid(pid, &mut status, libc::WNOHANG) == pid {
                    return Some(libc::WEXITSTATUS(status));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, &mut status, 0);
            None
        }
    }
}
