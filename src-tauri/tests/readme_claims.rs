//! The README, held to the same standard as the code.
//!
//! This project's own thesis is that "probably the same" is not a shipping
//! standard: `contracts/ipc.json` is checked against `lib.rs` and `commands.rs`,
//! `mcp.rs` cross-checks every tool against the command it names, and the Bash
//! generator was replaced against byte-for-byte fixtures. The README was the one
//! surface outside that culture — and by the time anyone measured it, both of
//! its measurable claims were wrong:
//!
//!   * "Thirty-four commands take an `AppHandle`" — the real count was 48.
//!   * "Two tools change things" — `--allow-writes` actually unlocks 7,
//!     including `stack_down`, which stops the entire stack.
//!
//! The second one is why this file exists rather than a commit that fixes the
//! numbers. A reader deciding whether to hand an assistant `--allow-writes` was
//! being told the flag unlocked Xdebug and a certificate reissue. That is a
//! security documentation defect, and the defence against it recurring is not
//! care — it is this test.
//!
//! ## Why the numbers are counted rather than generated
//!
//! `mcp.rs` already argued that generating the tool *list* is the wrong move:
//! dispatch cannot be generated, so a generated list advertises tools that fail
//! when called. The same holds a step further out. What is checkable is that the
//! prose agrees with the code, so that is what is checked — the README stays
//! hand-written, and a number in it that stops being true fails the build.

/// How the `AppHandle` count is arrived at, so a future reader does not have to
/// reverse-engineer the regex-free scan below.
///
/// A command is a `#[tauri::command]` (or `#[tauri::command(async)]`) attribute
/// followed by a function whose argument list mentions `AppHandle`. Attributes
/// inside a `#[cfg(test)]` module do not count: three of them exist purely to
/// exercise the registration machinery and are not part of the shipped surface.
mod scan {
    /// The byte ranges of every top-level `#[cfg(test)]` item.
    ///
    /// **Found by indentation, not by counting braces.** Brace counting is the
    /// obvious approach and it is wrong here, which this file learned the
    /// expensive way: a test that writes deliberately truncated JSON —
    /// `"{\"theme\": \"dark\", trunca"` — puts an unmatched `{` inside a string
    /// literal, the count never returns to zero, the last test module stops
    /// being recognised as a test module, and three test-only commands start
    /// counting as shipped surface. The scan reported 146 commands instead of
    /// 143 and the guard below caught it, which is the only reason this comment
    /// exists rather than a wrong number in the README.
    ///
    /// Skipping string and char literals properly would mean writing a Rust
    /// lexer. The cheaper invariant is already enforced by CI: `cargo fmt
    /// --check` runs on every push, and rustfmt closes a top-level item with a
    /// `}` in column zero. Nothing inside a literal can look like that, because
    /// rustfmt indents every line it owns.
    fn test_regions(src: &str) -> Vec<(usize, usize)> {
        let mut regions = Vec::new();
        let mut from = 0;

        // Anchored to the start of a line: a `#[cfg(test)]` on an inner item is
        // indented, and only the top-level ones enclose whole commands.
        while let Some(offset) = src[from..].find("\n#[cfg(test)]") {
            let start = from + offset + 1;

            // The first column-zero `}` after it closes the item.
            let end = match src[start..].find("\n}") {
                Some(i) => start + i + 2,
                // Unterminated: treat the rest of the file as inside, which is
                // the safe direction — it can only *exclude* commands, and the
                // total assertion below turns that into a visible failure.
                None => src.len(),
            };

            regions.push((start, end));
            from = end;
        }
        regions
    }

    /// The balanced argument list of the first `fn` after `at`.
    fn argument_list(src: &str, at: usize) -> Option<&str> {
        let rest = &src[at..];
        let f = rest.find(" fn ")?;
        let open = at + f + rest[f..].find('(')?;

        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open..=open + i]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The name of every shipped command, for the registration cross-check.
    pub fn command_names(src: &str) -> Vec<String> {
        let regions = test_regions(src);
        let shipped = |p: usize| !regions.iter().any(|&(s, e)| (s..=e).contains(&p));

        let mut names = Vec::new();
        let mut from = 0;
        while let Some(offset) = src[from..].find("#[tauri::command") {
            let at = from + offset;
            from = at + 1;
            if !shipped(at) {
                continue;
            }
            if let Some(name) = function_name(src, at) {
                names.push(name);
            }
        }
        names
    }

    /// The identifier after the first ` fn ` following `at`.
    fn function_name(src: &str, at: usize) -> Option<String> {
        let rest = &src[at..];
        let f = rest.find(" fn ")? + " fn ".len();
        let tail = &rest[f..];
        let end = tail.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
        Some(tail[..end].to_string())
    }

    /// `(commands, commands taking an AppHandle)` in shipped code.
    pub fn commands(src: &str) -> (usize, usize) {
        let regions = test_regions(src);
        let shipped = |p: usize| !regions.iter().any(|&(s, e)| (s..=e).contains(&p));

        let (mut total, mut with_handle) = (0, 0);
        let mut from = 0;
        while let Some(offset) = src[from..].find("#[tauri::command") {
            let at = from + offset;
            from = at + 1;
            if !shipped(at) {
                continue;
            }
            total += 1;
            if argument_list(src, at).is_some_and(|args| args.contains("AppHandle")) {
                with_handle += 1;
            }
        }
        (total, with_handle)
    }
}

