#!/usr/bin/env node
/**
 * Hold the bundle ceilings defined in `tools/bundle-budget.mjs`.
 *
 * ## A missing build is a failure, not a pass
 *
 * The same hole `check-coverage.mjs` closes. With no `dist/`, a naive gate has
 * nothing to compare and reports success — and a green tick is read as an
 * answer, not as "there was nothing to look at". Here the failure mode is
 * worse than usual, because `dist/` is gitignored: the ordinary state of a
 * fresh clone is *no build at all*, so the silent pass would be the normal
 * outcome rather than a rare one.
 *
 * Usage:
 *   node tools/check-bundle.mjs            after `npm run build`
 *   node tools/check-bundle.mjs --dir out  a different build directory
 */

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, basename } from 'node:path';
import { ceilings, measured } from './bundle-budget.mjs';

const argv = process.argv.slice(2);
const dirFlag = argv.indexOf('--dir');
const DIST = dirFlag === -1 ? 'dist' : argv[dirFlag + 1];

const kb = (bytes) => bytes / 1024;
const fmt = (n) => `${n.toFixed(1)} KB`;

function fail(message) {
  console.error(`\n  ✗ ${message}\n`);
  process.exit(1);
}

if (!existsSync(DIST)) {
  fail(`no build at ${DIST}/ — run \`npm run build\` first. Nothing was checked.`);
}

const indexHtml = join(DIST, 'index.html');
if (!existsSync(indexHtml)) {
  fail(`${DIST}/ has no index.html, so the eager set cannot be read. Nothing was checked.`);
}

/**
 * The assets the window loads before it can paint.
 *
 * Taken from the built HTML rather than a list kept here: the entry `<script>`,
 * every `modulepreload`, and every stylesheet. Vite writes exactly this set,
 * and reading it means a renamed or newly split chunk is accounted for without
 * anyone remembering to.
 */
function eagerAssets(html) {
  const refs = [
    ...html.matchAll(/<script[^>]+src="([^"]+)"/g),
    ...html.matchAll(/<link[^>]+rel="modulepreload"[^>]+href="([^"]+)"/g),
    ...html.matchAll(/<link[^>]+rel="stylesheet"[^>]+href="([^"]+)"/g),
  ].map((m) => m[1]);

  return [...new Set(refs)].map((ref) => basename(ref));
}

/** Every emitted file under `dist/`, recursively. */
function allFiles(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? allFiles(path) : [path];
  });
}

const html = readFileSync(indexHtml, 'utf8');
const eagerNames = eagerAssets(html);

if (!eagerNames.length) {
  fail(`${DIST}/index.html references no scripts or stylesheets — the parse found nothing.`);
}

const files = allFiles(DIST);
const sizeOf = (name) => {
  const match = files.find((path) => basename(path) === name);
  if (!match) fail(`${DIST}/index.html references ${name}, which is not in the build.`);
  return statSync(match).size;
};

const eagerBytes = eagerNames.reduce((sum, name) => sum + sizeOf(name), 0);
const totalBytes = files.reduce((sum, path) => sum + statSync(path).size, 0);

const rows = [
  { what: 'eager (first paint)', got: kb(eagerBytes), ceiling: ceilings.eagerKb, was: measured.eagerKb },
  { what: 'total (every asset)', got: kb(totalBytes), ceiling: ceilings.totalKb, was: measured.totalKb },
];

console.log('\n  Bundle budget — raw bytes, because nothing here crosses a network.\n');
console.log(`  eager set, from ${DIST}/index.html: ${eagerNames.join(', ')}\n`);

let over = false;
for (const { what, got, ceiling, was } of rows) {
  const ok = got <= ceiling;
  over ||= !ok;
  const drift = got - was;
  const arrow = drift > 0.05 ? `+${fmt(drift)}` : drift < -0.05 ? fmt(drift) : 'unchanged';
  console.log(
    `  ${ok ? '✓' : '✗'} ${what.padEnd(20)} ${fmt(got).padStart(10)}` +
      `   ceiling ${fmt(ceiling).padStart(9)}   since measured: ${arrow}`
  );
}

if (over) {
  console.error(
    '\n  A ceiling is not a suggestion. Either trim what grew, or raise the number\n' +
      '  in tools/bundle-budget.mjs with the reason in the commit message.\n'
  );
  process.exit(1);
}

console.log('\n  Within budget.\n');
