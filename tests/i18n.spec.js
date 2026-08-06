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

/** The value at a dotted path, for the compilation check below. */
function resolve_(object, path) {
  return path.split('.').reduce((node, key) => (node == null ? node : node[key]), object);
}

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
 * Every `t('some.key')` in the app, including the `$t` template form and the
 * `tc()` wrapper that LogView uses to render the console in its own language.
 *
 * The lookbehind matters: without it `emit('close')` matches, because `emit(`
 * ends in `t(`, and the check then demands translations for event names. It
 * still does its job with `tc` allowed — the character before the `t` is what
 * is tested, and in `emit(` that is `i`.
 */
const usedKeys = new Set(
  [...allSource.matchAll(/(?<![\w$.])\$?tc?\(\s*['"`]([a-zA-Z0-9_.]+)['"`]/g)].map((m) => m[1])
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
 * Vuetify's own labels, which this app made itself responsible for.
 *
 * `createVueI18nAdapter` routes every internal Vuetify string — a snackbar's
 * close button, an empty table's caption, a data table's pager — through this
 * i18n instance rather than Vuetify's own store. vue-i18n returns an unknown
 * key verbatim, so the moment the adapter went in and `$vuetify` was not
 * merged, those labels started rendering as their own names: a snackbar with
 * **$VUETIFY.DISMISS** where its close button should be.
 *
 * Checked against the real instance rather than the locale files, because the
 * merge happens in `i18n/index.js` and that is the thing that can be undone.
 */
describe('vuetify labels', () => {
  it('resolve through the app i18n instance in both locales', async () => {
    const { i18n } = await import('@/i18n');
    const { t } = i18n.global;

    // One per component family that leaked, not an exhaustive list — the point
    // is that `$vuetify` is present and reachable, not to restate Vuetify.
    const keys = ['$vuetify.dismiss', '$vuetify.close', '$vuetify.noDataText'];

    for (const locale of ['tr', 'en']) {
      i18n.global.locale.value = locale;
      for (const key of keys) {
        expect(t(key), `${locale}:${key} rendered as its own name`).not.toContain('$vuetify');
      }
    }
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

    // Reached through a template literal: the whole prefix stays live. `tc` is
    // included for the same reason as above, and `te` because asking whether a
    // translation exists is how a namespace with a fallback is read — the
    // fields under `serviceSettings.fields` are looked up that way, and
    // without it every one of them reads as dead. A key reached as
    // ``tc(`logs.level.${level}`)`` is reached, and the literal form of that
    // same call was already being counted through `indirect` below, so leaving
    // the template form out reported live keys as dead.
    const prefixes = [
      ...allSource.matchAll(/(?<![\w$.])\$?t[ce]?\(\s*`([a-zA-Z0-9_.]+)\.\$\{/g),
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

/**
 * Every message actually compiles.
 *
 * vue-i18n reads `{…}` as an interpolation placeholder, so a string containing
 * literal braces — `{{ VAR }} is substituted from .env` — is a *nested*
 * placeholder, which is not allowed. It does not fail loudly: the compiler logs
 * "Not allowed nest placeholder" and falls back to the raw string, so the text
 * looks right and every render writes an error to the console. Noise like that
 * is what a real error hides in.
 *
 * It shipped that way and was only noticed because the pane it lives in was
 * extracted and mounted (§14.16). The escape is vue-i18n's own literal syntax,
 * `{'{{ VAR }}'}`, and this is what stops the next one going unnoticed.
 */
describe('message compilation', () => {
  it('produces no compiler diagnostics in either locale', async () => {
    const { createI18n } = await import('vue-i18n');
    const complaints = [];
    const original = console.error;
    console.error = (...args) => complaints.push(args.join(' '));

    try {
      for (const [name, messages] of [
        ['en', en],
        ['tr', tr],
      ]) {
        const i18n = createI18n({ legacy: false, locale: name, messages: { [name]: messages } });
        // Compilation is lazy — a message is only parsed when it is asked
        // for, so every one has to be resolved for this to mean anything.
        //
        // Strings only. `flatten` walks arrays too, which turns an index into
        // a key like `nav.items.0` and makes vue-i18n warn about a key that
        // was never meant to be one — noise this assertion would then read as
        // a finding.
        for (const key of flatten(messages)) {
          if (typeof resolve_(messages, key) === 'string') i18n.global.t(key);
        }
      }
    } finally {
      console.error = original;
    }

    expect(complaints, `vue-i18n rejected ${complaints.length} message(s)`).toEqual([]);
  });
});
