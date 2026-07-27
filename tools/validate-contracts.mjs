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
 *
 * Zero dependencies — it implements the specific rules rather than pulling in a schema engine,
 * so it runs in CI and in a fresh clone with nothing installed.
 *
 *   node tools/validate-contracts.mjs [--root ../stackvo] [--json]
 */

import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join, dirname, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const CONTRACTS = join(HERE, '..', 'contracts');

// ---------------------------------------------------------------- args

const argv = process.argv.slice(2);
const asJson = argv.includes('--json');
const rootFlag = argv.indexOf('--root');
const STACKVO_ROOT = resolve(
  rootFlag !== -1 ? argv[rootFlag + 1] : process.env.STACKVO_ROOT || join(HERE, '..', '..', 'stackvo')
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

const list = (v) => (v ? v.split(',').map((s) => s.trim()).filter(Boolean) : []);

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

if (!existsSync(STACKVO_ROOT)) {
  console.error(`stackvo checkout not found at ${STACKVO_ROOT}\nPass --root <path>.`);
  process.exit(2);
}

const phpExt = readJson(join(CONTRACTS, 'php-extensions.json'));
const envSchema = readJson(join(CONTRACTS, 'env.schema.json'));

const envPath = existsSync(join(STACKVO_ROOT, '.env'))
  ? join(STACKVO_ROOT, '.env')
  : join(STACKVO_ROOT, '.env.example');
const env = parseEnv(readFileSync(envPath, 'utf8'));

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

  if (!m.domain) err('A', subject, 'MISSING_DOMAIN', '`domain` is required — the generator aborts without it');

  // -- runtime ---------------------------------------------------------
  let runtime = m.runtime;
  if (runtime === undefined) {
    runtime = 'php';
    warn('A', subject, 'RUNTIME_IMPLICIT', 'no `runtime` key — readers default to "php" (C-01); writers should emit it explicitly');
  } else if (RUNTIME_ALIASES[runtime]) {
    err('A', subject, 'C-01', `\`runtime\` "${runtime}" is a legacy alias — canonical id is "${RUNTIME_ALIASES[runtime]}"`);
    runtime = RUNTIME_ALIASES[runtime];
  } else if (!['php', 'node'].includes(runtime)) {
    err('A', subject, 'C-02', `\`runtime\` "${runtime}" has no generator (only php and node are implemented)`);
  }

  // -- one runtime block (W-02) ---------------------------------------
  const blocks = ['php', 'node', 'nodejs', 'python', 'ruby', 'golang', 'go', 'rust'].filter((k) => k in m);
  if (blocks.length > 1)
    err('A', subject, 'W-02', `${blocks.length} runtime blocks present (${blocks.join(', ')}) — the Bash parser reads the first "version" it finds and corrupts the output`);
  if (blocks.includes('nodejs'))
    err('A', subject, 'C-01', 'runtime block key is "nodejs" — canonical key is "node" (this manifest was written by the web UI and will be generated as PHP)');
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
      err('A', subject, 'INVALID_NODE_VERSION', `\`node.version\` "${m.node.version}" must be a bare major (e.g. "22")`);
    else if (!list(env.SUPPORTED_LANGUAGES_NODEJS_VERSIONS).includes(String(m.node.version)))
      warn('A', subject, 'UNLISTED_NODE_VERSION', `node ${m.node.version} is not in SUPPORTED_LANGUAGES_NODEJS_VERSIONS`);

    const port = m.node.port ?? 3000;
    if (!Number.isInteger(port) || port < 1 || port > 65535)
      err('A', subject, 'INVALID_PORT', `\`node.port\` ${port} is out of range`);
    // Only flag what is actually likely to bind loopback: an explicit localhost, or a dev
    // server (vite/next/nuxt/npm run dev) that defaults to it without an override.
    const start = m.node.start || '';
    const explicitLoopback = /localhost|127\.0\.0\.1/.test(start);
    const devServer = /\b(vite|next dev|nuxt dev|npm run dev|yarn dev|pnpm dev)\b/.test(start);
    if (start && (explicitLoopback || (devServer && !/--host/.test(start))))
      warn('A', subject, 'BIND_LOCALHOST', `\`node.start\` (${start}) binds loopback by default — Traefik cannot reach it; add --host 0.0.0.0`);
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
          warn('A', subject, 'UNLISTED_PHP_VERSION', `PHP ${v} is not in SUPPORTED_LANGUAGES_PHP_VERSIONS`);
        if (cmpVersion(v, '8.0') < 0)
          warn('A', subject, 'C-13', `PHP ${v} is below the v1 floor of 8.0 — the extension matrix assumes 8.0+`);
      }

      const exts = m.php.extensions;
      if (exts !== undefined) {
        if (!Array.isArray(exts)) {
          err('A', subject, 'INVALID_EXTENSIONS', '`php.extensions` must be an array');
        } else {
          // C-04: the grep -A 50 window
          if (exts.length > 50)
            err('A', subject, 'C-04', `${exts.length} extensions — the Bash parser reads only 50 lines past the marker, so ${exts.length - 50} will be SILENTLY DROPPED`);

          const seen = new Set();
          for (const e of exts) {
            if (typeof e !== 'string') { err('A', subject, 'INVALID_EXTENSIONS', 'extension entries must be strings'); continue; }
            if (seen.has(e)) warn('A', subject, 'DUPLICATE_EXTENSION', `"${e}" listed twice`);
            seen.add(e);

            if (!EXT_NAME.test(e)) {
              err('A', subject, 'C-14', `extension "${e}" contains characters outside [a-z0-9_] — the Bash extractor cannot match it and will silently drop it`);
              continue;
            }
            const spec = phpExt.extensions[e];
            if (!spec) { err('A', subject, 'UNKNOWN_EXTENSION', `"${e}" is not in the extension matrix`); continue; }
            if (spec.install === 'special')
              err('A', subject, 'UNSUPPORTED', `"${e}" needs a bespoke install sequence that v1 does not implement`);
            if (spec.install === 'composer')
              warn('A', subject, 'C-05', `"${e}" is a Composer package, not a PHP extension — it will produce no install line`);
            if (v && spec.removedIn && cmpVersion(v, spec.removedIn) >= 0)
              err('A', subject, 'C-06', `"${e}" was removed in PHP ${spec.removedIn} but this project targets ${v} — currently skipped silently`);
            if (v && spec.minPhp && cmpVersion(v, spec.minPhp) < 0)
              err('A', subject, 'MIN_PHP', `"${e}" requires PHP >= ${spec.minPhp}, project targets ${v}`);
          }

          // W-01: extensions must be the last key in the document
          const marker = raw.lastIndexOf('"extensions"');
          if (marker !== -1) {
            const close = raw.indexOf(']', marker);
            const tail = close === -1 ? '' : raw.slice(close + 1);
            if (!/^[\s}\],]*$/.test(tail))
              err('A', subject, 'W-01', 'keys appear after `php.extensions` — the Bash extractor swallows the next 50 lines of quoted tokens as extension names');

            const linesAfter = raw.slice(marker, close === -1 ? undefined : close).split('\n').length - 1;
            if (linesAfter > 50)
              err('A', subject, 'C-04', `the extensions array spans ${linesAfter} lines; the parser window is 50`);
          }
        }
      }
    }
  }
}

