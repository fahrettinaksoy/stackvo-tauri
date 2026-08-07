# ADR 0003 — One operation per subject, enforced in the back end

- **Status:** accepted
- **Date:** 2026-08 (recorded)
- **Source:** the module comment at the head of `src-tauri/src/inflight.rs`

## Context

The front end tracks a busy flag per project, and that flag is one view's idea
of what is happening. The tray menu, a second window and a keyboard shortcut all
reach the same commands, and none of them can see the others' flag.

Two `docker compose up` runs against one project, or a stop racing a restart,
produce failures that read as Docker being flaky. They are not: they are two
callers the UI never told about each other.

## Decision

The back end holds the truth. `AppState::inflight` is a registry of subjects
with an operation in progress; a command acquires its subject and holds it for
the life of the call.

**Two problems, two different answers**, and conflating them would be wrong:

- A user-initiated operation on a busy subject is a _mistake_ — a double click,
  a stale button. It **fails immediately** and says so. Queueing it would
  surprise somebody a minute later with an action they have forgotten
  requesting.
- Generation is an internal step of many operations, and it writes shared files
  (`docker-compose.projects.yml`, everything under `generated/`). Two builds
  must not both write them, but failing one because the other happened to
  regenerate at that instant would be wrong. Those **queue** — that is
  `AppState::generate_lock`, a different mechanism on purpose.

## Consequences

**Good.** The guarantee holds across every surface, including ones added later:
the MCP server got it without asking for it.

**The failure it introduces.** A user who clicks _Restart_ while a build is
running gets a refusal rather than a queued restart. That is the intended
behaviour and it has to be _said_ — a bare "operation in progress" with no
subject reads as a bug. The error carries a hint (ADR
[0004](0004-errors-are-codes-not-strings.md)) naming what is busy.

**Held for the life of the command**, not for the life of the Docker operation.
A command that returns an `OperationId` and lets the work continue in the
background (ADR [0005](0005-progress-through-a-sink.md)) has to keep the guard
alive with the work, not with the call.
