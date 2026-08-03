# Contract Conflicts Register — v1

Findings from freezing the StackVo config contract (Phase 0) and from porting the generator (Phase 4). Every item was verified against the
source or against the eight real projects in `projects/`, not inferred.

Each entry carries a **Decision** that the v1 contract implements. Items marked **BUG** are live
defects in StackVo today — they are worth fixing in the Bash CLI too, independently of the desktop port.

Severity: 🔴 broken behaviour · 🟠 silent data loss / wrong output · 🟡 confusion & drift

---

## 🔴 C-01 — The UI cannot create a working Node project · **BUG**

`ProjectService.createProject()` builds the manifest like this:

```js
config.nodejs = { version };   // key: "nodejs"
// ...and never writes a `runtime` key at all
```

But the generator reads:

```bash
get_project_runtime()  # reads "runtime", defaults to "php" when absent
parse_node_config()    # expects a "node" block
```

So a Node project created through the web UI has no `runtime` key → the generator defaults to `php`
→ `parse_project_config` picks up `"version": "22"` from the `nodejs` block and treats **22 as a PHP
version** → it generates an nginx + PHP-FPM Dockerfile for `FROM php:22-fpm`, which does not exist.

Confirming evidence: all three Node projects on disk (`l00kout`, `tracking.ajans`, `vue-builder`) use
`"runtime": "node"` + a `"node"` block — i.e. they were **hand-written**, because the UI path cannot
produce a working one.

**Decision.** Canonical key is `node`; canonical runtime id is `node`. `runtime` is written
explicitly by all writers and defaulted to `php` by all readers. Readers accept `nodejs`/`js` as
read-only aliases so any manifest already written by the UI can be repaired rather than rejected.

---

## 🔴 C-02 — Four advertised runtimes have no generator · **BUG**

`.env` offers `SUPPORTED_LANGUAGES=php,python,go,ruby,rust,nodejs` and the UI renders all six in the
new-project form. `ProjectService` even writes `config.python` / `config.ruby` / `config.golang`
blocks. But `core/cli/lib/generators/project/{compose,dockerfile}/` contains only:

```text
apache  caddy  frankenphp  nginx  node  swoole
```

Five PHP web servers and Node. Choosing Python, Go, Ruby or Rust writes a manifest that generates
nothing — the project silently never appears as a container.

Note also `runtime: "golang"` (what the UI writes) vs `go` (what `.env` lists) — a third spelling of
the same non-existent feature.

**Decision.** `project.schema.json` restricts `runtime` to `php | node`. `catalog_get` returns the
other runtimes with `available: false` so the UI can grey them out honestly instead of offering a
trap. `INVALID`/`UNSUPPORTED` is raised at `project_validate` time, before anything touches disk.

---

## 🟠 C-03 — Field order in `stackvo.json` changes the build output · **BUG**

The extension extractor is:

```bash
grep -A 50 '"extensions"' "$project_json" | grep -o '"[a-z_0-9]*"' | sed 's/"//g' | grep -v extensions
```

It takes **every quoted lowercase token in the 50 lines following `"extensions"`** and calls it an
extension. It works on all five real PHP projects only because `php.extensions` happens to be the last key
in every one of them. Move `document_root` after the `php` block and `document_root` and `public`
both get passed to `docker-php-ext-install`.

**Decision.** Write rule **W-01** in `project.schema.json`: `php.extensions` MUST be the last key in
the document. `tools/validate-contracts.mjs` enforces it. The constraint disappears once the
generator is ported to Rust with a real JSON parser — it is documented as *temporary*.

---

## 🟠 C-04 — Extension 51 and beyond is silently dropped · **BUG**

Same `grep -A 50`. The window is 50 lines; one extension per line. A project with 51+ extensions
loses the tail with no warning, no error, and a container that builds successfully but is missing
extensions.

This is reachable today: `SUPPORTED_LANGUAGES_PHP_EXTENSIONS` lists **78** extensions, so a user who
selects everything in the UI loses **28** of them.

**Decision.** `maxItems: 50` in the schema, enforced by the validator with an explicit error message
naming the limit and its cause. Removed when the Rust generator lands.

**Closed.** The Rust generator landed and the Bash CLI was deleted, so the window it protected no
longer exists. `maxItems` is off the schema, the count check is out of `manifest::normalize` and
`validate-contracts.mjs`, and `catalog.maxExtensions` is now the catalog's own length — selecting
every extension is a supported (slow to build) choice rather than silent truncation.

---

## 🟠 C-05 — Two different "default extension sets"

| Source                                                                       | Set                                                       |
| ---------------------------------------------------------------------------- | --------------------------------------------------------- |
| `get_default_extensions()` (generator, used when `php.extensions` is absent) | 7 extensions: `pdo pdo_mysql mysqli gd curl zip mbstring` |
| `SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT` (`.env`, pre-checked in the UI) | 33 extensions                                             |

