#!/usr/bin/env node
/**
 * validate-contracts — checks the frozen v1 contract against a real StackVo checkout.
 *
 * Runs four suites:
 *   A. every projects/<name>/stackvo.json against project.schema.json (+ the write rules
 *      JSON Schema cannot express)
 *   B. the .env extension catalog against php-extensions.json
 *   C. the service catalog: templates <-> .env keys <-> compose profiles
 *   D. .env keys against env.schema.json (unknown keys, dead keys still present)
 *   E. ipc.json against the Rust command registry and the JS wrapper — the
 *      three have to agree or a command is either unreachable or undocumented
 *   F. reachability: JS wrappers no view calls, and declared events nothing emits
 *   G. the three service-package schemas: headers, required-vs-declared, category agreement
 *
 * Zero dependencies — it implements the specific rules rather than pulling in a schema engine,
 * so it runs in CI and in a fresh clone with nothing installed.
 *
 *   node tools/validate-contracts.mjs [--root ../stackvo] [--json] [--allow-no-manifests]
 */

import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join, dirname, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const CONTRACTS = join(HERE, '..', 'contracts');

// ---------------------------------------------------------------- args

const argv = process.argv.slice(2);
const asJson = argv.includes('--json');

// Suite A needs manifests to check. Finding none is an error by default,
// because the likeliest cause is a `--root` pointing somewhere that is not a
// StackVo checkout — and this repo's own parent directory resolves to a folder
// of the same name, so that mistake is one keystroke away and used to produce a
// confident "0 error(s)".
//
// The flag exists because there is one honest case: `stackvo/stackvo` itself
// carries no `projects/` directory, so CI's checkout has nothing for suite A to
// read. That is a real hole and it belongs in the workflow file where someone
// will see it, not in a warning stream nobody reads.
const allowNoManifests = argv.includes('--allow-no-manifests');

const rootFlag = argv.indexOf('--root');
const STACKVO_ROOT = resolve(
  rootFlag !== -1
    ? argv[rootFlag + 1]
    : process.env.STACKVO_ROOT || join(HERE, '..', '..', 'stackvo')
);

// ---------------------------------------------------------------- reporting

const findings = [];
const add = (level, suite, subject, code, message) =>
  findings.push({ level, suite, subject, code, message });
const err = (...a) => add('error', ...a);
const warn = (...a) => add('warn', ...a);

// ---------------------------------------------------------------- helpers

const readJson = (p) => JSON.parse(readFileSync(p, 'utf8'));

/** Parse .env exactly the way StackVo does: first '=' wins, '#' comments, no unquoting. */
function parseEnv(text) {
  const out = {};
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const i = line.indexOf('=');
    if (i === -1) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

const list = (v) =>
  v
    ? v
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
    : [];

/** Compare PHP "major.minor" strings. Returns -1 | 0 | 1. */
function cmpVersion(a, b) {
  const pa = String(a).split('.').map(Number);
  const pb = String(b).split('.').map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d > 0 ? 1 : -1;
  }
  return 0;
}

const SAFE_NAME = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;
const EXT_NAME = /^[a-z0-9_]+$/;
const RUNTIME_ALIASES = { nodejs: 'node', js: 'node' };
const SERVERS = ['nginx', 'apache', 'caddy', 'frankenphp', 'swoole'];

// ---------------------------------------------------------------- load contracts

/**
 * Suites A–D read a workspace; E and F read this repository.
 *
 * This used to exit(2) when there was no workspace, which took E and F down
 * with it — and those are the two that check the IPC contract against the Rust
 * registry and the JS wrappers, i.e. the only suites that can catch a command
 * added to one side and not the other. Once the Bash CLI was retired there was
 * no reason for a developer to have a checkout at all, so the answer to "is the
 * contract still consistent" became "cannot run" on a normal machine.
 *
 * A missing workspace is now a stated fact rather than a stop: A–D see an empty
 * env and no manifests, which is exactly what they see for an empty workspace,
 * and report nothing.
 */
const HAVE_WORKSPACE = existsSync(STACKVO_ROOT);

const phpExt = readJson(join(CONTRACTS, 'php-extensions.json'));
const envSchema = readJson(join(CONTRACTS, 'env.schema.json'));

