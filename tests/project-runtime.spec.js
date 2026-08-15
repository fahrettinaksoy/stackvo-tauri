import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Runtime section, which is two panes that never appear together: a Node
 * project has a dev server and no php.ini, a PHP project the reverse.
 *
 * Both were 0%-covered inside `ProjectDetail.vue`. The parts worth pinning are
 * the ones where a plausible simplification is wrong: the php.ini save sends a
 * *patch* rather than the form, and an empty field in that patch is `null`
 * (remove the directive) rather than `''` (a directive PHP reads as zero).
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

const { useDevServer } = await import('@/composables/useDevServer');
const { usePhpIni, PHP_INI_FIELDS } = await import('@/composables/usePhpIni');
const { i18n } = await import('@/i18n');
const DevServerPane = (await import('@/components/project/DevServerPane.vue')).default;
const PhpIniPane = (await import('@/components/project/PhpIniPane.vue')).default;
const PerfPane = (await import('@/components/project/PerfPane.vue')).default;

const vuetify = createVuetify({ components, directives });
const ref = (value) => ({ value });

const DEV = {
  enabled: true,
  mounted: true,
  command: 'npm run dev -- --host',
  productionCommand: 'npm start',
  hostAllowed: true,
  needsRecreate: false,
  snippet: "server: { allowedHosts: ['shop.loc'] }",
};

const INI = {
  values: { memory_limit: '256M', upload_max_filesize: '8M' },
  effective: { memory_limit: '128M', max_execution_time: '0' },
  unmanaged: {},
  path: '/ws/projects/shop/php.ini',
};

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.devserverStatus = { ...DEV };
  replies.phpIniStatus = structuredClone(INI);
});

describe('the dev server', () => {
  /**
   * A PHP project has no dev server, and asking about one would answer an
   * error for a pane that should simply not be there.
   */
  it('does not ask about a runtime that has none', async () => {
    const d = useDevServer(ref('shop'));
    expect(await d.load('php')).toBe(null);
    expect(calls).toEqual([]);
  });

  it('adopts the command the back end reports', async () => {
    const d = useDevServer(ref('shop'));
    await d.load('node');

    expect(d.command.value).toBe('npm run dev -- --host');
    expect(d.status.value.enabled).toBe(true);
  });

  /**
   * The back end normalises what it stores. Echoing the typed string instead
   * would leave the field disagreeing with what will actually run.
   */
  it('shows the stored command rather than the typed one', async () => {
    const d = useDevServer(ref('shop'));
    d.command.value = '  npm run dev  ';
    replies.devserverSet = { ...DEV, command: 'npm run dev' };

    await d.toggle(true);
    expect(calls.at(-1)).toEqual(['devserverSet', 'shop', true, '  npm run dev  ']);
    expect(d.command.value).toBe('npm run dev');
  });

  it('sends no command at all rather than an empty one', async () => {
    const d = useDevServer(ref('shop'));
    d.command.value = '';
    replies.devserverSet = { ...DEV };

    await d.toggle(false);
    expect(calls.at(-1)).toEqual(['devserverSet', 'shop', false, null]);
  });

  /**
   * The state where the container is right and the site still answers 403:
   * Vite rejects a `.loc` host unless the project's own config names it.
   */
  it('flags the case where only the project config is wrong', async () => {
    const d = useDevServer(ref('shop'));
    replies.devserverStatus = { ...DEV, hostAllowed: false };
    await d.load('node');
    expect(d.blocked.value).toBe(true);

    replies.devserverStatus = { ...DEV, enabled: false, hostAllowed: false };
    await d.load('node');
    expect(d.blocked.value, 'a dev server that is off cannot be blocked').toBeFalsy();
  });

  it('reports a failed toggle and stops spinning', async () => {
    replies.devserverSet = () => Promise.reject({ code: 'DOCKER_UNAVAILABLE', message: 'down' });
    const d = useDevServer(ref('shop'));

    expect(await d.toggle(true)).toBe(null);
    expect(d.error.value.code).toBe('DOCKER_UNAVAILABLE');
    expect(d.busy.value).toBe(false);
  });

  /** No clipboard is a missing convenience — the snippet is on screen anyway. */
  it('survives a machine with no clipboard', async () => {
    const d = useDevServer(ref('shop'));
    await d.load('node');

    const original = globalThis.navigator.clipboard;
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText: () => Promise.reject(new Error('denied')) },
      configurable: true,
    });

    expect(await d.copySnippet()).toBe(false);
    expect(d.copied.value).toBe(false);

    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: original,
      configurable: true,
    });
  });
});

