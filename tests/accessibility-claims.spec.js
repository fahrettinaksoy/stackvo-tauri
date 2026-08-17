import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The accessibility statement, held to the thing it is a statement about.
 *
 * §3 #25 asked for a statement and the prerequisite was a measurement. The risk
 * with that pair is not that the statement is written badly — it is that the
 * measurement moves and the statement does not. A conformance claim that
 * silently describes a smaller product than the one shipping is worse than no
 * claim, because somebody relies on it.
 *
 * So the two documents are checked against each other, and against the router:
 * the statement's table, the axe suite's page list and the application's routes
 * are three copies of one fact, and any two of them disagreeing fails here.
 */

const root = join(import.meta.dirname, '..');
const read = (path) => readFileSync(join(root, path), 'utf8');

const statement = read('docs/accessibility.md');
const suite = read('tests/e2e/a11y.e2e.js');
const router = read('src/router/index.js');

/** Every top-level route the application registers, as a path. */
const routePaths = [...router.matchAll(/path:\s*'([^']+)'/g)]
  .map((m) => m[1])
  .filter((path) => path.startsWith('/') && !path.includes('*') && path !== '/:pathMatch(.*)*');

/**
 * The routes the axe suite actually opens, normalised to a router path.
 *
 * Read out of the `PAGES` array by name rather than by matching every pair of
 * quoted strings in the file — the first version of this did the latter and
 * counted `['serious', 'critical']`, the severity filter, as two routes.
 */
const pagesBlock = suite.match(/const PAGES = \[([\s\S]*?)\];/)?.[1] ?? '';
const measuredPaths = [...pagesBlock.matchAll(/\['([^']+)',\s*'([^']+)'\]/g)]
  .map((m) => m[2].replace(/^\/#/, '') || '/')
  // `/projects/shop` is the detail route with its parameter filled in.
  .map((path) => (/^\/projects\/[^/]+$/.test(path) ? '/projects/:name' : path));

describe('the statement and the measurement', () => {
  it('measures every route the application has', () => {
    const missing = routePaths.filter((path) => !measuredPaths.includes(path));
    expect(
      missing,
      'a route with no axe pass is a screen the statement claims and never checked'
    ).toEqual([]);
  });

  /**
   * The other direction, which is the one that rots quietly: a route removed
   * from the router leaves a row in the statement claiming a screen nobody can
   * open.
   */
  it('claims no route the application does not have', () => {
    const extra = measuredPaths.filter((path) => !routePaths.includes(path));
    expect(extra).toEqual([]);
  });

  it('lists every measured route in its own table', () => {
    for (const path of routePaths) {
      // The table writes the detail route as `/projects/:name`, and the rest
      // verbatim.
      expect(statement, `${path} is measured but the statement does not list it`).toContain(
        `\`${path}\``
      );
    }
  });

  /**
   * The number the whole document rests on. If the suite ever stops holding
   * violations at zero, this claim has to be rewritten rather than left
   * standing.
   */
  it('claims zero violations, and the suite is what asserts that', () => {
    expect(statement).toContain('zero violations');
    expect(suite).toMatch(/toEqual\(\[\]\)/);
    // Scoped to `#app`, the run cannot see the overlay container — the mistake
    // §2 of the statement records. It must not come back.
    expect(suite, 'the axe run must not be scoped away from the overlays again').not.toMatch(
      /\.include\(['"]#app/
    );
  });
});

describe('the statement itself', () => {
  it('says what it cannot claim', () => {
    for (const limitation of ['screen-reader', 'tauri-driver', 'self-assessment']) {
      expect(statement.toLowerCase()).toContain(limitation);
    }
  });

  /** A statement with no way to complain is a statement nobody can act on —
   *  EN 301 549 asks for the channel, and so does anybody using it. */
  it('names a feedback channel', () => {
    expect(statement.toLowerCase()).toMatch(/feedback|issue/);
  });
});
