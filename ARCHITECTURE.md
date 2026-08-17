# StackVo — architecture

The map a second person needs before touching this repository. `README.md`
describes the product; this describes the machine.

Everything here is measured against the tree it ships in, and the numbers are
checked by tests rather than maintained by hand — see
[Keeping this file honest](#keeping-this-file-honest) at the end.

---

## 1. What the program is

A desktop application that manages a local Docker stack for PHP and Node
projects: a reverse proxy, a certificate authority, a set of optional services
(databases, caches, admin UIs), and one container per project.

Three parts, in the order a request travels:

| Part                                | Where                | Size                    |
| ----------------------------------- | -------------------- | ----------------------- |
| Front end — Vue 3, Vuetify 3, Pinia | `src/`               | 38k lines               |
| Back end — Rust, 94 modules         | `src-tauri/src/`     | 76k lines               |
| The boundary between them           | `contracts/ipc.json` | 248 commands, 69 events |

The two halves never share a type. They share a **contract**, and §5 is about
why that is a deliberate cost rather than an omission.

---

## 2. The one flow worth knowing

Everything else is a variation on this. A user clicks _Create project_:

```
Vue component
  └─ composable (src/composables/useX.js)        state, no markup
       └─ api.projectCreate(spec)                src/lib/ipc.js
            └─ invoke('project_create', {...})   Tauri IPC, camelCase → snake_case
                 └─ #[tauri::command] project_create      src-tauri/src/commands.rs
                      ├─ state.root()?                    the workspace, or an error
                      ├─ workspace::canonical_name(...)   one string for three uses
                      ├─ state.inflight.acquire(...)      one operation per subject
                      ├─ manifest::parse / validate       schema, not free-form JSON
                      ├─ scaffold::write(...)             the project's files
                      ├─ generator::render(...)           compose + Dockerfile + proxy
                      └─ runner::run_operation(sink, op)  docker compose, streamed
                           └─ events: project:creating → project:created
                                                         back to the front end
```

Four things in that path are load-bearing and are each a decision with a
document behind it:

- **`state.inflight.acquire`** — one operation per subject, held for the life of
  the command. The front end has a busy flag per view; this is the boundary the
  tray, a second window and a keyboard shortcut all share.
  → [decision 0003](docs/durum.md)
- **`generator::render`** — the compose file and the Dockerfile are _rendered_
  from the manifest every time, never edited in place.
  → [decision 0002](docs/durum.md)
- **`runner::run_operation(sink, …)`** — the long half of the work reports
  through a sink rather than returning, so a build does not block a promise for
  four minutes. → [decision 0005](docs/durum.md)
- **the error that comes back** — a `StackvoError` with a `code`, not a string.
  → [decision 0004](docs/durum.md)

---

## 3. The back end

### 3.1 Layers

`src-tauri/src/` is flat — 94 modules, no subdirectories — but it is not
unstructured. There are four bands, and the dependency arrows only ever point
downward:

```
  entry              1.8k   lib.rs, main.rs, menu, tray — plugins, state, the
      │                     handler list, the window
      ▼
  commands.rs       12.8k   the IPC surface: 247 #[tauri::command] functions
      │                     argument validation, orchestration, nothing else
      ▼
  domain            53.3k   94 modules: generator, manifest, certs, hosts,
      │                     mail, xdebug, profile, preset, migrate, worktree, …
      │                     one subject each; no Tauri types
      ▼
  platform           5.6k   engine (Docker), runner, elevate, pty, watcher,
      │                     applog, atomic, paths, appdir, git
      ▼
  primitives         2.2k   error, events, progress, hints, inflight, logging,
                            crash, contracts
```

`commands.rs` is the only file that mentions `AppHandle` or `State<'_, …>`, and
that is the rule the band structure exists to enforce: everything below it can
be called from a test, from the `diagnose` example, or from the MCP surface,
with no running application. → [decision 0001](docs/durum.md)

The 12.7k-line `commands.rs` is the known cost of that rule. It is a directory of
thin functions rather than a module with a subject, and splitting it by subject
is the obvious next move; it has not been done because the file's size is
uncomfortable rather than harmful, and the split would touch every command at
once.

### 3.2 The modules, by subject

| Subject        | Modules                                                                     | What it owns                                                                                                               |
| -------------- | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Rendering      | `generator`, `template`, `skeleton`, `scaffold`                             | Everything under `generated/`, the per-project Dockerfile, and the files a new project starts with                         |
| The manifest   | `manifest`, `detect`, `migrate`                                             | `stackvo.json`: its schema, guessing one for an adopted folder, and moving old ones forward                                |
| Docker         | `engine`, `runner`, `inflight`                                              | Talking to the daemon (bollard), running `docker compose` as a streamed operation, and refusing two at once on one subject |
| Networking     | `certs`, `hosts`, `elevate`, `tunnel`                                       | TLS via mkcert, `/etc/hosts`, the one privileged call, and the Cloudflare sidecar                                          |
| Services       | `db`, `worker`, `quickcmd`, `repl`, `release`, `stats`                      | The optional stack, the per-project sidecars, the command catalogue, the snippet workbench, and the production image       |
| PHP            | `phpini`, `xdebug`, `profile`, `debugbridge`                                | The overlay that reaches a running container, and the profiler's output                                                    |
| Node           | `devserver`                                                                 | The dev-server sidecar and the `allowedHosts` snippet                                                                      |
| Mail           | `mail`                                                                      | The catcher, its search, and the HTML/link checks                                                                          |
| Branches       | `git`, `worktree`                                                           | Cloning with the user's own git, and giving a branch its own directory, hostname, database and environment                 |
| Diagnosis      | `doctor`, `preflight`, `diagnostics`, `applog`, `crash`                     | What is wrong, what can be fixed automatically, and what to send when it cannot                                            |
| The app itself | `workspace`, `preset`, `config`, `locale`, `watcher`, `tray`, `menu`, `mcp`, `cli` | Where things live, sharing that setup, and the three surfaces other than the window: the tray, the MCP server and `stackvo` |

### 3.3 State

`AppState` is managed once at startup and holds six things, each behind a
`Mutex`: the workspace pointer, a Docker stats sampler and its history, the live
log-stream handles, the in-flight registry, and the generation lock.

There is no database. The workspace directory _is_ the state:

```
<workspace>/
  .env                    the stack's own settings — services, ports, the TLD
  projects.path           a pointer, so projects can live outside the workspace
  generated/              every rendered file; safe to delete, rebuilt on demand
  projects/<name>/
    stackvo.json          the manifest — the only file a user is meant to edit
    Dockerfile            rendered
    php.ini               an overlay, present only when overridden
```

That `generated/` can be deleted at any time is the property the whole rendering
band is built to preserve, and `generator_verify` exists to prove it still
holds on a real machine rather than in a fixture.

---

## 4. The front end

### 4.1 Shape

```
src/
  views/          9 pages, one per route
  components/     shared widgets, plus project/ and settings/ panes
  composables/    18 files: state and boundary calls, no markup
  stores/         Pinia: app, appearance, inventory, metrics, operations
  lib/            ipc.js (the generated client), format, events, appearance
  i18n/           en.js, tr.js
  styles/         global.css, project-panes.css, settings-panes.css
```

The pattern every page follows: **a view composes panes, a pane owns markup, a
composable owns state, and only the composable talks to `api`.** A pane that
needs something the page owns — a dialog, a lifecycle operation — emits an event
rather than reaching for it.

That is not how it started. `Settings.vue` and `ProjectDetail.vue` were 3.4k and
3.0k lines, held every section's state in one `<script setup>`, and could not be
mounted in a test at all — so neither was covered by anything. Splitting them
into 26 panes (14 for the project page, 12 for settings) is the largest single
change in this repository's history; its entry is in `CHANGELOG.md`.

Two things that split taught, both now enforced by tests:

- a pane's markup and **its styles** move together; a `<style scoped>` block
  reaches only the elements its own component renders, and leaving the rules
  behind renders every pane unstyled while every test stays green
  (`tests/pane-styles.spec.js`);
- mounting a page finds bugs that reading it does not — an untyped `null` from
  the boundary, a badge keyed on a section that no longer exists, a form control
  with no accessible name. All three were found that way.

### 4.2 The boundary client

`src/lib/ipc.js` is generated from the contract. It does three things: maps
camelCase wrappers to snake_case command names, rebuilds the Rust error struct
into a `StackvoError` a caller can branch on, and exports `asList`, which is the
guard against an untyped boundary answering something that is not an array.

`asList` is not defensive programming for its own sake: `projects_list`
answering `null` once made every inventory computed throw and the window go
blank, and it sat in the suite for months as four unchased "unhandled
rejections".

---

## 5. The contract

`contracts/ipc.json` is the specification of the boundary: 244 commands, 69
events, 97 named types, 3 error shapes, and — for most entries — a `why`.

It is a **hand-maintained document, not generated code**, and that is the
trade-off worth stating plainly. Generating TypeScript types from the Rust
(`tauri-specta`) would make drift impossible; it was measured and deferred
because it changes how every command is declared and belongs on its own branch.
→ [decision 0006](docs/durum.md)

What keeps it honest in the meantime is `src-tauri/tests/contract_agreement.rs`,
which fails the build when the contract, the `#[tauri::command]` functions and
the `generate_handler!` list stop describing the same surface — in either
direction, including the quietest one, where a command is implemented and
documented but never registered, so calling it answers `command not found` as a
bare string at runtime.

### Conventions the contract fixes

- **No envelope.** A command returns `Result<T, StackvoError>`. `Ok(T)` is the
  payload; `Err` rejects the promise. The predecessor returned
  `{ success, data, message }` over HTTP 200, so a failure looked like a
  success until something read `.success`.
- **Anything over ~2 seconds does not block.** It returns an `OperationId` and
  reports through events.
- **Arguments are camelCase on the wire**, snake_case in Rust; Tauri binds them
  by name, which is why `tests/ipc.spec.js` checks the argument _names_ and not
  just the command names.

---

## 6. Errors

One shape, everywhere:

```rust
StackvoError { code, message, hint: Option<String>, hint_key: Option<String>, details }
```

`code` is what a caller branches on. `message` is for a human. `hint` is the
English sentence that says what to do about it, and `hint_key` names an entry in
`src-tauri/src/hints.rs` so the front end can render a translated one — the log
and the MCP surface get the English either way.
`src-tauri/tests/hint_translations.rs` fails the build if a catalogued hint has
no translation in both locales.

---

## 7. Testing

| Suite                   | Count | What it is for                                                                       |
| ----------------------- | ----: | ------------------------------------------------------------------------------------ |
| Rust unit + integration | 1,186 | The domain band, mostly against real files in a temp workspace                       |
| Front-end (vitest)      |   794 | Composables, and every page and pane **mounted**                                     |
| Differential            |     — | The Rust generator's output against what the Bash generator writes, on real projects |

The tests that exist to catch a _class_ of mistake rather than a case, and are
worth knowing about before adding to them:

- `contract_agreement.rs` — the boundary described in three places, agreeing.
- `version_agreement.rs` — one release, one number, in three files.
- `readme_claims.rs` — the numbers in `README.md` against the tree.
- `hint_translations.rs` — every catalogued hint translated in both locales.
- `independence.rs` — the binary renders a workspace from nothing.
- `tests/i18n.spec.js` — every key the app asks for exists, every key defined is
  reachable, and every message compiles.
- `tests/a11y-axe.spec.js` — axe over every mounted page and pane.
- `tests/pane-styles.spec.js` — every class a pane names resolves.

**What is not covered.** There is no end-to-end run. `tauri-driver` compiles on
macOS and then refuses — WKWebView has no WebDriver — so the scenarios would be
unrunnable until a Linux runner exists. That is tracked as §14.12 of the
readiness review rather than papered over with tests that cannot execute.

---

## 7a. Where the open work is, and where the decisions are

One document, [`docs/durum.md`](docs/durum.md). It replaced five — two
competitive reviews, a readiness review, a platform matrix and ten ADR files —
when keeping "what is left" in five places stopped being readable.

| Section | Answers |
| --- | --- |
| §1 | Where the record of delivered work is — `CHANGELOG.md`, §6 and the git history. Finished items leave that document. |
| §2–§3 | What the product cannot do against ten rivals, and what the engineering will not carry at ten developers and three hundred machines. |
| §4–§5 | What to do next, and what is waiting on a decision only the owner can make. |
| §6 | **The decisions**, numbered. Comments in this codebase say "ADR 0005"; that is §6. |
| §7 | The measurements, held to the tree by `platform_matrix_claims.rs`. |

Two of those sections have gates and three do not, and the document says which:
§6 and §7 fail the build when they drift, while "not done" is not a measurable
property of code and no test can pretend otherwise.

---

## Keeping this file honest

A document that describes a tree it no longer matches is worse than none, and
this repository has been wrong about itself before: the readiness review's own
first draft named a module as weakly tested that was 94% covered, and counted 33
of something there were 60 of.

So the checkable claims here are checked. `src-tauri/tests/readme_claims.rs`
covers `README.md`; the counts above (94 modules, 248 commands) come from
`contract_agreement.rs` and from the module list itself, and
a claim that drifts fails a test rather than aging quietly.

The prose is not machine-checkable, and that is what review is for. When a
decision in §2 changes, the ADR is the thing to write; this file only points at
it.
