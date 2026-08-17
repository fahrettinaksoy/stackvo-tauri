import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Does every class the panes use actually resolve?
 *
 * This is the test that was missing. §14.16 moved fourteen sections of
 * `ProjectDetail.vue` into child components and left the rules behind in the
 * view's `<style scoped>` block — which only ever reaches the elements its own
 * component renders. Every extracted pane rendered unstyled: cards with no
 * surface, and label/value pairs run together into `Adstackvo-parser.ajans`.
 *
 * 497 tests stayed green through all of it. Mount tests assert on text and on
 * roles; not one of them looks at a stylesheet, and jsdom does not apply the
 * `<style>` block of a single-file component at all. So the guard cannot be a
 * rendering assertion — it has to read the sources.
 *
 * What it checks: every class an extracted pane names in its markup is defined
 * somewhere that can reach it — the shared sheet, the pane's own `<style>`, or
 * a framework/utility prefix the app does not own.
 */

/**
 * Both pane families, because both broke the same way — and the Settings one
 * was found by pointing this test at it rather than by anybody looking at the
 * screen.
 */
const FAMILIES = [
  {
    name: 'project',
    dir: 'src/components/project',
    sheet: 'src/styles/project-panes.css',
    view: 'src/views/ProjectDetail.vue',
    // `.section-head` already means something else in `Projects.vue`, so these
    // are nested under the page rather than made global.
    ancestor: '.detail-content',
    chrome: /^detail-|^nav-item|^cmd-menu/,
  },
  {
    name: 'settings',
    dir: 'src/components/settings',
    sheet: 'src/styles/settings-panes.css',
    view: 'src/views/Settings.vue',
    ancestor: '.settings-scroll',
    chrome: /^settings-/,
  },
];

/**
 * Prefixes the app does not define and must not be asked to.
 *
 * Vuetify's own classes (`v-*`), its utility grid and spacing helpers, and the
 * two-letter typography scale. Listed rather than pattern-matched loosely: a
 * blanket "ignore anything short" would swallow the real misses.
 */
const NOT_OURS = [
  /^v-/,
  /^(d|ga|ma|pa|mx|my|px|py|mt|mb|ml|mr|pt|pb|pl|pr)-/,
  /^(text|bg|justify|align|flex|font|rounded|border|elevation|position|overflow|w|h)-/,
  /^(mono|gap)$/,
];

const owned = (cls) => !NOT_OURS.some((re) => re.test(cls));

/** Every class named in a `class="…"` or `:class="…"` attribute. */
function classesIn(source) {
  const found = new Set();

  for (const [, value] of source.matchAll(/\sclass="([^"]*)"/g)) {
    for (const cls of value.split(/\s+/)) if (cls && owned(cls)) found.add(cls);
  }
  // `:class="{ 'heat-cell': … }"` and `:class="heatLevel(value)"` — the object
  // form names classes as string keys; the expression form cannot be read
  // statically and is skipped rather than guessed at.
  for (const [, value] of source.matchAll(/\s:class="([^"]*)"/g)) {
    for (const [, cls] of value.matchAll(/'([a-z][\w-]*)'/g)) if (owned(cls)) found.add(cls);
  }
  return found;
}

