import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
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
const Dashboard = (await import('@/views/Dashboard.vue')).default;
const Mail = (await import('@/views/Mail.vue')).default;
const Projects = (await import('@/views/Projects.vue')).default;
const ProjectDetail = (await import('@/views/ProjectDetail.vue')).default;

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
async function render(component, locale = 'en', props = {}) {
  const i18n = createI18n({ legacy: false, locale, messages: { en, tr } });
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { Page: component },
      template: '<v-app><Page v-bind="$attrs" /></v-app>',
    },
    {
      attrs: props,
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
  ['Dashboard', Dashboard],
  ['Mail', Mail],
  ['Projects', Projects],
  // Added once §14.16 finished: this took a `name` prop, so it is mounted with
  // one below rather than in this list.
];

/**
 * A window width Vuetify's `useDisplay` will believe.
 *
 * It reads `window.innerWidth` and updates on `resize`, so both have to happen
 * — and before the mount, because the rails decide as their lists render.
 */
async function width(px) {
  window.innerWidth = px;
  window.dispatchEvent(new Event('resize'));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(async () => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
  // jsdom's own default is 1024, which is inside the band where the project
  // rail keeps only its icons — so without this every case here would be
  // asserting against a page in its narrow shape by accident.
  await width(1400);
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
  it('About names the app and its version', async () => {
    const wrapper = await render(About);
    const text = wrapper.text();
    expect(text).toContain('StackVo');
    expect(text).toContain('0.1.0');
    wrapper.unmount();
  });

  /**
   * The licence notice, which is a legal obligation rather than a feature.
   *
   * MIT, BSD, ISC and Apache-2.0 all require the notice to travel with the
   * software, and a `NOTICE.md` in the repository does not travel with a
   * `.dmg`. So the text is compiled into the binary and this window is the only
   * place somebody holding just the app can read it — which makes "the button
   * opens it" the assertion that the obligation is actually met.
   */
  it('About opens the third-party notice compiled into the build', async () => {
    replies.licencesNotice = '# Third-party notices\n\n| bollard | 0.21.0 | Apache-2.0 |';
    const wrapper = await render(About);

    const button = wrapper.findAll('button').find((b) => b.text().includes(en.about.licences));
    expect(button, 'no licences button in the About window').toBeTruthy();

    await button.trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    // The dialog teleports to <body>, so the wrapper's own tree does not hold
    // it — the same reason the shell suite stubs its dialogs out.
    expect(document.body.textContent).toContain('Third-party notices');
    expect(document.body.textContent).toContain('bollard');

    wrapper.unmount();
    delete replies.licencesNotice;
  });

  /**
   * A build that cannot answer must say so. An empty panel reads as "this app
   * has no dependencies", which is the one thing it certainly is not.
   */
  it('About says so when the notice cannot be read', async () => {
    replies.licencesNotice = () => Promise.reject(new Error('no notice'));
    const wrapper = await render(About);

    const button = wrapper.findAll('button').find((b) => b.text().includes(en.about.licences));
    await button.trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(document.body.textContent).toContain(en.about.licencesFailed);

    wrapper.unmount();
    delete replies.licencesNotice;
  });
});

/**
 * `ProjectDetail.vue`, mounted at last.
 *
 * This is the finish line for §14.16. The page was 3,007 lines and could not be
 * mounted at all: one `<script setup>` holding every section's state, so any
 * mount executed all of it. Fourteen panes and their composables later it is
 * 1,092, and the sections it now composes each have their own suite.
 *
 * The point of *these* cases is the composition itself — that the page wires up
 * fourteen children without throwing, and survives a boundary that answers
 * badly, which is precisely what could not be checked before.
 */
