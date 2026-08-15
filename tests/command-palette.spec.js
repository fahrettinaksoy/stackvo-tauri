import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';

/**
 * The palette lists what can run, and running a row reaches the right command.
 *
 * A-2. Two things here are only visible at this layer. The first is that the
 * list is *derived* — a stopped project must offer Start and not Stop, and the
 * difference between the two is one branch that reads correctly either way. The
 * second is the keyboard: arrows and Enter are the whole point of the feature
 * and a click test would pass with both of them broken.
 */

globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

const calls = vi.hoisted(() => []);

vi.mock('@/lib/ipc', () => ({
  asList: (value) => (Array.isArray(value) ? value : []),
  StackvoError: class extends Error {},
  call: vi.fn(),
  api: new Proxy(
    {},
    {
      get:
        (_t, name) =>
        (...args) => {
          calls.push([String(name), ...args]);
          return Promise.resolve(null);
        },
    }
  ),
}));

// Partial: `@/i18n` also exports the instance `plugins/vuetify.js` reaches for
// at import time, and a bare stub takes it away from a module this never
// touches directly.
vi.mock('@/i18n', async (importOriginal) => ({
  ...(await importOriginal()),
  setLocale: vi.fn(),
}));

import CommandPalette from '@/components/CommandPalette.vue';
import { matchCommands } from '@/composables/useCommands';
import { useAppStore } from '@/stores/app';
import { useInventoryStore } from '@/stores/inventory';

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/', name: 'Dashboard', component: { template: '<div />' } },
    { path: '/projects', name: 'Projects', component: { template: '<div />' } },
    { path: '/projects/:name', name: 'ProjectDetail', component: { template: '<div />' } },
    { path: '/market', name: 'Market', component: { template: '<div />' } },
    { path: '/logs', name: 'Logs', component: { template: '<div />' } },
    { path: '/dumps', name: 'Dumps', component: { template: '<div />' } },
    { path: '/mail', name: 'Mail', component: { template: '<div />' } },
    { path: '/settings', name: 'Settings', component: { template: '<div />' } },
  ],
});

async function open({ projects = [], engineUp = true } = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);

  const app = useAppStore();
  app.engine = { reachable: engineUp };
  app.workspace = { valid: true };

  const inventory = useInventoryStore();
  inventory.projects = projects;

  await router.push('/');
  await router.isReady();

  const wrapper = mount(CommandPalette, {
    props: { modelValue: true },
    global: {
      plugins: [
        pinia,
        router,
        createVuetify({ components, directives }),
        createI18n({ legacy: false, locale: 'en', messages: { en } }),
      ],
    },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
}

/** Row labels, in the order the palette draws them. */
function labels() {
  return [...document.querySelectorAll('.palette-row-label')].map((el) => el.textContent.trim());
}

function type(value) {
  const input = document.querySelector('.palette-input');
  input.value = value;
  input.dispatchEvent(new Event('input'));
  return flushPromises();
}

function press(key) {
  document
    .querySelector('.palette-input')
    .dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
  return flushPromises();
}

beforeEach(() => {
  calls.length = 0;
  document.body.innerHTML = '';
});

describe('what the palette offers', () => {
  it('lists every destination the rail does', async () => {
    await open();
    const found = labels();
    for (const item of [
      'Dashboard',
      'Projects',
      'Catalogue',
      'Logs',
      'Dumps',
      'Mail',
      'Settings',
    ]) {
      expect(found).toContain(item);
    }
  });

  /**
   * The branch that would be wrong in the direction nobody notices: an action
   * offered for a state it cannot apply to fails only when someone picks it.
   */
  it('offers Start for a stopped project and Stop for a running one', async () => {
    await open({
      projects: [
        { name: 'quiet', built: true, running: false, runtime: 'php' },
        { name: 'loud', built: true, running: true, runtime: 'php' },
      ],
    });
    const found = labels();
    expect(found).toContain('Start quiet');
    expect(found).not.toContain('Stop quiet');
    expect(found).toContain('Stop loud');
    expect(found).toContain('Restart loud');
    expect(found).not.toContain('Start loud');
  });

  it('offers Build, and nothing else, for a project that was never built', async () => {
    await open({ projects: [{ name: 'fresh', built: false, running: false, runtime: 'php' }] });
    const found = labels().filter((label) => label.includes('fresh'));
    expect(found).toEqual(['fresh', 'Build fresh']);
  });

  /** A hosts file with no entry means the browser lands on an error page. */
  it('withholds "open site" until the domain actually resolves', async () => {
    await open({
      projects: [
        {
          name: 'shop',
          built: true,
          running: true,
          runtime: 'php',
          domain: 'shop.test',
          domainConfigured: false,
        },
      ],
    });
    expect(labels().some((label) => label.startsWith('Open shop.test'))).toBe(false);

    await open({
      projects: [
        {
          name: 'shop',
          built: true,
          running: true,
          runtime: 'php',
          domain: 'shop.test',
          domainConfigured: true,
        },
      ],
    });
    expect(labels()).toContain('Open shop.test in the browser');
  });

  /** Absent would read as a missing feature; greyed reads as the state it is. */
  it('greys the stack-wide actions when the engine is down rather than hiding them', async () => {
    await open({ engineUp: false });
    await type('all containers');
    const rows = [...document.querySelectorAll('.palette-row')];
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((row) => row.classList.contains('is-disabled'))).toBe(true);
  });
});

