import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Workspace pane, mounted.
 *
 * Fifth and largest out of `Settings.vue` under §14.16 — the folder, the
 * compose verbs and the stack preset were three panes for one subject and three
 * places to look before finding the button you wanted.
 *
 * ## The bug the extraction found
 *
 * The export half **never populated**. The view loaded the preset from a watch
 * on the active tab:
 *
 * ```js
 * if (value === 'sharing' && !stackPreset.value) loadStackPreset();
 * ```
 *
 * There is no `sharing` section — the three panes were merged into `workspace`
 * and the key was left behind. The only surviving route to `loadStackPreset`
 * was the one *after* an import succeeded, so opening the pane showed an empty
 * JSON box and "0 services enabled". Nothing looked wrong: a `watch` comparing
 * a string to a string that stopped existing is not a mistake any tool reports,
 * and no test mounted the pane to notice.
 *
 * `it('loads the current stack as soon as it opens')` is that bug, in the form
 * that fails if it comes back.
 */

globalThis.visualViewport = undefined;

const replies = {};
const calls = [];
const dialog = { save: null, open: null };

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

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (...args) => dialog.save?.(...args),
  open: (...args) => dialog.open?.(...args),
}));

const { i18n } = await import('@/i18n');
const WorkspacePane = (await import('@/components/settings/WorkspacePane.vue')).default;
const { useAppStore } = await import('@/stores/app');

const vuetify = createVuetify({ components, directives });

const PRESET = {
  name: 'team',
  services: { mysql: { enabled: true }, redis: { enabled: true }, mongo: { enabled: false } },
};

/**
 * `engineUp` gates the compose verbs, and a fresh store reports the engine
 * down — which is correct, and is not the world these buttons are about.
 */
