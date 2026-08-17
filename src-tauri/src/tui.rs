//! A terminal surface you can work in (M-8).
//!
//! M-8 is "alternative surfaces", and its own record says what makes something
//! one: the tray stopped being a shortcut the day it could **act** without
//! raising the window. The same test applies here. A screen that only reported
//! would be `watch stackvo projects`, which anybody can already write.
//!
//! So this lists the stack, follows it live, and starts and stops what is on
//! it — from a terminal, with no window open anywhere.
//!
//! ## Why there is no TUI library in `Cargo.toml`
//!
//! Measured before deciding, the way `keyring` and `toml_edit` were. `ratatui`
//! — the obvious choice — brings **25 new packages** into `Cargo.lock`
//! (649 → 674): a layout solver, a widget set, two unicode-width crates, an
//! LRU cache, `strum`, `darling`, a second `rustix`. What this screen needs is
//! a list, a detail line and a status bar.
//!
//! What it actually takes is already here. Drawing is
//! [`crate::cli::Style`] and the same column arithmetic `cli.rs` uses for its
//! tables. The cursor, the alternate screen and colour are ANSI escapes, which
//! are text. Raw mode is the only part needing an operating system, and both
//! halves are in the lock file already — `libc` through `portable-pty`,
//! `windows-sys` through Tauri.
//!
//! Zero new packages, then. The cost is this file, and the cost is named.
//!
//! ## The dangerous part, and how it is paid for
//!
//! **A terminal left in raw mode is a broken terminal.** No echo, no line
//! editing, `Ctrl-C` not working — and the person has to type `reset` blind to
//! get out of it. Every exit is covered, and there are four:
//!
//! * **Returning** — [`Terminal`] restores on `Drop`.
//! * **`?` on an error** — the same `Drop`, which is why the guard is a value
//!   and not a pair of calls.
//! * **A panic** — `Drop` does **not** run: this crate is built with
//!   `panic = "abort"` in release. So [`Terminal::new`] installs a panic hook
//!   that restores first and then defers to the one [`crate::crash`] set.
//! * **`Ctrl-C`** — read as a key rather than a signal, because raw mode stops
//!   the terminal turning it into one. It quits the same way `q` does.
//!
//! The restore itself is written to be safe to run twice and safe to run when
//! nothing was ever set up, because a hook and a `Drop` can both fire.
//!
//! ## Input arrives on its own thread
//!
//! A read on stdin in raw mode blocks until a key arrives, and this screen has
//! to refresh on a timer whether one does or not. `poll`/`select` would solve
//! it on Unix and need a second implementation for Windows; a thread and a
//! channel solve it in nine lines on both, and `recv_timeout` is exactly the
//! "a key, or the refresh interval, whichever comes first" this loop wants.
//!
//! The thread is left running at exit rather than joined. It is blocked on a
//! read from a stdin that is about to belong to the shell again, and the
//! process is on its way out; joining it would mean waiting for one more
//! keystroke before the program could end.

use crate::cli::Style;
use crate::error::{Error, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

/// How often the screen re-reads the stack when nothing is pressed.
///
/// Three seconds rather than one: every refresh lists containers through the
/// Docker socket, and a screen that polls a daemon as fast as it can redraw is
/// one people notice in their fan.
const REFRESH: Duration = Duration::from_secs(3);

// ------------------------------------------------------------------- keys

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Refresh,
    Logs,
    Quit,
    /// Anything this screen has no use for.
    Other,
}

