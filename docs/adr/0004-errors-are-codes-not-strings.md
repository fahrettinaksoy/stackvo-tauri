# ADR 0004 — Errors are codes with catalogued hints

- **Status:** accepted
- **Date:** 2026-08 (recorded)

## Context

The predecessor answered HTTP 200 with `{ success: false, message: "..." }`, and
the client's interceptor unwrapped `.data`. A failure therefore looked like a
success until something remembered to read `.success` — and the message was an
English sentence, so the only way to branch on _what_ went wrong was to match
its text.

Two states the web UI could never report, because it had no way to name them:
Docker not running, and the workspace not chosen. Both are the ordinary case for
this app, not the exceptional one.

## Decision

One shape, everywhere:

```rust
StackvoError { code, message, hint: Option<String>, hint_key: Option<String>, details }
```

- `code` is what a caller branches on. `Ok(T)` is the payload directly; `Err`
  rejects the promise. There is no envelope.
- `message` is for a human to read.
- `hint` is the English sentence saying what to do about it.
- `hint_key` names an entry in `src-tauri/src/hints.rs`, so the front end can
  render a _translated_ hint — while the log, the crash report and the MCP
  surface still get the English.

## Consequences

**Good.** `StackvoError` has named shortcuts for the two states above, and the
front end has a gate for each. A hint is a catalogue entry rather than a string
built at the call site, so it can be translated.

**What keeps it honest.** `src-tauri/tests/hint_translations.rs` fails the build
if a catalogued hint has no translation in both `en.js` and `tr.js`. That test
has broken twice on its own reader rather than on a missing translation —
Prettier wraps a long value onto its own line, and switches to double quotes for
a value containing an apostrophe — and both fixes are in the test.

**A defect this exposed.** `hint_key` was added to the Rust struct and to the
contract, and the front end's `StackvoError` silently dropped it: the field
crossed the boundary and was discarded in the constructor. Nothing failed. It
was found by a test that asserts the field survives `call`, which is now
`tests/ipc.spec.js`.

**The remaining gap.** Some hints are still built at the call site rather than
catalogued, so they are English-only. `hint_key` is `None` for those, and the
front end falls back to `hint`, which is correct but untranslated.
