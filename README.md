# StackVo Desktop

A Docker-based local development environment manager, as a native desktop app.

**Self-contained.** It used to require a clone of
[StackVo](https://github.com/stackvo/stackvo) to read its generator and
templates from. The generator was ported to Rust (the shell was deleted), and
the service templates now ship inside the binary — so a workspace is a folder
this app creates, not one you have to fetch. Point it at an empty directory and
it writes the `.env`, the templates and the project tree itself. An existing
checkout still works and is left exactly as it is.

## Why a desktop app

In StackVo today the dashboard is itself a container: it runs inside the Docker stack it manages,
reaches the engine through a mounted `docker.sock`, is routed by Traefik at `stackvo.loc`, and needs
a hosts-file entry and a self-signed-certificate click-through to open. That one decision is where
most of the friction comes from — the UI can't tell you Docker is down (it needs Docker to run), it
can't stop the stack (that would kill itself), it writes files as root and chowns them back, and it
reads host CPU stats from inside a container, where they're wrong.

StackVo Desktop inverts the relationship. The app runs on the host as a normal user process and
drives Docker directly. Traefik and the project/service containers are unchanged — only the control
plane moves.

|                    | Web UI (today)                           | Desktop                            |
| ------------------ | ---------------------------------------- | ---------------------------------- |
| Runs as            | container, root, `chmod 666 docker.sock` | host process, invoking user        |
| Docker down        | unreachable                              | opens, reports, offers to start it |
| Host metrics       | `/proc` inside a container               | `sysinfo` on the host              |
| Stopping the stack | impossible (kills the UI)                | `compose_down`                     |
| Hosts file         | manual `sudo tee -a /etc/hosts`          | reviewed diff, one elevated write  |
| Windows            | WSL2 only                                | native — no shell, no bash         |
| Install size       | ~600 MB of images                        | ~10 MB                             |

## Status

**Phase 0 — contracts.** Complete. See [contracts/](contracts/).
**Phase 1 — skeleton + read-only views.** Complete. The app runs, finds a StackVo
checkout, reports the Docker engine, and reads real host metrics.
**Phase 2 — control.** Complete. Start/stop/restart/build projects and services,
enable/disable services, live container logs, and stack-wide `up`/`down` — all
with streamed progress instead of a blocked request.
**Phase 3 — desktop integration.** Complete. Tray, native notifications, a
watcher on `projects/*/stackvo.json`, an elevated hosts-file helper that shows a
diff first, real terminals (container _and_ host), autostart and single-instance.

**Phase 4 — generator port.** Complete. All five web servers, the Node runtime,
`docker-compose.projects.yml` and both Traefik files are ported to Rust and
verified byte-for-byte against the Bash generator. Windows path and named-pipe
handling is written and unit-tested; see the caveat below.

**Phase 5 — releases.** Signed auto-updates are wired: the app checks an
endpoint, verifies the bundle signature against the key compiled into the build,
installs and restarts. `.github/workflows/release.yml` builds and signs for four
targets. **Two things are yours to supply** and are deliberately absent from
this repo — the signing key pair and the endpoint that serves `latest.json`:

```bash
npm run tauri signer generate -- -w ~/.tauri/stackvo.key
```

Put the public half in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`,
and the private half in the `TAURI_SIGNING_PRIVATE_KEY` repository secret.
Without both, builds produce unsigned artifacts that the updater refuses — the
correct failure, not a bug to route around.

Verification is differential, not by inspection — a generator that produces
"basically the same" output silently changes people's images:

- `tools/make-fixtures.sh` runs the real Bash generator in a throwaway sandbox
  (a copy of `core/` + `.env`, never the user's projects) and freezes its output
  as fixtures, one per server.
- `tests/fixtures_differential.rs` compares the Rust renderer against them.
- `npm run diagnose` runs the same comparison **live** against your own
  projects, and reports which match.

### Taking the generator over

The Rust generator runs _alongside_ the Bash one; it does not replace it.
`generate_with` has three modes:

| Mode     | Behaviour                                                          |
| -------- | ------------------------------------------------------------------ |
| `bash`   | What StackVo does today. The default.                              |
| `verify` | Bash writes; the Rust port renders the same files and is compared. |
| `rust`   | Refuses to write unless the two agree byte-for-byte.               |

Bash runs in every mode. The generator's output is the input to every container
you run, so "probably identical" is not a standard worth shipping — `rust` mode
cannot silently change an image, because a disagreement stops it.

### Windows status

The pure logic — drive-letter to bind-mount conversion (`C:\Users\me` →
`/c/Users/me`), named-pipe detection, `DOCKER_HOST` scheme stripping — lives in
`src-tauri/src/paths.rs` with no `cfg` gates, so its tests run on every
platform. That is deliberate: Windows behaviour verified only on Windows is
Windows behaviour nobody verifies.

What is **not** verified: the handful of `#[cfg(target_os = "windows")]` blocks
in `engine.rs`, `hosts.rs` and `pty.rs`. Cross-compiling from macOS fails in
`tauri-build`, which needs `llvm-rc` to embed the app manifest — a toolchain
gap, not a code error, but it means those blocks have never been compiled. They
need a real Windows machine or a CI runner before anyone claims Windows works.

```bash
npm install
npm run tauri:dev      # run the app

npm test               # everything: vitest + Rust unit, integration, differential
npm run test:js        # front end only
npm run lint           # eslint + prettier
npm run audit          # cargo-deny + npm audit
npm run contracts:check
npm run diagnose       # headless end-to-end check
npm run bundle:budget  # raw asset sizes against tools/bundle-budget.mjs
```

`cargo bench` measures the generator's render path. It is deliberately not in
CI: a hosted runner's variance is wider than the regressions a threshold would
be set to catch, so it is an instrument you run on a quiet machine rather than
a gate. The bundle budget is a gate for the opposite reason — bytes are the
same on every machine.

CI runs all of these on Linux, macOS and Windows, with `cargo clippy -- -D
warnings` and `cargo fmt --check`. The Rust toolchain is pinned in
`src-tauri/rust-toolchain.toml`, so a new stable release cannot turn the build
red without a commit.

### Driving it from an AI assistant

`stackvo-mcp` is an MCP server over the same core the app drives, so an
assistant can answer "why is shop.loc not loading?" from the preflight report,
the hosts file, the certificate's SAN list and a container's last hundred log
lines — without a window open.

```bash
cargo build --release --bin stackvo-mcp
```

Then **Settings → AI assistants**, which lists Claude Code, Claude Desktop,
Cursor, Windsurf, VS Code and the Gemini CLI, says which of them are on this
machine and which already point at the server, and registers it in one click.
It reads each client's own configuration file, inserts a single `stackvo` entry
and writes it back, so every other server in that file survives; a copy is kept
beside it as `.stackvo-backup` first. A file that is JSON _with comments_ — VS
Code's format — is reported rather than rewritten, with the block to paste,
because stripping the comments to make the edit possible would delete the
reader's own notes.

Codex and Zed are not in that list on purpose: Codex configures itself in TOML,
which needs a format-preserving editor rather than a serialiser, and Zed's
`context_servers` shape could not be verified against a running copy. Both take
the block by hand:

```json
{ "mcpServers": { "stackvo": { "command": "/path/to/stackvo-mcp" } } }
```

**Read-only by default.** 7 of the 17 tools change things and appear only with
`--allow-writes`: `xdebug_set`, `certificates_reissue`, `project_start`,
`project_stop`, `stack_up`, `stack_down`, `generate`. Read that list before
passing the flag — it grants an assistant the ability to **stop the whole
stack**, not just to toggle Xdebug. Every tool is annotated `readOnlyHint` /
`destructiveHint`, so a client can require confirmation for a tool it has never
seen.

The tool table names, for each tool, the `contracts/ipc.json` command it
implements, and three tests cross-check the two: a tool naming a command that
does not exist fails, a read-only tool backed by a declared `mutation` fails,
and a write-gated tool backed by a mere `query` fails — the gate would be
guarding nothing. Generating the list outright was the obvious move and is the
wrong one: dispatch cannot be generated, so a generated list advertises tools
that fail when called.

**Not exposed:** the rest of the mutating surface. 49 of the 177 commands take
an `AppHandle` because they report progress through Tauri's event system, and a
stdio subprocess has no app to emit into. Decoupling that is a refactor of its
own; pretending otherwise would mean advertising `project_build` and having it
fail.

### When something goes wrong

The app writes a rotating log — seven days, then it drops the oldest.
**Settings → Application log** shows where and opens the folder. Password and
token values are masked as the log is written, so it is safe to attach to an
issue, but read it first.

**Settings → Diagnostic bundle** packages the same log with `preflight`,
`doctor`, the engine state and the version into one zip, masked a second time on
the way in. What it holds and what it deliberately leaves out — no `.env`, no
project sources — is listed in [PRIVACY.md](PRIVACY.md), along with everything
this app stores and every host it can reach. There is no telemetry; that
sentence is held up by a test rather than by a promise
(`src-tauri/tests/privacy_claims.rs`).

### Where things are kept

Three directories, and which one you want depends on whose failure you are
looking at. `src-tauri/src/appdir.rs` owns the first two and the reasoning.

|             | macOS                                    | Windows                        | Linux                          |
| ----------- | ---------------------------------------- | ------------------------------ | ------------------------------ |
| App log     | `~/Library/Logs/StackVo/`                | `%LOCALAPPDATA%\StackVo\logs\` | `~/.local/state/stackvo/logs/` |
| Preferences | `~/Library/Application Support/StackVo/` | `%APPDATA%\StackVo\`           | `~/.config/stackvo/`           |
| Stack state | `~/.stackvo/`                            | `~/.stackvo/`                  | `~/.stackvo/`                  |

**The app log and preferences** follow each platform rather than one string
forced onto all three: Apple's log folder, `%LOCALAPPDATA%`, and
`XDG_STATE_HOME` — which is where the XDG specification puts logs, and which
this wrote outside of until it was noticed. Both are named `StackVo` and not
`com.stackvo.desktop`: the bundle identifier is what the OS calls this app —
the `Preferences` plist, the code signature, the privacy prompts — and these
two folders are ours to name and the ones a person is asked to open. Postman,
Termius and Redis Insight all split it the same way.

**Stack state** is `~/.stackvo`, or wherever `STACKVO_ROOT` points: the `.env`,
the templates, the generated compose files, the certificates, and
`logs/projects/<name>/` — which is where a _project's_ web server writes, and is
not the same thing as the app log above. It is the user's to move and safe to
delete; nothing the app needs in order to start is kept there.

Raise the log level with `STACKVO_LOG=stackvo_desktop=debug`.

### Keeping credentials out of `.env`

**Settings → Where credentials are kept** moves a database password, token or
server id into this machine's keystore — Keychain, Credential Manager or the
Secret Service — and leaves a reference in its place:

```sh
SERVICE_MYSQL_ROOT_PASSWORD=keychain:SERVICE_MYSQL_ROOT_PASSWORD@a1b2c3d4
```

That takes it out of the file that gets backed up, synced, and pasted into
support threads. **It does not take it off the disk**: the real value is still
rendered into `generated/docker-compose.dynamic.yml`, because that is where
Compose reads it from. Getting it out of there too changes the generated bytes
and is a v2 change — [decision 0010](docs/durum.md)
has the reasoning and the rest of the rules.

One key at a time and reversible, because `stackvo.sh` reads `.env` directly and
would use the reference string as the password. If you use both tools on one
workspace, leave the credentials where they are; **Settings → Doctor** says so
if you have not.

### Deploying it to more than one machine

There is a fourth location, and it is the only one the person at the keyboard
does not own: a policy file an administrator writes.

|         |                                                         |
| ------- | ------------------------------------------------------- |
| macOS   | `/Library/Managed Preferences/com.stackvo.desktop.json` |
| Windows | `%ProgramData%\StackVo\policy.json`                     |
| Linux   | `/etc/stackvo/policy.json`                              |

```json
{
  "schemaVersion": 1,
  "settings": { "DEFAULT_TLD_SUFFIX": "corp.test", "SERVER_TYPE": "nginx" },
  "locked": ["DEFAULT_TLD_SUFFIX"],
  "registryPrefix": "registry.corp.example/proxy"
}
```

`settings` overrides both the shipped default and the workspace's `.env`.
`locked` refuses a write to those keys from Settings, and only works for keys
`settings` also sets — "do not change this" without saying _to what_ leaves
every machine on whatever it happened to have. `registryPrefix` is prepended to
every image reference in the generated Dockerfiles and compose files, except
one that already names a registry, one already carrying the prefix, and the
`stackvo-*` images that are built here and exist in no registry at all.

A file that does not parse applies nothing and the app starts normally, but the
failure is shown in Settings rather than logged — a policy that quietly does
nothing is one the administrator believes is in force.

`STACKVO_POLICY_FILE` points at a different file, which is how you test one
without root. **This is not a security boundary.** The override exists, the
file is usually within the user's reach, and the layer tells a co-operating app
what your organisation intends — nothing more. See
[decision 0009](docs/durum.md).

`diagnose` is the headless equivalent of the dashboard — it exercises every
read-only command and prints what the UI would show, which makes it a genuine
troubleshooting tool as well as the port's end-to-end check. It deliberately
does not run the mutating commands: those would restart your stack.

The CLI is not being replaced. Both tools read the same `stackvo.json` and `.env`, so a project
created in either works in the other. That compatibility is enforced by a checked-in contract and a
validator, not by convention.

Phase 0 turned up four live bugs in shipped StackVo, found purely by writing the format down:

- Node projects created through the web UI generate as PHP and cannot build ([C-01](contracts/CONFLICTS.md))
- Four of the six advertised runtimes have no generator at all ([C-02](contracts/CONFLICTS.md))
- The default extension set can't build on the default PHP version ([C-06](contracts/CONFLICTS.md))
- `mongo-express` never starts in minimal mode — profile name mismatch ([C-09](contracts/CONFLICTS.md))

## Roadmap

| Phase | Scope                                                                              |
| ----- | ---------------------------------------------------------------------------------- |
| 0 ✅  | Freeze the config contract, extract the extension matrix, derive the IPC surface   |
| 1 ✅  | Tauri + Vue skeleton; port the existing dashboard; read-only views on real metrics |
| 2 ✅  | Container control via `bollard`; streamed build/log progress                       |
| 3 ✅  | Tray, notifications, fs-watcher, hosts helper, PTY, autostart, single-instance     |
| 4 🚧  | Generator port to Rust, native Windows, signed auto-updates                        |

**What is still open is written down, not remembered.**
[`docs/durum.md`](docs/durum.md) is the single work queue: every product gap and
every engineering item this project has diagnosed and not yet closed, with what
was checked to confirm it is still open. The ones no commit can close — the
update endpoint, the signing secrets, the Apple/Windows certificates and the
decisions only the owner can take — are in §5.

## How it is built

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — the map: the four bands of the Rust
  side, the one request flow worth knowing, and what the front end's panes and
  composables are for.
- **[docs/durum.md](docs/durum.md)** — the one document under `docs/`: what is
  done, what is left, and the ten numbered decisions with their consequences.
  Comments in the source say "ADR 0005"; that is §6 of this file.
- **E — IPC surface.** Every command in `contracts/ipc.json` must be registered
  in `src-tauri/src/lib.rs` and wrapped in `src/lib/ipc.js`.
- **F — reachability.** Every wrapper must be called by some view or store, and
  every declared event must actually be emitted.

Without them the contract quietly drifts ahead of the code. It had: by the end
of Phase 3, **22 declared commands had no implementation** — including
`project_create`, so the app could not create a project at all. Suite F then
found **21 wrappers no view called** and **4 events nothing emitted**.

Current state: **no errors**, six warnings — five wrappers no view calls yet,
and one for a checkout with no projects in it.

That number is deliberately not a promise. It was written here as "4 errors, all
of them pre-existing StackVo bugs" and stayed after the four were fixed, because
nothing checks a number in prose. `src-tauri/tests/contract_agreement.rs` is the
part that _is_ checked, and it covers the two edges this suite does not — see
[decision 0006](docs/durum.md).

`tools/measure-env-usage.mjs` is a separate check with the same intent: it
measures which `.env` keys the checkout actually reads and reconciles that
against the `status` labels in the contract. It exists because the first,
hand-run version of that measurement was executed from the wrong directory and
mislabelled twelve keys — a number that looked like evidence and was not
checkable later.

## License

MIT