describe('choosing a row', () => {
  it('runs the highlighted row on Enter and closes', async () => {
    const wrapper = await open({
      projects: [{ name: 'loud', built: true, running: true, runtime: 'php' }],
    });
    await type('Stop loud');
    await press('Enter');

    expect(calls).toEqual([['projectStop', 'loud']]);
    expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([false]);
  });

  it('walks the list with the arrows and wraps at the ends', async () => {
    await open();
    await type('go');
    const first = document.querySelector('.palette-row.is-current');
    await press('ArrowUp');
    const last = document.querySelector('.palette-row.is-current');
    expect(last).not.toBe(first);
    await press('ArrowDown');
    expect(document.querySelector('.palette-row.is-current')).toBe(first);
  });

  it('does nothing when Enter lands on a disabled row', async () => {
    await open({ engineUp: false });
    await type('Start all');
    await press('Enter');
    expect(calls).toEqual([]);
  });

  /** Focus stays in the field, so the current row has to be named some other way. */
  it('names the current row through aria-activedescendant', async () => {
    await open();
    await type('logs');
    const input = document.querySelector('.palette-input');
    expect(input.getAttribute('aria-activedescendant')).toBe('palette-row-0');
    expect(document.getElementById('palette-row-0').getAttribute('aria-selected')).toBe('true');
  });

  it('says so when nothing matches, quoting what was typed', async () => {
    await open();
    await type('zzzz');
    expect(document.body.textContent).toContain('“zzzz”');
  });
});

describe('the matcher', () => {
  const list = [
    { id: 'a', label: 'Settings', section: 'Go to' },
    { id: 'b', label: 'Restart all containers', section: 'Stack' },
    { id: 'c', label: 'shop', section: 'Projects', hint: 'settings.test' },
  ];

  it('puts a label that starts with the query above one that merely contains it', () => {
    expect(matchCommands(list, 'set').map((c) => c.id)).toEqual(['a', 'c']);
  });

  it('ranks a hit in the hint below every hit in a label', () => {
    expect(matchCommands(list, 'sett').map((c) => c.id)).toEqual(['a', 'c']);
    expect(matchCommands([list[2], list[0]], 'sett').map((c) => c.id)).toEqual(['a', 'c']);
  });

  /**
   * The reason this is substring and not a subsequence matcher: `rac` is a
   * subsequence of "Restart all containers", and a fuzzy matcher would return
   * it for a query the user meant nothing by.
   */
  it('does not match a scattered subsequence', () => {
    expect(matchCommands(list, 'rac')).toEqual([]);
  });

  it('returns everything for an empty query', () => {
    expect(matchCommands(list, '   ')).toHaveLength(3);
  });
});