fn readme() -> String {
    // `CARGO_MANIFEST_DIR` is `src-tauri`; the README is one level up. Read
    // rather than `include_str!` so a failure says "the README moved" instead of
    // failing to compile.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("a repository root above src-tauri")
        .join("README.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Every `commands::<name>,` in `lib.rs`'s `generate_handler!` list.
///
/// This is the *registration*, as opposed to the implementation the scanner
/// above finds. Tauri only routes what is named here, and a command written in
/// `commands.rs` and left out of this list **compiles and silently does
/// nothing** — the frontend gets "command not found" at runtime, on a screen
/// nobody opened during development.
fn registered() -> Vec<String> {
    let source = include_str!("../src/lib.rs");
    let start = source
        .find("generate_handler!")
        .expect("lib.rs registers its commands with generate_handler!");
    let body = &source[start..];
    let end = body.find("])").unwrap_or(body.len());

    body[..end]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("commands::"))
        .filter_map(|name| name.split(',').next())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Registration and implementation agree — checked here, in Rust, with nothing
/// fetched from anywhere.
///
/// This is what suite E in `tools/validate-contracts.mjs` does, and that job
/// **checks out an external repository to run**. The readiness review flagged
/// the dependency as a risk rather than a defect, and it is: the day
/// `stackvo/stackvo` goes private, is renamed or rate-limits, the contract gate
/// disappears and nothing says so. This half of it needs no network, no
/// checkout and no Node, so it keeps working when that job cannot.
///
/// It also replaces a hardcoded `143` that used to sit below. That number was a
/// sanity check on the scanner and it made every new command look like a
/// scanner fault — noise, where this is signal.
#[test]
fn every_implemented_command_is_registered_and_the_reverse() {
    use std::collections::BTreeSet;

    let source = include_str!("../src/commands.rs");
    let implemented: BTreeSet<String> = scan::command_names(source).into_iter().collect();
    let registered: BTreeSet<String> = registered().into_iter().collect();

    let unregistered: Vec<&String> = implemented.difference(&registered).collect();
    assert!(
        unregistered.is_empty(),
        "{} command(s) are implemented but never registered in lib.rs: {unregistered:?}\n\
         These compile, and fail at runtime with \"command not found\".",
        unregistered.len()
    );

    let unimplemented: Vec<&String> = registered.difference(&implemented).collect();
    assert!(
        unimplemented.is_empty(),
        "{} command(s) are registered in lib.rs but not implemented: {unimplemented:?}",
        unimplemented.len()
    );

    assert!(
        implemented.len() > 100,
        "the scan found only {} commands, so it is not reading the file it thinks it is",
        implemented.len()
    );
}

/// The claim that was wrong by 14, in the paragraph explaining why MCP cannot
/// reach the rest of the command surface.
#[test]
fn the_readme_states_the_real_app_handle_count() {
    let source = include_str!("../src/commands.rs");
    let (total, with_handle) = scan::commands(source);

    // The scanner is trusted because the test above proved it agrees with the
    // registration list, not because of a number written down here.
    let claim = format!("{with_handle} of the {total} commands take");
    assert!(
        readme().contains(&claim),
        "the README should say {claim:?} — it is now the measured number"
    );
}

/// The claim that mattered: what `--allow-writes` actually hands over.
#[test]
fn the_readme_states_every_tool_allow_writes_unlocks() {
    use stackvo_desktop_lib::mcp;

    let readme = readme();
    let writers: Vec<&str> = mcp::TOOLS
        .iter()
        .filter(|t| t.writes)
        .map(|t| t.name)
        .collect();

    let claim = format!(
        "{} of the {} tools change things",
        writers.len(),
        mcp::TOOLS.len()
    );
    assert!(readme.contains(&claim), "the README should say {claim:?}");

    // Naming them is the part that has security value. A count alone would have
    // gone from "two" to "seven" and still not told anyone that `stack_down` is
    // in the set.
    for tool in writers {
        // The README drops the `stackvo_` prefix every tool carries, because the
        // prose is already about StackVo's tools.
        let short = tool.strip_prefix("stackvo_").unwrap_or(tool);
        assert!(
            readme.contains(&format!("`{short}`")),
            "`{short}` is unlocked by --allow-writes but the README does not name it"
        );
    }
}

/// Both numbers came from the same paragraph pair, and the paragraphs are only
/// honest together: a reader who sees "17 tools" needs the same table to have
/// been counted. Cheap, and it catches a tool added without a `writes` flag
/// decision being made at all.
#[test]
fn every_mcp_tool_declares_whether_it_writes() {
    use stackvo_desktop_lib::mcp;

    let readable = mcp::visible(false).count();
    let all = mcp::visible(true).count();

    assert_eq!(all, mcp::TOOLS.len(), "a tool is visible in neither mode");
    assert_eq!(
        all - readable,
        mcp::TOOLS.iter().filter(|t| t.writes).count(),
        "the write gate and the `writes` flag disagree"
    );
}
