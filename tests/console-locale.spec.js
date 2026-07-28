import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The console panels can be pinned to a language of their own.
 *
 * Worth a test rather than a look: the wiring is a locale passed per `t()` call,
 * which fails silently — a wrong argument position leaves every string in the
 * interface language and the setting simply does nothing visible.
 */

globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
}));

const { default: LogView } = await import('@/components/LogView.vue');
const { useAppearanceStore } = await import('@/stores/appearance');
const { i18n } = await import('@/i18n');

const vuetify = createVuetify({ components, directives });

function mountPanel() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return {
    store: useAppearanceStore(),
    wrapper: mount(LogView, {
      props: { container: 'stackvo-demo' },
      global: { plugins: [pinia, vuetify, i18n] },
    }),
  };
}

describe('console language', () => {
  it('follows the interface by default and pins when asked', async () => {
    i18n.global.locale.value = 'tr';
    const { store, wrapper } = mountPanel();
    await wrapper.vm.$nextTick();
    const text = () => wrapper.text();

    const turkish = i18n.global.t('logs.waiting', {}, { locale: 'tr' });
    const english = i18n.global.t('logs.waiting', {}, { locale: 'en' });
    // The fixture is only meaningful if the two locales actually differ.
    expect(turkish).not.toBe(english);

    expect(text()).toContain(turkish);

    store.value = { ...store.value, consoleLocale: 'en' };
    await wrapper.vm.$nextTick();

    expect(text()).toContain(english);
    expect(text()).not.toContain(turkish);
  });
});