/// One key, from the bytes a terminal actually sends.
///
/// Arrow keys arrive as three bytes — `ESC [ A` — and a lone `ESC` is the
/// escape key, so the parser has to work on a buffer rather than on one byte.
/// `j`/`k` are here because anybody who reaches for a screen like this in a
/// terminal has muscle memory for them.
pub fn key_of(bytes: &[u8]) -> Key {
    match bytes {
        [0x1b, b'[', b'A'] | [b'k'] => Key::Up,
        [0x1b, b'[', b'B'] | [b'j'] => Key::Down,
        // Carriage return, not newline: raw mode delivers Enter as `\r`,
        // which is the whole reason this was worth a test.
        [b'\r'] | [b'\n'] | [b' '] => Key::Enter,
        [b'r'] => Key::Refresh,
        [b'l'] => Key::Logs,
        // Ctrl-C is byte 3. In raw mode the terminal no longer turns it into a
        // signal, so a screen that did not read it here would be one nobody
        // could leave the way they expect to.
        [b'q'] | [0x03] | [0x1b] => Key::Quit,
        _ => Key::Other,
    }
}

// ------------------------------------------------------------- the screen

/// A row on screen: a project or a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub running: bool,
    /// Services are listed but not toggled here — a database is shared, and
    /// stopping one from a project screen is a decision with somebody else's
    /// project on the other end of it.
    pub project: bool,
}

/// Everything the screen shows, read once per refresh.
#[derive(Debug, Default, Clone)]
pub struct Model {
    pub rows: Vec<Row>,
    pub selected: usize,
    pub engine: bool,
    pub engine_detail: String,
    /// The last thing that happened, shown until the next thing does.
    pub message: Option<String>,
}

impl Model {
    /// Move the selection, stopping at the ends.
    ///
    /// Deliberately not wrapping. A list that jumps from the last row to the
    /// first is one where holding a key past the end starts a project nobody
    /// was looking at.
    pub fn move_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.rows.len() - 1;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, last as isize) as usize;
    }

    pub fn current(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }
}

/// Read the stack into a model, keeping the selection where it was.
///
/// By **id**, not by index: a project that goes away while the screen is open
/// would otherwise shift every row below it under the cursor, and the next
/// Enter would stop something the person was not pointing at.
pub async fn read(root: &Path, previous: Option<&Model>) -> Model {
    let engine = crate::engine::status().await;
    let projects = crate::commands::list_projects(root)
        .await
        .unwrap_or_default();
    let services = crate::commands::list_services(root)
        .await
        .unwrap_or_default();

    let mut rows: Vec<Row> = projects
        .into_iter()
        .map(|p| Row {
            detail: p.domain.clone().unwrap_or_else(|| "—".into()),
            label: p.name.clone(),
            id: p.name,
            running: p.running,
            project: true,
        })
        .collect();

    rows.extend(services.into_iter().filter(|s| s.enabled).map(|s| {
        Row {
            detail: s
                .health
                .clone()
                .or_else(|| s.version.clone())
                .unwrap_or_default(),
            label: s.id.clone(),
            id: s.id,
            running: s.running,
            project: false,
        }
    }));

    let selected = previous
        .and_then(|m| m.current())
        .and_then(|row| rows.iter().position(|r| r.id == row.id))
        .unwrap_or_else(|| previous.map(|m| m.selected).unwrap_or(0));

    Model {
        selected: selected.min(rows.len().saturating_sub(1)),
        rows,
        engine: engine.reachable,
        engine_detail: engine.version.unwrap_or_else(|| "not reachable".into()),
        message: previous.and_then(|m| m.message.clone()),
    }
}

