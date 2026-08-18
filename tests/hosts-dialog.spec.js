import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';

/**
 * The review dialog in front of the app's only elevation prompt.
 *
 * Everything it shows comes from one `hosts_plan` call: the path in the
 * subtitle, the diff, and — through `plan.changed` — whether Apply can be
 * pressed at all. So a dialog that never made that call is not a dialog missing
 * a detail; it is a shield icon, two paragraphs, and a disabled button, with no
 * error on screen and nothing to say why.
 *
 * That is what a `v-if` produced. The watcher on `modelValue` was not
 * `immediate`, so a component created with the flag already true had nothing to
 * fire on — which is exactly how the two callers that mount it on demand use
 * it, and the three that keep it mounted are why it went unnoticed.
 */

const replies = {};
const calls = [];

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get:
        (_t, name) =>
        (...args) => {
          calls.push([String(name), ...args]);
          const reply = replies[name];
          if (reply instanceof Error) return Promise.reject(reply);
          return typeof reply === 'function' ? reply(...args) : Promise.resolve(reply);
        },
    }
  ),
}));

const HostsDialog = (await import('@/components/HostsDialog.vue')).default;

const vuetify = createVuetify({ components, directives });
const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } });

const PLAN = {
  changed: true,
  add: ['shop.loc'],
  remove: [],
  current: '127.0.0.1 localhost\n',
  preview: '127.0.0.1 localhost\n# >>> stackvo >>>\n127.0.0.1 shop.loc\n# <<< stackvo <<<\n',
  path: '/etc/hosts',
};

function render(props) {
  return mount(HostsDialog, {
    props: { modelValue: true, add: ['shop.loc'], ...props },
    global: { plugins: [vuetify, i18n] },
    attachTo: document.body,
  });
}

/**
 * The Apply button, found in the document rather than the wrapper: a `v-dialog`
 * teleports its card out of the component's own tree.
 */
function applyButton() {
  const buttons = [...document.body.querySelectorAll('.v-card-actions button')];
  return buttons[buttons.length - 1];
}

let wrapper;

afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.hostsPlan = { ...PLAN };
  replies.hostsApply = { ...PLAN };
});

describe('the hosts review dialog', () => {
  it('plans as soon as it is mounted already open', async () => {
    wrapper = render();
    await flushPromises();

    expect(calls).toEqual([['hostsPlan', ['shop.loc'], []]]);
    expect(document.body.textContent).toContain('/etc/hosts');
    expect(document.body.textContent).toContain('127.0.0.1 shop.loc');
    expect(applyButton().disabled).toBe(false);
  });

  it('plans when a mounted dialog is opened later', async () => {
    // The other half of the callers: kept in the DOM, flag flipped.
    wrapper = render({ modelValue: false });
    await flushPromises();
    expect(calls).toEqual([]);

    await wrapper.setProps({ modelValue: true });
    await flushPromises();
    expect(calls).toEqual([['hostsPlan', ['shop.loc'], []]]);
  });

  it('re-plans when the domain it was opened for changes', async () => {
    wrapper = render();
    await flushPromises();

    await wrapper.setProps({ add: ['api.loc'] });
    await flushPromises();
    expect(calls).toEqual([
      ['hostsPlan', ['shop.loc'], []],
      ['hostsPlan', ['api.loc'], []],
    ]);
  });

  it('does not re-plan when the parent merely re-renders', async () => {
    // `:add="[hostsFixFor]"` is a fresh array every render; an identity watch
    // would reload the plan for ever.
    wrapper = render();
    await flushPromises();

    await wrapper.setProps({ add: ['shop.loc'] });
    await flushPromises();
    expect(calls).toHaveLength(1);
  });

  it('keeps Apply disabled when the lines are already there', async () => {
    replies.hostsPlan = { ...PLAN, changed: false };
    wrapper = render();
    await flushPromises();

    expect(applyButton().disabled).toBe(true);
    expect(document.body.textContent).toContain(en.hosts.noChange);
  });

  it('writes only what was previewed, and says so upward', async () => {
    wrapper = render();
    await flushPromises();
    applyButton().click();
    await flushPromises();

    expect(calls).toContainEqual(['hostsApply', ['shop.loc'], []]);
    expect(wrapper.emitted('applied')).toBeTruthy();
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([false]);
  });

  it('shows a refused elevation rather than closing on it', async () => {
    replies.hostsApply = Object.assign(new Error('User cancelled'), {
      code: 'Elevation',
      message: 'User cancelled',
    });
    wrapper = render();
    await flushPromises();
    applyButton().click();
    await flushPromises();

    expect(wrapper.emitted('applied')).toBeFalsy();
    expect(document.body.textContent).toContain('User cancelled');
  });
});
