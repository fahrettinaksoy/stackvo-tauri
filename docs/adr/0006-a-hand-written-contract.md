# ADR 0006 — The IPC contract is written, not generated

- **Status:** accepted, with a known successor
- **Date:** 2026-08 (recorded)
- **Superseded by:** nothing yet; `tauri-specta` is the candidate (§14.10)

## Context

The front end is JavaScript and the back end is Rust. Nothing in the toolchain
joins them: a command's argument names, its return shape and its error codes
exist twice, and the compiler can see only one copy.

`contracts/ipc.json` is the other copy — 148 commands, 59 events, 58 named
types, and for most entries a `why` explaining what the command exists for and
what it replaced. It is the document the front-end client is written against,
the readiness review is argued from, and `tests/ipc.spec.js` checks the
wrappers against.

`tauri-specta` would generate TypeScript types from the Rust and make drift
impossible.

## Decision

Keep the hand-written contract for now, and **make drift loud instead of
impossible**.

`tauri-specta` was measured and deferred: it changes how every one of the 144
commands is declared, and doing that in the middle of other work would make
every other change unreviewable. It belongs on its own branch.

In the meantime, `src-tauri/tests/contract_agreement.rs` fails the build when
the three descriptions of the boundary stop agreeing:

- the contract file — what the boundary is documented to be;
- `#[tauri::command]` in `src/*.rs` — what is implemented;
- `generate_handler!` in `lib.rs` — what is reachable.

In both directions, including the quietest one: a command that is implemented
_and_ documented but never registered answers `command not found` as a bare
string at runtime, and no build, lint or other test sees it.

## Consequences

**Good.** The contract carries something a generator cannot: `why`. Roughly
every command explains what it replaced and what was wrong with it, and that is
the closest thing this repository has to a design history.

**The cost.** Types are still written twice. A field added to a Rust struct and
not to the contract is invisible until something reads it — which is exactly how
`hint_key` was lost across the boundary (ADR
[0004](0004-errors-are-codes-not-strings.md)). `contract_agreement.rs` checks
the _set of commands_, not their shapes; the shapes are still on trust.

**Exclusions are read, not hardcoded.** Three commands are `frontend-plugin`
(served by a Tauri plugin from the JavaScript side, with no Rust function by
design) and one is `deferred`. The test reads both facts from the contract, so
adding a fourth plugin command needs no edit — and un-deferring `updates_check`
correctly starts demanding an implementation.
