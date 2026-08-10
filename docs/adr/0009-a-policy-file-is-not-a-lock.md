# ADR 0009 — A policy file is not a lock

- **Status:** accepted
- **Date:** 2026-08 (recorded)

## Context

StackVo gets deployed to more than one machine at a time. When it does, some of
its settings stop belonging to the person at the keyboard: the domain suffix
every project is addressed under, the web server the organisation standardises
on, and — the one that stops the app working at all — the registry images come
from, on a network where Docker Hub is not reachable.

Until now there was no way to say any of that. Every setting lived in the
workspace's `.env`, which is a file on the developer's own disk, and the only
distribution mechanism was telling people what to type.

The readiness review asked for "central policy plus a private registry prefix"
and left the shape open. Two things about that shape had to be decided before
any code was worth keeping, and a first attempt at the module was written and
deliberately deleted precisely because they had not been.

## Decision

### 1. It is a co-operation mechanism, not a security boundary — and it says so

The app reads a JSON file that, on a normally-configured machine, the user's own
account can very often write. `STACKVO_POLICY_FILE` redirects it to anywhere at
all, which is the only way a test can reach the layer and is equally available
to the user. Both facts are true and neither is treated as a defect to be
patched.

The alternative was to pretend otherwise: hide the override, read a
root-owned-only path, refuse to run if the permissions look wrong. That would
have bought nothing — a desktop app runs as the user and anything it can read it
can be made to read differently — while implying a guarantee that does not
exist. **Selling a lock with the key taped to it is worse than selling no lock**,
because somebody plans around it.

What the layer actually buys is real and is worth having on its own terms: a
managed default arrives on a hundred machines without anybody typing it, a
locked key stops a well-meaning user from breaking their own stack, and a
Settings pane can say _why_ a field cannot be edited instead of looking broken.
That is the promise, and it is the whole promise. `policy_status`, the contract
entry, the pane and the module comment all state the limitation in those words.

### 2. One JSON file at three paths, not each platform's native store

macOS MDM writes a `.plist` into `/Library/Managed Preferences`. Windows Group
Policy writes registry keys. Reading those natively is two parsers this crate
does not have, for two mechanisms that can both deliver a _file_ as easily as
they can deliver a key.

So: one format, three locations, one parser.

| Platform | Path                                                    |
| -------- | ------------------------------------------------------- |
| macOS    | `/Library/Managed Preferences/com.stackvo.desktop.json` |
| Windows  | `%ProgramData%\StackVo\policy.json`                     |
| Linux    | `/etc/stackvo/policy.json`                              |

Native readers are an obvious next step and are deliberately **not** guessed at
now. Guessing at a `.plist` key layout nobody has deployed produces a reader
that is wrong in a way no test can catch, because there is nothing to test it
against.

### 3. Three rules the file has to obey

**Precedence is embedded default < `.env` < policy.** A setting pushed to a
fleet that a stale `.env` silently overrides is not a policy, it is a
suggestion.

**It cannot lock a key it does not set.** `"locked": ["SSL_ENABLE"]` without a
matching entry in `settings` says "do not change this" without saying _to what_,
which holds each machine at whatever it happened to have — the opposite of what
a managed fleet is for. Such an entry is dropped, and named in `error`.

**A broken policy must not make the app unstartable.** A typo in a file pushed
to every machine cannot mean a fleet that will not open. Parsing failure yields
an empty policy. But the failure is _reported_, never swallowed: a policy that
quietly does nothing is one the administrator who deployed it believes is in
force, and that is the worst of the three outcomes.

### 4. The registry mirror rewrites rendered text, and skips three references

The rewrite happens once, over the finished text of every generated file, in the
one function that renders all of them. Not in the twenty `.tpl` files: those are
the contract with the Bash generator, which knows nothing about a mirror, and
editing them would fail every differential comparison for a reason unrelated to
the port — and would still miss a workspace's own overridden copies.

Three references are left exactly as written, each because rewriting it breaks
something:

- **one that already names a registry** (`ghcr.io/x/y`, `localhost:5000/z`),
  by Docker's own rule — first component containing `.` or `:`, or equal to
  `localhost`. Redirecting a deliberate choice is not what a mirror does.
- **one that already starts with the prefix** — a second render must not be a
  second rewrite; `proxy/proxy/mysql` is a 404.
- **an image named `stackvo-*`** — built on this machine by Compose, present in
  no registry anywhere. A prefix makes it simultaneously unpullable and
  unbuildable.

A fourth case is not an exception but a correctness requirement: `FROM base`
where `base` was declared by an earlier `FROM … AS base` in the same file names
a build stage, not an image. Prefixing it turns a working multi-stage build into
a pull of something that has never existed.

## Consequences

**The layer can be bypassed, and the documentation says so in the same breath
as it describes it.** An organisation that needs an actual boundary needs
device management, not this. Saying that plainly is the cost of the design and
is preferable to the alternative cost, which is somebody believing otherwise.

**Policy is read once per process.** A change needs a restart. This matches
every managed-preferences reader on every platform, and the alternative is a
`stat` on every `.env` load for a value that has not moved — but it does mean an
administrator who pushes a fix has to say "restart the app", and the app will
not notice on its own.

**`FORBIDDEN` joins the error vocabulary** and is deliberately not
`PERMISSION_DENIED`. The latter means the OS refused and can be answered by
elevating; this one never can, so a UI offering a retry would be promising
something that cannot happen. The distinction has to be honoured by every future
caller, which is a rule that only lives in ADR
[0004](0004-errors-are-codes-not-strings.md) and this file.

**A managed value is invisible in `.env`.** The file on disk still says whatever
it said; the app reports the policy's value. Somebody reading the workspace with
`cat` sees something different from what the app is using — which is inherent to
having a layer above the file, and is why `policy_status` exists and why the
Settings pane names the source path rather than just greying the field out.

**The rewrite is narrowed to Dockerfiles and compose files by filename.**
`image:` is a key that could plausibly appear in a service's own configuration
one day, and a rewrite that silently edited `elasticsearch.yml` would be a bug
nobody could find. The cost is that a future generated file with images in it
has to be added to `policy::rewrites`, and `src-tauri/tests/policy_claims.rs`
is what makes that omission loud rather than silent.
