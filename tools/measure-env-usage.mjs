#!/usr/bin/env node
/**
 * Measure which `.env` keys anything in the StackVo checkout actually reads,
 * and check the answer against the `status` labels in env.schema.json.
 *
 * This exists because the first pass of that measurement was done by hand and
 * was wrong: it ran from the wrong working directory, so `grep core/` resolved
 * to a path that did not exist and reported zero consumers for every key. That
 * made 17 keys look dead. Two were live. Two keys labelled active were dead.
 *
 * A hand-run grep produces a number that looks like evidence and is not
 * checkable later. This is.
 *
 *   node tools/measure-env-usage.mjs [--root ../stackvo] [--fix]
 */

import { readFileSync, writeFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCHEMA = join(HERE, '..', 'contracts', 'env.schema.json');

const argv = process.argv.slice(2);
const fix = argv.includes('--fix');
const rootFlag = argv.indexOf('--root');
const ROOT = resolve(
  rootFlag !== -1
    ? argv[rootFlag + 1]
    : process.env.STACKVO_ROOT || join(HERE, '..', '..', 'stackvo')
);

if (!existsSync(join(ROOT, 'core'))) {
  console.error(`No core/ under ${ROOT} — is that a StackVo checkout?`);
  process.exit(2);
}

/**
 * The UI builds these from a language name at runtime
 * (`SUPPORTED_LANGUAGES_${langUpper}_VERSIONS`), so no literal appears in the
 * source and a text search cannot find them. Declared rather than measured.
 */
const DYNAMIC = /^SUPPORTED_LANGUAGES_(PHP|PYTHON|GO|RUBY|RUST|NODEJS)_/;
const DYNAMIC_CONSUMER =
  'core/ui/server/routes/supported-languages.js (built from a template string)';

/** Every file under core/, minus the noise. */
function sources(dir, acc = []) {
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules' || entry === '.git' || entry === 'dist') continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) sources(full, acc);
    else acc.push(full);
  }
  return acc;
}

const files = sources(join(ROOT, 'core')).map((path) => {
  let text = '';
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    /* binary */
  }
  return { path: path.slice(ROOT.length + 1), text };
});

function consumersOf(key) {
  return files.filter((f) => f.text.includes(key)).map((f) => f.path);
}

// ---------------------------------------------------------------- compare

const schema = JSON.parse(readFileSync(SCHEMA, 'utf8'));
const mismatches = [];
let checked = 0;

for (const group of Object.values(schema.groups)) {
  for (const [key, spec] of Object.entries(group)) {
    if (key === '_note' || typeof spec !== 'object') continue;
    checked++;

    const found = DYNAMIC.test(key) ? [DYNAMIC_CONSUMER] : consumersOf(key);
    const measured = found.length ? 'active' : 'dead';
    const labelled = spec.status;

    // `conflicting` implies active; it is a finer label, not a contradiction.
    const labelledActive = labelled === 'active' || labelled === 'conflicting';
    const agrees = measured === 'active' ? labelledActive : labelled === 'dead';

    if (!agrees) {
      mismatches.push({ key, labelled, measured, count: found.length });
      if (fix) {
        spec.status = measured === 'active' ? 'active' : 'dead';
      }
    }

    if (fix) {
      if (found.length) spec.consumers = found;
      else delete spec.consumers;
    }
  }
}

if (fix) {
  writeFileSync(SCHEMA, JSON.stringify(schema, null, 2) + '\n');
}

// ---------------------------------------------------------------- report

console.log(`\nenv usage — ${checked} keys checked against ${ROOT}\n`);

if (!mismatches.length) {
  console.log('  every status label matches what the checkout actually reads\n');
  process.exit(0);
}

for (const m of mismatches) {
  console.log(
    `  ${m.key.padEnd(36)} labelled ${m.labelled}, measured ${m.measured} (${m.count} files)`
  );
}
console.log(
  `\n  ${mismatches.length} mismatch(es)${fix ? ' — schema updated' : ' — rerun with --fix to update the schema'}\n`
);
process.exit(fix ? 0 : 1);
