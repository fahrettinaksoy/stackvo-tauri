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
| Framework scaffold / quick app | ✅ | ✅ detect | – | – | – | – | ✅ | ⚠️ detect | ⚠️ Bitnami | **✅** |
| Import an existing setup | ✅ guides | – | ✅ Laragon | – | – | – | – | – | – | **✅ compose** |
| DB backup / restore / snapshot | – | ✅ | – | – | ✅ | – | ✅ | – | – | **✅** |
| Mail catcher with a UI | ✅ | – | ✅ | ✅ | ✅ server | ✅ | – | ✅ | – | **✅** |
| Xdebug toggle | ✅ | – | ✅ | – | – | ✅ | ✅ | ✅ | ✅ | **✅** |
| **MCP server** | ✅ | ✅ | ✅ | ✅ | ✅ | – | – | – | – | **✅** |
| Unified log viewer + search | ✅ | ✅ | ✅ | ✅ | ✅ | – | ✅ | – | – | **✅ + cross-project** |
| Tunnel / shareable public URL | ✅ | – | – | ✅ | ✅ | – | ✅ | – | – | **✅** |
| Doctor with repair actions | – | ✅ | ✅ | – | ✅ | – | – | – | – | **✅** |
| Cron / queue worker management | – | ✅ heal | ✅ | ✅ | – | – | – | ✅ | – | **✅ heal** |
| Dump / `dd()` catcher | ✅ | ✅ | ✅ | – | – | – | – | – | – | **✅** |
| Profiler | ✅ | ✅ SPX | – | – | – | – | – | ✅ | – | **✅ Xdebug** |
| Team config sharing | ✅ | – | – | ✅ sync | ✅ | – | ✅ | ✅ | – | **✅** |
| Runtimes beyond PHP | Node | polyglot | +Py | **12** | 8 | – | +Py/Go | 100+ svc | Perl | **❌ 2 of 6** |
| Config editing from the UI | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | – | – | ⚠️ | **✅** |
| PHP limits (`memory_limit`, uploads) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | ✅ | **✅** |
| Host metrics, disk I/O, network | – | – | – | – | – | – | – | – | – | **✅ only one** |
| Reviewed elevated hosts write | – | – | – | – | ✅ | – | ✅ auto | – | – | **✅** |
| Byte-verified config generator | – | – | – | – | – | – | – | – | – | **✅ only one** |
| Container + host PTY | – | ✅ TUI | – | – | – | ✅ | ✅ | ✅ | – | **✅** |
| Heavy services (Kafka, ES, Cassandra) | – | – | – | ⚠️ | – | – | – | ✅ | – | **✅** |
| Node dev server with hot reload | ✅ | – | ✅ | – | – | – | – | – | – | **✅** |
| Build a deployable production image | – | – | – | – | – | – | – | ✅ ship | – | **✅ + verified** |

### 2.1 The StackVo column, cell by cell

Each `⚠️` hides a decision, so they are itemised. The `⚠️` rows are cheaper than they look; the
`❌` rows are the real work.

