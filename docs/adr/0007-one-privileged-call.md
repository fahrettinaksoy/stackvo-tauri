# ADR 0007 — Exactly one privileged call

- **Status:** accepted
- **Date:** 2026-08 (recorded)
- **Source:** the module comment at the head of `src-tauri/src/elevate.rs`

## Context

Two things in a local development stack want administrator rights: writing
`/etc/hosts`, and installing a certificate authority into the system trust
store.

The second one taught the rule. `mkcert -install` shells out to

```text
sudo --prompt=Sudo password: -- security add-trusted-cert …
```

and `sudo` reads the password from a terminal. A windowed app has no terminal,
so the prompt goes nowhere and the process waits — forever, with no output, no
error, and nothing on screen. The first-run screen sat on _"Issuing the
certificate"_ until it was killed.

A failure would have been fine: there is a retry button and an error area.
**Hanging is the one outcome nothing recovers from.**

## Decision

**A windowed app must never let a child process ask for a password.**

Elevation happens in exactly one module, `elevate.rs`, through the mechanism the
platform gives a windowed app: `osascript`'s `with administrator privileges`,
which puts up the standard authentication panel. Exactly one operation uses it —
replacing `/etc/hosts`.

Two supporting rules:

- **Every helper this app spawns gets its stdin closed**, so one that decides to
  prompt anyway fails instead of stopping.
- **The certificate authority is not a caller.** Root through an AppleScript was
  tried and refused — `SecTrustSettingsSetTrustSettings: the authorization was
denied since no user interaction was possible` — because writing the admin
  trust domain needs the Security framework's own confirmation, which cannot
  happen inside a non-interactive `osascript`. So the CA is installed by asking
  the user to run one command in their own terminal, where the prompt has
  somewhere to appear.

## Consequences

**Good.** One place to audit, one place to test, one prompt a user can be
warned about before it appears.

**The argv rule this needed.** `osascript … with administrator privileges` takes
a _shell string_, so the arguments have to be joined — and a naive join is a
command injection waiting for a path with a space in it. `elevate::run` takes
`&[&str]` and does the quoting itself; `JOIN_ARGV` is the tested seam.

**What it costs the user.** Trusting the CA is a manual step with a copyable
command rather than a button. That is worse to explain and better than a screen
that hangs.