A project created via the UI gets 33; a hand-written manifest that omits the key gets 7. Same
"default", 26 extensions apart.

**Decision.** Both are recorded in `php-extensions.json` under `defaultSets`, explicitly named
`generatorFallback` and `uiDefault`, with the trigger condition for each. Not unified in v1 —
unifying would silently change existing projects' images. Revisit in v2 behind a migration.

---

## 🔴 C-06 — The out-of-the-box default extension set cannot build · **BUG**

`SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT` includes **`imap`**. `imap` was removed from PHP core in
**8.2**. `SUPPORTED_LANGUAGES_PHP_DEFAULT` is **8.4**.

So the default selection, on the default PHP version, requests an extension that cannot be installed.
`is_deprecated_extension()` detects this — and the generator's response is to skip it silently, so
the user gets an image quietly missing `imap` rather than an error.

Two real projects on disk hit this: `api.oxoeashop` and `parser.ajans`, both PHP 8.4 with `imap`
requested.

**Decision.** `imap` carries `removedIn: "8.2"` in `php-extensions.json`, and resolution step 5 makes
it a **hard error** rather than a silent skip. `imap` is removed from the recommended default set.
The silent-skip behaviour is called out in `resolution.errorPolicy` as a deliberate v1 behaviour change.

---

## 🟡 C-07 — `ldap` configure line is x86_64-only

```bash
docker-php-ext-configure ldap --with-libdir=lib/x86_64-linux-gnu
```

Hardcoded multiarch triplet. On an arm64 base image (every Apple Silicon Mac, which is the primary
development platform here) the path is `lib/aarch64-linux-gnu` and configure fails.

**Decision.** Recorded as a `note` on the `ldap` entry. The Rust generator substitutes the triplet
from the target platform. Not fixed in the Bash path in v1 — `ldap` is not in any default set and no
real project uses it.

---

## 🟡 C-08 — `xmlrpc` is unreachable but still offered

`is_deprecated_extension()` returns "deprecated" for `xmlrpc` on **every** PHP version
(the check has no version guard). StackVo's minimum is 8.0. So `xmlrpc` can never install — yet it
is listed in `SUPPORTED_LANGUAGES_PHP_EXTENSIONS` and pickable in the UI.

**Decision.** `removedIn: "8.0"` + `UNSUPPORTED` error. Removed from the catalog in v2.

---

## 🔴 C-09 — Enabling `mongo-express` starts nothing · **BUG**

The `up` command derives compose profiles from `.env`:

```bash
if [[ "$key" =~ ^SERVICE_([A-Z_]+)_ENABLE$ ]] && [ "$value" = "true" ]; then
    SERVICE_PROFILE=$(echo "$SERVICE_NAME" | tr '[:upper:]' '[:lower:]')
    PROFILE_ARGS="$PROFILE_ARGS --profile $SERVICE_PROFILE"
fi
```

`SERVICE_MONGO_EXPRESS_ENABLE=true` → profile **`mongo_express`** (underscore).
The template declares **`mongo-express`** (dash):

```yaml
profiles: ["services", "mongo-express"]
```

Verified across all 20 templates: every profile uses the dash form. `mongo-express` is the only
service whose name contains a separator, so it is the only one affected — and it is 100% broken in
minimal mode.

**Decision.** The env-key ↔ service-id mapping is now explicit in `env.schema.json`
(`servicePattern.profileDerivation`): service id → env key is `uppercase + '-'→'_'`; the reverse
mapping MUST use the service catalog, never a naive `tr '_' '-'`. `services` in `env.schema.json`
lists the 20 canonical ids.

**Resolved in the desktop app.** There is no derivation left to get wrong: `compose_up_service`
passes the service id through unchanged (`--profile mongo-express`), and `Env::service_prefix` maps
in the one direction that is safe — id → env key. The validator asserted the Bash derivation until
the shell was deleted out from under it, and then spent a while failing this repo for a bug this
repo does not contain. It now checks what the app actually depends on: the template must declare
both its own id and `services`. C-09 remains open **upstream**, in StackVo's `up`; the reproduction
lives in `tests/real_checkout.rs` and only runs when a Bash checkout is present.

---

## 🟡 C-10 — `server` vs `webserver`

Three-way state: the parser prefers `"server"` and falls back to `"webserver"`; `ProjectService`
writes `"server"`; **all five** real PHP projects on disk use `"webserver"`.

**Decision.** Canonical is `server`. `webserver` stays read-supported and is marked `deprecated` in
the schema — it cannot be dropped until the five on-disk projects are migrated. Emitting both is a
schema error.