> **Shipped since this was written.** Sprint 1 is complete — P0-2
> (certificates), P0-3 (Xdebug), P0-4 (database backup) and P0-5 (the in-app
> inbox) — Sprint 2 shipped P1-6 (the MCP server), Sprint 3 is complete —
> P1-7 (the log viewer), P1-8 (the doctor), P2-16 (disk hygiene, both the
> prune and the per-member attribution), P1-10 (the tunnel) — Sprint 4
> delivered P2-13 (workers), the create half of P0-1 (scaffolding) and
> the click-through half of P1-7b, and **Sprint 5 closed the two halves that
> were still open**: the cross-project log view (P1-7b) and `.stackvo/php.ini`
> with the form P1-9 had to cut (P2-16b). **Sprint 6 shipped P2-14 (stack presets)**, which
> takes the last `⚠️` out of the matrix bar the profiler, and **Sprint 7 shipped P2-12** — the
> `docker-compose.yml` half of the migration assistant, and **Sprint 8 shipped P2-15** — the node
> dev server, which turned out not to be a routing change at all, and **Sprint 9 shipped P3-19** —
> the project command palette, and **Sprint 10 shipped P3-17** — the profiler, via Xdebug rather
> than either tool the item named, and **Sprint 11 shipped P3-20** — the production image, which
> turned out to be a build rather than an export, and **Sprint 12 shipped P3-18** — the dump
> catcher, which was rated expensive and was not. Their rows above read `✅`. One qualification on the MCP row: the read surface is complete, and
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
| **Doctor** | ✅ | **Since delivered.** `doctor.rs` reports the gate rows plus the failures that arrive later, each next to its repair in Settings → Doctor: port conflicts with the culprit *named* (the stack's own container is fine, someone else's container is named, a host process is named with pid — one `lsof`/`ss`/`netstat` spawn for the whole table), generated config older than its inputs (oldest-output vs newest-input mtime, so one fresh file cannot mask a stale one), hosts gaps routed through the reviewed-diff dialog, and reclaimable space with `docker_prune` behind a confirmation — volumes opt-in with their own warning, because the engine's "unused" means "not currently mounted" and a stopped project's database qualifies. Exposed as the `stackvo_doctor` MCP tool and in `npm run diagnose`, which found real findings on first run: a stale `parser.ajans` manifest and 13.8 GB reclaimable. | Port conflicts are diagnosed, not repaired — killing someone else's process is not a button this app should have. |
| **Log viewer** | ✅ | **Since delivered, now including the cross-project view.** `LogView.vue` has search, level filtering with counts, and a picker over both log roots; `app_logs` / `app_log_open` read them from the host, so they work with the engine down. Click-through shipped in Sprint 4. The cross-project tail is Sprint 5: a Logs destination of its own, one `Fanout` across every project — measured on this checkout, 52 files across 11 projects, all under the 60-file cap. | Nothing outstanding. Two constraints are stated on screen rather than hidden: the tail is live-only, and it follows at most 60 files. |
| **PHP limits** | ✅ | **Since delivered (Sprint 5).** `memory_limit`, `upload_max_filesize`, `post_max_size` and `max_execution_time` as a form over a real `.stackvo/php.ini`, mounted read-only at `/usr/local/etc/php/conf.d/zz-stackvo.ini` by a fifth compose overlay. Verified end to end against a real StackVo project image: 128M → 777M, 2M → 64M. | Only four directives are managed. Anything else in the file is preserved verbatim and shown, but is edited by hand. |
| **Mail** | ⚠️ | `core/templates/services/mailhog/` ships and starts. | mailhog is unmaintained upstream; Mailpit is what everyone else ships. No inbox in the app — the user leaves for a browser tab. **Since resolved in the app**: the inbox reads both APIs, so a checkout on either image gets one. The image swap itself remains an upstream change — it renames a service, its `.env` keys, its container and its volume, which is a migration for every running stack, not a one-line edit. |
| **Profiler** | ✅ | **Since delivered (Sprint 10), by a different route than the item proposed.** Blackfire needs an account and SPX is not in the extension contract; Xdebug already is, and `xdebug.mode=profile` writes cachegrind files. A generated ini sends them into the mounted log tree, and a cachegrind parser aggregates them into a top-function table. | Self cost and per-call inclusive cost, not a flame graph — the call *tree* would need the caller edges reconstructed, which is a much larger promise. `core/templates/services/blackfire/` still ships and is still unwired. |
| **Team config sharing** | ✅ | **Since delivered (Sprint 6).** The roadmap said "turn the commit-friendly `stackvo.json` into a flow"; read against the code that framing was wrong. `stackvo.json` needs no flow — it is already in the teammate's clone. What they do *not* get is the **stack**: which of the twenty services are on and at which versions, which lives in `.env`, the one file nobody commits, because it is also where every password is. A preset carries that and, by construction, has nowhere to put a secret. Import is plan-then-apply, like the hosts file and the certificate. | Ports and paths are deliberately excluded — they are properties of one machine, and importing somebody else's is how two people end up fighting over 3306. |
| **Node dev server** | ✅ | **Since delivered (Sprint 8).** A compose overlay that mounts the source, keeps `node_modules` in an anonymous volume, and replaces the container's command with the dev server. Plus the `vite.config.js` snippet the project itself needs, generated with its domain in it. | The snippet is shown, never written — it is the user's build config. Anything that is not Vite, Nuxt or Next gets the mount but no config advice. |
| **Runtimes** | ❌ | `C-02`, re-read in Sprint 9 and **half of it is stale**. `.env` still advertises `php,python,go,ruby,rust,nodejs`, but *this app* does not render all six: `commands.rs` marks each runtime `available` against `IMPLEMENTED_RUNTIMES = ["php", "node"]`, and the project form only ever emits one of those two. The "UI offers four choices that cannot build" half described the old web UI and was already fixed here. | What remains is the real gap: `core/cli/lib/generators/project/` holds generators for PHP servers and node only. Four of six languages have no Dockerfile template. Still a competitive gap; no longer a shipped bug in the desktop app. |
| **Migration** | ✅ | **Since delivered (Sprint 7).** A folder's own `docker-compose.yml`, read by `docker compose config --format json` — Docker parses its own format, so no YAML dependency was added. Yields the PHP version, domain, extensions, document root and, the part no marker file states, the backing services, as the same reviewed diff a preset import shows. | Image matching is exact, not fuzzy: `ghcr.io/acme/redis-shim` is not Redis. Anything unrecognised is named rather than dropped. |
| **Scaffold** | ✅ | **Both halves delivered.** Import first: detection infers runtime, server, document root and PHP/Node version from `artisan`, `wp-config.php`, `composer.json` and `package.json` — on this checkout 11 of 21 directories were unmanaged before it. The create half followed as predicted, "a container run on top": `project_scaffold` runs the framework's own installer (composer create-project for Laravel/Symfony, wp-cli, create-next-app) in a throwaway `--rm` container — nothing installed on the host, `--user uid:gid` on unix so nothing lands root-owned, every command pinned non-interactive because a prompt inside an operation console is a hang. Then the *same adoption path* configures the project from what the installer wrote. | Framework choice is the four with one-command installers; anything else arrives by clone and adoption. |
| **Tunnel** | ✅ | **Since delivered.** A cloudflared *quick tunnel* as a sidecar container on the stack network — no account, no token; Cloudflare assigns a random `trycloudflare.com` URL. The sidecar targets the project container directly with the local domain as Host header, because with SSL on every Traefik project router is `websecure`-only, which a public visitor cannot handshake against. The URL is never cached: it is read out of the sidecar's own log on every status call, so app restarts and crashed tunnels stay truthful for free. Start refuses when the project container is down (a tunnel serving 502s looks like it worked), the UI states plainly that the URL is public and unauthenticated, and `--rm` means stop is also removal. | Verified by unit tests against the real cloudflared banner format; a live end-to-end start was deliberately not run unattended — it would expose a real project publicly. |
| **Workers / cron** | ✅ | **Since delivered.** Each worker (queue, scheduler, Horizon) is a sidecar container built from the project's *own image* — same PHP, extensions, bind mount and network, so `.env` and the database resolve exactly as the web container sees them. Self-healing is deliberately not reimplemented: `--restart unless-stopped` is Docker's own supervisor, and the restart count is read back and shown, so a crash loop is a number on screen. Detection is file-based (`artisan` → queue + scheduler; `laravel/horizon` in composer.json → Horizon); `schedule:work` replaces a host cron entry outright. Stop is removal, because with unless-stopped a merely-stopped container is one engine restart away from coming back. | Run history (EnvKit ships it) is not kept; the worker's own log is one click away in the container logs pane. |
| **Dumps** | ❌ | — | No command, no template, no plan. |

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
- **A production image built from the development one** (Sprint 11). The lineage is the point: the
  PHP version, the extensions and the web server in the artefact are the ones that were developed
  against, because the image is literally `FROM` the one the project runs. A native-binary
  competitor has no image to start from, so it cannot offer this at all.
- **Container and host PTY**, both from the app.

## 3. Priority order

Ordered by impact ÷ effort against the verified state.

### P0 — the cost of entry

**1. Project scaffolding and framework detection** · impact very high · effort medium · **delivered** (Sprints 1–4)
The largest genuine `❌`. Two directions are needed: creating a project should be able to run
`composer create-project` / `npm create` for Laravel, WordPress, Symfony or Next.js inside the
container; and importing an existing folder should infer `runtime`, `server` and `document_root`
by looking for `artisan`, `wp-config.php` or `package.json`. Laragon's "Quick app" and Lerd's
nine-framework detection carry their entire onboarding story. `NewProjectDrawer.vue` and
`project_create` are already in place — the generation step is what is absent.

**2. Bring certificates into the app** · impact very high · effort **medium-low** ⭐ · **delivered** (Sprint 1)
Re-scoped by verification: mkcert already works, so this is surfacing, not building.
Add a mkcert check to `preflight.rs`; add `cert_status` / `cert_plan` / `cert_apply` to the IPC
contract; regenerate automatically when a project is created or its domain changes; extend trust
to Linux and Windows (`update-ca-certificates`, `certutil`) using the reviewed-diff-then-elevate
pattern that `hosts_plan` / `hosts_apply` already established. Today a fresh install on Linux
gets a browser warning and no explanation of why.

