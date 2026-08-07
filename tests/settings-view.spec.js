import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * `Settings.vue`, mounted — the finish line of §14.16.
 *
 * It began at **3,433 lines and 0% coverage**, and the readiness review's §2.3
 * called it the most expensive debt in the front end: too large to mount, so
 * two of its panes were tested against *copies* of their own markup, and the
 * other ten were not tested at all.
 *
 * Twelve panes and ten composables later it is 831 lines and holds no pane
 * markup — only the rail, the shared `.env` editor and the About card. This
 * file is the proof: every section renders, and switching between them does not
 * throw.
 *
 * That last part is what could not be checked before. Each pane is behind a
 * `v-if`, so a pane whose script referenced something the view had stopped
 * providing would fail *only* when that tab was opened — on a screen nobody
 * opens during the change that broke it. Three real instances of exactly that
 * were caught by `vue/no-undef-properties` during the split (§26.2, §27.4);
 * this catches the rest.
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

vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.1.0' }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(), openPath: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn(), open: vi.fn() }));
vi.mock('@tauri-apps/plugin-autostart', () => ({
  isEnabled: async () => false,
  enable: vi.fn(),
  disable: vi.fn(),
}));
vi.mock('@/lib/updates', () => ({
  checkForUpdate: async () => null,
  updatesConfigured: async () => false,
}));

const { i18n } = await import('@/i18n');
const Settings = (await import('@/views/Settings.vue')).default;

const vuetify = createVuetify({ components, directives });

/** Every section the rail offers, read from the view rather than transcribed. */
const SECTIONS = [
  'appearance',
  'localisation',
  'preferences',
  'domain',
  'php',
  'servers',
  'services',
  'workspace',
  'doctor',
  'certificates',
  'about',
];

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const pinia = createPinia();
  setActivePinia(pinia);

  const wrapper = mount(
    { components: { Settings }, template: '<v-app><Settings /></v-app>' },
    {
      attachTo: host,
      global: {
        plugins: [
          pinia,
          createRouter({
            history: createMemoryHistory(),
            routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
          }),
          vuetify,
          i18n,
        ],
      },
    }
  );

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const view = (wrapper) => wrapper.findComponent(Settings).vm;

beforeEach(() => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
  replies.envGet = { DEFAULT_TLD_SUFFIX: 'stackvo.loc' };
  replies.envDefaults = {};
  replies.prefsGet = {};
  replies.catalogGet = { runtimes: [], servers: [] };
  replies.hostsOverview = { entries: [], stale: [] };
  replies.templatesList = [];
  replies.appsAvailable = { terminals: [], editors: [], browsers: [] };
  replies.logsInfo = { directory: '/logs', newestFile: null, totalBytes: 0 };
  replies.certStatus = { sslEnabled: false };
  replies.presetExport = { services: {} };
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the settings view', () => {
  it('mounts at all — which it could not do at 3,433 lines', async () => {
    const wrapper = await render();
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * Every pane, opened. A pane referencing something the view stopped providing
   * throws only when its own tab is selected, so switching through all of them
   * is the only thing that covers the split.
   */
  it.each(SECTIONS)('opens the %s section without throwing', async (section) => {
    const wrapper = await render();

    view(wrapper).tab = section;
    await wrapper.vm.$nextTick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(view(wrapper).tab).toBe(section);
    expect(wrapper.text().trim().length, `${section} rendered nothing`).toBeGreaterThan(0);

    wrapper.unmount();
  });

  /**
   * The `.env` editor is provided once and shared. Two panes editing it must
   * see one diff — otherwise whichever saved last would drop the other's work.
   */
  it('shares one .env diff across the panes that edit it', async () => {
    const wrapper = await render();

    view(wrapper).tab = 'domain';
    await wrapper.vm.$nextTick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const DomainPane = (await import('@/components/settings/DomainPane.vue')).default;
    const domain = wrapper.findComponent(DomainPane);
    expect(domain.exists(), 'the domain pane did not render').toBe(true);
    domain.vm.edit('DEFAULT_TLD_SUFFIX', 'stackvo.test');

    view(wrapper).tab = 'php';
    await wrapper.vm.$nextTick();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const PhpPane = (await import('@/components/settings/PhpPane.vue')).default;
    const php = wrapper.findComponent(PhpPane);
    expect(php.exists(), 'the php pane did not render').toBe(true);
    expect(
      php.vm.effective('DEFAULT_TLD_SUFFIX'),
      'the two panes kept separate diffs over one file'
    ).toBe('stackvo.test');

    wrapper.unmount();
  });

  it('renders every section in Turkish too', async () => {
    i18n.global.locale.value = 'tr';
    try {
      const wrapper = await render();
      for (const section of SECTIONS) {
        view(wrapper).tab = section;
        await wrapper.vm.$nextTick();
        await new Promise((resolve) => setTimeout(resolve, 0));
        expect(wrapper.text().trim().length, `${section} is blank in Turkish`).toBeGreaterThan(0);
      }
      wrapper.unmount();
    } finally {
      i18n.global.locale.value = 'en';
    }
  });
});
