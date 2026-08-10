# ADR 0008 — What a breaking contract change is

- **Status:** accepted
- **Date:** 2026-08 (recorded)

## Context

`contracts/ipc.json` has carried a `contractVersion` since the first commit,
and nothing anywhere said what would change it. The readiness review named this
exactly: the field exists, and _what counts as major is undefined_.

That is worse than having no version. A number that moves when somebody
remembers cannot be read backwards — a consumer looking at `1.0.0` and `1.0.0`
across two builds learns nothing, because they would have looked the same if
the whole surface had been rewritten. And this contract has real consumers
beyond this repository: the StackVo CLI writes the same `stackvo.json` and
`.env`, `stackvo-mcp` exposes seventeen of these commands to assistants, and
the front end is written against all of it.

ADR [0006](0006-a-hand-written-contract.md) chose to keep the contract
hand-written and make drift **loud instead of impossible**. This is the same
choice one level up: the version stays a number a human writes, and something
else fails the build when the number and the diff disagree.

## Decision

**The version describes what a caller would notice, and nothing else.**

| Change                                                   | Version   |
| -------------------------------------------------------- | --------- |
| A command, event or type is removed or renamed           | **major** |
| A command's `kind` or `returns` changes                  | **major** |
| An argument is removed, renamed, or its type changes     | **major** |
| A **required** argument is added                         | **major** |
| A command stops emitting an event it declared            | **major** |
| A field is removed from an event payload or a named type | **major** |
| A command's `status` goes to `deferred`                  | **major** |
| A command, event or type is added                        | **minor** |
| An **optional** argument is added                        | **minor** |
| A field is added to an event payload or a named type     | **minor** |
| `deferred` becomes answerable                            | **minor** |
| Anything else — `why`, `notes`, a clearer description    | unchanged |

Two rules decide the rest:

**Prose is not surface.** `why` and `notes` are the reason this contract is
worth reading, and rewriting one must never demand a version. The same goes for
a `note` key inside a type: it is documentation that happens to live in an
object.

**Optionality is judged conservatively.** Argument types here are written for a
human — `string?`, `u32 (default 200)`, `string[]? (all when omitted)` — so
"can the caller leave this out" is a heuristic, and it errs towards _required_.
The cost of being wrong that way is a major version nobody needed. The cost of
being wrong the other way is a break shipped as a minor, which is the thing the
version exists to prevent.

### The mechanism

`contracts/surface.lock.json` records the **last released** call surface and
the version it went out as. `src-tauri/tests/contract_version.rs` compares the
working contract against it on every `cargo test`, classifies the difference by
the table above, and fails when `contractVersion` is lower than the change
demands — naming the specific commands, arguments or fields that ask for it.

The lock is refreshed at a release and at no other time:

```text
UPDATE_CONTRACT_LOCK=1 cargo test --test contract_version
```

Refreshing it whenever it disagrees is the one way this gate can be defeated,
which is why the point of it is written down here rather than in a comment: the
lock is the memory of what other people were promised, and a memory rewritten
on every objection is not one.

## Consequences

**Good.** The number is now derivable. Anyone can reconstruct it from the diff,
which means it can be read backwards: `1.x` to `2.0` says a client has to
change, and the failure message says which line of it.

It also closed the half of ADR 0006 that was on trust. `contract_agreement.rs`
checks the _set_ of commands against the code; the shapes were checked by
nobody, so a field dropped from `Project` changed no command's `returns` and
nothing complained — the front end just read `undefined` and rendered a blank
cell. Named types are now compared field by field against the lock.

**The cost — the surface is compared to a document, not to the code.** This is
a contract-versus-contract check. A field removed from a Rust struct without
being removed from the contract is invisible here, exactly as it is to every
other gate in this repository. `tauri-specta` (ADR 0006's known successor) is
what removes that gap; this narrows it and does not close it.

**The second cost — the lock is 55 KB of generated JSON in review.** A contract
change now shows up twice, once as intent and once as the recorded baseline
when a release refreshes it. That is the price of the baseline existing at all,
and it is paid only at releases.

**The heuristic will be wrong sometimes.** An argument described as
`'keep' | 'dangling' | 'all' (default 'keep')` is read as optional because it
says "default"; one described as `bool — false unless the caller says so` is
read as required, and a major version will be asked for where a minor would
have done. The direction is deliberate, and the way out is to write the type so
a reader can see it too.