**3. Xdebug toggle per project** · impact high · effort **low-medium** · **delivered** (Sprint 1; the profile mode followed in Sprint 10)
Also cheaper than it first appeared — the catalog entry exists. What is left is a switch that
adds the extension, writes the ini with `client_host` and a path mapping derived from the bind
mount, and offers the IDE configuration to copy. EnvKit's on-demand-only default is the right
one; Xdebug should never be left compiled in and listening.

**4. Database backup, restore and snapshot** · impact high · effort low · **delivered** (Sprint 1)
A true `❌`, and small: wrap `mysqldump` / `pg_dump` through `docker exec`, write to a chosen
path, restore the same way. Credentials are already read from `.env` and rendered in
`Services.vue`, so the surrounding UI exists. Add "open Adminer against this database" and
"copy connection string" alongside. Lerd sells snapshots; Laragon sells automatic backups.

**5. Mailhog → Mailpit, with a panel in the app** · impact medium-high · effort low · **partly delivered** (Sprint 1)
Swap an unmaintained image for the one everyone else ships, and render the inbox in-app instead
of sending the user to a browser tab. Cheap, and visible on the first send.

### P1 — differentiation

**6. MCP server** · impact very high · effort **low** ⭐ *best ratio on this list* · **partly delivered** (Sprint 2)
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

Click-through shipped with Sprint 4 — a stack-frame path in a line opens the file in the editor,
by the substitution the bind mount states.

**The cross-project view shipped with Sprint 5**, as a Logs destination of its own — it belongs to
no project, which is the point: it is where you look *before* you know which project to open. Two
decisions are worth recording, because both are constraints stated on screen rather than hidden.

It is **live only, with no history**. Nothing in `applog` parses a timestamp — Laravel, nginx and
supervisord do not agree on a format — so the only chronology available across sixty files is the
order bytes arrive in: real for new output, invented for old. Interleaving the existing tails of
sixty files would present an ordering the code cannot justify, so each file is adopted at its
current end. History stays in the per-project viewer, which reads one file and can honestly show
all of it. The counterpart is rediscovery every thirty seconds: a daily channel rolls over into a
filename that did not exist when the tail started, so a fixed file set would go quiet exactly at
midnight. Files found by a later scan are adopted at offset zero, because everything in a file that
new was written while the tail was already watching.

And it follows at most 60 files, with what it dropped reported — `following 52 of 52 files · 11
projects` on this checkout. A view that caps its own coverage and says nothing reads as "nothing
else is happening".

One correctness detail the single-stream viewer never had to face: **level inheritance is now
per origin**. A stack trace is a dozen lines that declare nothing under one line that declared
ERROR, and in a multiplexed buffer the line above a frame is routinely from another project — a
single running level would paint one project's INFO with another's ERROR and then hide it under a
filter that has no idea it is looking at the wrong file.

**8. Finish the doctor** · impact high · effort medium · **delivered**
Every repair named in the original scope now exists, but as six repair *actions* rather than six
arms in one function — deliberately, because three of them already had confirmation flows that a
blind `fix(id)` would have bypassed: hosts repair opens the reviewed diff before the one elevated
write, space reclaim confirms first and keeps volumes opt-in behind their own warning, and
generator re-run goes through `generate_run`'s operation events. `preflight::fix()` itself gained
`engine` (for headless callers; the UI keeps `engine_start`'s start-and-poll). Port conflicts —
the check no gate had — read the ports from the generated compose files (which *are* the enabled
set, since the generator only writes enabled services) and name the holder: the stack's own
container, someone else's container, or a host process with pid. Lerd ships "doctor", EnvKit
ships "self-healing", ServBay ships port-conflict checking; none of them name the process.

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
nothing reads. Left out, and carried as P2-16b. **Sprint 5 closed it the only way that was
honest: the mount first, the form second.** The manifest schema still cannot hold these keys and
was not asked to — they live in a real ini file, mounted by a compose overlay, which is a
different mechanism for a different kind of setting.

**10. Tunnel / shareable public URL** · impact medium-high · effort low-medium · **delivered**
A cloudflared quick-tunnel sidecar per project, from a Share pane in the project detail. One
design note against the original sketch: the sidecar does *not* go through Traefik — with SSL on,
every project router is `websecure`-only, and a public visitor cannot complete a TLS handshake
against a hostname no DNS resolves. It targets the project container directly on the port the
generator derives (node → manifest port, Swoole → 8000, else 80), with the local domain as the
Host header so framework URL checks behave as they do locally. The container-network attachment
is the part no host-binary competitor can reproduce.

### P2 — breadth

**11. The four missing runtime generators (Python, Go, Ruby, Rust)** · impact high · effort high
`C-02`. Being container-based is a real advantage here — a native competitor must package and
maintain a binary per language and version, whereas StackVo needs a Dockerfile template. FlyEnv
sells 12 languages, ServBay 8.

The last clause of this entry — "closing this also stops the UI offering four choices that cannot
build" — was checked in Sprint 9 and **is no longer true of this app**. `build_catalog` marks every
runtime `available` against `IMPLEMENTED_RUNTIMES = ["php", "node"]`, and the project form emits
only those two. The UI half was the old web UI's problem. What is left is purely the generator
work, which is what section 4 already says about scheduling it.

**12. Migration assistant** · impact medium-high · effort medium · **delivered**
Herd publishes three migration guides; EnvKit bulk-imports from Laragon. The variant that pays off
here was always the other one: point at an existing project and emit a `stackvo.json`. The *folder*
half shipped with adoption in Sprint 4. The `docker-compose.yml` half is Sprint 7.

What it adds over detection is the part detection cannot see. Detection reads the **code** —
`artisan`, `wp-config.php`, `composer.json` — and gets runtime, framework and document root. A
compose file records what its author **decided**: the PHP version, the domain, the extensions, and
the one thing with no equivalent in any marker file — *which backing services the project needs*. A
`docker-compose.yml` with `mysql:8.0` and `redis:7.2` is a statement about the stack. Adopting
without reading it produces a project that builds, starts, and cannot reach its database.

Where both have an answer the compose file wins, because a guess loses to a declaration.

**Docker does the parsing.** `docker compose config --format json` is the reference implementation
of the Compose spec — it resolves anchors, `extends`, `.env` interpolation and profiles, and
normalises every shorthand. The alternative was a YAML crate, and that was the wrong trade twice
over: `serde_yaml` is archived by its author and `deny.toml` here says a *direct* dependency going
unmaintained still fails the build, and a hand parser would be wrong on real files anyway.
(`xdebug::generated_services` parses YAML by indentation only because it reads a file this project
generated itself, whose shape is fixed. A user's compose file is arbitrary YAML written by somebody
else.) Compose is already a hard preflight requirement, so this adds **no new dependency at all**.

Three kinds of compose service, told apart because they become three different things: the one with
a `build:` is the application and becomes the manifest; nginx/apache/caddy become the manifest's
`server` field and *not* a StackVo service, since StackVo runs the web server inside the project
container and importing it as a sidecar would give the project two; catalog images become services
to enable, expressed as the same reviewed diff a preset import shows. Everything else is **named**
in `unmapped`.

**13. Cron and queue worker management, with self-healing** · impact medium-high · effort medium ·
**delivered**
Queue, scheduler and Horizon as sidecars of the project's own image, healed by Docker's
`unless-stopped` restart policy rather than by a reimplemented supervisor — the restart count is
surfaced so the healing is visible and a crash loop cannot wear a green chip. Reverb was left
out on purpose (it is a WebSocket *server*, which wants a port and Traefik routing, not a worker
slot); run history likewise — the worker's log is one click away.

**14. Presets and export/import** · impact medium · effort low-medium · **delivered**
The original wording — "turn the existing commit-friendly `stackvo.json` into a flow" — did not
survive reading the code, and the correction is the whole feature. `stackvo.json` needs no flow: it
is already in the project directory, already schema-validated, and a teammate who clones the
repository already has it. Exporting it would be a button that copies a file out of a repository
into the same repository.

What the teammate does *not* get is the **stack** — which of the twenty services are enabled and at
which versions. That lives in `<root>/.env`, the one file nobody commits, precisely because it is
also where every password is. So the clone succeeds, the manifest is perfect, and the project still
will not start until somebody says out loud "you need MySQL 8.0, Redis and Elasticsearch on". That
sentence is the preset.

**A preset can never carry a secret, and that is enforced by construction rather than by
filtering.** A service entry has two named fields, `enabled` and `version` — not a passthrough map
— so there is no code path by which `SERVICE_MYSQL_ROOT_PASSWORD` reaches one; it is not that a
filter drops it, it is that there is nowhere to put it. Global settings are an **allow-list** of
five keys, not a deny-list, and the list is re-checked on *import* as well as on export. A
deny-list would have been wrong even though `Env::is_secret` exists: it matches on suffix, so a key
added upstream tomorrow called `SERVICE_FOO_APIKEY` would sail straight through. An allow-list
fails closed against a file this app does not own.

Import is plan-then-apply, the shape `hosts_plan`/`hosts_apply` and `cert_plan`/`cert_apply`
already established, and rejections are **named** rather than dropped — a preset that quietly skips
half of what it was given is how somebody concludes it worked and then loses an afternoon to the
service it ignored.

**15. Proxy the Node dev server through Traefik with HMR** · impact medium · effort low-medium ·
**delivered**
Written as a routing change. Reading `core/cli/lib/generators/project/compose/node.sh` against the
other five server generators turns up something larger.

**A node project has no bind mount at all.** `nginx.sh`, `caddy.sh`, `apache.sh`, `swoole.sh` and
`frankenphp.sh` all call `generate_common_volumes`; `node.sh` calls nothing, and its Dockerfile does
`COPY . .` and `RUN npm install` at build time. The container holds a *snapshot* of the source taken
when the image was built. Hot reload was not misconfigured — it was **structurally impossible**, and
no amount of WebSocket plumbing helps when there is nothing to reload. `runtime: node` is a
production-style container; that is a legitimate mode and it stays the default. What was missing is
the other one.

Three things have to be true at once, and they are reported separately because they fail
separately:

1. **The source is live** — a bind over `/app`, plus an **anonymous volume on
   `/app/node_modules`**. That second line is load-bearing, and it was verified rather than
   assumed: with the bind alone, `require()` inside the container fails with `MODULE_NOT_FOUND`
   because the mount hides the install the image did for its own platform; with the anonymous
   volume, the module loads. Measured on a real image, both ways.
2. **The dev server is what is running** — the overlay *replaces* the container's `command`, which
   is checked against Docker's own merge rather than assumed (an appended command would run the
   production entrypoint and the dev server one after the other).