describe('the project page', () => {
  const PROJECT = {
    name: 'shop',
    runtime: 'php',
    domain: 'shop.loc',
    domainConfigured: true,
    running: true,
    built: true,
    containerName: 'stackvo-shop',
    path: '/ws/projects/shop',
    manifest: { runtime: 'php', documentRoot: 'public' },
  };

  const seed = () => {
    replies.projectGet = PROJECT;
    replies.projectManifestRead = { runtime: 'php', documentRoot: 'public' };
    replies.containerInspect = {
      name: 'stackvo-shop',
      running: true,
      ports: [],
      networks: [],
      mounts: [],
      env: [],
      restartCount: 0,
    };
    replies.releasePlan = {
      tag: 'shop:1.0.0',
      baseImage: 'php:8.3-fpm-alpine',
      excluded: [['node_modules', 'rebuilt during the image build']],
      warnings: [],
      dockerfile: 'FROM php:8.3-fpm-alpine\n',
    };
    // `profiles`, `trigger`, `bytes`, `directory` — the shape of
    // `commands::ProfilerStatus`, whose `Vec` is never absent.
    replies.profilerStatus = {
      mode: 'profile',
      trigger: 'XDEBUG_TRIGGER=1',
      profiles: [],
      bytes: 0,
      directory: '/ws/projects/shop/.stackvo/profiles',
      xdebug: { running: false, active: false, activeMode: null },
    };
    replies.phpIniStatus = { values: {}, effective: {}, unmanaged: {} };
    replies.xdebugStatus = { enabled: false, needsRebuild: false, active: false };
    replies.tunnelStatus = [];
    replies.workerOptions = [];
    replies.workerStatus = [];
    replies.appLogs = [];
  };

  /**
   * Every section, by clicking the rail — not by mounting and asserting on
   * whatever the default tab happens to be.
   *
   * The first version of this test did exactly that, and passed while five of
   * the six sections were never rendered at all. Breaking a pane on purpose
   * changed nothing, which is how it was caught.
   */
  /**
   * A phrase only that section's panes can produce.
   *
   * Asserting "something rendered" is not enough: the page shell is itself a
   * stack of cards, so a section whose panes all failed still leaves plenty on
   * screen. The first version of this test did that and passed with five of the
   * seven sections never rendered at all.
   */
  const OWN_TEXT = {
    indicator: () => en.projectDetail.composition,
    configuration: () => en.detail.manifest,
    container: () => en.tunnel.title,
    logs: () => en.logs.sources,
    debug: () => en.profiler.title,
    runtime: () => en.phpIni.title,
    release: () => en.release.excluded,
  };

  /**
   * Rebuild is reachable on a project that is running, and it is a different
   * act from restart.
   *
   * It used to be the `v-else` of stop/start, so the hammer disappeared the
   * moment a project had an image — which is the moment it starts mattering,
   * because changing the PHP version or an extension changes the *Dockerfile*.
   * The only route left was stop, rebuild, start, and nothing on screen said so.
   *
   * The three are genuinely different and the test pins that rather than the
   * markup: `project_build` regenerates, builds the image and recreates the
   * container; `project_restart` gives back the same container from the same
   * image; `generate_run` only rewrites files.
   */
  it('offers a rebuild on a running project, and it is not the restart', async () => {
    seed();
    const wrapper = await render(ProjectDetail, 'en', { name: 'shop' });

    const labels = wrapper
      .findAll('button')
      .map((b) => b.attributes('aria-label'))
      .filter(Boolean);

    expect(labels, 'PROJECT.running is true, so this is the case that regressed').toContain(
      en.actions.rebuild
    );
    expect(labels).toContain(en.actions.restart);
    expect(en.actions.rebuild).not.toBe(en.actions.restart);

    // And it draws something. The first version of this button used v-btn's
    // `icon="mdi-…"` prop with a tooltip in the default slot, which Vuetify
    // reads as "the slot wins" — a correctly labelled, correctly wired,
    // completely blank button. Every assertion above passed on it.
    const rebuild = wrapper
      .findAll('button')
      .find((b) => b.attributes('aria-label') === en.actions.rebuild);
    expect(rebuild.find('.mdi-hammer-wrench').exists(), 'the rebuild button drew no icon').toBe(
      true
    );

    wrapper.unmount();
  });

  it('renders every section in the rail', async () => {
    seed();
    const wrapper = await render(ProjectDetail, 'en', { name: 'shop' });

    const rail = wrapper.findAll('.detail-nav .nav-item');
    expect(rail.length, 'a PHP project offers every section').toBe(7);
    expect(Object.keys(OWN_TEXT)).toHaveLength(rail.length);

    for (const key of Object.keys(OWN_TEXT)) {
      const item = rail.find((i) => i.text().includes(en.projectDetail[key] ?? '\u0000'));
      await (item ?? rail[Object.keys(OWN_TEXT).indexOf(key)]).trigger('click');
      await new Promise((resolve) => setTimeout(resolve, 0));
      await wrapper.vm.$nextTick();

      expect(wrapper.text(), `the ${key} section rendered nothing of its own`).toContain(
        OWN_TEXT[key]()
      );
    }

    wrapper.unmount();
  });

  /**
   * The rail is built from the runtime: a Node project has a dev server and no
   * Xdebug, so it must not offer a Debug tab that would open onto nothing.
   */
  it('offers a Node project a different rail', async () => {
    seed();
    replies.projectGet = { ...PROJECT, runtime: 'node' };

    const wrapper = await render(ProjectDetail, 'en', { name: 'shop' });
    const labels = wrapper.findAll('.detail-nav .nav-item').map((i) => i.text());

    expect(labels).not.toContain(en.projectDetail.debug);
    expect(labels).toContain(en.projectDetail.runtime);
    wrapper.unmount();
  });

  /**
   * The rail drops its labels before the pane loses its width.
   *
   * 240px of "Çalışma zamanı ayarları" is a third of a narrow window spent on
   * navigation, and the entry that says the most is the first one an ellipsis
   * eats. The labels are not hidden with CSS: they are not rendered, and the
   * accessible name moves to `aria-label` in the same breath — a `display:
   * none` title is absent from the accessible name too, which would leave a
   * column of buttons called nothing.
   */
  it('keeps only the icons in the rail when the window is narrow', async () => {
    seed();
    await width(950);
    const wrapper = await render(ProjectDetail, 'en', { name: 'shop' });

    const nav = wrapper.get('.detail-nav');
    expect(nav.classes()).toContain('detail-nav--rail');

    const items = nav.findAll('.nav-item');
    expect(items.length).toBe(7);
    expect(nav.text()).not.toContain(en.projectDetail.runtime);
    expect(items.map((i) => i.attributes('aria-label'))).toContain(en.projectDetail.runtime);
    expect(items.every((i) => i.attributes('aria-label')?.trim())).toBe(true);

    // The icon keeps a `.v-list-item__spacer` behind it — the 32px that used
    // to sit between icon and label, an element rather than a margin, so it
    // outlives the label. Left alone it holds the prepend column wider than
    // the item and the icon hangs over the edge of its own highlight. jsdom
    // measures nothing, so the assertion is that the rail says something.
    expect(items[0].find('.v-list-item__spacer').exists()).toBe(true);
    const source = readFileSync('src/views/ProjectDetail.vue', 'utf8');
    const rail = source.slice(source.indexOf('.detail-nav--rail :deep(.v-list-item)'));
    expect(rail).toMatch(/--v-list-prepend-gap:\s*0/);
    expect(rail).toMatch(/grid-column:\s*1\s*\/\s*-1/);

    wrapper.unmount();
  });

  /** The state that used to be unreachable: a project that is not there. */
  it('renders when the project cannot be read', async () => {
    replies.projectGet = () => Promise.reject(new Error('no such project'));

    const wrapper = await render(ProjectDetail, 'en', { name: 'gone' });
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it('renders when every command answers null', async () => {
    for (const command of ['projectGet', 'projectManifestRead', 'containerInspect']) {
      replies[command] = null;
    }

    const wrapper = await render(ProjectDetail, 'en', { name: 'shop' });
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it('renders in Turkish', async () => {
    replies.projectGet = PROJECT;

    const wrapper = await render(ProjectDetail, 'tr', { name: 'shop' });
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });
});
