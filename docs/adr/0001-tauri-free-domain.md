# ADR 0001 — The domain band knows nothing about Tauri

- **Status:** accepted
- **Date:** 2026-08 (recorded; the rule predates the record)
- **Relates to:** [0005](0005-progress-through-a-sink.md)

## Context

This program began as an Express server with a browser UI and became a Tauri
desktop app. The rewrite was an opportunity to put the logic somewhere a test
could reach it, and the first version did not take it: `AppHandle` was threaded
into functions that only wanted it to emit an event, and `State<'_, AppState>`
into functions that only wanted the workspace path.

Both are Tauri types that exist only inside a running application. A function
holding one cannot be called from a unit test, from the `diagnose` example, or
from the MCP surface — so the interesting half of the program was reachable
only by clicking it.

The concrete case that forced the rule: `runner::run_operation` took an
`AppHandle` for two unrelated reasons, the managed lock and the progress sink.
Its event contract — which events fire, in which order, with which payload —
had no test at all, because there was no way to call it.

## Decision

`commands.rs` is the only module that names a Tauri type.

Everything below it takes what it actually needs: a `&Path` rather than the
state that holds one, a `&dyn ProgressSink` rather than the handle that can
emit. A command's job is to unwrap the Tauri-shaped world into plain arguments,
call one domain function, and shape the result back.

## Consequences

**Good.** 538 Rust tests, most of them against real files in a temporary
workspace. `independence.rs` renders a whole workspace from nothing with no app
running. The MCP surface reuses the domain functions directly rather than
re-implementing them.

**The cost, stated plainly.** `commands.rs` is 6,653 lines — a directory of thin
functions rather than a module about a subject. That is the price of putting the
whole Tauri-shaped edge in one place, and it is a real cost: the file is
uncomfortable to navigate and its unit tests sit 6,000 lines below the code they
cover.

Splitting it by subject (`commands/project.rs`, `commands/mail.rs`, …) is the
obvious next move and has not been done, because it touches every command at
once and the discomfort has not yet become a defect.

**What enforces it.** `src-tauri/tests/architecture_claims.rs` fails the build
when a module outside the entry band takes Tauri's managed state. The entry band
itself is exempt by name: `lib.rs` builds the app, `events.rs` is the Tauri-side
implementation of the sink in ADR [0005](0005-progress-through-a-sink.md), and
`menu.rs`/`tray.rs` are windows.

The first version of that test looked for the literal `State<'_, AppState>` and
passed with a deliberately broken module in front of it — there are three
spellings of managed state in this tree. It matches `State<'_,` now.