/** Every class the sheet defines, at any depth of the selector. */
function definedIn(css) {
  return new Set([...css.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
}

describe.each(FAMILIES)('the $name panes', ({ dir, sheet, view, ancestor, chrome }) => {
  const panes = readdirSync(dir).filter((f) => f.endsWith('.vue'));
  const shared = definedIn(readFileSync(sheet, 'utf8'));

  it('are actually a set of panes, not one file that was renamed', () => {
    expect(panes.length).toBeGreaterThan(10);
  });

  it.each(panes)('%s uses no class that nothing defines', (file) => {
    const source = readFileSync(join(dir, file), 'utf8');

    // A pane may carry its own rules; those count too.
    const own = definedIn(source.slice(source.indexOf('<style')));
    const missing = [...classesIn(source)].filter((c) => !shared.has(c) && !own.has(c));

    expect(missing, `${file} names classes no stylesheet reaches`).toEqual([]);
  });

  /**
   * The sheet has to be loaded, and it is plain CSS rather than a component
   * block — nothing imports it implicitly.
   */
  it('has its stylesheet imported by the app entry point', () => {
    expect(readFileSync('src/main.js', 'utf8')).toContain(sheet.replace('src/', ''));
  });

  /** Nested rather than global: these names are not reserved to one page. */
  it('scopes every rule under the page that owns them', () => {
    const rules = readFileSync(sheet, 'utf8')
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .split('}')
      .map((chunk) => chunk.slice(0, chunk.indexOf('{')).trim())
      // An `@media` wrapper is a condition, not a selector; the rules it holds
      // are checked on their own like any other.
      .filter((selector) => selector && !selector.startsWith('@'));

    expect(rules.length).toBeGreaterThan(10);
    for (const selector of rules) {
      for (const part of selector.split(',')) {
        expect(part.trim(), `${part.trim()} would leak to every page`).toMatch(
          new RegExp(`^\\${ancestor}`)
        );
      }
    }
  });

  /**
   * `:deep()` is scoped-block syntax. Carried into a plain sheet it is simply
   * an invalid selector, and the rule silently does nothing — which looks
   * exactly like the bug this file exists to catch.
   */
  it('carries no scoped-block syntax into plain CSS', () => {
    const css = readFileSync(sheet, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
    expect(css).not.toMatch(/:deep\(/);
  });

  /**
   * And the view must not keep a scoped copy of what it handed over: two
   * definitions of one class, the reachable one gone, is how this broke.
   */
  it('leaves no scoped copy behind in the view', () => {
    const source = readFileSync(view, 'utf8');
    const scoped = definedIn(source.slice(source.indexOf('<style scoped>')));

    // Vuetify's own classes are named by both files and owned by neither —
    // `:deep(.v-list-item)` on one side, a descendant selector on the other.
    const overlap = [...scoped].filter(
      (c) => shared.has(c) && !chrome.test(c) && !c.startsWith('v-')
    );
    expect(overlap, 'the view still defines classes the shared sheet owns').toEqual([]);
  });
});

/**
 * Every pair of stacked cards is spaced, not just the pairs that happen to be
 * the same kind of card.
 *
 * The rule was `.pane + .pane`, and `RequirementsPane` is the one pane on the
 * configuration tab that is not a `.pane` — it renders a `SettingsGroup`, whose
 * card is `.group`. Sitting between the configuration card and the manifest
 * card, it broke the adjacency on both sides at once, and the tab shipped as
 * three bordered boxes stacked edge to edge.
 *
 * Read from the source for the reason the rest of this file is: jsdom applies
 * no stylesheet, so a mount assertion cannot see a margin either way.
 */
describe('cards stacked on a project-detail tab', () => {
  const css = readFileSync('src/styles/project-panes.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');

  /** The selectors of every rule that sets a top margin between siblings. */
  const spaced = css
    .split('}')
    .filter((chunk) => /margin-top/.test(chunk))
    .flatMap((chunk) => chunk.slice(0, chunk.indexOf('{')).split(','))
    .map((selector) => selector.trim());

  const CARDS = ['pane', 'group'];

  it.each(CARDS.flatMap((first) => CARDS.map((second) => [first, second])))(
    'a .%s followed by a .%s',
    (first, second) => {
      const wanted = new RegExp(`\\.${first}\\s*\\+\\s*\\.${second}$`);
      expect(
        spaced.some((selector) => wanted.test(selector)),
        `nothing spaces a .${first} from the .${second} under it`
      ).toBe(true);
    }
  );

  /**
   * And the pane that made the exception is still the exception — if it ever
   * becomes a `.pane` like its neighbours, this test is the thing that says the
   * `.group` half of the rule has stopped earning its place.
   */
  it('is the shape RequirementsPane actually renders', () => {
    const source = readFileSync('src/components/project/RequirementsPane.vue', 'utf8');
    expect(source).toContain('<SettingsGroup');
  });
});
