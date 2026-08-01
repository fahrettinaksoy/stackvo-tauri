import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import ErrorAlert from '@/components/ErrorAlert.vue';
import en from '@/i18n/locales/en.js';

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