// A workspace with neither file is the normal state now: every setting has a
// default in the binary, and `.env` is written only when one is changed. So an
// absent file means "no overrides", not "nothing to check against".
const envPath = ['.env', '.env.example']
  .map((name) => join(STACKVO_ROOT, name))
  .find((p) => existsSync(p));
const env = envPath ? parseEnv(readFileSync(envPath, 'utf8')) : {};

/**
 * Keys the app carries in its binary as defaults.
 *
 * This check used to assume `.env` was the only place a value could come from,
 * so every key that moved into the binary was reported as missing. That is the
 * opposite of what happened — the value did not go away, the copy did — and a
 * warning that fires on the intended state trains people to ignore the list.
 *
 * Read from the source rather than restated here, so the two cannot drift.
 */
const EMBEDDED = (() => {
  const source = join(HERE, '..', 'src-tauri', 'src', 'config.rs');
  if (!existsSync(source)) return new Set();
  const block = readFileSync(source, 'utf8').match(/const EMBEDDED[^=]*=\s*\[([\s\S]*?)\n\];/);
  if (!block) return new Set();
  return new Set([...block[1].matchAll(/\(\s*"([A-Z0-9_]+)"/g)].map((m) => m[1]));
})();

// Flatten the grouped env schema into one key -> spec map.
const envSpec = {};
for (const group of Object.values(envSchema.groups)) {
  for (const [k, v] of Object.entries(group)) {
    if (k !== '_note') envSpec[k] = v;
  }
}

// ================================================================ SUITE A — manifests

const projectsDir = join(STACKVO_ROOT, 'projects');
const projectDirs = existsSync(projectsDir)
  ? readdirSync(projectsDir).filter(
      (d) => !d.startsWith('.') && statSync(join(projectsDir, d)).isDirectory()
    )
  : [];

let manifestCount = 0;

for (const dir of projectDirs) {
  const file = join(projectsDir, dir, 'stackvo.json');
  if (!existsSync(file)) continue;
  manifestCount++;

  const raw = readFileSync(file, 'utf8');
  const subject = `projects/${dir}/stackvo.json`;

  let m;
  try {
    m = JSON.parse(raw);
  } catch (e) {
    err('A', subject, 'PARSE_ERROR', `not valid JSON: ${e.message}`);
    continue;
  }

  // -- required fields -------------------------------------------------
  if (!m.name) err('A', subject, 'MISSING_NAME', '`name` is required');
  else if (!SAFE_NAME.test(m.name))
    err('A', subject, 'INVALID_NAME', `\`name\` "${m.name}" violates ^[a-zA-Z0-9][a-zA-Z0-9._-]*$`);

  if (m.name && m.name !== dir)
    err('A', subject, 'W-04', `\`name\` "${m.name}" does not match directory "${dir}"`);

  if (!m.domain)
    err('A', subject, 'MISSING_DOMAIN', '`domain` is required — the generator aborts without it');

  // -- runtime ---------------------------------------------------------
  let runtime = m.runtime;
  if (runtime === undefined) {
    runtime = 'php';
    warn(
      'A',
      subject,
      'RUNTIME_IMPLICIT',
      'no `runtime` key — readers default to "php" (C-01); writers should emit it explicitly'
    );
  } else if (RUNTIME_ALIASES[runtime]) {
    err(
      'A',
      subject,
      'C-01',
      `\`runtime\` "${runtime}" is a legacy alias — canonical id is "${RUNTIME_ALIASES[runtime]}"`
    );
    runtime = RUNTIME_ALIASES[runtime];
  } else if (!['php', 'node'].includes(runtime)) {
    err(
      'A',
      subject,
      'C-02',
      `\`runtime\` "${runtime}" has no generator (only php and node are implemented)`
    );
  }

  // -- one runtime block (W-02) ---------------------------------------
  const blocks = ['php', 'node', 'nodejs', 'python', 'ruby', 'golang', 'go', 'rust'].filter(
    (k) => k in m
  );
  if (blocks.length > 1)
    err(
      'A',
      subject,
      'W-02',
      `${blocks.length} runtime blocks present (${blocks.join(', ')}) — the Bash parser reads the first "version" it finds and corrupts the output`
    );
  if (blocks.includes('nodejs'))
    err(
      'A',
      subject,
      'C-01',
      'runtime block key is "nodejs" — canonical key is "node" (this manifest was written by the web UI and will be generated as PHP)'
    );
  for (const b of ['python', 'ruby', 'golang', 'go', 'rust'])
    if (b in m) err('A', subject, 'C-02', `runtime block "${b}" has no generator`);

  // -- server / webserver ---------------------------------------------
  if ('server' in m && 'webserver' in m)
    err('A', subject, 'C-10', 'both `server` and `webserver` present — emit only `server`');
  else if ('webserver' in m)
    warn('A', subject, 'C-10', '`webserver` is the deprecated spelling; canonical is `server`');

  const server = m.server ?? m.webserver;
  if (server !== undefined && !SERVERS.includes(server))
    err('A', subject, 'INVALID_SERVER', `server "${server}" is not one of ${SERVERS.join(', ')}`);

  if (runtime === 'node') {
    for (const k of ['server', 'webserver', 'document_root', 'php'])
      if (k in m) err('A', subject, 'NODE_EXTRA_KEY', `\`${k}\` is meaningless for runtime=node`);
    if (!m.node) err('A', subject, 'MISSING_NODE_BLOCK', 'runtime=node requires a `node` block');
  }

  // -- node block ------------------------------------------------------
  if (m.node) {
    if (!m.node.version) err('A', subject, 'MISSING_NODE_VERSION', '`node.version` is required');
    else if (!/^[0-9]+$/.test(String(m.node.version)))
      err(
        'A',
        subject,
        'INVALID_NODE_VERSION',
        `\`node.version\` "${m.node.version}" must be a bare major (e.g. "22")`
      );
    else if (!list(env.SUPPORTED_LANGUAGES_NODEJS_VERSIONS).includes(String(m.node.version)))
      warn(
        'A',
        subject,
        'UNLISTED_NODE_VERSION',
        `node ${m.node.version} is not in SUPPORTED_LANGUAGES_NODEJS_VERSIONS`
      );

    const port = m.node.port ?? 3000;
    if (!Number.isInteger(port) || port < 1 || port > 65535)
      err('A', subject, 'INVALID_PORT', `\`node.port\` ${port} is out of range`);
    // Only flag what is actually likely to bind loopback: an explicit localhost, or a dev
    // server (vite/next/nuxt/npm run dev) that defaults to it without an override.
    const start = m.node.start || '';
    const explicitLoopback = /localhost|127\.0\.0\.1/.test(start);
    const devServer = /\b(vite|next dev|nuxt dev|npm run dev|yarn dev|pnpm dev)\b/.test(start);
    if (start && (explicitLoopback || (devServer && !/--host/.test(start))))
      warn(
        'A',
        subject,
        'BIND_LOCALHOST',
        `\`node.start\` (${start}) binds loopback by default — Traefik cannot reach it; add --host 0.0.0.0`
      );
  }

  // -- php block -------------------------------------------------------
  if (runtime === 'php') {
    if (!m.php) {
      err('A', subject, 'MISSING_PHP_BLOCK', 'runtime=php requires a `php` block');
    } else {
      const v = m.php.version;
      if (!v) err('A', subject, 'MISSING_PHP_VERSION', '`php.version` is required');
      else if (!/^[0-9]+\.[0-9]+$/.test(v))
        err('A', subject, 'INVALID_PHP_VERSION', `\`php.version\` "${v}" must be major.minor`);
      else {
        if (!list(env.SUPPORTED_LANGUAGES_PHP_VERSIONS).includes(v))
          warn(
            'A',
            subject,
            'UNLISTED_PHP_VERSION',
            `PHP ${v} is not in SUPPORTED_LANGUAGES_PHP_VERSIONS`
          );
        if (cmpVersion(v, '8.0') < 0)
          warn(
            'A',
            subject,
            'C-13',
            `PHP ${v} is below the v1 floor of 8.0 — the extension matrix assumes 8.0+`
          );
      }

      const exts = m.php.extensions;
      if (exts !== undefined) {
        if (!Array.isArray(exts)) {
          err('A', subject, 'INVALID_EXTENSIONS', '`php.extensions` must be an array');
        } else {
          // No count limit: C-04's `grep -A 50` window went out with the Bash
          // extractor, and the Rust generator installs whatever is listed.
          const seen = new Set();
          for (const e of exts) {
            if (typeof e !== 'string') {
              err('A', subject, 'INVALID_EXTENSIONS', 'extension entries must be strings');
              continue;
            }
            if (seen.has(e)) warn('A', subject, 'DUPLICATE_EXTENSION', `"${e}" listed twice`);
            seen.add(e);

            if (!EXT_NAME.test(e)) {
              err(
                'A',
                subject,
                'C-14',
                `extension "${e}" contains characters outside [a-z0-9_] — the Bash extractor cannot match it and will silently drop it`
              );
              continue;
            }
            const spec = phpExt.extensions[e];
            if (!spec) {
              err('A', subject, 'UNKNOWN_EXTENSION', `"${e}" is not in the extension matrix`);
              continue;
            }
            if (spec.install === 'special')
              err(
                'A',
                subject,
                'UNSUPPORTED',
                `"${e}" needs a bespoke install sequence that v1 does not implement`
              );
            if (spec.install === 'composer')
              warn(
                'A',
                subject,
                'C-05',
                `"${e}" is a Composer package, not a PHP extension — it will produce no install line`
              );
            if (v && spec.removedIn && cmpVersion(v, spec.removedIn) >= 0)
              err(
                'A',
                subject,
                'C-06',
                `"${e}" was removed in PHP ${spec.removedIn} but this project targets ${v} — currently skipped silently`
              );
            if (v && spec.minPhp && cmpVersion(v, spec.minPhp) < 0)
              err(
                'A',
                subject,
                'MIN_PHP',
                `"${e}" requires PHP >= ${spec.minPhp}, project targets ${v}`
              );
          }

          // W-01: extensions must be the last key in the document
          const marker = raw.lastIndexOf('"extensions"');
          if (marker !== -1) {
            const close = raw.indexOf(']', marker);
            const tail = close === -1 ? '' : raw.slice(close + 1);
            if (!/^[\s}\],]*$/.test(tail))
              err(
                'A',
                subject,
                'W-01',
                'keys appear after `php.extensions` — the canonical layout puts the array last'
              );
          }
        }
      }
    }
  }
}

