#!/usr/bin/env node
/**
 * Hold the coverage floors defined in `tools/coverage-floors.mjs`.
 *
 * Two reports, one gate. The alternative — `--fail-under-lines` on the Rust
 * side and `thresholds` on the front-end side — puts the same policy in two
 * files with two syntaxes, and the first time somebody raises one and forgets
 * the other the build is enforcing a rule nobody wrote down.
 *
 * ## A missing report is a failure, not a pass
 *
 * The measuring steps in CI are `continue-on-error`, so a failed test run still
 * produces a table instead of taking the job down for a reason the `build` job
 * already reports. That is deliberate, and it has an obvious hole: if the
 * measurement never ran, there is nothing to compare and a naive gate reports
 * success. This one refuses to. A gate that passes when it cannot see is worse
 * than no gate, because the green tick is read as an answer.
 *
 * Usage:
 *   node tools/check-coverage.mjs                    both halves
 *   node tools/check-coverage.mjs --frontend         only the front end
 *   node tools/check-coverage.mjs --rust             only the Rust core
 *   node tools/check-coverage.mjs --rust-report path override the default path
 */

import { readFileSync } from 'node:fs';
import { floors, measured } from './coverage-floors.mjs';

const DEFAULT_RUST_REPORT = 'rust-coverage.json';
const DEFAULT_FRONTEND_REPORT = 'coverage/coverage-summary.json';

function parseArgs(argv) {
  const opts = {
    rust: false,
    frontend: false,
    rustReport: DEFAULT_RUST_REPORT,
    frontendReport: DEFAULT_FRONTEND_REPORT,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--rust') opts.rust = true;
    else if (arg === '--frontend') opts.frontend = true;
    else if (arg === '--rust-report') opts.rustReport = argv[(i += 1)];
    else if (arg === '--frontend-report') opts.frontendReport = argv[(i += 1)];
    else {
      console.error(`unknown argument: ${arg}`);
      process.exit(2);
    }
  }
  // Neither named means both, which is what CI wants and what a bare local run
  // should mean too.
  if (!opts.rust && !opts.frontend) {
    opts.rust = true;
    opts.frontend = true;
  }
  return opts;
}

function read(path, what) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (e) {
    throw new Error(
      `no ${what} coverage report at ${path} (${e.code ?? e.message}).\n` +
        `  Rust:     npm run test:rs:coverage\n` +
        `  Frontend: npm run test:js:coverage`
    );
  }
}

/** `cargo llvm-cov report --json --summary-only`. */
function rustTotals(path) {
  const report = read(path, 'Rust');
  const totals = report?.data?.[0]?.totals;
  if (!totals?.lines) {
    throw new Error(
      `${path} is not an llvm-cov JSON export — no data[0].totals.lines. ` +
        `Produce it with: cargo llvm-cov report --json --summary-only`
    );
  }
  return { lines: totals.lines.percent };
}

/** vitest's `json-summary` reporter. */
function frontendTotals(path) {
  const report = read(path, 'front-end');
  const total = report?.total;
  if (!total?.lines) {
    throw new Error(`${path} has no \`total\` block — is it vitest's json-summary output?`);
  }
  return Object.fromEntries(Object.entries(total).map(([k, v]) => [k, v.pct]));
}

/** One row per floor, so the passing ones are visible too. */
function check(label, actual, floor, reference) {
  const rows = [];
  let failed = 0;

  for (const [metric, min] of Object.entries(floor)) {
    const pct = actual[metric];
    if (typeof pct !== 'number' || Number.isNaN(pct)) {
      rows.push(`  ${label} ${metric}: no number in the report — expected one for a floor of ${min}%`);
      failed += 1;
      continue;
    }
    const ok = pct >= min;
    if (!ok) failed += 1;
    // What this metric read when the floor was set, so the distance between the
    // floor and reality is visible at the moment somebody is looking at it.
    const was = reference?.[metric];
    const drift = typeof was === 'number' ? ` (floor set at ${was.toFixed(2)}%)` : '';
    rows.push(
      `  ${ok ? 'ok  ' : 'FAIL'} ${label} ${metric.padEnd(10)} ${pct.toFixed(2).padStart(6)}%  floor ${String(min).padStart(3)}%${drift}`
    );
  }

  return { rows, failed };
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const lines = [];
  let failed = 0;

  if (opts.rust) {
    const { rows, failed: n } = check(
      'rust    ',
      rustTotals(opts.rustReport),
      floors.rust,
      measured.rust
    );
    lines.push(...rows);
    failed += n;
  }

  if (opts.frontend) {
    const { rows, failed: n } = check(
      'frontend',
      frontendTotals(opts.frontendReport),
      floors.frontend,
      measured.frontend
    );
    lines.push(...rows);
    failed += n;
  }

  console.log('Coverage floors');
  console.log(lines.join('\n'));

  if (failed > 0) {
    console.error(
      `\n${failed} floor${failed === 1 ? '' : 's'} breached.\n\n` +
        `A floor is a regression alarm: something that used to be exercised is not any more.\n` +
        `Restore the coverage, or — if the drop is intended and understood — lower the floor in\n` +
        `tools/coverage-floors.mjs in the same commit, with the reason in the message.`
    );
    process.exit(1);
  }
}

try {
  main();
} catch (e) {
  console.error(e.message);
  process.exit(1);
}
