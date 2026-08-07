import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Localisation pane, mounted.
 *
 * Eighth and smallest out of `Settings.vue` under §14.16 — three controls with
 * three different owners, which is exactly why it is worth mounting rather than
 * reading. The app locale goes through `setLocale`, because changing it also
 * persists the preference and relabels the tray; the console locale and the RTL
 * flag are appearance state and go straight to the store. Wiring any of the
 * three to the wrong owner produces a control that appears to work and forgets
 * itself on the next launch.
 */

globalThis.visualViewport = undefined;

const setLocale = vi.fn();

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
}));

vi.mock('@/i18n', async (importOriginal) => ({
  ...(await importOriginal()),
  setLocale: (...args) => setLocale(...args),
}));

const { i18n } = await import('@/i18n');
const LocalisationPane = (await import('@/components/settings/LocalisationPane.vue')).default;
const { useAppearanceStore } = await import('@/stores/appearance');

const vuetify = createVuetify({ components, directives });

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const pinia = createPinia();
  setActivePinia(pinia);

  const wrapper = mount(LocalisationPane, {
    attachTo: host,
    global: { plugins: [pinia, vuetify, i18n] },
  });

  await wrapper.vm.$nextTick();
  return { wrapper, store: useAppearanceStore(pinia) };
}

beforeEach(() => {
  setLocale.mockClear();
  setActivePinia(createPinia());
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the localisation pane', () => {
  it('renders with no workspace and no engine', async () => {
    const { wrapper } = await render();
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * Not `locale.value = x`. `setLocale` also writes the preference and relabels
   * the tray, so assigning the ref directly would change the window and forget
   * it on the next launch.
   */
  it('changes the app language through setLocale, not by assigning the ref', async () => {
    const { wrapper } = await render();

    const select = wrapper.findAllComponents({ name: 'VSelect' })[0];
    expect(select, 'no language select').toBeTruthy();
    await select.vm.$emit('update:modelValue', 'tr');

    expect(setLocale).toHaveBeenCalledWith('tr');
    wrapper.unmount();
  });

  /**
   * The console locale is a different question from the app's: the stack's
   * output is read by whoever debugs it, which is not always the language the
   * window is in. It belongs to appearance state.
   */
  it('writes the console locale and the RTL flag to the appearance store', async () => {
    const { wrapper, store } = await render();
    const set = vi.spyOn(store, 'set');

    const selects = wrapper.findAllComponents({ name: 'VSelect' });
    await selects[1].vm.$emit('update:modelValue', 'en');
    expect(set).toHaveBeenCalledWith({ consoleLocale: 'en' });

    const toggle = wrapper.findComponent({ name: 'VSwitch' });
    await toggle.vm.$emit('update:modelValue', true);
    expect(set).toHaveBeenCalledWith({ rtl: true });

    expect(setLocale, 'appearance state went through the app locale').not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