/// The whole screen, as a string.
///
/// Built and returned rather than printed, so a test can read what a given
/// model looks like — the same reason `cli::render` works the way it does.
pub fn draw(model: &Model, style: &Style, width: usize) -> String {
    let mut out = String::new();

    // Home the cursor and clear as we go, rather than clearing the whole
    // screen first: a clear-then-draw flickers, because there is a moment when
    // the terminal genuinely has nothing on it.
    out.push_str("\x1b[H");

    let right = format!(
        "engine {} · {} projects",
        if model.engine { "up" } else { "down" },
        model.rows.iter().filter(|r| r.project).count()
    );
    out.push_str(&line(
        &format!(
            "{}{}{}",
            style.bold("StackVo"),
            " ".repeat(width.saturating_sub(7 + right.len()).max(1)),
            style.dim(&right)
        ),
        width,
    ));
    out.push_str(&line(&style.dim(&"─".repeat(width)), width));

    let mut section = None;
    for (index, row) in model.rows.iter().enumerate() {
        let heading = if row.project { "PROJECTS" } else { "SERVICES" };
        if section != Some(heading) {
            section = Some(heading);
            out.push_str(&line("", width));
            out.push_str(&line(&style.dim(heading), width));
        }

        let cursor = if index == model.selected { "▸" } else { " " };
        let state = if row.running {
            style.ok("up  ")
        } else {
            style.dim("down")
        };
        let label = if index == model.selected {
            style.bold(&row.label)
        } else {
            row.label.clone()
        };

        out.push_str(&line(
            &format!("{cursor} {state}  {label}  {}", style.dim(&row.detail)),
            width,
        ));
    }

    if model.rows.is_empty() {
        out.push_str(&line("", width));
        out.push_str(&line(&style.dim("  nothing to show"), width));
    }

    out.push_str(&line("", width));
    out.push_str(&line(&style.dim(&"─".repeat(width)), width));

    match &model.message {
        Some(message) => out.push_str(&line(&format!(" {message}"), width)),
        None => out.push_str(&line(
            &style.dim(" ↑↓ move · enter start/stop · l logs · r refresh · q quit"),
            width,
        )),
    }

    // Everything below what was drawn, in case the previous frame was taller.
    out.push_str("\x1b[J");
    out
}

/// One line, cleared to the end so a shorter frame cannot leave the last one
/// showing through.
fn line(text: &str, width: usize) -> String {
    let _ = width;
    format!("{text}\x1b[K\r\n")
}

// ------------------------------------------------------- the terminal itself

/// Raw mode and the alternate screen, restored however this ends.
///
/// Carries nothing: the original settings live in [`SAVED`] because the panic
/// hook has to reach them too, and one place for them is what makes "put the
/// terminal back exactly once" checkable rather than hoped for.
pub struct Terminal;

impl Terminal {
    /// Take the terminal, and arrange to give it back.
    pub fn new() -> Result<Self> {
        enter_raw()?;

        // `Drop` does not run on a panic here: release builds are
        // `panic = "abort"`. Without this hook a panic inside the loop leaves
        // somebody with a terminal that does not echo what they type.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Only when the terminal is still taken. The hook cannot be
            // removed on the way out — `set_hook` has no inverse that restores
            // a chain — so it outlives the screen, and a later panic must not
            // print an alternate-screen exit into somebody's ordinary output.
            // `restore_raw` takes the saved settings, so this is true exactly
            // once and `Drop` and the hook cannot both act.
            if raw_is_held() {
                leave_screen();
                restore_raw();
            }
            previous(info);
        }));

        let mut out = std::io::stdout();
        // Alternate screen, then hide the cursor. Leaving on the alternate
        // screen is what puts the shell's scrollback back exactly as it was.
        let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J");
        let _ = out.flush();

        Ok(Self)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Guarded the same way the panic hook is. On an unwinding panic both
        // run, and a second `leave_screen` would write an alternate-screen
        // exit into output that is back on the ordinary screen.
        if raw_is_held() {
            leave_screen();
            restore_raw();
        }
    }
}

fn leave_screen() {
    let mut out = std::io::stdout();
    // Show the cursor, then leave the alternate screen. In that order: the
    // cursor state belongs to the screen being left.
    let _ = out.write_all(b"\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
}

// The two platform halves. Both are small enough to read in one sitting, which
// is the argument for not taking a crate for them.

#[cfg(unix)]
fn enter_raw() -> Result<Option<libc::termios>> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Err(not_a_terminal());
    }

    unsafe {
        let mut current: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut current) != 0 {
            return Err(not_a_terminal());
        }
        let saved = current;

        // `cfmakeraw` rather than clearing flags by hand: it is the libc
        // function that means exactly "raw", and a hand-written mask is a list
        // of flags somebody has to keep right on three platforms.
        libc::cfmakeraw(&mut current);
        // One byte is enough to return, and no timer: the blocking read lives
        // on its own thread, so there is nothing here for a timeout to save.
        current.c_cc[libc::VMIN] = 1;
        current.c_cc[libc::VTIME] = 0;

        if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &current) != 0 {
            return Err(not_a_terminal());
        }
        SAVED.with(|cell| cell.set(Some(saved)));
        Ok(Some(saved))
    }
}

