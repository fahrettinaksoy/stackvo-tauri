import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Appearance pane, mounted.
 *
 * Sixth out of `Settings.vue` under §14.16 and the cleanest seam so far: the
 * only pane that touches neither the `.env` editor nor the operation console.
 * Everything it changes lives in `useAppearanceStore`, which persists and
 * applies on its own.
 *
 * What is worth asserting is therefore not "does the store work" — `appearance`
 * has its own coverage — but the three things that only exist once the markup
 * runs and that a reviewer cannot see by reading it:
 *
 *   * the reset affordance tells the truth about whether anything is customised;
 *   * a preset cannot be saved without a name, and saving clears the field;
 *   * every swatch, font and palette the library ships is actually offered,
 *     rather than a subset somebody transcribed by hand.
 */

globalThis.visualViewport = undefined;

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
}));

const { i18n } = await import('@/i18n');
const AppearancePane = (await import('@/components/settings/AppearancePane.vue')).default;
const { useAppearanceStore } = await import('@/stores/appearance');
const { DEFAULT_APPEARANCE, PRIMARY_SWATCHES, FONT_FAMILIES, STATUS_PALETTES } =
  await import('@/lib/appearance');

const vuetify = createVuetify({ components, directives });

/**
 * One pinia, handed to `mount` and resolved explicitly.
 *
 * `setActivePinia` alone gave the test one store and the component another in
 * `settings-workspace.spec.js` — state written by the test was invisible to the
 * thing under test, and the assertion failed for no stated reason.
 */
async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const pinia = createPinia();
  setActivePinia(pinia);

  const wrapper = mount(AppearancePane, {
    attachTo: host,
    global: { plugins: [pinia, vuetify, i18n] },
  });

  await wrapper.vm.$nextTick();
  return { wrapper, store: useAppearanceStore(pinia) };
}

const button = (wrapper, label) => wrapper.findAll('button').find((b) => b.text().includes(label));

beforeEach(() => {
  setActivePinia(createPinia());
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the appearance pane', () => {
  it('renders without a workspace, an engine or a .env', async () => {
    const { wrapper } = await render();
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * "Back to defaults" should not be a leap of faith. The flag has to follow
   * the store rather than being set once — a stale one either offers a reset
   * that does nothing, or hides one that would help.
   */
  it('knows whether anything has been customised', async () => {
    const { wrapper, store } = await render();
    expect(wrapper.vm.isDefaultAppearance, 'a fresh store is not the default').toBe(true);

    const key = Object.keys(DEFAULT_APPEARANCE)[0];
    store.value[key] = 'something-else';
    await wrapper.vm.$nextTick();
    expect(wrapper.vm.isDefaultAppearance, 'a change went unnoticed').toBe(false);

    store.value[key] = DEFAULT_APPEARANCE[key];
    await wrapper.vm.$nextTick();
    expect(wrapper.vm.isDefaultAppearance, 'undoing the change did not restore it').toBe(true);

    wrapper.unmount();
  });
});

describe('saving a preset', () => {
  it('will not save one without a name', async () => {
    const { wrapper, store } = await render();
    const saved = vi.spyOn(store, 'savePreset');

    const save = button(wrapper, i18n.global.t('settings.savePreset'));
    expect(save, 'no save button').toBeTruthy();
    expect(save.attributes('disabled'), 'an unnamed preset was saveable').toBeDefined();

    await save.trigger('click');
    expect(saved).not.toHaveBeenCalled();

    wrapper.unmount();
  });

  /**
   * The field has to clear, or the next preset is offered the previous one's
   * name and a second click quietly overwrites it.
   */
  it('saves under the typed name and then clears the field', async () => {
    const { wrapper, store } = await render();
    const saved = vi.spyOn(store, 'savePreset').mockResolvedValue(undefined);

    wrapper.vm.presetName = 'midnight';
    await wrapper.vm.$nextTick();

    const save = button(wrapper, i18n.global.t('settings.savePreset'));
    expect(save.attributes('disabled'), 'a named preset was not saveable').toBeUndefined();

    await save.trigger('click');
    await vi.waitFor(() => expect(saved).toHaveBeenCalledWith('midnight'));
    expect(wrapper.vm.presetName, 'the name stayed in the box').toBe('');

    wrapper.unmount();
  });
});

describe('what the pane offers', () => {
  /**
   * Derived from the library, not transcribed. A hand-written subset is how a
   * palette gets added to `appearance.js` and never appears in the app — and
   * nothing would report it.
   */
  it('offers every status palette and font the library ships', async () => {
    const { wrapper } = await render();

    expect(wrapper.vm.statusItems.map((i) => i.value)).toEqual(STATUS_PALETTES.map((p) => p.id));
    expect(wrapper.vm.fontItems.map((i) => i.value)).toEqual(FONT_FAMILIES.map((f) => f.id));

    // And each carries a translated label rather than the raw id.
    for (const item of [...wrapper.vm.statusItems, ...wrapper.vm.fontItems]) {
      expect(item.title, `${item.value} has no label`).toBeTruthy();
      expect(item.title).not.toContain('settings.');
    }

    wrapper.unmount();
  });

  it('renders a swatch for every primary colour', async () => {
    const { wrapper } = await render();
    const text = wrapper.html();

    // The swatches are drawn from the same list; a missing one is a colour
    // nobody can choose.
    for (const swatch of PRIMARY_SWATCHES) {
      const value = typeof swatch === 'string' ? swatch : (swatch.value ?? swatch.id);
      expect(text.includes(value), `${value} is not offered`).toBe(true);
    }

    wrapper.unmount();
  });
});
