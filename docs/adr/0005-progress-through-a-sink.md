# ADR 0005 — Long operations report through a sink

- **Status:** accepted
- **Date:** 2026-08 (recorded)
- **Relates to:** [0001](0001-tauri-free-domain.md), [0003](0003-one-operation-per-subject.md)

## Context

A `docker compose up --build` on a cold cache takes minutes. The predecessor
blocked an HTTP request on it and raised nginx's proxy timeout to 600 seconds to
stop the connection dropping — a number chosen because 300 was not enough.

Under Tauri the same shape is available and just as wrong: a command can hold
the `invoke()` promise for as long as it likes. The window stays responsive, but
the caller learns nothing until it ends, and nothing can be cancelled.

The second problem was testability. `runner::run_operation` emitted its progress
through an `AppHandle`, so its event contract — which events, in which order,
with which payload — could not be exercised without a running app, and had no
test at all.

## Decision

Two rules.

**Anything that can exceed ~2 seconds does not block.** It returns an
`OperationId` immediately and reports through events: a `…:starting` event, a
stream of progress lines, and a terminal `…:finished` or `…:error`. This is
written into `contracts/ipc.json` as a convention, not left to each command.

**Progress goes through a trait, not a handle.**

```rust
pub trait ProgressSink: Send + Sync {
    fn event(&self, name: &str, payload: Value);
}
```

The Tauri `Sink` implements it. `progress::Null` discards. `progress::Recording`
keeps every event, with `names()`, `named()` and `last()` to assert on.

## Consequences

**Good.** `run_operation` now has tests for its event contract, which is the
part of it users actually see. The trait is three lines and cost nothing.

**A pattern for the next thing.** ADR [0006](0006-a-hand-written-contract.md)'s
counterpart problem — Docker itself, reached directly through bollard — has the
same shape and no such seam yet. `ProgressSink` is the small proof that the
larger one (a `Docker` trait, tracked as §14.19) is worth doing.

**What it does not solve.** Cancellation. An operation reports that it is
running; nothing stops it. Closing the window that started it does not stop it
either — which is correct for a build, and arguable for a pull.