3. **The dev server accepts the request** — and this one is in the *user's* repository.

That third one is worth its own paragraph, because it is the failure nobody can diagnose. Read out
of Vite's own source — `hostValidationMiddleware`, verified against the copy in this repository's
`node_modules` — a request whose `Host` is not `localhost`, not `*.localhost`, not an IP literal and
not matched by `server.allowedHosts` gets a flat **403**. A `.loc` domain is exactly that case, so
the symptom is a site that is plainly up and returns 403, with nothing in any log pointing at the
dev server's config. The HMR client is the second half: StackVo's node router is `websecure`-only
with `tls=true`, so the page loads over 443 while the dev server listens on 3000, and Vite's client
would dial `wss://shop.loc:3000` — which nothing routes — and degrade silently to full page reloads.

Both live in `vite.config.js`. The app generates the exact snippet with the project's domain already
in it, and **stops there**. Silently rewriting somebody's build config is not something a local
environment manager should ever do, and a snippet they paste is one they have read.

End to end on a real image: an edit on the host reached the container with no rebuild —
`serving v1` → `serving v2 EDITED ON HOST`, `--no-build`.

**16. Disk and image hygiene** · impact medium · effort low · **delivered**
Both halves shipped inside the Doctor's Disk group. The prune half came with P1-8 (`docker_prune`,
volumes opt-in behind their own warning). The attribution half is `docker_disk_usage`: every stack
member with its image size and writable layer, shared upstream images marked as such (removing the
member cannot free them), and **orphaned builds** — images this stack produced whose container is
gone, which no list in the app showed and which are exactly the bytes nobody remembers spending.
On this checkout the table's first row was a 2.0 GB project image with a 63 MB writable layer.
**No native-binary competitor can ever ship this** — it is the cheapest way to turn being
container-based into something visible.

### P3 — later, or selectively

