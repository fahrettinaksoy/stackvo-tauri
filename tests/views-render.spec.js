import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';
import tr from '@/i18n/locales/tr.js';

/**
 * The pages, actually mounted.
 *
 * `src/views/` was at **0% coverage** — not one line of any page executed by any
 * test. 9,490 lines of the thing the user actually looks at, verified by
 * nobody. The readiness review put the blame on the two god components, and for
 * `Settings.vue` (3,433 lines) and `ProjectDetail.vue` (3,007) that is right:
 * splitting them is its own work item. It was not right about the rest. Seven
 * pages were always mountable and simply had no test — including `Projects.vue`
 * (1,022 lines) and `Mail.vue` (762), which nobody had tried.
 *
 * ## Why this rather than the E2E run
 *
 * The plan called for `tauri-driver` here. It cannot run on this machine —
 * `tauri-driver` compiles on macOS and then refuses: *"tauri-driver is not
 * supported on this platform"*, because WKWebView has no WebDriver. Writing
 * scenarios that nobody can execute until a Linux runner sees them would be
 * shipping unverified test infrastructure, which is the failure mode this
 * project keeps naming.
 *
 * These do not replace it. They cannot click through a real window, cannot
 * measure contrast, cannot catch a focus trap. What they *can* do is the thing
 * that was actually broken: run the page and prove it renders.
 *
 * ## What each case checks
 *
 * That the page mounts and produces content, against a boundary that answers
 * badly — which is the shape of the bug this exact gap was hiding. `null` from
 * `projects_list` made every inventory computed throw and the window go blank,
 * and it lived in the app-shell suite for months as four unchased "unhandled
 * rejections". A page that renders an empty state is correct; a page that
 * throws is a blank window.
 */

const vuetify = createVuetify({ components, directives });

/** Whatever a test wants the boundary to answer, keyed by camelCase command. */
const replies = {};

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  // The real one, not a stub: it is the guard the stores and views rely on, and
  // a mock that returned the value unchanged would quietly disable the very
  // defence these tests exist to prove.
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

// Event subscriptions are the other ambient dependency every page has. They
// resolve to a teardown function, which is what `onUnmounted` calls.
vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.1.0' }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(), openPath: vi.fn() }));

const About = (await import('@/views/About.vue')).default;
const Dumps = (await import('@/views/Dumps.vue')).default;
const Logs = (await import('@/views/Logs.vue')).default;
const Services = (await import('@/views/Services.vue')).default;
const Dashboard = (await import('@/views/Dashboard.vue')).default;
const Mail = (await import('@/views/Mail.vue')).default;
const Projects = (await import('@/views/Projects.vue')).default;

function router() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
  });
}

/**
 * Mount a page the way the app does: inside `v-app`.
 *
 * Not decoration. Vuetify's layout components — the toolbars and drawers these
 * pages use — resolve an injected layout from a `v-app` ancestor, and mounting
 * a page bare throws "Could not find injected layout". `App.vue` provides it in
 * the real app, so a test that skipped it would be exercising a tree the user
 * never sees.
 */
async function render(component, locale = 'en') {
  const i18n = createI18n({ legacy: false, locale, messages: { en, tr } });
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { Page: component },
      template: '<v-app><Page /></v-app>',
    },
    {
      attachTo: host,
      global: { plugins: [createPinia(), router(), vuetify, i18n] },
    }
  );

  // Let `onMounted`'s awaits settle, which is where every one of these pages
  // talks to the boundary for the first time.
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const PAGES = [
  ['About', About],
  ['Dumps', Dumps],
  ['Logs', Logs],
  ['Services', Services],
  ['Dashboard', Dashboard],
  ['Mail', Mail],
  ['Projects', Projects],
];

beforeEach(() => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('every mountable page', () => {
  it.each(PAGES)('%s renders with a boundary that answers nothing', async (_name, page) => {
    const wrapper = await render(page);
    expect(wrapper.exists()).toBe(true);
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * The failure that was hiding in the gap. Every one of these commands can
   * answer `null` at an untyped boundary, and the page has to survive it.
   */
  it.each(PAGES)('%s renders when every command answers null', async (_name, page) => {
    for (const command of [
      'projectsList',
      'servicesList',
      'engineStatus',
      'preflight',
      'hostStats',
      'dockerSystemResources',
      'workspaceGet',
      'envGet',
      'hostsMissingCore',
    ]) {
      replies[command] = null;
    }

    const wrapper = await render(page);
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * And when it rejects. A dead Docker daemon is the ordinary case for this
   * app, not the exceptional one — the whole reason `engine_status` exists.
   */
  it.each(PAGES)('%s renders when every command rejects', async (_name, page) => {
    const boom = () => Promise.reject(new Error('engine unreachable'));
    for (const command of [
      'projectsList',
      'servicesList',
      'engineStatus',
      'preflight',
      'hostStats',
      'dockerSystemResources',
      'workspaceGet',
      'envGet',
      'hostsMissingCore',
    ]) {
      replies[command] = boom;
    }

    const wrapper = await render(page);
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  /**
   * Turkish is longer than English almost everywhere, and a page that renders
   * in one locale and throws in the other is a page nobody tested in the other.
   */
  it.each(PAGES)('%s renders in Turkish', async (_name, page) => {
    const wrapper = await render(page, 'tr');
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });
});

describe('what the pages show once there is data', () => {
  /** The shape `services_list` really returns — see `commands::Service`. */
  function service(id, extra = {}) {
    return {
      id,
      category: 'database',
      enabled: true,
      running: false,
      built: true,
      version: null,
      containerName: `stackvo-${id}`,
      url: null,
      hostPort: null,
      ports: [],
      credentials: [],
      required: [],
      optional: [],
      unmetDependencies: [],
      ...extra,
    };
  }

  it('Services lists what the boundary returned', async () => {
    replies.servicesList = [
      service('mysql', { running: true, version: '8.4' }),
      service('redis', { category: 'cache', enabled: false }),
    ];

    const wrapper = await render(Services);
    const text = wrapper.text();

    expect(text).toContain('mysql');
    expect(text).toContain('redis');
    // The container name is the column that tells a user what to `docker exec`
    // into, and it is derived rather than typed — worth asserting once.
    expect(text).toContain('stackvo-mysql');
    expect(text).toContain('8.4');

    wrapper.unmount();
  });

  it('About names the app and its version', async () => {
    const wrapper = await render(About);
    const text = wrapper.text();
    expect(text).toContain('StackVo');
    expect(text).toContain('0.1.0');
    wrapper.unmount();
  });
});