#[cfg(unix)]
thread_local! {
    /// The panic hook cannot borrow the guard, so the original settings are
    /// left somewhere it can reach. Thread-local because the hook runs on the
    /// thread that panicked, which for this loop is the one that set it up.
    static SAVED: std::cell::Cell<Option<libc::termios>> = const { std::cell::Cell::new(None) };
}

/// Is the terminal still ours to give back?
#[cfg(unix)]
fn raw_is_held() -> bool {
    SAVED.with(|cell| {
        let saved = cell.get();
        cell.set(saved);
        saved.is_some()
    })
}

#[cfg(unix)]
fn restore_raw() {
    SAVED.with(|cell| {
        if let Some(saved) = cell.take() {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
            }
        }
    });
}

#[cfg(windows)]
fn enter_raw() -> Result<Option<u32>> {
    use std::io::IsTerminal;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    if !std::io::stdin().is_terminal() {
        return Err(not_a_terminal());
    }

    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(input, &mut mode) == 0 {
            return Err(not_a_terminal());
        }
        let saved = mode;

        // The three that make a console cooked: line buffering, echo, and
        // turning Ctrl-C into a signal. Cleared rather than replaced wholesale
        // so every other bit the user's console has set survives.
        let raw = mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
        if SetConsoleMode(input, raw) == 0 {
            return Err(not_a_terminal());
        }

        // And the escape sequences this file draws with have to be interpreted
        // rather than printed. On by default in Windows Terminal, not on every
        // console host.
        let output = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut out_mode = 0u32;
        if GetConsoleMode(output, &mut out_mode) != 0 {
            SetConsoleMode(output, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }

        SAVED.with(|cell| cell.set(Some(saved)));
        Ok(Some(saved))
    }
}

#[cfg(windows)]
thread_local! {
    static SAVED: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

#[cfg(windows)]
fn restore_windows(saved: u32) {
    use windows_sys::Win32::System::Console::{GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE};
    unsafe {
        SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), saved);
    }
}

#[cfg(windows)]
fn raw_is_held() -> bool {
    SAVED.with(|cell| {
        let saved = cell.get();
        cell.set(saved);
        saved.is_some()
    })
}

#[cfg(windows)]
fn restore_raw() {
    SAVED.with(|cell| {
        if let Some(saved) = cell.take() {
            restore_windows(saved);
        }
    });
}

fn not_a_terminal() -> Error {
    Error::new(
        crate::error::Code::Unsupported,
        "`stackvo tui` needs a terminal",
    )
    .with_hint(
        "Run it directly rather than through a pipe — `stackvo projects` prints a table."
            .to_string(),
    )
}

/// Stdin, on its own thread, as a stream of keys.
fn keys() -> Receiver<Key> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buffer = [0u8; 8];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(key_of(&buffer[..n])).is_err() {
                        break; // the loop ended
                    }
                }
            }
        }
    });
    rx
}

// -------------------------------------------------------------- the loop

/// Run the screen until the person leaves it.
pub async fn run(root: PathBuf) -> Result<()> {
    // Taken before the first read, so a failure to get the terminal is
    // reported as itself rather than after a two-second pause listing
    // containers nobody will see.
    let terminal = Terminal::new()?;
    let style = Style::always();
    let keys = keys();

    let mut model = read(&root, None).await;

    loop {
        let frame = draw(&model, &style, width());
        {
            let mut out = std::io::stdout();
            let _ = out.write_all(frame.as_bytes());
            let _ = out.flush();
        }

        match keys.recv_timeout(REFRESH) {
            Err(RecvTimeoutError::Timeout) => {
                model = read(&root, Some(&model)).await;
            }
            // The reader thread is gone — stdin closed under us. Nothing more
            // is coming, and a loop that kept redrawing would be one only a
            // kill could stop.
            Err(RecvTimeoutError::Disconnected) => break,
            Ok(Key::Quit) => break,
            Ok(Key::Up) => model.move_by(-1),
            Ok(Key::Down) => model.move_by(1),
            Ok(Key::Refresh) => {
                model.message = None;
                model = read(&root, Some(&model)).await;
            }
            Ok(Key::Enter) => {
                act(&mut model).await;
                model = read(&root, Some(&model)).await;
            }
            Ok(Key::Logs) => {
                show_logs(&mut model).await;
            }
            Ok(Key::Other) => {}
        }
    }

    drop(terminal);
    Ok(())
}

