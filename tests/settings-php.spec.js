import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The PHP / runtime-defaults pane, mounted.
 *
 * Seventh out of `Settings.vue` under §14.16. The version choices come from the
 * catalog compiled into the binary rather than a list typed into the view, so a
 * release added there shows up without a second edit.
 *
 * The rule worth pinning is the one in `itemsFor`: **the value currently in
 * `.env` is always offered**, even when the catalog does not list it. There are
 * two ordinary ways to reach that state — a catalog call that failed, and a
 * value written by an older build — and in both a select whose only item is
 * missing renders blank, which reads as data loss rather than as "that version
 * is no longer shipped".
 */

globalThis.visualViewport = undefined;

const replies = {};

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get: (_t, name) => () => {
        const reply = replies[name];
        return typeof reply === 'function' ? reply() : Promise.resolve(reply);
      },
    }
  ),
}));

const { i18n } = await import('@/i18n');
const PhpPane = (await import('@/components/settings/PhpPane.vue')).default;
const { useEnvEditor, provideEnvEditor } = await import('@/composables/useEnvEditor');
const { useCatalog } = await import('@/composables/useCatalog');

const vuetify = createVuetify({ components, directives });

const CATALOG = {
  runtimes: [
    { id: 'php', versions: ['8.3', '8.4'] },
    { id: 'node', versions: ['20', '22'] },
    { id: 'python', versions: ['3.12'] },
  ],
  servers: ['nginx', 'caddy'],
};

let editor;

/**
 * Under a host that provides a *loaded* editor, which is what `Settings.vue`
 * does — a pane that loaded `.env` on its own mount would discard whatever the
 * other five had typed every time the user changed tab.
 */
async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { PhpPane },
      setup() {
        editor = provideEnvEditor(useEnvEditor());
        return {};
      },
      template: '<PhpPane />',
    },
    { attachTo: host, global: { plugins: [vuetify, i18n] } }
  );

  await editor.loadDefaults();
  await editor.load();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const pane = (wrapper) => wrapper.findComponent(PhpPane).vm;

beforeEach(() => {
  // The catalog is module-scoped so two panes share one fetch; a test must not
  // inherit the previous one's.
  useCatalog().reset();
  for (const key of Object.keys(replies)) delete replies[key];
  replies.envGet = { SUPPORTED_LANGUAGES_PHP_DEFAULT: '8.4', PHP_TOOL_NODEJS_VERSION: '22' };
  replies.envDefaults = {};
  replies.catalogGet = { ...CATALOG };
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the version choices', () => {
  it('come from the catalog, not from a list in the view', async () => {
    const wrapper = await render();

    expect(pane(wrapper).phpVersions).toEqual(['8.3', '8.4']);
    expect(pane(wrapper).nodeVersions).toEqual(['20', '22']);
    expect(pane(wrapper).serverChoices).toEqual(['nginx', 'caddy']);

    wrapper.unmount();
  });

  /** A value written by an older build, or one the catalog stopped shipping. */
  it('always offer the value that is actually set', async () => {
    replies.envGet = { SUPPORTED_LANGUAGES_PHP_DEFAULT: '8.1' };

    const wrapper = await render();
    expect(pane(wrapper).phpVersions, 'the configured version was not among the choices').toEqual([
      '8.1',
      '8.3',
      '8.4',
    ]);

    wrapper.unmount();
  });

  /**
   * A catalog that cannot be read is a narrower list, not a blank select — and
   * a blank select over a configured value reads as data loss.
   */
  it('survive the catalog being unreadable', async () => {
    replies.catalogGet = () => Promise.reject(new Error('binary is odd'));

    const wrapper = await render();
    expect(pane(wrapper).phpVersions, 'the pane lost the value it had').toEqual(['8.4']);
    expect(wrapper.text().trim().length).toBeGreaterThan(0);

    wrapper.unmount();
  });

  it('offer nothing rather than undefined when neither knows a value', async () => {
    replies.envGet = {};
    replies.catalogGet = { runtimes: [], servers: [] };

    const wrapper = await render();
    expect(pane(wrapper).phpVersions).toEqual([]);
    expect(pane(wrapper).serverChoices).toEqual([]);

    wrapper.unmount();
  });

  /** Every runtime the app can build gets a default, from the same list. */
  it('cover every runtime the catalog knows', async () => {
    const wrapper = await render();
    expect(
      pane(wrapper).runtimeItems({ id: 'python', key: 'SUPPORTED_LANGUAGES_PYTHON_DEFAULT' })
    ).toEqual(['3.12']);
    wrapper.unmount();
  });
});

describe('editing', () => {
  it('writes through the shared editor rather than its own', async () => {
    const wrapper = await render();

    pane(wrapper).edit('SUPPORTED_LANGUAGES_PHP_DEFAULT', '8.3');
    expect(editor.edits.value.SUPPORTED_LANGUAGES_PHP_DEFAULT, 'the pane kept its own diff').toBe(
      '8.3'
    );
    expect(editor.dirty.value).toBe(true);

    wrapper.unmount();
  });

  it('asks the view to save, because the file is shared', async () => {
    const wrapper = await render();

    pane(wrapper).edit('SUPPORTED_LANGUAGES_PHP_DEFAULT', '8.3');
    await wrapper.vm.$nextTick();

    const save = wrapper
      .findAll('button')
      .find((b) => b.text().includes(i18n.global.t('settings.save', { count: 1 })));
    expect(save, 'no save button once something changed').toBeTruthy();

    await save.trigger('click');
    expect(wrapper.findComponent(PhpPane).emitted('save')).toBeTruthy();

    wrapper.unmount();
  });
});
