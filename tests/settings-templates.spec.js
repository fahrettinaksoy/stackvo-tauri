import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The template-override pane, mounted for real.
 *
 * This replaces `tests/template-overrides.spec.js`, the second and last shape
 * mirror. That file rebuilt the button and its two refs inside the test, then
 * read `Settings.vue` as text and asserted the copy still matched — including
 * an assertion on the exact source line `const busyWith = (path) => !!path &&
 * templateBusy.value === path`. It worked, and it was pinned to a string: the
 * guard could be moved, renamed or wrapped and the test would fail for a reason
 * that had nothing to do with the button.
 *
 * The bug it existed for is real and is asserted below against the shipped
 * component. The button shipped **spinning**: the binding was
 * `templateBusy === templateToOverride`, which reads correctly and is wrong for
 * exactly the state nobody thinks to check — both refs start null, null equals
 * null, and the button reported itself busy before anyone had chosen a file,
 * and again after every successful override, which clears the selection back to
 * null. Nothing else could catch it. The markup lints clean, the comparison is
 * valid JavaScript, and the two names in it are the right two names.
 */

globalThis.visualViewport = undefined;

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
          return typeof reply === 'function' ? reply(...args) : Promise.resolve(reply);
        },
    }
  ),
}));

const { i18n } = await import('@/i18n');
const TemplateOverridesPane = (await import('@/components/settings/TemplateOverridesPane.vue'))
  .default;

const vuetify = createVuetify({ components, directives });

const REDIS = 'services/redis/docker-compose.redis.tpl';
const NGINX = 'core/servers/nginx.conf';

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(TemplateOverridesPane, {
    attachTo: host,
    global: { plugins: [createPinia(), vuetify, i18n] },
  });

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

/** The "take over" button, found by its label rather than by a class. */
function overrideButton(wrapper) {
  return wrapper
    .findAll('button')
    .find((b) => b.text().includes(i18n.global.t('settings.templates.override')));
}

const spinning = (button) => button.classes().includes('v-btn--loading');

beforeEach(() => {
  setActivePinia(createPinia());
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.templatesList = [
    { path: REDIS, overridden: false },
    { path: NGINX, overridden: true },
  ];
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the template override button', () => {
  /** The state the pane opens in, and the state it shipped broken in. */
  it('is idle and disabled before a template is chosen', async () => {
    const wrapper = await render();
    const button = overrideButton(wrapper);

    expect(button, 'no override button').toBeTruthy();
    expect(spinning(button), 'spinning with nothing selected').toBe(false);
    expect(button.attributes('disabled')).toBeDefined();

    wrapper.unmount();
  });

  it('stays idle once a template is chosen but no work has started', async () => {
    const wrapper = await render();
    wrapper.vm.chosen = REDIS;
    await wrapper.vm.$nextTick();

    const button = overrideButton(wrapper);
    expect(spinning(button), 'selecting a file is not doing work on it').toBe(false);
    expect(button.attributes('disabled'), 'a chosen file must be actionable').toBeUndefined();

    wrapper.unmount();
  });

  /**
   * Driven through the real flow rather than by poking the composable's
   * internals: the old mirror set two refs by hand, which is how it managed to
   * assert on a shape instead of on behaviour. Here the button is clicked, the
   * boundary is held open, and the spinner is read off the DOM.
   *
   * The return to idle is the half that matters. The shipped bug was not "it
   * never spins" — it was that finishing put both values back to null, and
   * `null === null` spun it again for ever.
   */
  it('spins while its own work runs and returns to idle after it', async () => {
    let release;
    replies.templateOverride = () =>
      new Promise((resolve) => {
        release = () => resolve('/ws/core/servers/redis.tpl');
      });
    replies.openInEditor = () => Promise.resolve();

    const wrapper = await render();
    wrapper.vm.chosen = REDIS;
    await wrapper.vm.$nextTick();
    expect(spinning(overrideButton(wrapper)), 'spinning before the click').toBe(false);

    await overrideButton(wrapper).trigger('click');
    await vi.waitFor(() =>
      expect(spinning(overrideButton(wrapper)), 'the button did not report its own work').toBe(true)
    );

    release();
    await vi.waitFor(() => expect(wrapper.vm.chosen).toBe(null));
    await wrapper.vm.$nextTick();

    expect(
      spinning(overrideButton(wrapper)),
      'back to both null, back to spinning — the bug this pane shipped with'
    ).toBe(false);

    wrapper.unmount();
  });
});