// An error, not a warning, unless the caller declared it expects none: suite A
// is the whole reason this validator reads a StackVo checkout, and with no
// manifests it asserts nothing while still printing "0 error(s)". A gate whose
// green means "looked at nothing" is worse than no gate, which is the lesson
// the coverage floors already learned.
if (manifestCount === 0)
  (allowNoManifests ? warn : err)(
    'A',
    'projects/',
    'NO_MANIFESTS',
    `no stackvo.json found under ${projectsDir} — suite A checked nothing.` +
      (allowNoManifests
        ? ' Expected: --allow-no-manifests was passed.'
        : ' Is --root pointing at a StackVo checkout? Pass --allow-no-manifests if it genuinely has none.')
  );

// ================================================================ SUITE B — extension catalog

const catalog = list(env.SUPPORTED_LANGUAGES_PHP_EXTENSIONS);
const defaultSet = list(env.SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT);
const defaultPhp = env.SUPPORTED_LANGUAGES_PHP_DEFAULT || env.DEFAULT_PHP_VERSION || '8.2';

for (const e of catalog) {
  if (!phpExt.extensions[e])
    err(
      'B',
      '.env',
      'CATALOG_UNKNOWN',
      `SUPPORTED_LANGUAGES_PHP_EXTENSIONS offers "${e}" but it is not in php-extensions.json`
    );
}
for (const e of defaultSet) {
  if (!catalog.includes(e))
    err(
      'B',
      '.env',
      'DEFAULT_NOT_IN_CATALOG',
      `"${e}" is in the default set but not in the catalog`
    );
}