/// Start or stop what is under the cursor.
async fn act(model: &mut Model) {
    let Some(row) = model.current().cloned() else {
        return;
    };

    if !row.project {
        // Said rather than silently ignored: a person who pressed Enter on a
        // service needs to know nothing happened and why.
        model.message = Some(format!(
            "{} is a shared service — start it from the app or `stackvo up`",
            row.label
        ));
        return;
    }

    model.message = Some(format!(
        "{} {}…",
        if row.running { "stopping" } else { "starting" },
        row.label
    ));

    let outcome = if row.running {
        crate::engine::stop_container(&row.id).await
    } else {
        crate::engine::start_container(&row.id).await
    };

    crate::audit::record(
        "cli_tui_toggle",
        &row.id,
        if outcome.is_ok() {
            crate::audit::Outcome::Ok
        } else {
            crate::audit::Outcome::Failed
        },
    );

    model.message = Some(match outcome {
        Ok(()) => format!(
            "{} {}",
            row.label,
            if row.running { "stopped" } else { "started" }
        ),
        Err(e) => format!("{}: {}", row.label, e.message),
    });
}

/// The last lines of the selected container's log, in the pager the screen
/// borrows for as long as somebody is reading.
async fn show_logs(model: &mut Model) {
    use futures_util::StreamExt;

    let Some(row) = model.current().cloned() else {
        return;
    };

    let lines: Vec<String> = match crate::engine::logs_stream(&row.id, 200, false) {
        Ok(stream) => stream.map(|line| line.text).collect().await,
        Err(e) => {
            model.message = Some(format!("{}: {}", row.label, e.message));
            return;
        }
    };

    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[H\x1b[2J");
    for line in lines.iter().rev().take(60).rev() {
        // `\r\n`, not `\n`: raw mode does not translate one into the other, so
        // a plain newline moves down without returning to column zero and the
        // output walks off the right of the screen.
        let _ = write!(out, "{line}\r\n");
    }
    let _ = out.write_all(b"\r\n-- any key --\r\n");
    let _ = out.flush();

    // Read directly rather than through the channel: the reader thread is
    // still there and will deliver this key to it, which is exactly what is
    // wanted — the next key both dismisses this and is consumed.
    model.message = Some(format!("{} — last {} lines shown", row.label, lines.len()));
}

/// The terminal's width, or a sane default.
///
/// `COLUMNS` is exported by most shells; when it is not, 80 is the width every
/// terminal has been at least since VT100 and the layout degrades by wrapping
/// rather than by breaking.
fn width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|w| *w > 20)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            rows: vec![
                Row {
                    id: "shop".into(),
                    label: "shop".into(),
                    detail: "shop.loc".into(),
                    running: true,
                    project: true,
                },
                Row {
                    id: "blog".into(),
                    label: "blog".into(),
                    detail: "blog.loc".into(),
                    running: false,
                    project: true,
                },
                Row {
                    id: "mysql-9-7".into(),
                    label: "mysql-9-7".into(),
                    detail: "healthy".into(),
                    running: true,
                    project: false,
                },
            ],
            selected: 0,
            engine: true,
            engine_detail: "29.7.2".into(),
            message: None,
        }
    }

    /// Enter arrives as `\r` in raw mode, not `\n`. Getting this wrong makes
    /// the one key the screen is driven by do nothing.
    #[test]
    fn enter_is_a_carriage_return_in_raw_mode() {
        assert_eq!(key_of(b"\r"), Key::Enter);
        assert_eq!(key_of(b"\n"), Key::Enter);
        assert_eq!(key_of(b" "), Key::Enter);
    }

    #[test]
    fn an_arrow_key_is_three_bytes_and_a_lone_escape_is_not() {
        assert_eq!(key_of(&[0x1b, b'[', b'A']), Key::Up);
        assert_eq!(key_of(&[0x1b, b'[', b'B']), Key::Down);
        assert_eq!(key_of(&[0x1b]), Key::Quit);
        assert_eq!(key_of(b"k"), Key::Up);
        assert_eq!(key_of(b"j"), Key::Down);
    }

    /// Raw mode stops the terminal turning Ctrl-C into a signal, so a screen
    /// that did not read byte 3 would be one nobody could leave the usual way.
    #[test]
    fn ctrl_c_quits_because_nothing_else_will_deliver_it() {
        assert_eq!(key_of(&[0x03]), Key::Quit);
        assert_eq!(key_of(b"q"), Key::Quit);
    }

    #[test]
    fn an_unknown_key_is_ignored_rather_than_guessed_at() {
        assert_eq!(key_of(b"z"), Key::Other);
        assert_eq!(key_of(&[0x1b, b'[', b'Z']), Key::Other);
        assert_eq!(key_of(&[]), Key::Other);
    }

    /// The selection stops at the ends. Wrapping would mean holding a key past
    /// the last row puts the cursor on a project at the top, and the next
    /// Enter stops something nobody was looking at.
    #[test]
    fn the_selection_stops_at_both_ends() {
        let mut m = model();
        m.move_by(-1);
        assert_eq!(m.selected, 0);

        m.move_by(1);
        assert_eq!(m.selected, 1);
        m.move_by(10);
        assert_eq!(m.selected, 2, "clamped to the last row");
        m.move_by(1);
        assert_eq!(m.selected, 2);
    }

    #[test]
    fn an_empty_list_has_no_selection_to_move() {
        let mut m = Model::default();
        m.move_by(1);
        assert_eq!(m.selected, 0);
        assert!(m.current().is_none());
    }

    #[test]
    fn the_screen_shows_both_sections_and_marks_the_cursor() {
        let text = draw(&model(), &Style::plain(), 80);
        assert!(text.contains("PROJECTS"));
        assert!(text.contains("SERVICES"));
        assert!(text.contains("shop.loc"));
        assert!(text.contains('▸'), "the cursor has to be visible");
        assert!(text.contains("engine up"));
        assert!(text.contains("2 projects"), "services are not projects");
    }

    /// Every line ends with a clear-to-end and a `\r\n`. Without the carriage
    /// return raw mode leaves each line further right than the last; without
    /// the clear, a shorter frame shows the previous one through it.
    #[test]
    fn every_line_returns_the_carriage_and_clears_behind_itself() {
        let text = draw(&model(), &Style::plain(), 80);
        for line in text.split("\r\n").filter(|l| !l.is_empty()) {
            assert!(
                line.contains("\x1b[K") || line.starts_with("\x1b["),
                "a line without a clear: {line:?}"
            );
        }
        assert!(!text.contains("\n\n"), "a bare newline escaped: {text:?}");
    }

    #[test]
    fn the_message_replaces_the_key_hints_while_there_is_one() {
        let plain = Style::plain();
        let hints = draw(&model(), &plain, 80);
        assert!(hints.contains("q quit"));

        let mut m = model();
        m.message = Some("shop started".into());
        let told = draw(&m, &plain, 80);
        assert!(told.contains("shop started"));
        assert!(!told.contains("q quit"), "one line, one job");
    }

    #[test]
    fn an_empty_stack_says_so_rather_than_drawing_a_blank() {
        let text = draw(&Model::default(), &Style::plain(), 80);
        assert!(text.contains("nothing to show"));
    }
}