| # | Item | Note |
| --- | --- | --- |
| 16b | ✅ Make `.stackvo/php.ini` real | **Delivered in Sprint 5** — see §5 for what verification changed. |
| 17 | ✅ Profiler | **Delivered in Sprint 10** via Xdebug's `profile` mode — see below. A flame graph is still not on the list. |
| 18 | ✅ Dump / `dd()` catcher | **Delivered in Sprint 12.** The cost estimate here was wrong twice — both the collector *and* the Composer package already ship with Laravel. See below. |
| 19 | ✅ Project commands (was "tinker quick action") | **Delivered in Sprint 9**, as the set rather than the one button — see below. Lerd's Monaco-plus-LSP REPL is still not on the list. |
| 20 | ✅ Export a production image | **Delivered in Sprint 11.** What `laradock ship` does, and the item that turned out to have a security property at its centre — see below. |
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
| 2 | ✅ P1-6 MCP · ✅ P0-1 scaffolding (both halves) · ✅ P1-9 config forms | Leverage, then onboarding. The create half of P0-1 landed in Sprint 4. |
| 3 | ✅ P1-7 log viewer · ✅ P1-8 doctor · ✅ P2-16 disk hygiene · ✅ P1-10 tunnel | Depth, and the container-only advantages. **Sprint 3 is complete.** |
| 4 | ✅ P2-13 workers · ✅ P0-1b scaffold (create half) · ✅ P1-7b click-through | Laravel's daily loop, and onboarding's other half. **Sprint 4 is complete.** |
| 5 | ✅ P1-7b cross-project logs · ✅ P2-16b `php.ini` + form · ✅ test debt | Finishing the halves. **Sprint 5 is complete** — nothing marked delivered is now partly delivered, which is the state a roadmap has to be in before it is trusted. |
| 6 | ✅ P2-14 stack presets | **Sprint 6 is complete.** Team config sharing moves `⚠️` → `✅`; the only `⚠️` left in the matrix is the profiler, which is P3-17 and deliberately unscheduled. |
| 7 | ✅ P2-12 compose migration | **Sprint 7 is complete.** Onboarding from somebody else's setup, with no new dependency — Docker parses its own format. |
| 8 | ✅ P2-15 node dev server + HMR | **Sprint 8 is complete.** `runtime: node` had no bind mount at all, so this was not a routing change; the P2 list is now empty except `C-02`. |
| 9 | ✅ P3-19 project commands | **Sprint 9 is complete.** The daily loop, from a fixed catalog. Also re-read `C-02` and found half of it stale. |
| 10 | ✅ P3-17 profiler | **Sprint 10 is complete.** Xdebug's own profiler, because the two the item named both need something this project does not control. |
| 11 | ✅ P3-20 production image | **Sprint 11 is complete.** The differentiator, and the first feature here whose central property is a security one. |
| 12 | ✅ P3-18 dump catcher | **Sprint 12 is complete.** Rated expensive; measurement said otherwise. Only P3-21 is left, and P2-11 remains Phase 4 work. |
| 14 | ✅ Template renderer ported · dump-collector leak fixed | **Sprint 14.** The first real step of the Bash removal, chosen after measuring the port surface rather than guessing at it. See §7. |
| 13 | ✅ Extension check in the doctor, **with its repair** | **Sprint 13.** Not a roadmap item — it came from finally reading the validator's *output* instead of its summary line. See below; the correction is partly to this document's own reporting. |

`C-02` (P2-11) is scheduled separately: it is a Phase 4 generator concern, not a UI one, and
belongs with the Rust generator port rather than in a feature sprint.

## 5. What is left

**The carried-over section is empty as of Sprint 5.** Both halves that were listed here — the
cross-project log view and `.stackvo/php.ini` — are done, so nothing marked delivered above is
partly delivered. That was the whole reason this section existed: a half-finished item that reads
as done is how a roadmap starts lying.

Two things verification changed on the way, both worth carrying forward as method rather than as
trivia.

**The php.ini mount was the feature; the form was the easy part.** The plan said the Xdebug overlay
"could carry the mount". It could not — sharing one document means a fault in either takes the
other's projects down, and the two are independent (one adds `environment`, the other `volumes`).
So `php.ini` got its own fifth `-f`, re-derived on every compose invocation for the same reason
Xdebug's is: an overlay naming a deleted project declares a service with neither an image nor a
build context, and compose then refuses *every* command against the stack, including the `down`
that would clear it.

Then three assumptions did not survive the running stack:

- `php -i` in a real project container reports **`Loaded Configuration File => (none)`**. These
  images ship no `php.ini` at all, so `conf.d` is not one layer among several — it is the only one.
- The defaults are not the ones the manual lists. `max_execution_time` is **0** under FPM, not 30.
  The form's placeholders were going to say 30, which is telling the user something untrue about
  their own container; they are now *measured* from the running container through one
  `docker exec php -r ini_get`, and that reading is also where an override is confirmed to have
  landed after a restart.
- The first probe against the real checkout failed with *"service has neither an image nor a build
  context"* — the exact failure the module guards against. The cause was benign (this checkout's
  `docker-compose.projects.yml` is `services: {}` between a regenerate and a build), but it is the
  guard earning its place on the first attempt rather than in a bug report.

End to end, on a real StackVo project image: `memory_limit` 128M → 777M, `upload_max_filesize`
2M → 64M. Compose was confirmed to *merge* the overlay's volume onto the generated service rather
than replace it — the project's own `/var/www/html` bind survives alongside the new read-only
mount, which is the thing that would have silently broken every project had it gone the other way.

**Sprint 6 (P2-14) had the same shape: the roadmap's framing was the thing to fix.** "Turn
`stackvo.json` into a flow" describes work with no value — the file is already in the clone. The
gap was the stack, not the project, and naming it correctly is what made the feature small.

The verification that mattered ran against the real `.env`: export a preset, then assert that not
one of the twelve genuine secrets on this machine appears in it. It earned its place immediately by
failing for the **wrong** reason — `SERVICE_GRAFANA_ADMIN_PASSWORD=admin`, and `admin` is a
substring of the service ids `phpmyadmin`, `pgadmin` and `phpcacheadmin`, which a preset
legitimately contains. The tempting repair was to raise a length threshold; that would have
quietened a coincidence by letting a real five-character secret through. The right one was to
compare exactly against the strings actually in the document, which is what the claim always was.
The round trip is checked on real files too: saving this stack and planning it straight back
proposes zero changes, and flipping one service in the saved file produces exactly one line of
diff — measured, `SERVICE_ADMINER_ENABLE false → true`.

**Sprint 7 (P2-12) found a latent bug in code that predates it.** `parse_spec` validates an
incoming project spec, and two of the contract's rules — W-01 (`php.extensions` must be the last
key, because the Bash extractor swallows whatever follows the array) and C-04 (the 50-line parser
window) — are rules about the *layout of a file*. A spec is a JSON value, and a value has no
meaningful key order: `serde_json`'s map is sorted, so `extensions` lands before `version` and W-01
fires on a spec that is perfectly fine. **Every caller before the compose importer happened to omit
extensions**, so nothing had ever hit it; the first spec to carry them was refused with a layout
complaint about a file that did not exist yet. The fix is to re-validate against
`manifest::to_json` — the serialiser that exists precisely to satisfy those rules — so what is
checked is the bytes that will actually be written.

A second defect, in new code, came from the same test: switching a project's runtime to node
patched a `node` block that `detected_spec` only creates when *detection* said node — and the case
that reaches that branch is exactly the one where it did not (a Laravel repository whose compose
file runs the Vite container). It silently produced a node project with no node settings. It now
builds the block instead of patching it.

Verification of the Docker bet itself is a test that writes a compose file in the shorthands a hand
parser gets wrong — a YAML anchor with a merge key, `"8080:80"` as a string, labels as a list, a
relative bind — and asserts all four arrive normalised.

