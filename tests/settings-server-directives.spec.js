import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The extra-directives editor, mounted.
 *
 * Third pane out of `Settings.vue` under §14.16, and the first with no shape
 * mirror behind it — which is the point: the two mirrors existed because
 * somebody had a bug they could not otherwise pin. This pane had neither a bug
 * nor a test, and its most breakable behaviour is invisible in review.
 *
 * That behaviour is the tab watcher. Switching from nginx to caddy has to
 * reload the file; a version that forgot would show nginx's directives under
 * caddy's name and then **save them there** — silently rewriting the wrong
 * server's config with the right one's contents. Nothing about the markup
 * suggests that can happen.
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
const ServerDirectivesPane = (await import('@/components/settings/ServerDirectivesPane.vue'))
  .default;

const vuetify = createVuetify({ components, directives });

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(ServerDirectivesPane, {
    attachTo: host,
    global: { plugins: [vuetify, i18n] },
  });

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const saveButton = (wrapper) =>
  wrapper
    .findAll('button')
    .find((b) => b.text().includes(i18n.global.t('settings.save', { count: 1 })));

const textarea = (wrapper) => wrapper.find('textarea');

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.serverConfigGet = (server) => Promise.resolve(`# ${server} directives\n`);
  replies.serverConfigSet = () => Promise.resolve();
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the server directives pane', () => {
  it('opens on the first configurable server and loads its file', async () => {
    const wrapper = await render();

    expect(calls[0]).toEqual(['serverConfigGet', 'nginx']);
    expect(textarea(wrapper).element.value).toBe('# nginx directives\n');

    // All three servers are offered, and only the three that have a generated
    // config to append to.
    const text = wrapper.text();
    for (const server of ['nginx', 'caddy', 'frankenphp']) {
      expect(text).toContain(server);
    }

    wrapper.unmount();
  });

  /**
   * The behaviour with no visible trace in the markup: the tab and the file
   * have to move together, or a save writes one server's directives into
   * another's config.
   */
  it('reloads the file when the server tab changes', async () => {
    const wrapper = await render();

    wrapper.vm.server = 'caddy';
    await vi.waitFor(() => expect(textarea(wrapper).element.value).toBe('# caddy directives\n'));
    expect(calls.some(([name, arg]) => name === 'serverConfigGet' && arg === 'caddy')).toBe(true);

    wrapper.unmount();
  });

  /**
   * Save is a comparison against what is on disk, not a flag somebody has to
   * remember to clear. Untouched means nothing to save.
   */
  it('cannot be saved until the text differs from what was loaded', async () => {
    const wrapper = await render();
    expect(saveButton(wrapper).attributes('disabled'), 'saveable while untouched').toBeDefined();

    await textarea(wrapper).setValue('# nginx directives\nclient_max_body_size 64m;\n');
    await wrapper.vm.$nextTick();
    expect(saveButton(wrapper).attributes('disabled')).toBeUndefined();

    wrapper.unmount();
  });

  it('saves against the server whose tab is open, and goes clean again', async () => {
    const wrapper = await render();

    wrapper.vm.server = 'frankenphp';
    await vi.waitFor(() =>
      expect(textarea(wrapper).element.value).toBe('# frankenphp directives\n')
    );

    await textarea(wrapper).setValue('# frankenphp directives\nheader X-Test 1\n');
    await wrapper.vm.$nextTick();
    await saveButton(wrapper).trigger('click');

    await vi.waitFor(() =>
      expect(saveButton(wrapper).attributes('disabled'), 'still dirty after a save').toBeDefined()
    );
    expect(
      calls.find(([name]) => name === 'serverConfigSet'),
      'saved against the wrong server'
    ).toEqual(['serverConfigSet', 'frankenphp', '# frankenphp directives\nheader X-Test 1\n']);

    wrapper.unmount();
  });

  /**
   * Directives reach a container only through a regenerate. The pane cannot
   * show that notice itself — it is shared with the `.env` editor — so it says
   * so upward, and a save that failed must not.
   */
  it('announces a successful save and stays silent about a failed one', async () => {
    const wrapper = await render();
    await textarea(wrapper).setValue('changed');
    await wrapper.vm.$nextTick();
    await saveButton(wrapper).trigger('click');

    await vi.waitFor(() => expect(wrapper.emitted('saved')).toBeTruthy());
    expect(wrapper.emitted('saved')[0]).toEqual([['SERVER_CONFIG']]);
    wrapper.unmount();

    calls.length = 0;
    replies.serverConfigSet = () => Promise.reject(new Error('read-only workspace'));

    const failing = await render();
    await textarea(failing).setValue('changed');
    await failing.vm.$nextTick();
    await saveButton(failing).trigger('click');

    await vi.waitFor(() => expect(failing.text()).toContain('read-only workspace'));
    expect(failing.emitted('saved'), 'a failed save announced itself as done').toBeFalsy();

    failing.unmount();
  });

  it('reports a load failure instead of showing an empty editor', async () => {
    replies.serverConfigGet = () => Promise.reject(new Error('no workspace selected'));

    const wrapper = await render();
    expect(wrapper.text()).toContain('no workspace selected');

    wrapper.unmount();
  });
});
