# Architectural decision records

One file per decision, numbered so a later one can supersede an earlier one.
That is the property these have and a code comment does not: `elevate.rs`'s
opening paragraph is a decision record in every respect except that it cannot be
found by number, referred to from elsewhere, or written over by a successor.

Most of these were _moved_ rather than written. The reasoning already existed in
the module comments — this repository documents its reasons unusually well — and
what it lacked was a place where a reason is addressable.

## Format

Context → Decision → Consequences. The consequences section is not a formality:
it is where the cost goes, including the cost nobody wanted. An ADR that lists
only benefits is a decision that has not been examined.

`src-tauri/tests/architecture_claims.rs` fails the build if a record here is
missing a status, a decision or a consequences section, or if `ARCHITECTURE.md`
and this directory stop describing the same set.

## The records

| #                                                        | Decision                                            | Status                     |
| -------------------------------------------------------- | --------------------------------------------------- | -------------------------- |
| [0001](0001-tauri-free-domain.md)                        | The domain band knows nothing about Tauri           | accepted                   |
| [0002](0002-generated-files-are-rendered.md)             | Generated files are rendered, never edited          | accepted                   |
| [0003](0003-one-operation-per-subject.md)                | One operation per subject, enforced in the back end | accepted                   |
| [0004](0004-errors-are-codes-not-strings.md)             | Errors are codes with catalogued hints              | accepted                   |
| [0005](0005-progress-through-a-sink.md)                  | Long operations report through a sink               | accepted                   |
| [0006](0006-a-hand-written-contract.md)                  | The IPC contract is written, not generated          | accepted, with a successor |
| [0007](0007-one-privileged-call.md)                      | Exactly one privileged call                         | accepted                   |
| [0008](0008-what-a-breaking-contract-change-is.md)       | What a breaking contract change is                  | accepted                   |
| [0009](0009-a-policy-file-is-not-a-lock.md)              | A policy file is not a lock                         | accepted                   |
| [0010](0010-secrets-move-out-of-env-not-off-the-disk.md) | Secrets move out of `.env`, not off the disk        | accepted                   |

## Writing a new one

Take the next number. If it replaces an existing decision, say so in both files:
`Superseded by 00NN` on the old one, `Supersedes 00NN` on the new. Do not edit
the old record's reasoning — the point of the number is that the earlier
thinking stays readable.