---

## 🟡 C-11 — Nineteen dead `.env` keys

Measured by `tools/measure-env-usage.mjs`, which greps the checkout's `core/`
and allows for the `SUPPORTED_LANGUAGES_*` family that the UI builds from a
template string at runtime:

```text
LETSENCRYPT_ENABLE     LETSENCRYPT_EMAIL      DEFAULT_DOCUMENT_ROOT
DEFAULT_WEB_SERVER     DEFAULT_SQL_SERVER     DEFAULT_CACHE_SERVER
DEFAULT_TIMEOUT        SYSTEM_COMMAND_TIMEOUT CACHE_ENABLE
LOG_ENABLE             TRAEFIK_URL            DOCKER_PRUNE_ON_REBUILD
DOCKER_FORCE_RECREATE  DOCKER_REMOVE_ORPHANS  HOST_TIMEZONE
STACKVO_VERBOSE        STACKVO_SHOW_BANNER    STACKVO_STRICT
STACKVO_DRY_RUN        STACKVO_VERSION        STACKVO_GENERATE_LOG
ALLOW_HTTPD            ALLOW_NGINX            ALLOWED_PHP_VERSIONS
HOST_PORT_PERCONA      HOST_PORT_ADMINER
```

Some of these are actively misleading. `DEFAULT_CACHE_SERVER=mysql` reads like a
misconfiguration. `ALLOWED_PHP_VERSIONS` looks like a security control and
enforces nothing. `HOST_TIMEZONE` suggests containers inherit a timezone they do
not. `DOCKER_FORCE_RECREATE=true` and `DOCKER_REMOVE_ORPHANS=true` describe
behaviour that is hardcoded in `up` either way, so setting them to `false`
changes nothing.

> **This entry was wrong when first written.** The original measurement was a
> hand-run `grep` executed from the wrong working directory, so `core/` did not
> resolve and *every* key came back with zero consumers. That produced a list of
> 17 "dead" keys that included two live ones — `DEFAULT_TLD_SUFFIX` (8
> consumers: the Traefik router generator and seven service templates) and
> `CACHE_TTL` — while missing several genuinely dead ones. The measurement is
> now a checked-in tool, and `--fix` reconciles the schema with reality.

**Decision.** Tagged `status: "dead"` in `env.schema.json`, each with a note.
Kept for one release, then removed. Two exceptions: `STACKVO_DRY_RUN` is worth
*implementing* rather than deleting, and `STACKVO_VERSION` is dead inside
StackVo but read by the desktop app to show which checkout it is driving.

---

## 🟡 C-12 — Overlapping version keys

| Keys                                                                  | Values                | Problem                                                                                  |
| --------------------------------------------------------------------- | --------------------- | ---------------------------------------------------------------------------------------- |
| `DEFAULT_PHP_VERSION` / `SUPPORTED_LANGUAGES_PHP_DEFAULT`             | `8.2` / `8.4`         | Which wins depends on which code path you hit                                            |
| `ALLOWED_PHP_VERSIONS` / `SUPPORTED_LANGUAGES_PHP_VERSIONS`           | `7.4–8.4` / `5.6–8.5` | Different ranges, one of them dead                                                       |
| `PHP_TOOL_NODEJS_VERSION` / `SUPPORTED_LANGUAGES_NODEJS_DEFAULT`      | `20` / `22`           | Node inside the PHP image vs Node runtime projects — may be intentional, is undocumented |
| `DEFAULT_SERVER` / `DEFAULT_WEB_SERVER` / `SUPPORTED_SERVERS_DEFAULT` | all `nginx`           | Three names for one setting                                                              |

**Decision.** Precedence is now written down in `project.schema.json` → `x-stackvo-read-rules` step 4:
`php.version` → `SUPPORTED_LANGUAGES_PHP_DEFAULT` → `DEFAULT_PHP_VERSION` → `8.2`. The
`PHP_TOOL_NODEJS_VERSION` divergence is documented as intentional-but-separate (build toolchain, not
runtime).

---

## 🟡 C-13 — PHP 5.6–7.4 is offered but unsupported by the toolchain

`SUPPORTED_LANGUAGES_PHP_VERSIONS` starts at `5.6`, but `php-extensions.sh` assumes 8.0+ throughout —
`is_builtin_extension()` defaults its version parameter to `8.0` and its "builtin since 8.0" list is
applied unconditionally. On PHP 7.4, extensions like `dom` and `simplexml` would be skipped as
"builtin" when they actually need installing.

**Decision.** Documented as `conflicting` in `env.schema.json`. v1 declares **PHP 8.0 as the floor**;
the validator warns when a manifest requests below 8.0.

---

## 🟠 C-14 — Extension names outside `[a-z0-9_]` vanish

