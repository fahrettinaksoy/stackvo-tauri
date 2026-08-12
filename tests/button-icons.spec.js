import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Does every button that says it has an icon actually draw one?
 *
 * Vuetify's `v-btn` reads `icon="mdi-…"` **only while its default slot is
 * empty**. Put anything in the slot — most often a `v-tooltip` bound to the
 * parent, which is how every tooltipped control in this app is written — and
 * the prop is ignored and the button renders blank.
 *
 * It renders blank *and nothing complains*: the prop is valid, the component
 * mounts, the `aria-label` is right, and the mount tests that assert on text
 * and roles all pass. The rebuild button on the project page shipped this way
 * and was caught by somebody looking at the screen, which is the one reviewer
 * this project cannot schedule.
 *
 * So the guard reads the sources, as `pane-styles.spec.js` does and for the
 * same reason: jsdom will happily render an empty button forever.
 *
 * The rule is not "never use the prop". A button with no slot content is the
 * case the prop exists for and is the common one. The rule is that the two
 * cannot both be true, and this says which files break it.
 */

/** `icon="…"`, and not `prepend-icon` / `append-icon` / `:icon`. */
const ICON_PROP = /(?<![\w:-])icon\s*=\s*"[^"]+"/;

const OPEN = /<v-btn(\s[^>]*?)?(\/?)>/s;
const OPEN_OR_CLOSE = /<v-btn(\s[^>]*?)?(\/?)>|<\/v-btn>/s;

/**
 * Every `v-btn` in a file, as `(line, attrs, slot)`.
 *
 * Nesting-aware, because a menu activator is a button inside a button and a
 * naive non-greedy match reads the outer one's slot as ending at the inner
 * one's close tag — which reports the wrong file and misses the real case.
 */
function buttons(text) {
  const out = [];
  let i = 0;
  for (;;) {
    const open = OPEN.exec(text.slice(i));
    if (!open) return out;
    const start = i + open.index;
    const attrs = open[1] ?? '';
    if (open[2] === '/') {
      i = start + open[0].length;
      out.push({ line: lineOf(text, start), attrs, slot: '' });
      continue;
    }
    let depth = 1;
    let j = start + open[0].length;
    const bodyFrom = j;
    while (depth > 0) {
      const next = OPEN_OR_CLOSE.exec(text.slice(j));
      if (!next) return out;
      const at = j + next.index;
      if (next[0] === '</v-btn>') depth -= 1;
      else if (next[2] !== '/') depth += 1;
      j = at + next[0].length;
    }
    out.push({ line: lineOf(text, start), attrs, slot: text.slice(bodyFrom, j - '</v-btn>'.length) });
    i = start + open[0].length;
  }
}

const lineOf = (text, at) => text.slice(0, at).split('\n').length;

/** Comments are not slot content — an empty button may still be commented. */
const meaningful = (slot) => slot.replace(/<!--[\s\S]*?-->/g, '').trim();

function vueFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...vueFiles(path));
    else if (entry.endsWith('.vue')) out.push(path);
  }
  return out;
}

describe('every v-btn with an icon prop', () => {
  it('leaves its default slot empty, or the icon is never drawn', () => {
    const offenders = [];

    for (const file of vueFiles('src')) {
      const text = readFileSync(file, 'utf8');
      for (const { line, attrs, slot } of buttons(text)) {
        if (!ICON_PROP.test(attrs)) continue;
        if (!meaningful(slot)) continue;
        offenders.push(`${file}:${line} — ${ICON_PROP.exec(attrs)[0]} with a non-empty slot`);
      }
    }

    expect(
      offenders,
      'Vuetify ignores `icon="mdi-…"` when the default slot has content, so these buttons ' +
        'render blank. Write `icon` as a flag and put `<v-icon>mdi-…</v-icon>` in the slot ' +
        'beside whatever else is there.'
    ).toEqual([]);
  });

  /**
   * The scanner has to be able to fail, or it is a test that passes because it
   * found nothing to look at. Both shapes, against text rather than the tree.
   */
  it('finds the shape it is looking for, and only that shape', () => {
    const bad = `<v-btn icon="mdi-x"><v-tooltip>hi</v-tooltip></v-btn>`;
    const good = `<v-btn icon="mdi-x" />`;
    const alsoGood = `<v-btn icon><v-icon>mdi-x</v-icon><v-tooltip>hi</v-tooltip></v-btn>`;
    // `prepend-icon` sits beside slot content by design and must not be caught.
    const notThisOne = `<v-btn prepend-icon="mdi-x">Label</v-btn>`;

    const caught = (source) =>
      buttons(source).some((b) => ICON_PROP.test(b.attrs) && meaningful(b.slot));

    expect(caught(bad)).toBe(true);
    expect(caught(good)).toBe(false);
    expect(caught(alsoGood)).toBe(false);
    expect(caught(notThisOne)).toBe(false);
  });
});
