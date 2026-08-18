import { describe, it, expect } from 'vitest';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

/**
 * Suite A, actually run. §3 #33.
 *
 * The row said "checkout var ama suite A hiç koşmuyor", and it was right for a
 * reason nobody had looked at closely: suite A reads `stackvo.json` files under
 * `<root>/projects/`, and neither CI's checkout nor a developer's has any. So
 * the manifest half of the validator has never checked a manifest — not once,
 * on any machine — and the only signal was a `NO_MANIFESTS` error that read
 * like a configuration problem.
 *
 * It was not a configuration problem. It was a **design** one: the validator
 * had no input of its own, so whether it ran at all depended on what happened
 * to be lying around next to it. `tools/fixtures/validator-workspace/` is that
 * input, pinned in the repository, and this file is suite A running.
 *
 * ## Why the fixture is mostly broken on purpose
 *
 * A validator shown only valid input has never been shown to detect anything.
 * Three of the four projects are wrong, each in a different way, and the codes
 * they produce are asserted exactly — so a check that silently stopped firing
 * fails here rather than reporting a clean workspace.
 *
 * ## What running it found
 *
 * Two dead checks, both in the validator rather than in a manifest:
 *
 *  * `EMBEDDED` was scraped with a regex expecting a literal array. §3 #36 split
 *    the defaults into `SETTINGS` and `LEGACY_SERVICES` and made `EMBEDDED` a
 *    `const fn`, the regex stopped matching that day, and the scrape returned an
 *    **empty set** — twenty keys with binary defaults went back to being
 *    reported as missing from `.env`.
 *  * Even repaired, it read only the key NAMES. `.env` is written only when a
 *    setting is changed, so an untouched workspace has no file at all, and every
 *    value lookup resolved to nothing: every project ran an "unlisted" PHP
 *    version. The defaults are now merged under the file, which is the order the
 *    app itself resolves them in.
 */

const ROOT = resolve(import.meta.dirname, '..');
const FIXTURE = resolve(ROOT, 'tools/fixtures/validator-workspace');

function validate(root) {
  // `--json` rather than parsing the human table: a gate that reads a
  // column-aligned report is a gate that starts comparing the wrong column.
  // The validator exits non-zero when it finds errors, and the fixture is
  // *meant* to produce errors, so the exit code is not the signal here.
  let out;
  try {
    out = execFileSync(
      process.execPath,
      ['tools/validate-contracts.mjs', '--root', root, '--json'],
      { cwd: ROOT, encoding: 'utf8' }
    );
  } catch (e) {
    out = e.stdout;
  }
  return JSON.parse(out);
}

/** Suite A findings as `CODE @ project`, sorted so the comparison is stable. */
function suiteA(report, level) {
  return (report[level] ?? [])
    .filter((f) => f.suite === 'A')
    .map((f) => `${f.code} @ ${f.subject.split('/')[1]}`)
    .sort();
}

describe('suite A, against a workspace that exists', () => {
  const report = validate(FIXTURE);

  it('finds manifests at all, which is the thing that never happened', () => {
    expect(report.manifests).toBe(4);
    expect(suiteA(report, 'errors').concat(suiteA(report, 'warnings'))).not.toHaveLength(0);
  });

  it('reports exactly the errors the fixture was built to produce', () => {
    // One code per project, each a different failure mode: a version that is
    // not major.minor, an extension the matrix does not have, and a name the
    // Bash extractor would silently drop.
    expect(suiteA(report, 'errors')).toEqual([
      'C-14 @ bad-name',
      'INVALID_PHP_VERSION @ bad-php',
      'UNKNOWN_EXTENSION @ bad-extension',
    ]);
  });

  it('reports exactly the warnings, and no more', () => {
    // `no more` is the half that matters. Before the embedded defaults were
    // read, three spurious UNLISTED_PHP_VERSION warnings sat in this list —
    // one per project, including the valid one.
    expect(suiteA(report, 'warnings')).toEqual([
      'C-13 @ bad-name',
      'DUPLICATE_EXTENSION @ bad-extension',
    ]);
  });

  it('says nothing at all about the valid project', () => {
    const about = [...(report.errors ?? []), ...(report.warnings ?? [])].filter((f) =>
      f.subject.includes('projects/good/')
    );
    expect(about).toEqual([]);
  });
});

describe('the defaults the binary carries', () => {
  it('are read as values and not only as names', () => {
    // The fixture has no `.env`. If the embedded values were not merged in,
    // `SUPPORTED_LANGUAGES_PHP_VERSIONS` would be empty and PHP 8.3 — a version
    // the binary ships — would be reported as unlisted.
    const report = validate(FIXTURE);
    const unlisted = [...(report.warnings ?? [])].filter(
      (f) => f.code === 'UNLISTED_PHP_VERSION'
    );
    expect(unlisted).toEqual([]);
  });

  it('are scraped in a quantity that means the scrape worked', () => {
    // Not the exact 186 — that number is §7's business and
    // `platform_matrix_claims.rs` holds it. This only refuses the failure a
    // regex over source text actually has: matching nothing and looking calm.
    const report = validate(ROOT);
    const collapsed = [...(report.errors ?? [])].filter(
      (f) => f.code === 'EMBEDDED_UNREADABLE'
    );
    expect(collapsed).toEqual([]);
  });
});

describe('the repository itself', () => {
  it('still has nothing for suite A, and says so rather than passing quietly', () => {
    // The honest state, kept visible: `stackvo/stackvo` carries no `projects/`
    // directory. The fixture does not paper over that — it gives suite A
    // something to check *as well*, so the two runs answer two questions.
    const report = validate(ROOT);
    expect(report.manifests).toBe(0);
    expect((report.errors ?? []).map((f) => f.code)).toContain('NO_MANIFESTS');
  });
});