describe('php.ini', () => {
  it('does not ask about a runtime that has none', async () => {
    const p = usePhpIni(ref('shop'));
    expect(await p.load('node')).toBe(null);
    expect(calls).toEqual([]);
  });

  it('seeds every field, including the ones with no stored value', async () => {
    const p = usePhpIni(ref('shop'));
    await p.load('php');

    expect(Object.keys(p.draft.value).sort()).toEqual([...PHP_INI_FIELDS].sort());
    expect(p.draft.value.memory_limit).toBe('256M');
    expect(p.draft.value.post_max_size, 'unset must be empty, not undefined').toBe('');
    expect(p.dirty.value).toBe(false);
  });

  /**
   * Only what changed. Sending the unchanged fields too would rewrite lines the
   * user may have commented next to, for no reason.
   */
  it('sends a patch of the changed fields, not the whole form', async () => {
    replies.phpIniSet = structuredClone(INI);
    const p = usePhpIni(ref('shop'));
    await p.load('php');

    p.draft.value.post_max_size = '32M';
    expect(p.dirty.value).toBe(true);

    await p.save();
    expect(calls.at(-1)).toEqual(['phpIniSet', 'shop', { post_max_size: '32M' }]);
  });

  /**
   * An empty field is a removal. `memory_limit =` with nothing after it is a
   * directive PHP reads as zero, which is not what clearing a box means.
   */
  it('sends null for a cleared field rather than an empty string', async () => {
    replies.phpIniSet = { ...INI, values: {} };
    const p = usePhpIni(ref('shop'));
    await p.load('php');

    p.draft.value.memory_limit = '   ';
    await p.save();

    expect(calls.at(-1)).toEqual(['phpIniSet', 'shop', { memory_limit: null }]);
  });

  it('sends nothing when only whitespace was added around an unchanged value', async () => {
    replies.phpIniSet = structuredClone(INI);
    const p = usePhpIni(ref('shop'));
    await p.load('php');

    p.draft.value.memory_limit = '  256M  ';
    await p.save();
    expect(calls.at(-1)).toEqual(['phpIniSet', 'shop', {}]);
  });

  it('re-seeds the draft from what was saved', async () => {
    replies.phpIniSet = { ...structuredClone(INI), values: { memory_limit: '512M' } };
    const p = usePhpIni(ref('shop'));
    await p.load('php');

    p.draft.value.memory_limit = '512M';
    await p.save();

    expect(p.dirty.value, 'a saved form still showing as dirty invites a second save').toBe(false);
    expect(p.draft.value.upload_max_filesize).toBe('');
  });

  /** Clearing every field with nothing unmanaged left deletes the file. */
  it('knows when saving would remove the file entirely', async () => {
    const p = usePhpIni(ref('shop'));
    await p.load('php');
    for (const key of PHP_INI_FIELDS) p.draft.value[key] = '';
    expect(p.wouldRemoveFile.value).toBe(true);

    replies.phpIniStatus = { ...structuredClone(INI), unmanaged: { opcache_enable: '1' } };
    await p.load('php');
    for (const key of PHP_INI_FIELDS) p.draft.value[key] = '';
    expect(p.wouldRemoveFile.value, 'a directive we do not manage still needs the file').toBe(
      false
    );
  });
});

describe('the panes', () => {
  const open = (Pane, runtime) =>
    mount(
      {
        components: { Pane },
        template: `<v-app><Pane name="shop" runtime="${runtime}" /></v-app>`,
      },
      { global: { plugins: [vuetify, i18n] } }
    );

  it('DevServerPane shows the snippet a Vite config needs', async () => {
    const wrapper = open(DevServerPane, 'node');
    await vi.waitFor(() => expect(wrapper.text()).toContain('allowedHosts'));

    expect(calls[0]).toEqual(['devserverStatus', 'shop']);
  });

  /** Mounted for a PHP project, it must render nothing rather than throw. */
  it('DevServerPane is silent for a project that has no dev server', async () => {
    const wrapper = open(DevServerPane, 'php');
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([]);
    expect(wrapper.text()).not.toContain('allowedHosts');
  });

  it('PhpIniPane fills the fields from the stored values', async () => {
    const wrapper = open(PhpIniPane, 'php');
    await vi.waitFor(() => expect(wrapper.findAll('input').length).toBeGreaterThan(0));

    const values = wrapper.findAll('input').map((i) => i.element.value);
    expect(values).toContain('256M');
    // The measured figure is a placeholder, not a value — it must not be
    // saved back as if the user had typed it.
    const measured = wrapper.findAll('input').find((i) => i.attributes('placeholder') === '128M');
    expect(measured.element.value).toBe('256M');
  });

  it('PhpIniPane is silent for a Node project', async () => {
    const wrapper = open(PhpIniPane, 'node');
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([]);
  });
});