`grep -o '"[a-z_0-9]*"'` cannot match an uppercase letter or a dash. Any such extension name is
silently dropped from the install list, and the build succeeds without it.

**Decision.** Schema pattern `^[a-z0-9_]+$` on extension items, plus a validator error explaining the
cause rather than just rejecting.

---

## 🟡 C-15 — `HOST_UID`/`HOST_GID` default is wrong on macOS

`.env.example` ships `HOST_UID=1000` / `HOST_GID=1000`. macOS user accounts start at **501** with
group **20**. This repo's own `.env` has been hand-corrected to `HOST_UID=501` / `HOST_GID=20` —
direct evidence the default is wrong for the primary platform.

The whole mechanism exists only because the UI container runs as root and must chown its writes back
to the host user.

**Decision.** Marked `conflicting` with `v1Change: OBSOLETE`. The desktop app writes as the invoking
user, so these keys become irrelevant to it. They stay in `.env` while the containerised UI ships.

---

## 🟡 C-16 — Service dependency graph is 3 entries out of 20

`serviceDependencies.json` declares `kibana`, `grafana`, `kafka` only — and `grafana`'s optional
dependency is `prometheus`, **which is not a StackVo service**. Six admin UIs have real dependencies
that are undeclared, so the UI cheerfully starts phpMyAdmin with no database behind it.

**Decision.** The completed graph lives in `env.schema.json` → `serviceDependencies` (9 entries,
`prometheus` removed). `service_dependencies` reads from there.

---

## 🟡 C-17 — Documented service count is wrong three ways

README badge says **40+ services**; the README table says **14**; `core/templates/services/` contains
**20**.

**Decision.** 20 is authoritative and is listed by category in `env.schema.json` → `services`.

---

## 🟡 C-18 — Blackfire credentials in `.env.example`

`SERVICE_BLACKFIRE_SERVER_ID` and `SERVICE_BLACKFIRE_SERVER_TOKEN` carry real-looking UUID/hex values
in the committed example file.

**Decision.** Flagged in `env.schema.json` → `secrets.committedSecretWarning`. **If these are live
Blackfire credentials, rotate them** — they are in the public repository's git history. Replace with
empty placeholders.

---

---

## 🟡 C-19 — Node writes its Dockerfile into the user's source tree

PHP projects get their generated Dockerfile in `generated/projects/<name>/`.
Node projects get theirs in `projects/<name>/` — next to the user's own code,
along with a generated `.dockerignore`.

There is a real reason: `COPY . .` needs the build context to be the actual
source, so the Dockerfile has to live there. But the consequence is that
`stackvo generate` writes two files into a directory the user very likely has
under their own version control, and nothing says so.

**Decision.** Documented rather than changed — moving it would break the build
context. `tools/make-fixtures.sh` and the Rust generator both account for the
asymmetry. What v1 adds is that the desktop app can *tell* you: the two files
are generated, and belong in the project's `.gitignore`.


---

## 🔴 C-20 — With `SSL_ENABLE=false` nothing is reachable · **BUG**

The two Traefik files disagree about which entry point exists.

`traefik.yml` only declares `websecure` when SSL is on:

```bash
if [ "$ssl_enabled" = "true" ]; then
    ...
    echo "  websecure:"
    echo "    address: \":443\""
fi
```

`routes.yml` targets it unconditionally — the Traefik dashboard router and every
service router are emitted with `entryPoints: [websecure]` and `tls: {}`
regardless.

So setting `SSL_ENABLE=false`, which reads like "serve over plain HTTP", instead
produces routers pointing at an entry point that does not exist. Verified by
generating both variants: with SSL off, `traefik.yml` mentions `websecure` zero
times while `routes.yml` mentions it three.

Project routers are unaffected — they come from container labels, which hardcode
`websecure` too, but that at least fails consistently.

**Decision.** Reproduced rather than fixed: the differential tests exist to prove
this port does not change what the Bash generator emits, and silently repairing
it here would mean the two tools disagree. `generator::traefik_routing_warning`
returns the diagnostic so the desktop app can *say* the configuration is broken,
which is more than StackVo does today.


## Summary

| Severity            | Count | Items                                                      |
| ------------------- | ----- | ---------------------------------------------------------- |
| 🔴 Broken behaviour  | 4     | C-01, C-02, C-06, C-09                                     |
| 🟠 Silent data loss  | 4     | C-03, C-04, C-05, C-14                                     |
| 🟡 Drift & confusion | 10    | C-07, C-08, C-10, C-11, C-12, C-13, C-15, C-16, C-17, C-18 |

Four of these (C-01, C-02, C-06, C-09) are user-visible bugs in shipped StackVo, found purely by
writing the contract down. That is the return on Phase 0 — none of them required running anything.