describe('the template list', () => {
  /**
   * Overridden files first and always visible: they are the answer to "why does
   * my stack not match the docs", and a forgotten edit is why that gets asked.
   */
  it('separates what has been taken over from what has not', async () => {
    const wrapper = await render();
    const text = wrapper.text();

    expect(text).toContain(NGINX);
    expect(text).toContain(i18n.global.t('settings.templates.count', { count: 1, total: 2 }));

    wrapper.unmount();
  });

  it('says so plainly when nothing has been taken over', async () => {
    replies.templatesList = [{ path: REDIS, overridden: false }];

    const wrapper = await render();
    expect(wrapper.text()).toContain(i18n.global.t('settings.templates.none', { total: 1 }));

    wrapper.unmount();
  });

  /**
   * The whole point of the pane is that this list is answerable. A boundary
   * that hands back something other than a list used to make the render throw —
   * see `asList` in `src/lib/ipc.js`.
   */
  it('renders an empty list rather than throwing when the boundary misbehaves', async () => {
    replies.templatesList = null;

    const wrapper = await render();
    expect(wrapper.text().trim().length).toBeGreaterThan(0);

    wrapper.unmount();
  });

  it('reports a failure instead of showing an empty list', async () => {
    replies.templatesList = () => Promise.reject(new Error('workspace is gone'));

    const wrapper = await render();
    expect(wrapper.text()).toContain('workspace is gone');

    wrapper.unmount();
  });
});

describe('taking a template over', () => {
  /**
   * Copy the file in, then open it in the user's own editor — these are compose
   * fragments and server configs, and the tool for editing YAML is the one they
   * already have open.
   */
  it('copies the file in and opens it, then clears the selection', async () => {
    replies.templateOverride = () => Promise.resolve('/ws/core/servers/redis.tpl');
    replies.openInEditor = () => Promise.resolve();

    const wrapper = await render();
    wrapper.vm.chosen = REDIS;
    await wrapper.vm.$nextTick();

    await overrideButton(wrapper).trigger('click');
    await vi.waitFor(() => expect(wrapper.vm.chosen).toBe(null));

    expect(calls.some(([name, arg]) => name === 'templateOverride' && arg === REDIS)).toBe(true);
    expect(
      calls.some(([name, arg]) => name === 'openInEditor' && arg === '/ws/core/servers/redis.tpl')
    ).toBe(true);
    // The list is re-read, or the file it just wrote is still listed as shipped.
    expect(calls.filter(([name]) => name === 'templatesList').length).toBeGreaterThan(1);

    wrapper.unmount();
  });

  /**
   * Reverting deletes the file the user edited and there is no copy of it
   * anywhere — the binary holds the shipped version, not theirs. So it is
   * confirmed in a dialog, and the dialog closing must not perform it.
   */
  it('does not revert until the dialog is confirmed', async () => {
    replies.templateRevert = () => Promise.resolve();

    const wrapper = await render();
    wrapper.vm.revertTarget = NGINX;
    await wrapper.vm.$nextTick();

    expect(document.body.textContent).toContain(i18n.global.t('settings.templates.revertTitle'));
    expect(calls.some(([name]) => name === 'templateRevert')).toBe(false);

    await wrapper.vm.revert();
    expect(calls.some(([name, arg]) => name === 'templateRevert' && arg === NGINX)).toBe(true);
    expect(wrapper.vm.revertTarget, 'the dialog stayed open over a finished action').toBe(null);

    wrapper.unmount();
  });
});
