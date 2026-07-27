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
