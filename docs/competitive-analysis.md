# Competitive analysis — where StackVo Desktop stands

Nine competing local development environment managers, read against StackVo's actual surface.
The point is not a feature count. It is to decide which gaps are worth closing, in what order,
and which are traps.

**Evidence basis.** The competitor columns come from vendor marketing pages and, where published,
their documentation indexes (Herd's `llms.txt`, ForgeKit's `/docs`, Lerd's landing page), read
2026-07-29. Vendor pages overstate; a `–` means *not claimed*, not *proven absent*.

The **StackVo column is different**: every cell is verified against this repository and the
upstream checkout, and carries a file reference. Where the two disagree, the file wins.

> **Verification changed the answer.** A first pass, written from the marketing pages alone,
> scored StackVo `❌` on trusted HTTPS, Xdebug, and UI config editing. All three were wrong —
> mkcert is already wired, `xdebug` is already in the extension catalog, and both `.env` and the
> project manifest are already editable from the app. Those three moved from *build it* to
> *surface it and finish it*, which is a materially cheaper class of work. The priority order
> below reflects the verified state, not the first pass.

## 1. The field splits three ways

| Class | Products | Bet |
| --- | --- | --- |
| **Native binaries** | [Herd](https://herd.laravel.com/), [ForgeKit](https://forgekit.tools/), [ServBay](https://www.servbay.com/), [FlyEnv](https://flyenv.com/), [Laragon](https://laragon.org/), [XAMPP](https://www.apachefriends.org/) | No Docker. PHP and MySQL run on the host. Sold on startup speed and RAM. |
| **Containers** | [Laradock](https://laradock.io/), [Lerd](https://lerd.sh/) (rootless Podman) | Isolation and reproducibility. **StackVo is here.** |
| **Native control plane over an isolated runtime** | ForgeKit (Tauri, like this app), Lerd | Desktop UX in front of a managed runtime. The direction StackVo Desktop is already taking. |

Pricing, for positioning: Herd Pro $99/yr · ServBay $59/yr · Laragon $49–199 commercial ·
FlyEnv freemium (3 sites) · EnvKit, ForgeKit, XAMPP free · Lerd, Laradock, StackVo MIT.

**Consequence for StackVo.** The speed argument is unwinnable and not worth contesting — a
container will not beat a host binary at cold start. StackVo's claim is Laradock's power with
Herd's ergonomics. Every item below is prioritised by that: *remove the friction that comes from
being container-based, and surface the advantages that only a container-based tool has.*

One number matters more than the rest: **five of the eight active competitors ship an MCP
server** (Herd, Lerd, EnvKit, FlyEnv, ServBay). In 2026 this is table stakes, and it is the
single cheapest thing on StackVo's list — see P1-6.

## 2. Gap matrix

`✅` shipped and surfaced · `⚠️` present but incomplete or not exposed in the UI · `❌` absent ·
`–` not claimed by the vendor

| Capability | Herd | Lerd | EnvKit | FlyEnv | ServBay | ForgeKit | Laragon | Laradock | XAMPP | **StackVo** |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| Trusted local HTTPS (local CA) | ✅ | ✅ | ✅ | ✅ | ✅ +ACME | ✅ | ✅ | – | – | **✅** |
| Framework scaffold / quick app | ✅ | ✅ detect | – | – | – | – | ✅ | ⚠️ detect | ⚠️ Bitnami | **⚠️ detect** |
| DB backup / restore / snapshot | – | ✅ | – | – | ✅ | – | ✅ | – | – | **✅** |
| Mail catcher with a UI | ✅ | – | ✅ | ✅ | ✅ server | ✅ | – | ✅ | – | **✅** |
| Xdebug toggle | ✅ | – | ✅ | – | – | ✅ | ✅ | ✅ | ✅ | **✅** |
| **MCP server** | ✅ | ✅ | ✅ | ✅ | ✅ | – | – | – | – | **✅** |
| Unified log viewer + search | ✅ | ✅ | ✅ | ✅ | ✅ | – | ✅ | – | – | **✅ per project** |
| Tunnel / shareable public URL | ✅ | – | – | ✅ | ✅ | – | ✅ | – | – | **❌** |
| Doctor with repair actions | – | ✅ | ✅ | – | ✅ | – | – | – | – | **⚠️** |
| Cron / queue worker management | – | ✅ heal | ✅ | ✅ | – | – | – | ✅ | – | **❌** |
| Dump / `dd()` catcher | ✅ | ✅ | ✅ | – | – | – | – | – | – | **❌** |
| Profiler | ✅ | ✅ SPX | – | – | – | – | – | ✅ | – | **⚠️** |
| Team config sharing | ✅ | – | – | ✅ sync | ✅ | – | ✅ | ✅ | – | **⚠️** |
| Runtimes beyond PHP | Node | polyglot | +Py | **12** | 8 | – | +Py/Go | 100+ svc | Perl | **❌ 2 of 6** |
| Config editing from the UI | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | – | – | ⚠️ | **✅** |
| Host metrics, disk I/O, network | – | – | – | – | – | – | – | – | – | **✅ only one** |
| Reviewed elevated hosts write | – | – | – | – | ✅ | – | ✅ auto | – | – | **✅** |
| Byte-verified config generator | – | – | – | – | – | – | – | – | – | **✅ only one** |
| Container + host PTY | – | ✅ TUI | – | – | – | ✅ | ✅ | ✅ | – | **✅** |
| Heavy services (Kafka, ES, Cassandra) | – | – | – | ⚠️ | – | – | – | ✅ | – | **✅** |

### 2.1 The StackVo column, cell by cell

Each `⚠️` hides a decision, so they are itemised. The `⚠️` rows are cheaper than they look; the
`❌` rows are the real work.

> **Shipped since this was written.** Sprint 1 is complete — P0-2
> (certificates), P0-3 (Xdebug), P0-4 (database backup) and P0-5 (the in-app
> inbox) — and Sprint 2 has begun with P1-6, the MCP server. Their rows above
> read `✅`. One qualification on the MCP row: the read surface is complete, and
> only two writing tools are exposed, because thirty-four commands report
> progress through Tauri's event system and are not reachable from a stdio
> subprocess until that is decoupled. The per-row notes below describe the
> state they were in when the gap was measured. Two findings came out of the
> work and are worth carrying forward: the contract had drifted (four commands
> were registered in `lib.rs` and absent from `ipc.json`, so suite E was **not**
> clean at 4 errors as the README claimed — it was 8), and `cargo fmt --check`
> was already failing on the pinned toolchain. Both are fixed.

| Row | Verdict | What actually exists | What is missing |
| --- | --- | --- | --- |
| **Trusted HTTPS** | ⚠️ | mkcert is already wired. `core/cli/utils/generate-ssl-certs.sh` issues a wildcard cert over `stackvo.loc`, `*.stackvo.loc` and every project domain; `SSL_ENABLE=true` in `.env`; `core/cli/lib/generators/traefik.sh:55` sets it as Traefik's `defaultCertificate`; the CA lands in `generated/certs/stackvo-ca.crt`. | Three holes. (a) `trust_ca_in_keychain` returns early on anything but macOS — Linux and Windows trust is manual. (b) mkcert is an undeclared dependency: `src-tauri/src/preflight.rs` checks workspace, engine, compose, network, projects and bash — not mkcert. (c) **The desktop app has no certificate commands at all** — none of the 66 in `contracts/ipc.json`. Cert state is invisible, and a new project's domain is absent from the SAN list until someone re-runs the script by hand. |
| **Xdebug** | ⚠️ | `xdebug` is in `contracts/php-extensions.json:171` with per-version pecl pins (3.3.2 → 3.4.0), so it compiles into any project that lists it in `php.extensions`. | No toggle, no generated ini (`client_host`, `start_with_request`, path mapping), no IDE hint. The hard part — the extension matrix — is done; the ergonomics are not. |
| **Config editing** | ✅ | **Since delivered.** `ProjectSettingsSheet.vue` edits the manifest as a form — the same fields as the create drawer, from the same `ProjectFormFields.vue` and the same `lib/manifest.js`, validated live through `project_validate`. Every field was previously set-once-at-create: changing a PHP version meant hand-editing JSON against write rules a text area cannot enforce. `Settings.vue:240` still writes `.env` through `envSet`, comment- and order-preserving. | The raw JSON pane stays as the escape hatch. `.env` remains field-by-field rather than schema-driven — a smaller gap, since `env.schema.json` fields are flat scalars. **`memory_limit` / `upload_max_filesize` were dropped from this item on evidence**: the manifest schema is `additionalProperties: false`, and `php.ini` appears nowhere in `core/cli` — the docs and the old web UI list `.stackvo/php.ini` as supported, but no generator mounts it. Making it real is a separate item. |
| **Doctor** | ⚠️ | `preflight.rs` runs six checks behind `RequirementsGate.vue`; `preflight::fix()` repairs two of them (`network`, `projects`). `npm run diagnose` exercises every read-only command headlessly. | Four checks diagnose without repairing. No port-conflict detection — the single most common Docker failure, and one every competitor that ships a doctor covers. |
| **Log viewer** | ✅ | **Since delivered.** `LogView.vue` has search, level filtering with counts, and a picker over both log roots; `app_logs` / `app_log_open` read them from the host, so they work with the engine down. 52 files found across 8 real projects, none of which were reachable before. | Still per project: no cross-project view, and no click-through to the editor (`open_in_editor` is wired, the line-to-file mapping is not). Herd sells this as a Pro feature. |
| **Mail** | ⚠️ | `core/templates/services/mailhog/` ships and starts. | mailhog is unmaintained upstream; Mailpit is what everyone else ships. No inbox in the app — the user leaves for a browser tab. **Since resolved in the app**: the inbox reads both APIs, so a checkout on either image gets one. The image swap itself remains an upstream change — it renames a service, its `.env` keys, its container and its volume, which is a migration for every running stack, not a one-line edit. |
| **Profiler** | ⚠️ | `core/templates/services/blackfire/` ships. | No activation path, no UI, no result view. |
| **Team config sharing** | ⚠️ | `stackvo.json` is per-project, commit-friendly and schema-validated — the substance of Herd's `herd.yml` is already there. | No export/import, no stack presets, no onboarding flow that says "clone this and run". The artefact exists; the story around it does not. |
| **Runtimes** | ❌ | — | `C-02`: `.env` advertises `php,python,go,ruby,rust,nodejs` and the UI renders all six, but `core/cli/lib/generators/project/` holds generators for PHP servers and node only. Four of six are a dead end. This is a shipped bug *and* a competitive gap. |
| **Scaffold** | ⚠️ | **Since delivered — the import half.** `project_create` refuses when the directory exists, so a cloned folder could not be adopted at all: on this checkout 11 of 21 directories under `projects/` were unmanaged, three of them Laravel. Detection now infers runtime, server, document root and PHP/Node version from `artisan`, `wp-config.php`, `composer.json` and `package.json`, and reports the files it read plus a confidence. | The create half — running `composer create-project` for a brand-new Laravel or WordPress. Detection is the harder and more-used direction; scaffolding is a container run on top of it. |
| **Tunnel, cron, dumps** | ❌ | — | No command, no template, no plan. |

### 2.2 What only StackVo has

Worth defending, because none of it is reachable for a native-binary competitor:

- **Real host metrics** — CPU history, disk I/O and network throughput from `sysinfo` on the host.
  No competitor advertises this at all; it exists because the control plane left the container.
- **A byte-verified generator.** `generate_with` runs `bash` / `verify` / `rust` modes and refuses
  to write in `rust` mode unless the two agree byte-for-byte, checked against frozen fixtures
  from the real Bash generator. Nobody else treats generated config as something to verify.
- **A reviewed, elevated hosts write** — diff shown first, one privileged operation.
- **Heavy services** — Kafka, Kafbat, Elasticsearch, Kibana, Cassandra, Grafana, RabbitMQ.
  Only Laradock competes here, and it has no desktop UI.
- **Container and host PTY**, both from the app.

## 3. Priority order

Ordered by impact ÷ effort against the verified state.

### P0 — the cost of entry

**1. Project scaffolding and framework detection** · impact very high · effort medium
The largest genuine `❌`. Two directions are needed: creating a project should be able to run
`composer create-project` / `npm create` for Laravel, WordPress, Symfony or Next.js inside the
container; and importing an existing folder should infer `runtime`, `server` and `document_root`
by looking for `artisan`, `wp-config.php` or `package.json`. Laragon's "Quick app" and Lerd's
nine-framework detection carry their entire onboarding story. `NewProjectDrawer.vue` and
`project_create` are already in place — the generation step is what is absent.

**2. Bring certificates into the app** · impact very high · effort **medium-low** ⭐
Re-scoped by verification: mkcert already works, so this is surfacing, not building.
Add a mkcert check to `preflight.rs`; add `cert_status` / `cert_plan` / `cert_apply` to the IPC
contract; regenerate automatically when a project is created or its domain changes; extend trust
to Linux and Windows (`update-ca-certificates`, `certutil`) using the reviewed-diff-then-elevate
pattern that `hosts_plan` / `hosts_apply` already established. Today a fresh install on Linux
gets a browser warning and no explanation of why.

**3. Xdebug toggle per project** · impact high · effort **low-medium**
Also cheaper than it first appeared — the catalog entry exists. What is left is a switch that
adds the extension, writes the ini with `client_host` and a path mapping derived from the bind
mount, and offers the IDE configuration to copy. EnvKit's on-demand-only default is the right
one; Xdebug should never be left compiled in and listening.

**4. Database backup, restore and snapshot** · impact high · effort low
A true `❌`, and small: wrap `mysqldump` / `pg_dump` through `docker exec`, write to a chosen
path, restore the same way. Credentials are already read from `.env` and rendered in
`Services.vue`, so the surrounding UI exists. Add "open Adminer against this database" and
"copy connection string" alongside. Lerd sells snapshots; Laragon sells automatic backups.

**5. Mailhog → Mailpit, with a panel in the app** · impact medium-high · effort low
Swap an unmaintained image for the one everyone else ships, and render the inbox in-app instead
of sending the user to a browser tab. Cheap, and visible on the first send.

### P1 — differentiation

**6. MCP server** · impact very high · effort **low** ⭐ *best ratio on this list*
Five of eight competitors ship one. For StackVo it is close to mechanical: `contracts/ipc.json`
already describes all 66 commands, their arguments, their return types and — critically — their
`kind` (`query` / `mutation` / `operation` / `stream`). The tool list can be generated from it:
expose `query` commands directly, put mutations behind confirmation. Contract suites E and F
already enforce that the IPC surface and the code agree; the same mechanism can cover the MCP
surface, so it cannot drift. No competitor derives its MCP surface from a checked contract.

**7. Log viewer: search, filter, and the application's own log** · impact high · effort medium ·
**mostly delivered**
`LogView.vue` now has substring search, level filtering with per-level counts, and a source picker
carrying both log roots — `app_logs` discovers 52 files across the 8 projects on this checkout,
where previously *none* of them were reachable from the app. The container stream is stdout and
stderr, which is what the entrypoint and the web server say; nothing an application records goes
there, so a Laravel exception, an nginx 502 and a dead queue worker were all invisible.

Read from the host rather than through `docker exec`, because the generated compose already mounts
both trees (`projects/<name>` at `/var/www/html`, `logs/projects/<name>` at `/var/log`). That is
not a shortcut: it needs no engine, and a container that crashed on boot is exactly when its log
matters and exactly when there is nothing left to exec into.

Two details carried the design. Discovery **recurses**, because real Laravel channels nest and
roll over daily (`storage/logs/parser/parser-2026-07-28.log` is a real path here) — a flat listing
found 1 of the 27 files `parser.ajans` writes. And level filtering **inherits**: a stack trace is
a dozen lines that declare nothing under one line that declared ERROR, so filtering to errors
without inheritance returns the line saying something broke and none of the lines saying where.
On the real `laravel.log`, 6 entries span 78 lines.

Paths never cross the IPC boundary — the UI gets an opaque `app:`/`server:` handle, since a log
viewer that accepts an absolute path from its own frontend is a file reader for the whole disk.

Still open: the cross-project view, and click-through to the editor.

**8. Finish the doctor** · impact high · effort medium
Extend `preflight::fix()` from two checks to six, then add what is missing entirely: port
conflict detection naming the offending process, hosts repair, engine start, generator re-run,
and reclaiming space from dangling images and volumes. Lerd ships "doctor", EnvKit ships
"self-healing", ServBay ships port-conflict checking. StackVo diagnoses well and repairs barely.

> **A shipped feature that did nothing.** `cert_apply` reissued the wildcard certificate,
> reported success, and left every browser on the old one. Traefik's file provider watches
> `generated/traefik/dynamic`; the certificates are not in it, and a `certFile` is read only while
> that directory is parsed. Measured on this checkout: Traefik up two days, serving a certificate
> a full day older than the one on disk. `apply` now rewrites the watched files with their own
> bytes to force a reparse, and reports `reloaded` so a failure to do so is visible. The rewrite
> must be **in place** — an atomic stage-and-rename was tested against the running proxy and was
> not picked up at all.

**9. Schema-driven config editing** · impact medium-high · effort low-medium · **delivered**
The manifest is now editable as a form, from the configuration pane the values are already read
in. The fields are literally the create drawer's — one `ProjectFormFields.vue` over one
`lib/manifest.js` — so the two surfaces cannot drift, which matters because the contract's write
rules (`extensions` last, one runtime block, the 50-entry parser cap) are things a form can break
while still producing valid JSON. Saving is followed by regenerate-then-rebuild, in that order:
`compose up --build` alone builds from a Dockerfile rendered before the edit, so it would succeed
and change nothing.

Two claims in the original entry did not survive contact with the code. `memory_limit` and
`upload_max_filesize` are not manifest keys and cannot become them — the schema is
`additionalProperties: false` — and the `.stackvo/php.ini` that the docs and the old web UI both
advertise is mounted by no generator in `core/cli`. A PHP-ini form would have written to a file
nothing reads. Left out, and recorded below as its own item.

**10. Tunnel / shareable public URL** · impact medium-high · effort low-medium
Webhook testing (Stripe, GitHub) has no answer today. With Traefik already in front, adding a
Cloudflare Tunnel or ngrok sidecar per project is easier here than in any host-binary product.

### P2 — breadth

**11. The four missing runtime generators (Python, Go, Ruby, Rust)** · impact high · effort high
Bug and feature at once: `C-02`. Being container-based is a real advantage here — a native
competitor must package and maintain a binary per language and version, whereas StackVo needs a
Dockerfile template. FlyEnv sells 12 languages, ServBay 8. Closing this also stops the UI
offering four choices that cannot build.

**12. Migration assistant** · impact medium-high · effort medium
Herd publishes three migration guides; EnvKit bulk-imports from Laragon. The variant that pays
off here is different: point at an existing `docker-compose.yml` or project folder and emit a
`stackvo.json`.

**13. Cron and queue worker management, with self-healing** · impact medium-high · effort medium
Lerd's worker self-heal (queue, scheduler, Horizon, Reverb) is a genuine differentiator; EnvKit
adds run history. Daily value for any Laravel user, and container restart policies are already
in hand.

**14. Presets and export/import** · impact medium · effort low-medium
Turn the existing commit-friendly `stackvo.json` into a flow: export a stack preset, import a
teammate's, switch between stacks the way Laragon's profiles do.

**15. Proxy the Node dev server through Traefik with HMR** · impact medium · effort low-medium
EnvKit does this. It is what makes `runtime: node` genuinely usable — a Vite or Next project
served with hot reload on its `.loc` domain.

**16. Disk and image hygiene** · impact medium · effort low
Per-project disk usage, dangling image and volume pruning, "reclaim space".
`docker_system_resources` already exists. **No native-binary competitor can ever ship this** —
it is the cheapest way to turn being container-based into something visible.

### P3 — later, or selectively

| # | Item | Note |
| --- | --- | --- |
| 16b | Make `.stackvo/php.ini` real | `docs/*/configuration/project.md` documents it and the old web UI's `DockerService.js:388` lists it as a project config file, but `php.ini` appears **nowhere** in `core/cli` — no generator mounts it, so dropping one in does nothing today. Every competitor exposes `memory_limit` and `upload_max_filesize`. The compose overlay built for Xdebug already layers a fourth `-f` file and could carry the mount; the alternative is a generator change, which the byte-for-byte contract forbids unilaterally. Only then is a PHP-ini form worth building. |
| 17 | Profiler UI (Blackfire / SPX, flame graph) | The Blackfire template already ships; activation and a result view are missing. |
| 18 | Dump / `dd()` catcher | Three competitors have it, but it needs a collector service and a Composer package. Expensive. |
| 19 | Tinker quick action | The PTY exists, so an `artisan tinker` button is nearly free. Lerd's Monaco-plus-LSP REPL is not. |
| 20 | Export a production image | What `laradock ship` does. The container lineage makes it possible and **no native-binary competitor can follow.** A long-horizon differentiator. |
| 21 | ACME / real certificates, wildcards | ServBay ships it; marginal for local development. |

### Not worth chasing

- **FlyEnv's 50+ utilities** (base64, QR codes, regex testers). Unfocused, and free in any browser.
- **ServBay's and FlyEnv's AI Gateway / LLM proxy.** Out of scope. MCP yes (P1-6); an LLM
  provider proxy no.
- **The native-binary speed war.** A losing fight. StackVo's answer is reproducibility, not
  milliseconds.
- **Portable mode** (Laragon, ForgeKit). Meaningless with a Docker dependency.
- **Paywalling the basics.** EnvKit and ForgeKit are attacking Herd's $99/yr by being wholly
  free. For an MIT project that is the side to be on.

## 4. Sequencing

| Sprint | Items | Theme |
| --- | --- | --- |
| 1 | P0-2 certificates · P0-3 Xdebug · P0-4 DB backup · P0-5 Mailpit | Daily friction. Three of the four are cheaper than the first pass assumed. |
| 2 | ✅ P1-6 MCP · ⚠️ P0-1 scaffolding (import half) · ✅ P1-9 config forms | Leverage, then onboarding. Sprint 2 is done except the create half of P0-1. |
| 3 | ✅ P1-7 log viewer · P1-8 doctor · P2-16 disk hygiene · P1-10 tunnel | Depth, and the container-only advantages. |

`C-02` (P2-11) is scheduled separately: it is a Phase 4 generator concern, not a UI one, and
belongs with the Rust generator port rather than in a feature sprint.

## 5. What is left

Written down because three of these are *halves* of items marked delivered, and a half-finished
item that reads as done is how a roadmap starts lying. The full-sized items keep their P-numbers
from section 3; the partial ones are named for what remains, not for what shipped.

### Carried over from delivered items

| # | What remains | Why it was left | Cost |
| --- | --- | --- | --- |
| **P0-1b** | **Scaffolding — the create half.** Run `composer create-project` / `npm create` for a brand-new Laravel, WordPress, Symfony or Next.js in a throwaway container, then adopt the result. | Detection shipped first deliberately: it is both the harder direction and the more used one, since people already have code. Scaffolding is a container run layered on top of machinery that now exists. | Low-medium |
| **P1-7b** | **Cross-project log view**, and **click-through from a stack frame to the editor**. `open_in_editor` is already wired; the missing piece is mapping a container path (`/var/www/html/app/…`) back to a host path — which the bind mount already states, so this is a substitution, not a search. | The per-project viewer was the part that made 52 previously unreachable files readable. A cross-project tail is a second stream multiplexer on top of that. | Low-medium |
| **P2-16b** | **Make `.stackvo/php.ini` real**, then add the PHP-ini form that P1-9 promised (`memory_limit`, `upload_max_filesize`). | Cut from P1-9 on evidence: the manifest schema is `additionalProperties: false`, and `php.ini` appears nowhere in `core/cli` — the docs and the old web UI advertise `.stackvo/php.ini`, but no generator mounts it. A form would have written to a file nothing reads. | Low-medium |

The mount itself is the cheap part: the compose overlay built for Xdebug already layers a fourth
`-f` file and is re-derived on every invocation, so it can carry a conditional `php.ini` mount
without touching the Bash generator — which the byte-for-byte contract forbids changing
unilaterally.

### Not yet started

| # | Item | Note |
| --- | --- | --- |
| **P1-8** | Doctor with repair actions | Next up. `preflight::fix()` covers two of six checks; missing entirely are port-conflict detection naming the offending process, hosts repair, engine start, generator re-run, and reclaiming space. |
| **P2-16** | Disk and image hygiene | `docker_system_resources` already exists. **No native-binary competitor can ship this at all.** |
| **P1-10** | Tunnel / shareable public URL | Webhook testing has no answer today. Traefik is already in front. |
| **P2-13** | Cron and queue worker management | The worker logs are now readable (P1-7); managing the workers is not. |
| **P2-11** | The four missing runtime generators | High effort, and gated on `C-02`. |
| **P2-12**, **P2-14**, **P2-15** | Migration assistant · presets and export/import · Node HMR through Traefik | Unchanged from section 3. |
| **P3** | 17–21 | Unchanged: profiler UI, dump catcher, tinker action, production image export, ACME. |

### Debt found while building, not yet paid

Two of these were shipped features that did nothing, found by testing against the running stack
rather than by reading the code. Both are fixed; they are recorded because the *method* is the
finding — neither was visible from the source alone.

- **Fixed.** `cert_apply` reissued the certificate and left every browser on the old one: Traefik
  watches `generated/traefik/dynamic`, and the certificates are not in it. Measured live —
  Traefik up two days, serving a certificate a day older than the one on disk. The rewrite that
  fixes it must be **in place**; an atomic stage-and-rename was tested against the running proxy
  and was not picked up at all.
- **Fixed.** `xdebug::dockerfile_path` read `projects/<name>/Dockerfile`, which is a user's own
  file. Compose builds `generated/projects/<name>/Dockerfile`. `needsRebuild` was therefore true
  permanently, for the wrong reason, no matter how often the image was rebuilt.
- **Open.** Two stale assertions in `tests/app-shell.spec.js` still look for the quick actions in
  the nav drawer; they moved to the app bar in `100a2d4`, and they are `:title` attributes that
  `wrapper.text()` cannot see. Two failures, pre-existing, untouched.
- **Open.** `contracts:check` reports 4 errors, down from 8 — the four fixed were commands
  registered in `lib.rs` and absent from `ipc.json`. The remaining four are upstream StackVo
  contract conflicts, not app defects.
- **Open, upstream.** Swapping the MailHog template to Mailpit in the `stackvo` repository
  renames the service, its `.env` keys, the container and the volume — a migration for every
  running stack. Needs an explicit decision before it is worth doing.
