import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import ErrorAlert from '@/components/ErrorAlert.vue';
import en from '@/i18n/locales/en.js';
import tr from '@/i18n/locales/tr.js';

const vuetify = createVuetify({ components, directives });
const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } });

function render(error) {
  return mount(ErrorAlert, {
    props: { error },
    global: { plugins: [vuetify, i18n] },
  });
}

/**
 * The alert used to read `error.message` and nothing else. That is right for
 * this app's own errors and wrong for everything else that reaches it — a
 * Tauri plugin rejects with a plain string — and the result was a red box with
 * nothing in it, which says something failed and refuses to say what.
 */
describe('ErrorAlert', () => {
  it('shows a StackVo error message', () => {
    expect(render({ code: 'NotFound', message: 'shop is not a directory' }).text()).toContain(
      'shop is not a directory'
    );
  });

  it('shows a plain string, which is what a plugin rejects with', () => {
    expect(render('opener.open_path not allowed').text()).toContain('not allowed');
  });

  it('shows something for an object with no message at all', () => {
    const text = render({ reason: 'forbidden' }).text();
    expect(text).toContain('forbidden');
    expect(text).not.toContain('[object Object]');
  });

  it('is never a box with nothing in it', () => {
    for (const error of ['boom', { message: 'boom' }, { reason: 'boom' }]) {
      expect(render(error).text().trim()).not.toBe('');
    }
    // Except when there is no error at all, where it renders nothing.
    expect(render(null).text().trim()).toBe('');
  });
});

/**
 * The hint, which is the line a user acts on.
 *
 * It was printed raw, so a Turkish user got a translated heading over an
 * English explanation over an English instruction. The Rust side now sends a
 * `hintKey` from the catalogue in `src-tauri/src/hints.rs` alongside the
 * English, and `hint_translations.rs` guarantees the key exists in both
 * locales. What is left to check here is the rendering rule — including the two
 * fallbacks, which are the cases where getting it wrong shows a user a key
 * instead of a sentence.
 */
describe('the hint', () => {
  function alert(error, locale = 'en') {
    const scoped = createI18n({ legacy: false, locale, messages: { en, tr } });
    return mount(ErrorAlert, { props: { error }, global: { plugins: [vuetify, scoped] } });
  }

  it('is translated when the error carries a key', () => {
    const text = alert(
      {
        code: 'EngineUnreachable',
        message: 'no socket',
        hintKey: 'startDocker',
        hint: 'Start Docker Desktop and try again.',
      },
      'tr'
    ).text();

    expect(text).toContain(tr.errorHints.startDocker);
    expect(text).not.toContain('Start Docker Desktop');
  });

  it('shows the English when there is no key — the runtime-built hints', () => {
    const text = alert(
      { code: 'GenerateFailed', message: 'could not run git', hint: '`git` is not on PATH.' },
      'tr'
    ).text();

    expect(text).toContain('`git` is not on PATH.');
  });

  /**
   * A key the locale does not know must fall through to the English sentence.
   * Rendering `errorHints.somethingNew` at a user is worse than English.
   */
  it('falls back to the English when the key is unknown', () => {
    const text = alert(
      {
        code: 'IoError',
        message: 'boom',
        hintKey: 'notAKeyAnyoneShipped',
        hint: 'Try turning it off and on again.',
      },
      'tr'
    ).text();

    expect(text).toContain('Try turning it off and on again.');
    expect(text).not.toContain('notAKeyAnyoneShipped');
  });

  it('shows nothing at all when there is no hint', () => {
    const text = alert({ code: 'NotFound', message: 'shop not found' }).text();
    expect(text).toContain('shop not found');
    expect(text.trim().endsWith('shop not found')).toBe(true);
  });
});
