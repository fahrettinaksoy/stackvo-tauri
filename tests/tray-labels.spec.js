import { describe, it, expect } from 'vitest';
import { createI18n } from 'vue-i18n';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { trayLabels } from '@/lib/trayLabels.js';
import en from '@/i18n/locales/en.js';
import tr from '@/i18n/locales/tr.js';

/**
 * The tray's catalog crosses a language boundary, so nothing else can check it.
 *
 * `tray.rs` names the keys it needs in `LABEL_KEYS`; `trayLabels()` builds them
 * in JavaScript. Neither side can see the other, and the failure is silent in a
 * specific way: Rust refuses an incomplete catalog and falls back to its
 * built-in table, so a forgotten key does not crash anything — the tray simply
 * keeps speaking English, which is exactly the bug this work removed.
 *
 * So the Rust source is read here and the two lists compared. A key added to
 * `LABEL_KEYS` without a line in `trayLabels` fails this, and so does the
 * reverse.
 */

const TRAY_RS = resolve(import.meta.dirname, '../src-tauri/src/tray.rs');

/** The `LABEL_KEYS` slice, as the strings it lists. */
function rustLabelKeys() {
  const source = readFileSync(TRAY_RS, 'utf8');
  const start = source.indexOf('pub const LABEL_KEYS');
  expect(start, 'tray.rs still declares LABEL_KEYS').toBeGreaterThan(-1);

  const body = source.slice(start, source.indexOf('];', start));
  // Quoted entries only: the slice carries a comment explaining the menu-bar
  // block, and a naive split would take words out of it.
  return [...body.matchAll(/"([a-zA-Z]+)"/g)].map((m) => m[1]);
}

/** Resolve a dotted key the way vue-i18n would, against one catalog. */
function lookup(catalogue, path) {
  return path.split('.').reduce((node, key) => (node == null ? node : node[key]), catalogue);
}

describe('the tray label catalog', () => {
  it('sends exactly the keys tray.rs asks for', () => {
    const wanted = rustLabelKeys();
    const sent = Object.keys(trayLabels((key) => key));

    expect(wanted.length, 'LABEL_KEYS was parsed out of the Rust source').toBeGreaterThan(15);
    expect([...sent].sort()).toEqual([...wanted].sort());
  });

  it('resolves every one of them in both locales', () => {
    // `trayLabels` is handed the *paths* here rather than a translator, so the
    // assertion is about which catalogue entries it reaches — including the
    // shared ones under `nav`, `system` and `about.links`, which is where a
    // rename in one of those blocks would otherwise silently un-translate the
    // tray.
    const paths = Object.values(trayLabels((key) => key));

    for (const [name, catalogue] of [
      ['en', en],
      ['tr', tr],
    ]) {
      const missing = paths.filter((path) => typeof lookup(catalogue, path) !== 'string');
      expect(missing, `${name} defines every path the tray asks for`).toEqual([]);
    }
  });

  it('keeps the placeholders the counted labels are filled through', () => {
    // Rust substitutes these by name. A translation that drops one renders a
    // label with no number in it, which reads as a bug in the count rather
    // than in the string.
    const counted = {
      'tray.containers': ['{count}'],
      'tray.more': ['{count}'],
      'tray.runningSummary': ['{running}', '{total}'],
    };

    for (const [name, catalogue] of [
      ['en', en],
      ['tr', tr],
    ]) {
      for (const [path, placeholders] of Object.entries(counted)) {
        const text = lookup(catalogue, path);
        for (const placeholder of placeholders) {
          expect(text, `${name} ${path} keeps ${placeholder}`).toContain(placeholder);
        }
      }
    }
  });

  /**
   * And the placeholders survive the **translator**, not just the file.
   *
   * This is the test the two above look like they already were, and were not.
   * Both hand `trayLabels` a `t` of `(key) => key`, which returns the path and
   * interpolates nothing — so the catalogue was checked, the Rust boundary was
   * checked, and the function in between was never run with the thing it is
   * given in production.
   *
   * What it does is not neutral. vue-i18n substitutes a named placeholder with
   * **no matching parameter** for the empty string rather than leaving it
   * alone, so every label built to reach Rust with a hole in it arrived with
   * the hole already closed:
   *
   * ```text
   * '{name}: durdur'         → ': durdur'
   * 'Konteynerler: {count}'  → 'Konteynerler: '
   * ```
   *
   * The tray drew a Start/stop submenu of rows that said `: başlat` and
   * `: durdur`, five of them, in a menu whose only job is to say which project.
   * A real `createI18n` is the cheapest thing that would have said so.
   */
  it('keeps them through a real translator, in both locales', () => {
    for (const locale of ['en', 'tr']) {
      const i18n = createI18n({
        legacy: false,
        locale,
        fallbackLocale: 'en',
        messages: { en, tr },
      });
      const labels = trayLabels(i18n.global.t);

      const expected = {
        containers: ['{count}'],
        more: ['{count}'],
        runningSummary: ['{running}', '{total}'],
      };

      for (const [key, placeholders] of Object.entries(expected)) {
        for (const placeholder of placeholders) {
          expect(labels[key], `${locale} ${key} reaches Rust with ${placeholder}`).toContain(
            placeholder
          );
        }
      }

      // And the words around them are the locale's, rather than the key or the
      // English fallback — an identity substitution that stopped translating
      // would satisfy every assertion above.
      expect(labels.containers).not.toBe('tray.containers');
      expect(labels.containers).toBe(lookup(locale === 'tr' ? tr : en, 'tray.containers'));

      // The verbs a project's submenu offers carry no placeholder at all any
      // more — the project is the row they hang under — so what matters about
      // them is that they arrive as words rather than as key paths.
      for (const key of ['openProject', 'startProject', 'stopProject']) {
        expect(labels[key], `${locale} ${key} is translated`).not.toContain('.');
        expect(labels[key]).not.toContain('{');
      }
    }
  });
});