// The headline check: can the shipped default selection actually build?
for (const e of defaultSet) {
  const spec = phpExt.extensions[e];
  if (!spec) continue;
  if (spec.removedIn && cmpVersion(defaultPhp, spec.removedIn) >= 0)
    err(
      'B',
      '.env',
      'C-06',
      `default extension "${e}" was removed in PHP ${spec.removedIn}, but the default PHP is ${defaultPhp} — the out-of-the-box selection cannot build`
    );
  if (spec.minPhp && cmpVersion(defaultPhp, spec.minPhp) < 0)
    err(
      'B',
      '.env',
      'C-06',
      `default extension "${e}" requires PHP >= ${spec.minPhp}, default PHP is ${defaultPhp}`
    );
  if (spec.install === 'composer')
    warn(
      'B',
      '.env',
      'C-05',
      `default set contains "${e}", which is a Composer package, not an extension`
    );
  if (spec.install === 'special')
    warn(
      'B',
      '.env',
      'UNSUPPORTED',
      `default set contains "${e}", which needs an unimplemented install path`
    );
}

// The catalog used to be checked against a ceiling of 50 here — C-04, the Bash
// parser window. It is gone, so selecting the whole catalog is now a supported
// (if slow to build) choice rather than a silent truncation.

// ================================================================ SUITE C — services