**Sprint 8 (P2-15) made it three roadmap items in a row whose framing was the thing to fix.** "Proxy
the Node dev server through Traefik with HMR" describes plumbing. The actual state was that a node
project has **no bind mount at all** — five of six server generators call `generate_common_volumes`
and `node.sh` calls nothing — so there was no source in the container to reload and no amount of
WebSocket work would have produced one.

Two claims here were checked against a running container rather than reasoned about, because both
are Docker's semantics and not this code's:

- **The anonymous volume is load-bearing.** With the bind alone, `require()` fails
  `MODULE_NOT_FOUND`: the mount hides the `node_modules` the image installed for its own platform.
  With `- "/app/node_modules"` it loads. Both directions measured on a real image.
- **Compose replaces the command rather than appending it** — an append would have started the
  production entrypoint and the dev server one after the other, which is the sort of thing that
  "works" until two processes fight over a port.

And the third requirement was read out of Vite's source rather than from memory: the 403 on an
unknown `Host` is `hostValidationMiddleware`, confirmed against the copy in this repository's own
`node_modules`, along with the detail that makes a one-line fix possible — the matcher accepts a
leading dot as a suffix.

**Sprint 9 (P3-19) was the first item whose framing was too small rather than wrong.** "A tinker
quick action — the PTY exists, so it is nearly free" is accurate, and it is also one button. The set
it belongs to is what people actually do: `artisan tinker`, `artisan migrate`, `composer install`,
`npm install`, `wp shell` — each of which today means opening a terminal, remembering the container
name, and typing `docker exec -it stackvo-<project> …`.

The design decision worth recording is that **the catalog is fixed, and that is the security
model**. The frontend sends an *id*; the argv is built on the Rust side from compiled-in words.
There is no code path by which the webview can name a program to execute — the same
handle-not-a-path rule `app_log_open` uses, and for the same reason: a project pane that accepted
an arbitrary command string from its own frontend is a remote shell with extra steps. Everything is
spawned as argv, never through a shell, so a project called `a; rm -rf ~` is a container name that
does not exist.

Three smaller decisions that each came from thinking about the failure rather than the feature:
`--force` on `migrate` and `--no-interaction` on the Composer commands, because Laravel and Composer
will stop for a prompt that nobody can see inside an operation console; `-it` **only** for the
interactive commands, because Docker refuses `-t` when stdin is not a TTY; and `migrate:fresh`,
`db:wipe` and `composer update` deliberately left out, because data loss should not be a button four
characters from `migrate`.

