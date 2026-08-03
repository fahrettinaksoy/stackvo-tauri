import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import ErrorAlert from '@/components/ErrorAlert.vue';
import { StackvoError } from '@/lib/ipc';

/**
 * The findings behind a rejected manifest, on screen.
 *
 * `parse_spec` attaches every one — code, path, and a sentence naming the field
 * — and nothing rendered them. A project refused over one unbuildable extension
 * out of thirty-two said only "the project definition is not valid", which does
 * not say which extension, or that the subject is an extension at all. The real
 * report was in `error.details.errors` the whole time.
 */

globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

const { i18n } = await import('@/i18n');
const vuetify = createVuetify({ components, directives });

const render = (error) =>
  mount(ErrorAlert, { props: { error }, global: { plugins: [vuetify, i18n] } });

describe('ErrorAlert', () => {
  it('names the field a rejected manifest was rejected over', () => {
    // The shape Rust sends, taken from a real refusal: a repository whose
    // committed stackvo.json lists `imap` while targeting PHP 8.4.
    const wrapper = render(
      new StackvoError({
        code: 'INVALID_MANIFEST',
        message: 'the project definition is not valid',
        details: {
          errors: [
            {
              code: 'C-06',
              path: 'php.extensions[imap]',
              message: '"imap" was removed in PHP 8.2 but this project targets 8.4',
            },
          ],
        },
      })
    );

    const text = wrapper.text();
    expect(text, 'the generic message is still shown').toContain(
      'the project definition is not valid'
    );
    expect(text, 'the offending field is not named').toContain('php.extensions[imap]');
    expect(text, 'the reason is not given').toContain('removed in PHP 8.2');
    expect(text).toContain('C-06');

    wrapper.unmount();
  });

  it('renders every finding, not only the first', () => {
    const wrapper = render(
      new StackvoError({
        code: 'INVALID_MANIFEST',
        message: 'the project definition is not valid',
        details: {
          errors: [
            { code: 'MISSING_NAME', path: 'name', message: '`name` is required' },
            { code: 'MISSING_DOMAIN', path: 'domain', message: '`domain` is required' },
          ],
        },
      })
    );

    expect(wrapper.findAll('li')).toHaveLength(2);
    wrapper.unmount();
  });

  it('adds nothing when an error carries no findings', () => {
    // Every other error in the app reaches this component too, and a stray
    // empty list under the message would read as a truncated report.
    for (const error of [
      new StackvoError({ code: 'ENGINE_UNREACHABLE', message: 'Docker is not running.' }),
      new StackvoError({ code: 'IO_ERROR', message: 'boom', details: { errors: 'not a list' } }),
      'a plugin rejected with a plain string',
    ]) {
      const wrapper = render(error);
      expect(wrapper.findAll('li'), String(error)).toHaveLength(0);
      wrapper.unmount();
    }
  });
});