if (manifestCount === 0) warn('A', 'projects/', 'NO_MANIFESTS', `no stackvo.json found under ${projectsDir}`);

// ================================================================ SUITE B — extension catalog

const catalog = list(env.SUPPORTED_LANGUAGES_PHP_EXTENSIONS);
const defaultSet = list(env.SUPPORTED_LANGUAGES_PHP_EXTENSIONS_DEFAULT);
const defaultPhp = env.SUPPORTED_LANGUAGES_PHP_DEFAULT || env.DEFAULT_PHP_VERSION || '8.2';

for (const e of catalog) {
  if (!phpExt.extensions[e])
    err('B', '.env', 'CATALOG_UNKNOWN', `SUPPORTED_LANGUAGES_PHP_EXTENSIONS offers "${e}" but it is not in php-extensions.json`);
}
for (const e of defaultSet) {
  if (!catalog.includes(e))
    err('B', '.env', 'DEFAULT_NOT_IN_CATALOG', `"${e}" is in the default set but not in the catalog`);
}

// The headline check: can the shipped default selection actually build?
for (const e of defaultSet) {
  const spec = phpExt.extensions[e];
  if (!spec) continue;
  if (spec.removedIn && cmpVersion(defaultPhp, spec.removedIn) >= 0)
    err('B', '.env', 'C-06', `default extension "${e}" was removed in PHP ${spec.removedIn}, but the default PHP is ${defaultPhp} — the out-of-the-box selection cannot build`);
  if (spec.minPhp && cmpVersion(defaultPhp, spec.minPhp) < 0)
    err('B', '.env', 'C-06', `default extension "${e}" requires PHP >= ${spec.minPhp}, default PHP is ${defaultPhp}`);
  if (spec.install === 'composer')
    warn('B', '.env', 'C-05', `default set contains "${e}", which is a Composer package, not an extension`);
  if (spec.install === 'special')
    warn('B', '.env', 'UNSUPPORTED', `default set contains "${e}", which needs an unimplemented install path`);
}