Verified against the real checkout: eleven projects, Laravel ones offering eleven commands and plain
node ones two, with every offer's marker file asserted present — and the exec path itself run
against a live container (`composer dump-autoload` → *"Generated optimized autoload files containing
7381 classes"*).

**Sprint 10 (P3-17) is the clearest case yet of the roadmap naming the wrong tool.** The item said
"Blackfire / SPX, flame graph". Blackfire ships a template already and needs an **account** — a
signup wall in a local development tool. SPX, XHProf and Excimer are not in
`contracts/php-extensions.json`; only `xdebug` is, so adding one is a change to a contract shared
with upstream, the same class of decision as the Mailpit swap. **Xdebug is already a profiler**, it
is already in the catalog, and the overlay that sets `XDEBUG_MODE` already belongs to this app.

Every fact the implementation rests on was measured against a running container before any of it was
written:

- `XDEBUG_CONFIG` **does** carry `output_dir` and `start_with_request` — both took effect from the
  environment — but it **silently ignores `use_compression`**, and Xdebug 3.4 gzips by default. One
  compressed file is the difference between a profile view and a parse error, so those settings go
  into a generated ini instead. Named `zzz-stackvo-xdebug.ini`, because `zz-stackvo.ini` is the
  *user's* `php.ini` from Sprint 5 and PHP's `conf.d` is last-wins.
- With `start_with_request=trigger` and no `XDEBUG_TRIGGER`, **no file is written**; with the
  trigger, a plain-text cachegrind file appears. Both directions checked.
- The mode is left in `XDEBUG_MODE` and deliberately kept *out* of the ini: the environment variable
  takes precedence over `xdebug.mode`, so setting both invites them to disagree.
- The two modes are **exclusive**, and that is a finding rather than a simplification. Stepping wants
  `start_with_request=default` so a breakpoint fires on the next page load; profiling wants
  `trigger`. A combined `debug,profile` would have to pick one and silently break the other.

The parser was written against real Xdebug 3.4.0 output rather than a specification, and four
properties of that output shaped it. Names are compressed **and the ids are not in order** — a real
file defined `fl=(2)` before `fl=(1)` — so names are resolved at the end; resolving as you read
produces blanks. A cost line means *self* cost after `fn=` and *inclusive* cost after `calls=`, and
conflating them doubles every caller's self time invisibly. Units are declared in the file
(`Time_(10ns)`), so assuming microseconds would be wrong by two orders of magnitude and the number
would still look plausible. And the scale: a 200,000-iteration loop produced **200,004 `fn=` blocks
across 1.6 million lines**, which is why aggregation is the feature rather than a nicety — the
parser turned that 10 MB file into **5 rows in 836 ms**, with `php::sqrt` correctly showing 200,000
calls.

One presentation decision worth stating: `percent` is a share of the **summed self cost**, not of
the file's own `summary:` line, which is larger. The two are reported separately rather than
conflated, because only the first denominator makes "how much of the work happened inside this
function" true and adds to 100.

**Sprint 11 (P3-20) is the first feature here whose central property is a security one.** "Export a
production image" sounds like `docker save`. What the images actually contain says otherwise, and
all of it was measured before anything was written:

- A PHP project's image holds **no application code at all** — `/var/www/html` contains exactly one
  file, `index.nginx-debian.html`, because the source arrives through a bind mount. Exporting that
  image ships a web server with nothing to serve.
- The same image has **Xdebug loaded** — `php -m` lists it, `docker-php-ext-xdebug.ini` is in
  `conf.d`. Shipping a step debugger that opens a connection on every request is not a deployment.
- A **node** image is the opposite: its Dockerfile does `COPY . .` and `npm install`, so it already
  holds the code and the build. Rebuilding from it would replace a Linux `node_modules` with
  whatever the host has. Two strategies, because the two runtimes genuinely differ.

And the part that decided the shape of the whole feature: **the project this was built against holds
five `.env` files** — `.env.local`, `.env.main`, `.env.stage`, `.env.testing` and a `.bak` of the
last — each with real credentials. A `COPY . .` puts every one into an image layer, where deleting
it later does not remove it.

So the exclusion list is not housekeeping, and it is **not trusted**. The built image is run and
asked what it contains. Proven both ways on a real project image:

| | `.env` files found |
| --- | --- |
| with the ignore file | **0** |
| without it | **5** — `.env.local`, `.env.main`, `.env.stage`, `.env.testing`, `.env.testing.bak-pagesize` |

A check that can only say "clean" is worth nothing; that second row is what makes the first one
mean something. The full verification on the real build came back
`{ envFiles: [], xdebugActive: false, hasApp: true, clean: true }` — and `hasApp` is there for the
other silent failure, a build whose context was empty producing an image that starts and serves
nothing.

One mechanism worth recording: the ignore file is written **beside the generated Dockerfile** as
`Dockerfile.dockerignore`, which BuildKit reads in preference to the context's. That was confirmed
to work before anything depended on it, and it is why building somebody's image writes nothing into
their repository.

**Sprint 12 (P3-18) is the one where the roadmap's cost estimate was the thing that was wrong.**
"Needs a collector service and a Composer package. Expensive." Checked against real projects,
neither half holds for Laravel: `symfony/var-dumper` is a `laravel/framework` dependency and is
present in every checkout here with a `vendor/` at all, and `vendor/bin/var-dump-server` — the
collector — ships inside it.

But there *is* a real cost, and it is hiding behind the obvious implementation.
`VAR_DUMPER_FORMAT=server` sends `base64(serialize([Data, context]))` over a socket. Decoding one
confirmed the shape: a `Cloner\Data` object whose `data` is a **per-depth slot table**, where a
nested value is an indirection like `a:1:{i:1;i:2;}` into the next level, and where every object is
a `Stub`. Reading that in Rust means a PHP `serialize()` parser *plus* a reimplementation of a
Symfony internal — a lot of code whose failure mode, when Symfony changes it, is a viewer that
renders nonsense.

So the collector is not written. **Symfony's own runs inside the project container and renders the
dumps itself**, and the app streams the text. Nothing here parses Symfony's internals, so nothing
here can break when they move.

Three things were checked in a running container before the design was fixed:

- **The fallback is clean.** With the variables set and *nothing* listening, `dump()` renders into
  the response exactly as it does today. That is what removed the need for an on/off switch — a
  state file for a setting with no observable off-state would be ceremony.
- **The environment reaches PHP-FPM.** The official PHP image's `docker.conf` sets
  `clear_env = no`, which is the reason a web request sees it at all.
- **And it reaches `$_SERVER`.** `variables_order` is `EGPCS`; the `E` is what puts it there, and
  `$_SERVER` is where `VarDumper` looks. Confirmed by reading the value back inside the container.

End to end on a real running project: the collector started, a dump sent from a second `exec`
arrived, Symfony rendered it, and the four-line startup banner was filtered out — **12 lines
caught**.

### Sprint 13 — a correction to this document

Every sprint report above ends with the same line: *`contracts:check` — 4 errors (unchanged —
upstream)*. That was written twelve times without anybody opening the output, and **it is wrong for
three of the four**:

| error | where | ours? |
| --- | --- | --- |
| `C-06` | `projects/api.oxoeashop/stackvo.json` | **yes** — a local manifest |
| `C-06` | `projects/parser.ajans/stackvo.json` | **yes** — a local manifest |
| `C-06` | `.env`, the default extension set | **yes** — the stack's own config |
| `C-09` | `templates/services/mongo-express` | no — genuinely upstream |

All three are one root cause: **`imap`, removed in PHP 8.2, selected on 8.4.** And the `.env` one is
the worst of them, because it means the *out-of-the-box* selection cannot build — a project created
right now, with nothing customised, comes up missing an extension it asked for.

What made it worth more than a `git` fix is *why* nobody noticed: **the Bash generator skips an
extension it cannot install silently.** Nothing fails, nothing is logged, the build reports success
and the container comes up. The symptom arrives much later as a fatal `Call to undefined function
imap_open()`, with nothing anywhere connecting it back to a build that said it worked.

And the desktop app showed none of it. The doctor knew about ports, disk, hosts and generated-file
staleness, and nothing about extensions — 0 mentions of `C-06` in `doctor.rs`. So the fix is a check
in the doctor rather than an edit to two files: `manifest::normalize` already computes the rule, so
project rows reuse its findings rather than restating them, and only the default-set check is new.

One detail that made the default-set check less obvious than it looks: the default PHP version is
itself contested. `DEFAULT_PHP_VERSION=8.2` and `SUPPORTED_LANGUAGES_PHP_DEFAULT=8.4` disagree —
that is `C-12` — so rather than pick one, **both are checked**, since an extension that fails on
either is a problem whichever key turns out to win. On this checkout `imap` fails on both, which is
the cleanest possible statement of the bug.

Measured on the real workspace, the new check reproduces the validator's three `C-06` errors exactly
and adds the second default-version row the validator does not report.

**The check shipped without a repair, which broke the doctor's own rule** — hosts get the reviewed
diff, stale output gets regenerate, disk gets prune, and this got a link. So the repair followed:
one button per row, removing the extension from the manifest or from the `.env` default set through
the same writers everything else uses.

It needs no confirmation dialog, and the reason is the whole argument for the feature:
**removing the extension changes nothing about what runs.** The generator already drops it, so it is
already absent from every built container. The button does not remove a capability — it stops the
manifest claiming one the container never had.

No alternative is offered because the catalog leaves none. `imap` is `install: core` with
`removedIn: 8.2` and no PECL package, so on 8.2 or newer there is nothing StackVo could install
instead; if a project genuinely needs it, the answer is an older PHP version, and that is the
project owner's decision rather than a button. The catalog's own note agrees on the diagnosis:
*"Requesting it on PHP >= 8.2 MUST fail the build with a clear message, not be silently dropped."*

### Not yet started

| # | Item | Note |
| --- | --- | --- |
| **P2-11** | The four missing runtime generators | High effort, and gated on `C-02`. **Now the largest open item on the list.** |
| — | *(nothing)* | Every P2 except `C-02` is delivered. What is left is P2-11 above and the P3 list below. |
| **P3** | 21 | ACME only, and it is marginal for local development: mkcert already produces a certificate the browser trusts, and a public CA cannot issue for `.loc`. Everything else on the P3 list is delivered. |

### Debt found while building

Three of these were shipped code that did nothing or lied, found by testing against the running
stack rather than by reading the source. All three are fixed; they are recorded because the
*method* is the finding — none of them was visible from the code alone. Only the two upstream
items remain open, and neither is this app's to close unilaterally.

- **Fixed.** `cert_apply` reissued the certificate and left every browser on the old one: Traefik
  watches `generated/traefik/dynamic`, and the certificates are not in it. Measured live —
  Traefik up two days, serving a certificate a day older than the one on disk. The rewrite that
  fixes it must be **in place**; an atomic stage-and-rename was tested against the running proxy
  and was not picked up at all.
- **Fixed.** `xdebug::dockerfile_path` read `projects/<name>/Dockerfile`, which is a user's own
  file. Compose builds `generated/projects/<name>/Dockerfile`. `needsRebuild` was therefore true
  permanently, for the wrong reason, no matter how often the image was rebuilt.
- **Fixed.** Two stale assertions in `tests/app-shell.spec.js` looked for the quick actions in the
  nav drawer; they moved to the app bar in `100a2d4`, and they are `:title` attributes that
  `wrapper.text()` cannot see. They now assert on the app bar and on the attribute. The suite was
  also throwing eight unhandled rejections from the metrics store, which polled on a timer and
  dereferenced a sample it never checked — a skipped tick is not a reason to poison the history.
  85 → 87 tests, all passing, no errors.
- **Open.** `contracts:check` reports 4 errors, down from 8 — the four fixed were commands
  registered in `lib.rs` and absent from `ipc.json`. The remaining four are upstream StackVo
  contract conflicts, not app defects.
- **Open, upstream.** Swapping the MailHog template to Mailpit in the `stackvo` repository
  renames the service, its `.env` keys, the container and the volume — a migration for every
  running stack. Needs an explicit decision before it is worth doing.

## 7. Removing Bash: what the port actually needs

The question was whether the Bash generator can be dropped and the app moved to
Rust outright. **Measured before answering**, because "probably identical" is not
a standard for the files that define every container the user runs.

### Where the port already is

`generator_verify` on the real checkout: **14 files compared, 14 match, 0 differ,
`readyToTakeOver: true`.** That covers all eleven project Dockerfiles,
`docker-compose.projects.yml`, `traefik/traefik.yml` and
`traefik/dynamic/routes.yml`.

But Bash writes **40 files**, so the honest figure was 14 of 40 — and the
remaining 26 are not 26 separate generators. They are **two mechanisms**:

| surface | files | how Bash makes it |
| --- | --- | --- |
| project `nginx.conf` / `supervisord.conf` | 22 | inline heredocs in `dockerfile/*.sh`, then a `sed` |
| `docker-compose.dynamic.yml` | 1 | `render_template` over 20 service templates |
| `configs/*` | 6 | `render_template` over 6 config templates |

So one function — `render_template` — stood between the port and 27 of the 29
remaining files. That is what Sprint 14 ported, and it now reproduces Bash's
bytes exactly: **5 config files and the whole 6030-byte
`docker-compose.dynamic.yml`, byte-for-byte**, asserted against what Bash wrote.

Two side findings from the measurement. `core/templates/servers/` is **dead** —
nothing in `core/` or `tools/` reads it, because `dockerfile/nginx.sh` writes the
config inline with a `DOCUMENT_ROOT_PLACEHOLDER` and `sed`s it. And of the 5048
lines of shell, most is CLI plumbing (`install`, `uninstall`, `pull`, logging,
colours) that the desktop app never calls; the *generator* is a much smaller
target than the line count suggests.

### Two things reading the shell would not have told me

Both were caught by comparing bytes, not by reading code:

- **`HOST_STACKVO_ROOT` is not in `.env`.** `env-loader.sh` computes and exports
  four variables — `STACKVO_ROOT`, `HOST_STACKVO_ROOT`, `HOST_UID`, `HOST_GID` —
  and all four match the substitution prefix list. A renderer fed `.env` alone
  emits empty strings, and every volume mount came out as
  `/generated/configs/mysql.cnf` instead of an absolute host path. That compose
  file would have mounted the wrong directory rather than failing.
- **`include_module` ends with a bare `echo ""`** after its `awk` pipeline, so
  every enabled service is followed by a blank line. Invisible in the awk.

One behaviour was reproduced rather than fixed: the volumes section is harvested
from **every** `.tpl` on disk, enabled or not, so the generated file declares
volumes for services that are switched off. Arguably wrong; it is what Bash
writes, and a "fix" here is a byte difference that fails the takeover check.

### The answer on removing Bash

**Not yet, and the order matters.** `GeneratorEngine::Rust` already exists and is
deliberately not a replacement — it writes only after Bash has produced the same
bytes, so the handover cannot silently change anyone's images. Removing Bash
removes that check. The sequence that keeps it:

1. Port the remaining surfaces, each landing in `Verify` until byte-identical on
   real data. **Sprint 14 did the renderer; `nginx.conf`/`supervisord.conf`
   heredocs are what is left.**
2. When all 40 files match, `Rust` mode no longer needs Bash, and Bash goes.

And this reorders the roadmap: **P2-11 belongs after the port, not before.**
Adding a Python generator today means writing it twice — once in Bash, once in
Rust — or breaking the parity check that makes the port safe. Finish the port and
each new runtime is written once.

### A bug the work turned up

The dump collector leaked. Aborting the stream kills the local `docker exec`
client; the PHP process **inside** the container keeps running and keeps port
9912, so the next open fails with Symfony's `Address already in use`. Found by
the integration test failing its *second* run — the first run passed.

Fixed with a `dumps_close` that stops the in-container process too, and the same
kill before starting, so a collector left by a crash does not block the next
attempt. Verified by running the test back to back and confirming no
`var-dump-server` survives in any container. The first count of leaked processes
was itself wrong — `pgrep -f var-dump-server` matches the shell running it, which
reported 12 containers affected when the real number was 2.

## 8. What is actually left

Everything in sections 3 and 5 is delivered except the four entries below. Three
of them are *partly* done, and the remainder in each case is named rather than
implied.

| item | state | what remains |
| --- | --- | --- |
| **P0-5 Mailpit** | partly delivered | The in-app inbox ships and reads both the MailHog and Mailpit APIs, so a checkout on either image gets one. **The image swap itself is upstream**: it renames a service, its `.env` keys, its container and its volume — a migration for every running stack, not a one-line edit. |
| **P1-6 MCP server** | partly delivered | The read surface is complete; only two writing tools are exposed. Thirty-four commands report progress through Tauri's event system and are unreachable from a stdio subprocess until that is decoupled. |
| **P2-11 four runtime generators** | not started | Python, Go, Ruby, Rust have no Dockerfile template. **Blocked on the Bash removal, not on effort** — see §7: adding one today means writing it twice or breaking the parity check. |
| **P3-21 ACME** | not started, **not recommended** | A public CA cannot issue for `.loc`, and mkcert already produces a certificate the browser trusts. The item buys nothing for local development. |

And one carried forward from §7, the only piece of the Bash removal still open:

| surface | state |
| --- | --- |
| `render_template`, `configs/*`, `docker-compose.dynamic.yml` | ✅ ported, byte-identical against Bash's output |
| project `nginx.conf` / `supervisord.conf` (22 files) | ❌ not ported — inline heredocs in `dockerfile/*.sh` plus a `sed` |

At 40/40 the `Rust` generator mode stops needing Bash and the shell can go.
