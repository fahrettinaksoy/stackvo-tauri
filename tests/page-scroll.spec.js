import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Does every page bound its own content, or does it get squashed?
 *
 * `PageLayout`'s body is `display: flex; flex-direction: column` with
 * `overflow: hidden`. A page whose content is taller than the card therefore
 * does **not** overflow and does not scroll — its children are flex items, so
 * the browser shrinks them to fit. The Market page shipped this way and the
 * symptom was not a missing scrollbar: it was a status alert compressed to a
 * blue line a few pixels tall, above a catalogue that ran off the bottom edge
 * with no way to reach it.
 *
 * That is worse than an obvious break. Everything renders, every test passes,
 * and the page merely looks wrong in a way that reads as a styling opinion.
 *
 * ## Why a source scan rather than a rendering assertion
 *
 * jsdom does no layout. `getBoundingClientRect` answers zero for everything, so
 * "is this taller than its container" cannot be asked of the tree — the same
 * wall `pane-styles.spec.js` hit, for the same reason, and it reads the sources
 * too.
 *
 * ## The three shapes that count, all of them in the tree already
 *
 * 1. A scroll region of the page's own: `overflow-y: auto` on a `flex: 1 1 auto;
 *    min-height: 0` box. Settings, Dashboard, Market.
 * 2. A data table handed `height="100%"` inside such a box, which is Vuetify's
 *    own scrolling. Services, Projects.
 * 3. Delegation: the body is one component that does 1 or 2 itself. Dumps and
 *    Logs are `DumpView` and `LogView` and nothing else.
 *
 * The third is followed rather than assumed — a page that delegates to a
 * component with no scroll region is the same bug one file further away.
 */

const VIEWS = 'src/views';

/**
 * Comments out, before anything is matched.
 *
 * Not a nicety. The first version of this scanner read the whole file, and the
 * comment that was added *beside the fix* — explaining that the table pages
 * hand their table `height="100%"` — was itself a match. So the guard passed on
 * a page with its scroll region deliberately removed, which is a guard that
 * reports confidence and holds none. Verified the way it should have been the
 * first time: by breaking the page on purpose and watching this fail.
 */
const code = (source) =>
  source
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/^\s*\/\/.*$/gm, '');

const scrolls = (source) => {
  const body = code(source);
  return /overflow(-y)?\s*:\s*auto/.test(body) || /height="100%"/.test(body);
};

/** `@/components/DumpView.vue` → `src/components/DumpView.vue`. */
function imported(source) {
  const out = [];
  for (const [, path] of code(source).matchAll(/from\s+'@\/(.+?\.vue)'/g)) {
    out.push(join('src', path));
  }
  return out;
}

/**
 * Does this page delegate its whole body to a component?
 *
 * Recognised narrowly: a component element carrying a grow class, which is how
 * both of the current delegating pages are written. Anything looser would start
 * excusing pages that merely happen to render a component somewhere.
 */
function delegatesTo(source) {
  const match = code(source).match(/<([A-Z][A-Za-z]*)\b[^>]*class="[^"]*flex-grow-1[^"]*"/);
  return match?.[1] ?? null;
}

describe('every page that uses PageLayout', () => {
  const views = readdirSync(VIEWS)
    .filter((name) => name.endsWith('.vue'))
    .map((name) => ({
      name,
      path: join(VIEWS, name),
      source: readFileSync(join(VIEWS, name), 'utf8'),
    }))
    .filter((v) => v.source.includes('PageLayout'));

  it('has pages to check', () => {
    // The guard's own floor. A rename that emptied this list would otherwise
    // turn the whole file into a test that passes by looking at nothing.
    expect(views.length).toBeGreaterThanOrEqual(8);
  });

  it.each(views.map((v) => [v.name, v]))(
    '%s bounds its body so the content scrolls rather than being compressed',
    (_name, view) => {
      if (scrolls(view.source)) return;

      const child = delegatesTo(view.source);
      expect(
        child,
        `${view.path} declares no scroll region and delegates to nothing. Its content will be ` +
          'shrunk to fit PageLayout’s fixed height instead of scrolling.'
      ).toBeTruthy();

      const target = imported(view.source).find((path) => path.endsWith(`${child}.vue`));
      expect(
        target && existsSync(target),
        `${view.path} renders <${child}> and does not import it`
      ).toBeTruthy();

      expect(
        scrolls(readFileSync(target, 'utf8')),
        `${view.path} delegates its body to ${target}, which declares no scroll region either`
      ).toBe(true);
    }
  );
});
