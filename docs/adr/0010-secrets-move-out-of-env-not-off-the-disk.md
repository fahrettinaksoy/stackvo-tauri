# ADR 0010 — Secrets move out of `.env`, not off the disk

- **Status:** accepted
- **Date:** 2026-08 (recorded)

## Context

The readiness review's §5.2: database passwords sit in `.env` in plain text.
On a company machine that file is backed up, synced to a second laptop, and
scanned by whatever the organisation runs — and it is the file people paste
into support threads. The review asked for the credential class
(`SERVICE_*_PASSWORD`, `*_TOKEN`, `*_SERVER_ID`) to live in the OS keystore with
a reference such as `keychain:stackvo/mysql-root` left behind, and said this
would be a v2 contract change because the Bash CLI reads the same file.

Writing it turned up something the review had not counted, and it changes what
this decision can honestly claim.

## The finding: the secret was in two files, not one

`generated/docker-compose.dynamic.yml` is rendered from

```yaml
MYSQL_ROOT_PASSWORD: '{{ SERVICE_MYSQL_ROOT_PASSWORD | default('root') }}'
```

so the real password has always been substituted into it, sitting in plain text
beside the file everybody was worried about. §5.2 counted `.env` and stopped.

That is not a reason to abandon the change, but it is a reason not to describe
it as "secrets are no longer on disk", which is what a keystore feature is
normally taken to mean.

## Decision

### 1. Move it out of `.env`, and say plainly that it is still in `generated/`

`.env` is hand-maintained, quoted in support threads, synced, and the thing a
backup tool gets pointed at. `generated/` is output that ADR
[0002](0002-generated-files-are-rendered.md) says is rewritten from scratch on
every run, and is the natural thing to exclude. Moving a value from the first to
the second is a **real reduction and a partial one**.

The alternative was to claim the whole thing. The module comment, the contract
entry, `PRIVACY.md` and the Settings pane all state the limit instead — the same
choice ADR [0009](0009-a-policy-file-is-not-a-lock.md) made about the policy
file, for the same reason: a security feature that is believed to do more than
it does is worse than none, because somebody plans around it.

### 2. Getting it out of `generated/` too is a v2 change, and is not half-done here

It means rendering `${SERVICE_MYSQL_ROOT_PASSWORD}` instead of the value, and
handing the value to `docker compose` through the environment of the process the
app spawns. That:

- changes the rendered bytes, so the differential comparison against the Bash
  generator — the mechanism the whole port's safety rests on — fails on every
  service;
- breaks `docker compose up` run by hand in that directory, which is a thing
  people do when something is wrong.

Both are solvable and neither is solvable quietly. Written down here rather than
started.

### 3. The reference is data, not a derivation

The entry name is `SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4` — the key first,
because that is what somebody scrolling Keychain Access is looking for, and a
digest of the workspace path after it, because two workspaces on one machine
must not share one entry.

It is **generated** from the key and the path, and never **recomputed** to look a
value up: `.env` carries it verbatim and that is what is read. Recomputing would
orphan every secret the moment somebody moved their workspace directory, and it
would do it silently and only to them.

The digest is FNV-1a written out in six lines rather than `DefaultHasher`, whose
output is explicitly not guaranteed between Rust releases. Nothing would break —
references are stored, not recomputed — but a persisted identifier whose
definition is "whatever the standard library did that year" is not one to write
into a user's file.

### 4. A key that has been moved stays moved

`env_writer::apply` looks at the _current_ line in `.env`. If it is a reference,
the new value goes to the keystore and never to the file. Without that rule the
first save from any Settings pane that happened to include the key would put the
password back, with the user having pressed nothing but **Save** and nothing on
screen saying so.

`apply_verbatim` is the exception and has exactly one caller — `secret_restore`,
whose whole job is to undo the redirection. `secrets_claims.rs` holds the caller
count at one.

### 5. An unresolvable reference removes the key rather than blanking it

`template::render` prefers a present-but-empty value over the template's own
`| default('root')`. So a reference the keystore would not answer, resolved to
`""`, renders `MYSQL_ROOT_PASSWORD: ""` — a database anybody on the network can
open, because a keychain was locked.

`secrets::resolve` therefore **removes** the key and returns its name, and
`render_generated` refuses outright before writing anything. Falling back to the
template default would have been the other option and is worse in a quieter way:
the container comes up on a password the user set years ago and does not know is
in force.

### 6. Moving is one key at a time, explicit, and reversible

The Bash CLI reads `.env` and would use the literal string `keychain:…` as the
password, so a fresh MySQL container would initialise with a root password
nobody chose and nothing would announce it. A sweep that moved every credential
at once would be that decision made silently, twelve times.

`secret_restore` exists because the cost of the decision is one a user may only
meet afterwards, the first time they run `stackvo.sh` on the same workspace. A
one-way door would leave them hand-editing `.env` with a value the app will not
show them, which is not a way out.

## Consequences

**The Bash CLI cannot read a moved key, and nothing can make it.** `doctor`
reports a workspace that has both, and the move flow says so before it happens.
That is mitigation, not a fix; the fix is a CLI that understands the scheme,
which is the same v2 as §2 above.

**One new crate on macOS and Windows, fourteen on Linux, twenty-nine in the
lock.** `keyring` is thin where the native store is already in the graph and
thick where the Secret Service session needs D-Bus and its own encrypted
handshake (aes, cbc, hkdf, hmac, and `num`/`num-*` for the Diffie-Hellman). The
third number is `Cargo.lock`, which resolves every target at once and is what
`NOTICE.md` is generated from — so the attribution file grew by more than any
one user installs, which is correct and worth knowing when the number is next
looked at. `crypto-rust` rather than `crypto-openssl` so no Linux user meets an
OpenSSL build. `linux-native` was cheaper and wrong: it is the kernel keyring,
scoped to a session and gone at reboot, and a password store that empties itself
is not one.

**A machine with no keystore is a supported machine.** A headless Linux box with
no Secret Service reports `available: false`, the pane offers no button, and
nothing else in the app behaves differently — because nothing else in the app
touches the keystore unless a reference is already in `.env`.

**The credential class is `Env::is_secret` and not a second list.** Two lists of
"what counts as a secret" is how a key comes to be starred out on screen and
stored in the clear, or the reverse.