if (defaultSet.length > 50)
  err('B', '.env', 'C-04', `the default extension set has ${defaultSet.length} entries; selecting them all loses ${defaultSet.length - 50} to the parser window`);
if (catalog.length > 50)
  warn('B', '.env', 'C-04', `the catalog offers ${catalog.length} extensions but a manifest can only carry 50 — selecting everything silently drops ${catalog.length - 50}`);

// ================================================================ SUITE C — services

const templatesDir = join(STACKVO_ROOT, 'core', 'templates', 'services');
const templateIds = existsSync(templatesDir)
  ? readdirSync(templatesDir).filter((d) => !d.startsWith('.') && statSync(join(templatesDir, d)).isDirectory())
  : [];

const declaredIds = Object.entries(envSchema.services)
  .filter(([k]) => k !== '_note')
  .flatMap(([, v]) => v);

for (const id of templateIds) {
  if (!declaredIds.includes(id))
    err('C', 'env.schema.json', 'SERVICE_UNDECLARED', `template "${id}" exists but is not listed in env.schema.json services`);

  const envKey = `SERVICE_${id.toUpperCase().replace(/-/g, '_')}_ENABLE`;
  if (!(envKey in env)) {
    err('C', '.env', 'SERVICE_NO_ENABLE_KEY', `template "${id}" has no ${envKey}`);
    continue;
  }

  // C-09: the CLI lowercases the env key's service part; the template declares the real profile.
  const derivedProfile = envKey.replace(/^SERVICE_/, '').replace(/_ENABLE$/, '').toLowerCase();
  const tplFile = join(templatesDir, id, `docker-compose.${id}.tpl`);
  if (existsSync(tplFile)) {
    const tpl = readFileSync(tplFile, 'utf8');
    const match = tpl.match(/profiles:\s*\[([^\]]*)\]/);
    const profiles = match ? match[1].split(',').map((s) => s.trim().replace(/^["']|["']$/g, '')) : [];
    if (profiles.length && !profiles.includes(derivedProfile))
      err('C', `templates/services/${id}`, 'C-09', `\`stackvo up\` derives --profile "${derivedProfile}" from ${envKey}, but the template declares [${profiles.join(', ')}] — this service never starts in minimal mode`);
  }
}

for (const id of declaredIds) {
  if (!templateIds.includes(id))
    err('C', 'env.schema.json', 'SERVICE_NO_TEMPLATE', `service "${id}" is declared but has no template directory`);
}

// Dependency graph must reference real services.
for (const [svc, dep] of Object.entries(envSchema.serviceDependencies)) {
  if (svc === '_note') continue;
  if (!declaredIds.includes(svc))
    err('C', 'env.schema.json', 'DEP_UNKNOWN_SERVICE', `dependency entry "${svc}" is not a known service`);
  for (const d of [...(dep.required || []), ...(dep.optional || [])])
    if (!declaredIds.includes(d))
      err('C', 'env.schema.json', 'DEP_UNKNOWN_TARGET', `"${svc}" depends on "${d}", which is not a known service`);
}

// ================================================================ SUITE D — env keys

const SERVICE_KEY = /^SERVICE_[A-Z0-9_]+$/;

for (const key of Object.keys(env)) {
  if (envSpec[key] || SERVICE_KEY.test(key)) continue;
  warn('D', '.env', 'UNKNOWN_KEY', `"${key}" is set but not described in env.schema.json`);
}

for (const [key, spec] of Object.entries(envSpec)) {
  if (spec.status === 'dead' && key in env)
    warn('D', '.env', 'C-11', `"${key}" is still present but has zero consumers — scheduled for removal`);
  if (spec.status !== 'dead' && !(key in env) && spec.default !== undefined)
    warn('D', '.env', 'MISSING_KEY', `"${key}" is absent; readers fall back to "${spec.default}"`);
}

// Secrets that look real in a committed example file.
if (basename(envPath) === '.env.example') {
  for (const [k, v] of Object.entries(env)) {
    if (/(PASSWORD|PASS|TOKEN|SECRET|SERVER_ID)$/.test(k) && v && !/^(root|admin|changeme|)$/i.test(v))
      err('D', '.env.example', 'C-18', `"${k}" carries a non-placeholder value in a committed file — rotate and replace with a placeholder`);
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
      warn('E', 'ipc.json', 'NOT_IMPLEMENTED', `"${cmd}" is declared in the contract but not registered in lib.rs`);
    }
  }

  for (const cmd of registered) {
    if (!declared.includes(cmd)) {
      err('E', 'src-tauri/src/lib.rs', 'UNDECLARED_COMMAND', `"${cmd}" is registered but absent from ipc.json — add it to the contract first`);
    }
    // `rustInternal` marks a command the front end deliberately does not call
    // because the same facts already arrive in another payload. Warning about
    // it forever would train people to ignore this suite.
    if (!called.includes(cmd) && !ipc.commands[cmd]?.rustInternal) {
      warn('E', 'src/lib/ipc.js', 'UNREACHABLE', `"${cmd}" is registered but has no wrapper in the JS api, so no view can call it`);
    }
  }

  for (const cmd of called) {
    if (!registered.includes(cmd)) {
      err('E', 'src/lib/ipc.js', 'CALLS_MISSING_COMMAND', `the JS api calls "${cmd}", which is not registered — this fails at runtime`);
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
      warn('F', 'src-tauri/src/', 'NEVER_EMITTED', `event "${name}" is declared but nothing emits it`);
    }
  }
}

// ================================================================ output

const errors = findings.filter((f) => f.level === 'error');
const warns = findings.filter((f) => f.level === 'warn');

if (asJson) {
  console.log(JSON.stringify({ root: STACKVO_ROOT, envFile: envPath, manifests: manifestCount, errors, warnings: warns }, null, 2));
} else {
  const SUITES = { A: 'manifests', B: 'extension catalog', C: 'services', D: 'env keys', E: 'IPC surface', F: 'reachability' };
  console.log(`\nstackvo contract check — v1`);
  console.log(`  root      ${STACKVO_ROOT}`);
  console.log(`  env       ${envPath}`);
  console.log(`  manifests ${manifestCount}\n`);

  for (const suite of Object.keys(SUITES)) {
    const rows = findings.filter((f) => f.suite === suite);
    if (!rows.length) { console.log(`  [${suite}] ${SUITES[suite]} — clean`); continue; }
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