/**
 * The templates this app ships, not the ones a workspace happens to hold.
 *
 * The workspace copy was the only source when the templates lived in a
 * checkout. They live in `skeleton/` now and are compiled into the binary, and
 * a workspace has a copy of one only when somebody overrode it — so reading the
 * workspace made this suite report all twenty-one services as having no
 * template on any normal machine, which is the reverse of the truth.
 *
 * Reading `skeleton/` also makes the check run everywhere: "is every service in
 * env.schema.json backed by a template" is a question about this repository,
 * and it now gets answered on every machine rather than only on one with a
 * clone of the retired project beside it.
 */
const templatesDir = join(HERE, '..', 'skeleton', 'core', 'templates', 'services');
const templateIds = existsSync(templatesDir)
  ? readdirSync(templatesDir).filter(
      (d) => !d.startsWith('.') && statSync(join(templatesDir, d)).isDirectory()
    )
  : [];

const declaredIds = Object.entries(envSchema.services)
  .filter(([k]) => k !== '_note')
  .flatMap(([, v]) => v);

for (const id of templateIds) {
  if (!declaredIds.includes(id))
    err(
      'C',
      'env.schema.json',
      'SERVICE_UNDECLARED',
      `template "${id}" exists but is not listed in env.schema.json services`
    );

  const envKey = `SERVICE_${id.toUpperCase().replace(/-/g, '_')}_ENABLE`;
  // Same reasoning as MISSING_KEY above: a key the app carries as a default is
  // not a key the service is missing. Only a service nothing can switch at all
  // is the defect this is looking for.
  if (!(envKey in env) && !EMBEDDED.has(envKey)) {
    err('C', '.env', 'SERVICE_NO_ENABLE_KEY', `template "${id}" has no ${envKey}`);
    continue;
  }

  // The profiles a template has to declare, checked against what actually
  // starts it.
  //
  // This used to assert the *Bash CLI's* derivation — lowercase the env key's
  // service part, so `SERVICE_MONGO_EXPRESS_ENABLE` yields `mongo_express`
  // while the template declares `mongo-express`. That is C-09, and it was a
  // real bug in a program this repo no longer contains: the shell was deleted,
  // and `compose_up_service` passes the service id straight through
  // (`--profile mongo-express`), which is the mapping CONFLICTS.md decided on —
  // "the reverse mapping MUST use the service catalog, never a naive
  // tr '_' '-'". So the check kept failing the desktop app for a bug the
  // desktop app does not have, on the one service whose id contains a dash.
  //
  // What matters now is the rule the app relies on, and both halves of it: the
  // id itself, for starting one service alone, and `services`, for starting the
  // enabled set. A template missing either really does never start that way.
  const tplFile = join(templatesDir, id, `docker-compose.${id}.tpl`);
  if (existsSync(tplFile)) {
    const tpl = readFileSync(tplFile, 'utf8');
    const match = tpl.match(/profiles:\s*\[([^\]]*)\]/);
    const profiles = match
      ? match[1].split(',').map((s) => s.trim().replace(/^["']|["']$/g, ''))
      : [];
    if (profiles.length) {
      if (!profiles.includes(id))
        err(
          'C',
          `templates/services/${id}`,
          'PROFILE_NOT_SERVICE_ID',
          `starting this service alone passes --profile "${id}", but the template declares [${profiles.join(', ')}] — nothing would start`
        );
      if (!profiles.includes('services'))
        err(
          'C',
          `templates/services/${id}`,
          'PROFILE_NOT_IN_SERVICES',
          `the template declares [${profiles.join(', ')}] — "services" is missing, so starting the enabled set skips it`
        );
    }
  }
}

for (const id of declaredIds) {
  if (!templateIds.includes(id))
    err(
      'C',
      'env.schema.json',
      'SERVICE_NO_TEMPLATE',
      `service "${id}" is declared but has no template directory`
    );
}

// Dependency graph must reference real services.
for (const [svc, dep] of Object.entries(envSchema.serviceDependencies)) {
  if (svc === '_note') continue;
  if (!declaredIds.includes(svc))
    err(
      'C',
      'env.schema.json',
      'DEP_UNKNOWN_SERVICE',
      `dependency entry "${svc}" is not a known service`
    );
  for (const d of [...(dep.required || []), ...(dep.optional || [])])
    if (!declaredIds.includes(d))
      err(
        'C',
        'env.schema.json',
        'DEP_UNKNOWN_TARGET',
        `"${svc}" depends on "${d}", which is not a known service`
      );
}

// ================================================================ SUITE D — env keys

const SERVICE_KEY = /^SERVICE_[A-Z0-9_]+$/;

for (const key of Object.keys(env)) {
  if (envSpec[key] || SERVICE_KEY.test(key)) continue;
  warn('D', '.env', 'UNKNOWN_KEY', `"${key}" is set but not described in env.schema.json`);
}

for (const [key, spec] of Object.entries(envSpec)) {
  if (spec.status === 'dead' && key in env)
    warn(
      'D',
      '.env',
      'C-11',
      `"${key}" is still present but has zero consumers — scheduled for removal`
    );
  if (spec.status !== 'dead' && !(key in env) && !EMBEDDED.has(key) && spec.default !== undefined)
    warn('D', '.env', 'MISSING_KEY', `"${key}" is absent; readers fall back to "${spec.default}"`);
}

// Secrets that look real in a committed example file.
if (envPath && basename(envPath) === '.env.example') {
  for (const [k, v] of Object.entries(env)) {
    if (
      /(PASSWORD|PASS|TOKEN|SECRET|SERVER_ID)$/.test(k) &&
      v &&
      !/^(root|admin|changeme|)$/i.test(v)
    )
      err(
        'D',
        '.env.example',
        'C-18',
        `"${k}" carries a non-placeholder value in a committed file — rotate and replace with a placeholder`
      );
  }
}

// ================================================================ SUITE E — IPC surface

// This suite checks THIS repo, not the StackVo checkout: ipc.json is the
// agreement between the Vue front end and the Rust core, and nothing enforces
// it at compile time. A command declared but never registered is a promise the
// app does not keep; one registered but never reachable from JS is dead weight.
const ipcPath = join(CONTRACTS, 'ipc.json');
const libPath = join(HERE, '..', 'src-tauri', 'src', 'lib.rs');
const jsApiPath = join(HERE, '..', 'src', 'lib', 'ipc.js');

if (existsSync(ipcPath) && existsSync(libPath) && existsSync(jsApiPath)) {
  const ipc = readJson(ipcPath);
  const libSource = readFileSync(libPath, 'utf8');
  const jsSource = readFileSync(jsApiPath, 'utf8');

  const declared = Object.keys(ipc.commands ?? {});
  // Only the invoke_handler! block counts as "registered".
  const handlerBlock = libSource.slice(
    libSource.indexOf('generate_handler!'),
    libSource.indexOf('.run(tauri::generate_context')
  );
  const registered = [...handlerBlock.matchAll(/commands::(\w+)/g)].map((m) => m[1]);
  // Strip comments first: the module docstring mentions `call('whatever')` as
  // an example of what NOT to do, and matching it would report a phantom
  // command that fails at runtime.
  const jsCode = jsSource.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
  const called = [...jsCode.matchAll(/call\(\s*'([a-z_]+)'/g)].map((m) => m[1]);

  for (const cmd of declared) {
    const spec = ipc.commands[cmd] ?? {};
    // Commands the contract explicitly says live in the front end, or that are
    // deferred with a stated reason, are not gaps.
    if (spec.kind === 'frontend-plugin' || spec.status === 'deferred') continue;

    if (!registered.includes(cmd)) {
      warn(
        'E',
        'ipc.json',
        'NOT_IMPLEMENTED',
        `"${cmd}" is declared in the contract but not registered in lib.rs`
      );
    }
  }

  for (const cmd of registered) {
    if (!declared.includes(cmd)) {
      err(
        'E',
        'src-tauri/src/lib.rs',
        'UNDECLARED_COMMAND',
        `"${cmd}" is registered but absent from ipc.json — add it to the contract first`
      );
    }
    // `rustInternal` marks a command the front end deliberately does not call
    // because the same facts already arrive in another payload. Warning about
    // it forever would train people to ignore this suite.
    if (!called.includes(cmd) && !ipc.commands[cmd]?.rustInternal) {
      warn(
        'E',
        'src/lib/ipc.js',
        'UNREACHABLE',
        `"${cmd}" is registered but has no wrapper in the JS api, so no view can call it`
      );
    }
  }

  for (const cmd of called) {
    if (!registered.includes(cmd)) {
      err(
        'E',
        'src/lib/ipc.js',
        'CALLS_MISSING_COMMAND',
        `the JS api calls "${cmd}", which is not registered — this fails at runtime`
      );
    }
  }
}

// ================================================================ SUITE F — reachability

// Suite E proves a command can be called. This one asks whether anything
// actually calls it: a wrapper no view uses is a feature the user cannot reach,
// which looks identical to "done" from the command registry alone.
const srcDir = join(HERE, '..', 'src');

function collectSources(dir, pattern, acc = []) {
  if (!existsSync(dir)) return acc;
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) collectSources(full, pattern, acc);
    else if (pattern.test(entry) && full !== jsApiPath) acc.push(full);
  }
  return acc;
}

if (existsSync(jsApiPath)) {
  const apiSource = readFileSync(jsApiPath, 'utf8');
  // Method names on the exported `api` object.
  const apiBlock = apiSource.slice(apiSource.indexOf('export const api'));
  const methods = [...apiBlock.matchAll(/^\s{2}([a-zA-Z][a-zA-Z0-9]*):/gm)].map((m) => m[1]);

  const consumers = collectSources(srcDir, /\.(vue|js)$/)
    .map((f) => readFileSync(f, 'utf8'))
    .join('\n');

  for (const method of methods) {
    // `api.foo(` or destructured `foo(` after an import from lib/ipc.
    const used = new RegExp(`\\bapi\\.${method}\\b`).test(consumers);
    if (!used) {
      warn('F', 'src/', 'UNUSED_API', `api.${method}() is defined but no view or store calls it`);
    }
  }

  // Events the contract declares but the Rust side never emits are the mirror
  // image: the front end can listen forever and nothing arrives.
  const ipcDoc = readJson(ipcPath);
  const rustText = collectSources(join(HERE, '..', 'src-tauri', 'src'), /\.rs$/)
    .map((f) => readFileSync(f, 'utf8'))
    .join('\n');

  for (const [name, spec] of Object.entries(ipcDoc.events ?? {})) {
    if (name.startsWith('_')) continue;
    if (spec?.status === 'deferred') continue;
    // Lifecycle events are emitted from a shared helper that builds the name
    // from a domain and a verb, so no literal appears in the source. The
    // contract marks those explicitly rather than the checker guessing.
    if (spec?.emittedDynamically) continue;
    if (!rustText.includes(`"${name}"`)) {
      warn(
        'F',
        'src-tauri/src/',
        'NEVER_EMITTED',
        `event "${name}" is declared but nothing emits it`
      );
    }
  }
}

// ================================================================ SUITE G — package contracts

/**
 * The three schemas the service package format is written in.
 *
 * This suite checks THIS repo, not a StackVo checkout — the packages live in
 * `stackvo/stackvo-service-packages` and that repository runs its manifests
 * against these same files. What is checked here is the schemas themselves,
 * because a schema with a `required` field it does not declare, or a property
 * nobody described, is one that passes every validator and teaches nobody
 * anything.
 *
 * The heavier check — that `pkg::Manifest` in Rust and
 * `package-version.schema.json` describe the same object — lives in
 * `src-tauri/tests/package_contract.rs`, because it needs the Rust type.
 */
{
  const SCHEMAS = [
    'package.schema.json',
    'package-version.schema.json',
    'registry.schema.json',
  ];

  for (const name of SCHEMAS) {
    const file = join(CONTRACTS, name);
    if (!existsSync(file)) {
      err('G', name, 'SCHEMA_MISSING', 'contracts/README.md lists it and it is not there');
      continue;
    }

    let schema;
    try {
      schema = readJson(file);
    } catch (e) {
      err('G', name, 'SCHEMA_UNREADABLE', String(e.message ?? e));
      continue;
    }

    for (const key of ['$schema', '$id', 'title', 'description']) {
      if (!schema[key]) {
        err('G', name, 'SCHEMA_HEADER', `has no "${key}"`);
      }
    }

    // Walk every object node, wherever it is nested.
    const visit = (node, path) => {
      if (!node || typeof node !== 'object') return;
      if (Array.isArray(node)) {
        node.forEach((v, i) => visit(v, `${path}[${i}]`));
        return;
      }
      if (node.properties && typeof node.properties === 'object') {
        // A required name that is not a property is a rule no document can
        // satisfy and no validator will explain.
        for (const req of node.required ?? []) {
          if (!(req in node.properties)) {
            err(
              'G',
              name,
              'REQUIRED_NOT_DECLARED',
              `${path}: "${req}" is required and is not among the properties`
            );
          }
        }
        for (const [key, value] of Object.entries(node.properties)) {
          // `$ref` borrows a description along with everything else, and a
          // const or an enum is its own explanation.
          const described =
            value?.description || value?.$ref || value?.const || value?.enum;
          // A node whose children are described needs no description of its
          // own: `ports` means what its `name`, `container` and `preferred`
          // mean. Warning on containers too produced 39 lines nobody would
          // read, which is how a suite gets ignored.
          const isContainer =
            !!value?.properties || !!value?.items?.properties || !!value?.items?.$ref;
          if (!described && !isContainer) {
            warn('G', name, 'PROPERTY_UNDESCRIBED', `${path}.${key} has no description`);
          }
          visit(value, `${path}.${key}`);
        }
      }
      for (const key of ['items', 'additionalProperties', '$defs', 'anyOf', 'oneOf']) {
        if (node[key] && typeof node[key] === 'object') visit(node[key], `${path}.${key}`);
      }
    };
    visit(schema, name);
  }

  // The two files that must agree about which categories exist. A package
  // directory named for a category the registry cannot express is a package
  // nothing will index.
  try {
    const pkg = readJson(join(CONTRACTS, 'package.schema.json'));
    const reg = readJson(join(CONTRACTS, 'registry.schema.json'));
    const a = pkg.properties?.category?.enum ?? [];
    const b = reg.properties?.packages?.items?.properties?.category?.enum ?? [];
    if (JSON.stringify(a) !== JSON.stringify(b)) {
      err(
        'G',
        'categories',
        'CATEGORY_DRIFT',
        `package.schema.json offers [${a}] and registry.schema.json offers [${b}]`
      );
    }
  } catch {
    // Already reported above as unreadable.
  }
}

// ================================================================ output

const errors = findings.filter((f) => f.level === 'error');
const warns = findings.filter((f) => f.level === 'warn');

if (asJson) {
  console.log(
    JSON.stringify(
      { root: STACKVO_ROOT, envFile: envPath, manifests: manifestCount, errors, warnings: warns },
      null,
      2
    )
  );
} else {
  const SUITES = {
    A: 'manifests',
    B: 'extension catalog',
    C: 'services',
    D: 'env keys',
    E: 'IPC surface',
    F: 'reachability',
    G: 'package contracts',
  };
  console.log(`\nstackvo contract check — v1`);
  console.log(`  root      ${STACKVO_ROOT}${HAVE_WORKSPACE ? '' : '  (not there)'}`);
  console.log(`  env       ${envPath ?? '(none — every setting is a binary default)'}`);
  console.log(`  manifests ${manifestCount}`);
  if (!HAVE_WORKSPACE) {
    console.log(`  note      no workspace, so A–D checked nothing. E and F are about this repo.`);
  }
  console.log('');

  for (const suite of Object.keys(SUITES)) {
    const rows = findings.filter((f) => f.suite === suite);
    if (!rows.length) {
      console.log(`  [${suite}] ${SUITES[suite]} — clean`);
      continue;
    }
    console.log(`  [${suite}] ${SUITES[suite]}`);
    for (const f of rows) {
      const tag = f.level === 'error' ? 'ERROR' : 'warn ';
      console.log(`    ${tag} ${f.code.padEnd(22)} ${f.subject}`);
      console.log(`          ${f.message}`);
    }
    console.log('');
  }

  console.log(`  ${errors.length} error(s), ${warns.length} warning(s)\n`);
}

process.exit(errors.length ? 1 : 0);
