import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import tr from '@/i18n/locales/tr.js';
import en from '@/i18n/locales/en.js';

/**
 * These started as one-off scripts run by hand while porting the views, and
 * each of them caught a real bug: a `_title` key left behind by a sed edit that
 * broke `projects.title`, and an `openInEditor` string that landed in the
 * `dashboard` block because the anchor text appeared twice. A check that only
 * runs when someone remembers to run it will eventually not be run.
 */

const SRC = resolve(import.meta.dirname, '../src');

function flatten(object, prefix = '') {
  return Object.entries(object).flatMap(([key, value]) =>
    value !== null && typeof value === 'object'
      ? flatten(value, `${prefix}${key}.`)
      : [`${prefix}${key}`]
  );
}

function sourceFiles(dir = SRC) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      // The locale files define the keys; they are not usage of them.
      return entry === 'locales' ? [] : sourceFiles(path);
    }
    return /\.(vue|js)$/.test(entry) ? [path] : [];
  });
}

const sources = sourceFiles().map((path) => readFileSync(path, 'utf8'));
const allSource = sources.join('\n');

/**
 * Every `t('some.key')` in the app, including the `$t` template form.
 *
 * The lookbehind matters: without it `emit('close')` matches, because `emit(`
 * ends in `t(`, and the check then demands translations for event names.
 */
const usedKeys = new Set(
  [...allSource.matchAll(/(?<![\w$.])\$?t\(\s*['"`]([a-zA-Z0-9_.]+)['"`]/g)].map((m) => m[1])
);

describe('translations', () => {
  it('define the same keys in every locale', () => {
    const trKeys = flatten(tr);
    const enKeys = flatten(en);

    expect([...trKeys].filter((k) => !enKeys.includes(k))).toEqual([]);
    expect([...enKeys].filter((k) => !trKeys.includes(k))).toEqual([]);
  });

  it('cover every key the app actually asks for', () => {
    const defined = new Set(flatten(en));

    // Keys assembled at runtime are excluded by construction: the regex only
    // matches literals, so a computed key never appears in `usedKeys`.
    const missing = [...usedKeys].filter((key) => !defined.has(key));

    expect(missing, `these keys would render as their own name`).toEqual([]);
  });

  it('resolve to a non-empty string in both locales', () => {
    const empty = [];
    for (const [name, locale] of [
      ['tr', tr],
      ['en', en],
    ]) {
      for (const key of flatten(locale)) {
        const value = key.split('.').reduce((o, part) => o?.[part], locale);
        if (typeof value !== 'string' || value.trim() === '') {
          empty.push(`${name}:${key}`);
        }
      }
    }
    expect(empty).toEqual([]);
  });
});

/**
 * The error codes are the contract's machine-readable half; the locales are how
 * they reach a person. A code Rust can emit but no locale names renders as the
 * raw English message from Rust, in an app that otherwise speaks two languages.
 */
describe('error codes', () => {
  // `errors` in the contract is { shape, codes, policy }; the codes are the
  // map underneath.
  const declared = Object.keys(
    JSON.parse(readFileSync(resolve(import.meta.dirname, '../contracts/ipc.json'), 'utf8')).errors
      .codes
  );

  it('are all translated', () => {
    expect(declared.length).toBeGreaterThan(0);
    for (const locale of [
      ['tr', tr],
      ['en', en],
    ]) {
      const [name, messages] = locale;
      const missing = declared.filter((code) => !messages.errors?.[code]);
      expect(missing, `${name} is missing error copy`).toEqual([]);
    }
  });

  it('carry no translation for a code the contract does not declare', () => {
    // UNKNOWN is the front end's own fallback for a panic or a missing command,
    // which never crosses the boundary as a typed error.
    const extra = Object.keys(en.errors).filter(
      (code) => code !== 'UNKNOWN' && !declared.includes(code)
    );
    expect(extra, 'dead error copy').toEqual([]);
  });
});

/**
 * Dead copy accumulates silently: a view is rewritten, its strings stay. The
 * first measurement here said 83 keys were unused and was wrong — it counted
 * `$vuetify.*`, which Vuetify consumes itself, and every key reached through a
 * template literal (`t(`errors.${code}`)`) or held as a string in an array
 * (`{ label: 'nav.projects' }`). The real number was 48. Detection has to model
 * all three ways a key is reached, or it reports confident nonsense.
 */
describe('unused translations', () => {
  it('are not defined at all', () => {
    const defined = flatten(en);

    // Reached through a template literal: the whole prefix stays live.
    const prefixes = [
      ...allSource.matchAll(/(?<![\w$.])\$?t\(\s*`([a-zA-Z0-9_.]+)\.\$\{/g),
    ].map((m) => m[1]);

    // Held as a plain string and passed to t() later.
    const indirect = new Set(
      [...allSource.matchAll(/['"]([a-zA-Z0-9_]+(?:\.[a-zA-Z0-9_]+)+)['"]/g)].map((m) => m[1])
    );

    const dead = defined.filter((key) => {
      // Vuetify's own component strings; it looks them up internally.
      if (key.startsWith('$vuetify.')) return false;
      if (usedKeys.has(key) || indirect.has(key)) return false;
      return !prefixes.some((prefix) => key.startsWith(prefix + '.'));
    });

    expect(dead, 'translated but unreachable').toEqual([]);
  });
});
