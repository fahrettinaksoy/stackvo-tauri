# ADR 0002 — Generated files are rendered, never edited

- **Status:** accepted
- **Date:** 2026-08 (recorded)

## Context

The stack is described by files Docker reads: `docker-compose.yml`, a
per-project `Dockerfile`, the proxy's routing rules, `/etc/hosts` entries. Every
one of them can be produced from two inputs — the workspace `.env` and each
project's `stackvo.json`.

The predecessor edited them. Enabling a service meant finding the right block in
the compose file and rewriting it in place; adding a PHP extension meant
inserting a line into a `Dockerfile`. The failure mode is not that this is
inelegant. It is that the file drifts: a hand edit, a half-finished operation or
a crash leaves a file that no input explains, and no amount of re-running the
tool converges it, because the tool is reading the same broken file it is
trying to fix.

## Decision

Everything under `generated/` and every generated per-project file is
**rendered whole from the manifest and the `.env`**, every time. Nothing is
patched. `generated/` can be deleted at any moment and rebuilt.

The only file a user is meant to edit is `stackvo.json`, and it has a schema
with `additionalProperties: false` — so a setting that is not in the schema
cannot be smuggled in as a manifest key.

## Consequences

**Good.** There is exactly one way for the stack's files to be wrong: the
manifest is wrong. Recovery is `rm -rf generated/` plus a regenerate.

**The migration this created.** The Bash generator that shipped first still
writes these files on existing installations, and the Rust port must produce
byte-comparable output before it can replace it. Fixtures alone cannot prove
that — they cover the cases somebody thought to write down — so
`generator_verify` runs the comparison against the _user's_ real projects and
real `.env`, and `project_dockerfile_preview` shows the two renderings
side by side with the differences named.

Two modes, because the two questions are different: `compat` reproduces what
Bash writes today, including the extensions it silently drops; `strict` refuses
where Bash drops. The comparison chip under the preview means something
different depending on which mode produced it, which is why the mode is state
and not two unlabelled buttons.

**A rule that follows.** A user's hand edit to a generated file is lost on the
next render, and that is correct rather than regrettable — but it means the
files must be recognisable as generated. They carry a header saying so.
