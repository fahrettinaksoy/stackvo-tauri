import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve, relative } from 'node:path';

/**
 * This app is icon-heavy: 35 of its buttons carry no text at all. A screen
 * reader announces an unlabelled icon button as "button", so a toolbar of nine
 * of them is nine identical announcements.
 *
 * A `v-tooltip` is not enough. Vuetify renders it as `aria-describedby`, which
 * is a description attached to a control that still has no *name* — and it only
 * appears on hover, which a keyboard user never triggers. Both are useful; only
 * one of them names the control.
 *
 * The first measurement of this said 11 buttons, by grepping for `<v-btn` and
 * `icon` on the same line. Most of them span several lines. The real number was
 * 35, of which 26 were unnamed.
 */

const SRC = resolve(import.meta.dirname, '../src');

function vueFiles(dir = SRC) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return vueFiles(path);
    return path.endsWith('.vue') ? [path] : [];
  });
}

describe('icon-only buttons', () => {
  it('all carry an accessible name', () => {
    const unnamed = [];
    let total = 0;

    for (const file of vueFiles()) {
      const text = readFileSync(file, 'utf8');

      for (const match of text.matchAll(/<v-btn\b([\s\S]*?)>/g)) {
        const attrs = match[1];
        // `icon`, `icon="mdi-x"` or `:icon="expr"` — a button with no text.
        const isIconOnly = /(^|\s):?icon[=\s>]/.test(attrs) || /\sicon\s*$/.test(attrs);
        if (!isIconOnly) continue;

        total += 1;
        // `title` maps to the native tooltip and is also exposed as a name when
        // nothing else provides one, so either attribute counts.
        if (/aria-label|:?title=/.test(attrs)) continue;

        const line = text.slice(0, match.index).split('\n').length;
        unnamed.push(`${relative(SRC, file)}:${line}`);
      }
    }

    expect(total).toBeGreaterThan(20);
    expect(unnamed, 'icon buttons a screen reader would announce as "button"').toEqual([]);
  });
});

/**
 * An anchor that navigates nowhere.
 *
 * `<a class="…-link" @click="…">` renders as a link, reads as a link, and is
 * neither: with no `href` it takes no focus, is skipped by every keyboard, and
 * a screen reader announces it as plain text. Six of them shipped — the domain
 * in every row of the projects table among them — so the whole table was
 * mouse-only, and nothing in this repository could see it. jsdom has no focus
 * model and a mount test asserting on the text passes either way.
 *
 * The browser suite found it (`tests/e2e/shell.e2e.js`, which asks for a
 * `link` role and got nothing). This is the guard that stops it coming back,
 * and it is a source read for the reason the other two are: the rule is about
 * what was written, and reading it needs no engine.
 *
 * `<a href="…">` is untouched — that is a real link and belongs in the markup.
 */
describe('anchors', () => {
  it('never carry a click handler instead of an href', () => {
    const offenders = [];

    for (const file of vueFiles()) {
      const text = readFileSync(file, 'utf8');

      // Each opening `<a …>` with its attributes. Non-greedy to the first `>`,
      // which is enough: none of these carry a `>` inside an attribute value.
      for (const match of text.matchAll(/<a(\s[^>]*)>/g)) {
        const attrs = match[1];
        if (/\bhref\b/.test(attrs)) continue;
        if (!/@click|v-on:click/.test(attrs)) continue;

        const line = text.slice(0, match.index).split('\n').length;
        offenders.push(`${relative(SRC, file)}:${line}`);
      }
    }

    expect(
      offenders,
      'anchors with a click handler and no href — use <button type="button">, which takes focus'
    ).toEqual([]);
  });
});