/**
 * The performance layer (I-1).
 *
 * Three things here are only true at this layer and each is the kind that looks
 * fine and is wrong: that turning a layer on is a *seed then save* the backend
 * owns (so the pane must not report it done on its own), that the price — an
 * editor that can no longer see the directory — is stated on the row rather
 * than discovered, and that deleting the volume is never the switch's side
 * effect.
 */
describe('the performance layer', () => {
  const LAYERS = [
    {
      path: 'vendor',
      enabled: false,
      volume: 'stackvo-cache-shop--vendor',
      exists: false,
      onHost: true,
      hostFiles: 8000,
    },
    {
      path: 'storage/framework',
      enabled: true,
      volume: 'stackvo-cache-shop--storage-framework',
      exists: true,
      bytes: 1024 * 1024,
      onHost: true,
      hostFiles: 12,
    },
  ];

  async function open(layers = LAYERS, runtime = 'php') {
    replies.perfStatus = layers;
    const wrapper = mount(
      {
        components: { PerfPane },
        template: '<v-app><PerfPane name="shop" :runtime="rt" /></v-app>',
        data: () => ({ rt: runtime }),
      },
      { global: { plugins: [vuetify, i18n] } }
    );
    await flushPromises();
    return wrapper;
  }

  it('lists a row per directory with what it currently is', async () => {
    const wrapper = await open();
    const rows = wrapper.findAll('[data-test="perf-layer"]');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('vendor');
    expect(rows[0].text()).toContain('8000');
    expect(rows[1].text()).toContain('stackvo-cache-shop--storage-framework');
  });

  /**
   * The price is on the row that charges it. An editor going quiet three days
   * later, with nothing on screen connecting it to a switch, is the failure
   * this sentence exists to prevent.
   */
  it('says the editor can no longer see a directory that moved', async () => {
    const wrapper = await open();
    const rows = wrapper.findAll('[data-test="perf-layer"]');
    expect(rows[0].text()).not.toContain(i18n.global.t('perf.editorCannotSee'));
    expect(rows[1].text()).toContain(i18n.global.t('perf.editorCannotSee'));
  });

  /**
   * The container is still reading the old arrangement until it is recreated,
   * so the pane asks for that rather than leaving somebody to wonder why
   * nothing got faster.
   */
  it('asks for the container to be recreated after a change', async () => {
    const wrapper = await open();
    replies.perfSet = LAYERS;

    await wrapper.findAll('input[type="checkbox"]')[0].setValue(true);
    await flushPromises();

    expect(calls.some(([n]) => n === 'perfSet')).toBe(true);
    expect(
      wrapper.findComponent(PerfPane).emitted('apply'),
      'the container has to be recreated'
    ).toBeTruthy();
  });

  /** Deleting thirty thousand files is never a side effect of a checkbox. */
  it('offers to delete a volume only when nothing is using it', async () => {
    const wrapper = await open();
    const rows = wrapper.findAll('[data-test="perf-layer"]');
    // vendor: off, and no volume yet — nothing to delete.
    expect(rows[0].text()).not.toContain(i18n.global.t('perf.forget'));
    // storage/framework: on — the switch turns it off, it does not delete.
    expect(rows[1].text()).not.toContain(i18n.global.t('perf.forget'));

    const stale = await open([{ ...LAYERS[0], enabled: false, exists: true, bytes: 500 }]);
    expect(stale.text()).toContain(i18n.global.t('perf.forget'));
  });

  /** A Go project has neither a vendor nor a node_modules to move. */
  it('draws nothing for a runtime it does not apply to', async () => {
    const wrapper = await open(LAYERS, 'go');
    expect(wrapper.findAll('[data-test="perf-layer"]')).toHaveLength(0);
    expect(calls.some(([n]) => n === 'perfStatus')).toBe(false);
  });
});