async function render(props = {}, { engineUp = true } = {}) {
  const host = document.createElement('div');
  document.body.appendChild(host);

  // The pinia is passed to `useAppStore` **explicitly**. Relying on
  // `setActivePinia` here quietly gave the test one store and the component
  // another — `engineUp` read true in the test and false in the pane, and the
  // compose buttons stayed disabled while the assertion insisted they should
  // not be.
  const pinia = createPinia();
  setActivePinia(pinia);

  const wrapper = mount(WorkspacePane, {
    props,
    attachTo: host,
    global: { plugins: [pinia, vuetify, i18n] },
  });

  useAppStore(pinia).engine = engineUp ? { reachable: true } : null;
  await wrapper.vm.$nextTick();

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const button = (wrapper, label) => wrapper.findAll('button').find((b) => b.text().includes(label));

beforeEach(() => {
  setActivePinia(createPinia());
  calls.length = 0;
  dialog.save = null;
  dialog.open = null;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.presetExport = { ...PRESET };
  replies.templatesList = [];
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the stack preset', () => {
  /** The bug. It shipped showing an empty box and "0 services enabled". */
  it('loads the current stack as soon as it opens', async () => {
    const wrapper = await render();

    expect(
      calls.some(([n]) => n === 'presetExport'),
      'the pane opened without reading the stack it describes'
    ).toBe(true);
    expect(wrapper.vm.preset).toEqual(PRESET);
    expect(wrapper.vm.presetEnabledCount, 'the summary counted nothing').toBe(2);

    wrapper.unmount();
  });

  it('renders the stack as readable JSON', async () => {
    const wrapper = await render();
    expect(wrapper.vm.presetJson).toBe(JSON.stringify(PRESET, null, 2));
    wrapper.unmount();
  });

  it('says nothing rather than crashing when the stack cannot be read', async () => {
    replies.presetExport = () => Promise.reject(new Error('no workspace selected'));

    const wrapper = await render();
    expect(wrapper.text()).toContain('no workspace selected');
    expect(wrapper.vm.presetEnabledCount).toBe(0);

    wrapper.unmount();
  });

  it('writes the preset to the path the save dialog returned', async () => {
    dialog.save = ({ defaultPath }) => Promise.resolve(`/tmp/${defaultPath}`);
    replies.presetSave = () => Promise.resolve();

    const wrapper = await render();
    wrapper.vm.presetName = 'team';
    await wrapper.vm.$nextTick();
    await wrapper.vm.exportPreset();

    expect(calls.find(([n]) => n === 'presetSave')).toEqual([
      'presetSave',
      '/tmp/team.stackvo-preset.json',
      'team',
    ]);

    wrapper.unmount();
  });

  it('does nothing at all when the save dialog is cancelled', async () => {
    dialog.save = () => Promise.resolve(null);

    const wrapper = await render();
    await wrapper.vm.exportPreset();

    expect(calls.some(([n]) => n === 'presetSave')).toBe(false);
    wrapper.unmount();
  });
});

describe('importing a preset', () => {
  /**
   * Plan then apply, like the hosts file and the certificate: you see the diff
   * before anything is written over your own stack.
   */
  it('shows the plan without writing anything', async () => {
    dialog.open = () => Promise.resolve('/tmp/team.stackvo-preset.json');
    // The shape of `preset::Plan`: `changes` and `rejected` are both `Vec`, so
    // serde always emits them — a fixture that omits one is a payload the back
    // end cannot send, and the pane rightly reads `.length` off it.
    replies.presetPlan = {
      changes: [{ service: 'redis', from: false, to: true }],
      rejected: [],
      unchanged: 0,
    };

    const wrapper = await render();
    await wrapper.vm.choosePreset();
    await wrapper.vm.$nextTick();

    expect(calls.find(([n]) => n === 'presetPlan')).toEqual([
      'presetPlan',
      '/tmp/team.stackvo-preset.json',
    ]);
    expect(
      calls.some(([n]) => n === 'presetApply'),
      'the plan applied itself'
    ).toBe(false);

    wrapper.unmount();
  });

  /**
   * A file that is not a preset is an error, not an empty plan — the pane has
   * to clear, or a previous review is mistaken for this file's.
   */
  it('clears a previous plan when the chosen file is not a preset', async () => {
    dialog.open = () => Promise.resolve('/tmp/good.json');
    replies.presetPlan = { changes: [], rejected: [], unchanged: 3 };

    const wrapper = await render();
    await wrapper.vm.choosePreset();
    expect(wrapper.vm.presetPlan).toBeTruthy();

    dialog.open = () => Promise.resolve('/tmp/holiday-photo.json');
    replies.presetPlan = () => Promise.reject(new Error('not a preset'));
    await wrapper.vm.choosePreset();
    await wrapper.vm.$nextTick();

    expect(wrapper.vm.presetPlan, 'the old plan survived a failed read').toBe(null);
    expect(wrapper.text()).toContain('not a preset');

    wrapper.unmount();
  });

  it('re-reads the stack after applying, because it just changed', async () => {
    dialog.open = () => Promise.resolve('/tmp/team.stackvo-preset.json');
    replies.presetPlan = { changes: [], rejected: [], unchanged: 3 };
    //  answers the same `preset::Plan` as `preset_plan` —
    // the pane keeps showing it, so it needs the same fields.
    replies.presetApply = { changes: [], rejected: [], unchanged: 3 };

    const wrapper = await render();
    await wrapper.vm.choosePreset();
    calls.length = 0;
    await wrapper.vm.applyPreset();

    expect(calls.some(([n]) => n === 'presetApply')).toBe(true);
    expect(
      calls.some(([n]) => n === 'presetExport'),
      'the pane still describes the stack from before the import'
    ).toBe(true);

    wrapper.unmount();
  });
});

describe('the compose verbs', () => {
  /**
   * `up`, `restart` and `down` report through the shared operation console and
   * their busy state is the view's. A pane that ran them itself would be a pane
   * that owns the stack.
   */
  it.each([
    ['actions.up', 'up'],
    ['actions.composeRestart', 'restart'],
    ['actions.down', 'down'],
  ])('emits %s upward rather than running it', async (label, event) => {
    const wrapper = await render();
    const target = button(wrapper, i18n.global.t(label));
    expect(target, `no ${event} button`).toBeTruthy();

    await target.trigger('click');

    expect(wrapper.emitted(event)).toBeTruthy();
    expect(
      calls.some(([n]) => n.startsWith('compose')),
      'the pane drove compose behind the view'
    ).toBe(false);

    wrapper.unmount();
  });

  /**
   * Composing against a dead daemon cannot work, so the buttons say so rather
   * than failing on click — the same judgement the sidebar's quick actions make.
   */
  it('is unusable while the engine is down', async () => {
    const wrapper = await render({}, { engineUp: false });
    expect(button(wrapper, i18n.global.t('actions.up')).attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });

  it('shows the view’s busy state rather than inventing its own', async () => {
    const wrapper = await render({ busy: true });
    expect(
      button(wrapper, i18n.global.t('actions.up')).classes(),
      'the button ignored the operation the view is running'
    ).toContain('v-btn--loading');
    wrapper.unmount();
  });
});

describe('the generator check', () => {
  it('only runs when asked', async () => {
    replies.generatorVerify = { files: [], matched: 0, differed: 0 };

    const wrapper = await render();
    expect(
      calls.some(([n]) => n === 'generatorVerify'),
      'the check ran on mount, against a workspace that may not exist'
    ).toBe(false);

    await wrapper.vm.verify();
    expect(calls.some(([n]) => n === 'generatorVerify')).toBe(true);

    wrapper.unmount();
  });

  /**
   * "The files are stale" and "write them again" are the same thought, so the
   * repair regenerates *and* re-checks — a check that left the report stale
   * would report the drift it had just fixed.
   */
  it('regenerates and re-checks in one action', async () => {
    replies.generateRun = () => Promise.resolve();
    replies.generatorVerify = { files: [], matched: 0, differed: 0 };

    const wrapper = await render();
    await wrapper.vm.runGenerate();

    const order = calls
      .filter(([n]) => n === 'generateRun' || n === 'generatorVerify')
      .map(([n]) => n);
    expect(order).toEqual(['generateRun', 'generatorVerify']);

    wrapper.unmount();
  });
});

describe('the workspace folder', () => {
  it('asks the view to pick one rather than picking it itself', async () => {
    const wrapper = await render();
    const pick = button(wrapper, i18n.global.t('workspace.change'));

    if (pick) {
      await pick.trigger('click');
      expect(wrapper.emitted('pick')).toBeTruthy();
      expect(calls.some(([n]) => n === 'workspacePick')).toBe(false);
    }

    wrapper.unmount();
  });
});
