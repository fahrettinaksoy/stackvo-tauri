import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import { axe } from 'vitest-axe';
// The matcher ships from its own entry point, not the package root — the root
// exports `axe` and `configureAxe` only.
import * as matchers from 'vitest-axe/matchers';

import ErrorAlert from '@/components/ErrorAlert.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';
import SettingsSection from '@/components/SettingsSection.vue';
import StatCard from '@/components/StatCard.vue';
import en from '@/i18n/locales/en.js';
import tr from '@/i18n/locales/tr.js';

/**
 * Rules a machine can check, on components that are really mounted.
 *
 * `a11y.spec.js` beside this file greps the sources for icon buttons with no
 * accessible name. That is a good test and it is one rule, checked against
 * text. This one runs axe over the rendered DOM, so it covers the rules that
 * only exist once a component has rendered: roles that only make sense in the
 * tree they ended up in, form controls and their labels, ARIA attributes
 * pointing at ids that exist, and names on the elements a framework generates
 * rather than the author.
 *
 * It earned its place on the first run, which is the only endorsement worth
 * quoting: `StatCard`'s meter had `role="progressbar"` and `aria-valuenow` and
 * no name, so a dashboard of four of them announced four bare numbers to a
 * screen reader. `BootstrapGate` had the same gap on the first-run screen.
 * Neither is visible in the source without knowing what Vuetify emits.
 *
 * ## Which pages, and which not
 *
 * `Settings.vue` (3,433 lines) and `ProjectDetail.vue` (3,007) still cannot be
 * mounted — that is the §2.3 finding and splitting them is its own work item.
 * The other seven always could, and `tests/views-render.spec.js` now mounts
 * them, so they are scanned here too. This file said it was "the reason to add
 * to that list"; this is that.
 *
 * ## Why axe is not the whole of accessibility
 *
 * It never is — axe finds roughly a third of what a manual audit does, and the
 * things it cannot see are the ones this app is most exposed to: whether the
 * operation console announces streaming output to a screen reader, whether a
 * drawer traps focus, whether the whole app is reachable from a keyboard. Those
 * need the E2E run this project does not have yet. An automated pass is the
 * floor, not the ceiling, and a green run here is not a VPAT.
 */

expect.extend(matchers);

const vuetify = createVuetify({ components, directives });
const i18n = createI18n({ legacy: false, locale: 'en', messages: { en, tr } });

/**
 * Vuetify renders overlays and menus into a teleport target that has to exist
 * before mount, and axe reads the document, so the mounted tree has to be
 * attached rather than detached in memory.
 */
function render(component, options = {}) {
  const host = document.createElement('div');
  document.body.appendChild(host);

  return mount(component, {
    attachTo: host,
    global: { plugins: [vuetify, i18n] },
    ...options,
  });
}

/**
 * Only the rules this project has decided to hold itself to.
 *
 * Left on: everything about names, roles, labels and contrast. Turned off:
 * `region`, which wants every piece of content inside a landmark — true of a
 * page, meaningless for a component mounted on its own, and it would fire on
 * every single case here for a reason that is an artefact of the test.
 */
const RULES = {
  region: { enabled: false },

  // **Off because it cannot run here, not because it does not matter.**
  //
  // axe measures contrast by painting to a canvas, and jsdom has no canvas —
  // it logs "Not implemented: HTMLCanvasElement.getContext" and the rule
  // reports nothing. Left enabled it would pass on every component for ever
  // while checking nothing, which is worse than not running it: a green suite
  // that claims a guarantee it has not made.
  //
  // Contrast is the rule this app most needs checked, too, because
  // `appearance.js` derives the theme from the OS accent colour — so the
  // palette is not fixed and cannot be audited once by hand. It needs a real
  // browser, which means the E2E run in §14.12. Named here so that work has a
  // reason attached to it.
  'color-contrast': { enabled: false },
};

async function scan(wrapper, rules = RULES) {
  const results = await axe(wrapper.element, { rules });
  wrapper.unmount();
  return results;
}

/**
 * `v-slider` renders a hidden `<input tabindex="-1">` beside the real
 * `role="slider"` control, purely so the slider carries a value in a form. It
 * has no label and cannot be given one — Vuetify forwards nothing to it — and
 * it is not focusable, so no keyboard or screen-reader user ever meets it.
 *
 * Scoped to the one pane that has sliders rather than turned off everywhere:
 * `label` is the rule that catches a genuinely unlabelled field, and it caught
 * two in `LogView` and `DumpView`. The sliders' *real* control is named — see
 * the `aria-label` on each in `AppearancePane.vue`, added because axe reported
 * "slider, 12" with nothing saying what was at 12.
 */
const SLIDER_HOST = { ...RULES, label: { enabled: false } };

describe('axe over the components that can be mounted', () => {
  /**
   * The error surface, in every shape it is documented to accept. It is the one
   * component a user meets at their worst moment, so an unreadable colour or an
   * unannounced role costs more here than anywhere else.
   */
  it.each([
    ['a StackVo error', { code: 'NotFound', message: 'shop is not a directory' }],
    ['a plain string, which is what a plugin rejects with', 'opener.open_path not allowed'],
    ['an object with no message', { reason: 'forbidden' }],
    ['an error carrying a hint', { code: 'InvalidInput', message: 'bad name', hint: 'Try again.' }],
  ])('ErrorAlert has no violations with %s', async (_label, error) => {
    expect(await scan(render(ErrorAlert, { props: { error } }))).toHaveNoViolations();
  });

  it('SettingsGroup has no violations', async () => {
    const wrapper = render(SettingsGroup, {
      props: { title: 'Appearance', icon: 'mdi-palette' },
      slots: { default: '<p>Body copy</p>' },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
  });

  it('SettingsSection has no violations', async () => {
    const wrapper = render(SettingsSection, {
      props: { title: 'Theme', subtitle: 'How the app looks' },
      slots: { default: '<p>Body copy</p>' },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
  });

  /**
   * With the meter, which is the case that found a real defect on this file's
   * first run: Vuetify's `v-progress-linear` renders `role="progressbar"` and
   * `aria-valuenow` and no name, so four of these on the dashboard announced
   * four bare numbers.
   */
  it('StatCard has no violations, meter and all', async () => {
    const wrapper = render(StatCard, {
      props: {
        title: 'CPU',
        icon: 'mdi-chip',
        value: 42,
        primary: '42%',
        secondary: '8 cores',
        details: [{ label: 'Load', value: '1.20' }],
      },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
  });

  /** And without it — the card is documented to work with no meter at all. */
  it('StatCard has no violations without a meter', async () => {
    const wrapper = render(StatCard, {
      props: { title: 'Containers', primary: '7 running' },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
  });

  /**
   * Turkish is not a spot check. It is longer than English almost everywhere,
   * and a label that wraps or truncates differently can change what a screen
   * reader is given — this app ships two locales and tests one.
   */
  it('ErrorAlert has no violations in Turkish', async () => {
    i18n.global.locale.value = 'tr';
    try {
      const wrapper = render(ErrorAlert, {
        props: { error: { code: 'NotFound', message: 'shop bir dizin değil' } },
      });
      expect(await scan(wrapper)).toHaveNoViolations();
    } finally {
      i18n.global.locale.value = 'en';
    }
  });
});

/**
 * The pages, once `views-render.spec.js` showed they mount.
 *
 * A component in isolation can be faultless and still produce a page with two
 * `<h1>`s, a landmark that repeats, or a control whose label only makes sense
 * beside a sibling it does not have. Those only exist at page scale, and they
 * are the ones a reviewer notices first.
 */
describe('axe over the pages that can be mounted', () => {
  const replies = {};

  const PAGES = ['About', 'Dumps', 'Logs', 'Dashboard', 'Mail', 'Projects'];

  /**
   * Rules Vuetify's own markup breaks, on top of the shared set.
   *
   * `v-data-table` renders a loader row unconditionally —
   * `VDataTableHeaders.js` builds a `<th colspan="{columns + 1}">` holding a
   * progress indicator, and when nothing is loading that `<th>` is empty and
   * that indicator has no name. Both are genuine findings and neither is
   * authored here: no prop, slot or class controls the row.
   *
   * Scoped to the page scans on purpose. The component scans above keep both
   * rules on, which is where they earned their place — `aria-progressbar-name`
   * is exactly the rule that caught `StatCard` and `BootstrapGate`. Turning it
   * off everywhere to silence a framework artefact would have thrown away the
   * finding that justified the whole file.
   */
  const FRAMEWORK = {
    ...RULES,
    'empty-table-header': { enabled: false },
    'aria-progressbar-name': { enabled: false },
  };

  it.each(PAGES)('%s has no violations', async (name) => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy({}, { get: () => () => Promise.resolve(replies[name]) }),
    }));
    vi.doMock('@/lib/events', async (importOriginal) => ({
      ...(await importOriginal()),
      listenAll: async () => () => {},
      listen: async () => () => {},
    }));
    vi.doMock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.1.0' }));
    vi.doMock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(), openPath: vi.fn() }));

    const { createPinia } = await import('pinia');
    const { createRouter, createMemoryHistory } = await import('vue-router');
    const page = (await import(`@/views/${name}.vue`)).default;

    const host = document.createElement('div');
    document.body.appendChild(host);

    const wrapper = mount(
      { components: { Page: page }, template: '<v-app><Page /></v-app>' },
      {
        attachTo: host,
        global: {
          plugins: [
            createPinia(),
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

    expect(await scan(wrapper, FRAMEWORK)).toHaveNoViolations();
    document.body.innerHTML = '';
  });
});

/**
 * The Settings panes extracted in §14.16.
 *
 * Mounted **with data**, through a mocked boundary. Scanning an empty pane is
 * scanning the empty state and calling it the pane: most of the markup is
 * behind a `v-if`, and Vuetify's own combobox markup is incomplete when a
 * select has no items to open — which reads as an app defect and is not one.
 * The list, the chips and the buttons are the part worth checking.
 */
/**
 * The panes coming out of `ProjectDetail.vue`.
 *
 * Same reasoning as the Settings block below: scanned with data, because most
 * of the markup is behind a `v-if` on there being a sample at all.
 */
describe('axe over the extracted project panes', () => {
  it('IndicatorPane has no violations', async () => {
    const IndicatorPane = (await import('@/components/project/IndicatorPane.vue')).default;
    const host = document.createElement('div');
    document.body.appendChild(host);

    const pie = [
      { key: 'a', title: 'A', value: 1, color: '#1976D2' },
      { key: 'b', title: 'B', value: 2, color: '#2A313C' },
    ];
    const wrapper = mount(
      {
        components: { IndicatorPane },
        template: '<v-app><IndicatorPane v-bind="$attrs" /></v-app>',
      },
      {
        attrs: {
          running: true,
          stats: { cpuPercent: 12, memoryUsed: 1, memoryLimit: 2, netRx: 1, netTx: 1 },
          cpuSeries: [1, 2, 3],
          memoryPie: pie,
          networkPie: pie,
          diskPie: pie,
          heatmap: [{ label: new Date('2026-08-06T00:00:00'), hours: Array(24).fill(5) }],
        },
        attachTo: host,
        global: { plugins: [vuetify, i18n] },
      }
    );

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('ReleasePane has no violations', async () => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy(
        {},
        {
          get: () => () =>
            Promise.resolve({
              tag: 'shop:1.4.0',
              baseImage: 'php:8.3-fpm-alpine',
              excluded: [['node_modules', 'rebuilt during the image build']],
              warnings: ['no .dockerignore found'],
              dockerfile: 'FROM php:8.3-fpm-alpine\n',
            }),
        }
      ),
    }));
    const ReleasePane = (await import('@/components/project/ReleasePane.vue')).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    // Scanned with a plan, not empty: the table, the accordion and the tag
    // field only exist once one has been read, and they are the whole pane.
    const wrapper = mount(
      {
        components: { ReleasePane },
        template: '<v-app><ReleasePane name="shop" /></v-app>',
      },
      { attachTo: host, global: { plugins: [vuetify, i18n] } }
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it.each([
    [
      'DevServerPane',
      'node',
      {
        devserverStatus: {
          enabled: true,
          mounted: true,
          command: 'npm run dev -- --host',
          productionCommand: 'npm start',
          hostAllowed: false,
          needsRecreate: true,
          snippet: "server: { allowedHosts: ['shop.loc'] }",
        },
      },
    ],
    [
      'PhpIniPane',
      'php',
      {
        phpIniStatus: {
          values: { memory_limit: '256M' },
          effective: { memory_limit: '128M' },
          unmanaged: { opcache_enable: '1' },
          warning: 'post_max_size is below upload_max_filesize',
          path: '/ws/projects/shop/php.ini',
        },
      },
    ],
  ])('%s has no violations', async (name, runtime, seed) => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy({}, { get: (_t, key) => () => Promise.resolve(seed[key]) }),
    }));
    const Pane = (await import(`@/components/project/${name}.vue`)).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    // Every alert and the snippet block are behind a `v-if` on state, so the
    // seed above deliberately turns all of them on at once.
    const wrapper = mount(
      {
        components: { Pane },
        template: `<v-app><Pane name="shop" runtime="${runtime}" /></v-app>`,
      },
      { attachTo: host, global: { plugins: [vuetify, i18n] } }
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it.each([
    [
      'TunnelPane',
      {
        tunnelStatus: [
          { project: 'shop', running: true, url: 'https://loud-fox.trycloudflare.com' },
        ],
      },
    ],
    [
      'RequirementsPane',
      {
        projectRequirements: {
          declared: [
            { id: 'mysql', known: true, enabled: true },
            { id: 'redis', known: true, enabled: false },
            { id: 'postgress', known: false, enabled: false },
          ],
          suggested: [{ service: 'meilisearch', key: 'SCOUT_DRIVER' }],
          plan: { changes: [], needsRegenerate: false },
        },
      },
    ],
    [
      'WorkersPane',
      {
        workerOptions: ['queue', 'scheduler'],
        workerStatus: [{ project: 'shop', kind: 'queue', restarts: 3 }],
      },
    ],
  ])('%s has no violations', async (name, seed) => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy({}, { get: (_t, key) => () => Promise.resolve(seed[key]) }),
    }));
    const Pane = (await import(`@/components/project/${name}.vue`)).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    const wrapper = mount(
      {
        components: { Pane },
        template: '<v-app><Pane name="shop" :running="true" /></v-app>',
      },
      { attachTo: host, global: { plugins: [vuetify, i18n] } }
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('ContainerPane has no violations', async () => {
    const ContainerPane = (await import('@/components/project/ContainerPane.vue')).default;
    const host = document.createElement('div');
    document.body.appendChild(host);

    const wrapper = mount(
      {
        components: { ContainerPane },
        template: '<v-app><ContainerPane v-bind="$attrs" /></v-app>',
      },
      {
        attrs: {
          project: { built: true, containerName: 'stackvo-shop' },
          details: {
            name: 'stackvo-shop',
            id: 'sha256:deadbeef',
            image: 'stackvo/php:8.3',
            imageSize: 1024,
            state: 'running',
            running: true,
            ports: [{ container: 80, host: 8080 }],
            networks: ['stackvo-net'],
            gateway: '172.20.0.1',
            restartCount: 0,
            restartPolicy: 'unless-stopped',
            startedAt: '2026-08-06T09:00:00Z',
            created: '2026-08-01T12:00:00Z',
          },
          running: true,
        },
        attachTo: host,
        global: { plugins: [vuetify, i18n] },
      }
    );

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it.each([
    [
      'XdebugPane',
      'php',
      { xdebugStatus: { enabled: true, needsRebuild: true, active: false, running: true } },
    ],
    [
      'ProfilerPane',
      'php',
      {
        profilerStatus: {
          mode: 'profile',
          trigger: 'XDEBUG_TRIGGER=1',
          bytes: 4096,
          directory: '/ws/projects/shop/.stackvo/profiles',
          profiles: [
            {
              id: 'cachegrind.out.1',
              name: 'cachegrind.out.1',
              bytes: 4096,
              recordedAt: 1_786_007_730,
            },
          ],
          xdebug: { running: true, active: true, activeMode: 'debug' },
        },
      },
    ],
  ])('%s has no violations', async (name, runtime, seed) => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy({}, { get: (_t, key) => () => Promise.resolve(seed[key]) }),
    }));
    const { createPinia } = await import('pinia');
    const Pane = (await import(`@/components/project/${name}.vue`)).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    const wrapper = mount(
      {
        components: { Pane },
        template: `<v-app><Pane name="shop" runtime="${runtime}" :running="true" /></v-app>`,
      },
      { attachTo: host, global: { plugins: [createPinia(), vuetify, i18n] } }
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('DockerfilePane has no violations', async () => {
    vi.resetModules();
    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy(
        {},
        {
          get: () => () =>
            Promise.resolve({
              dockerfile: 'FROM php:8.3-fpm-alpine\nCOPY . /app\n',
              matches: false,
              differences: ['pdo_mysql is dropped by the Bash generator'],
            }),
        }
      ),
    }));
    const DockerfilePane = (await import('@/components/project/DockerfilePane.vue')).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    const wrapper = mount(
      {
        components: { DockerfilePane },
        template: '<v-app><DockerfilePane name="shop" /></v-app>',
      },
      { attachTo: host, global: { plugins: [vuetify, i18n] } }
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('ManifestPane has no violations', async () => {
    const { createPinia } = await import('pinia');
    const ManifestPane = (await import('@/components/project/ManifestPane.vue')).default;

    const host = document.createElement('div');
    document.body.appendChild(host);
    // A bare textarea with a heading beside it rather than a label is exactly
    // the shape axe exists to catch, so it is scanned with content in it.
    const wrapper = mount(
      {
        components: { ManifestPane },
        template:
          '<v-app><ManifestPane name="shop" :dirty="true" model-value=\'{ "runtime": "php" }\' /></v-app>',
      },
      { attachTo: host, global: { plugins: [createPinia(), vuetify, i18n] } }
    );

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('LogsPane says so when the project has not been built', async () => {
    const LogsPane = (await import('@/components/project/LogsPane.vue')).default;
    const host = document.createElement('div');
    document.body.appendChild(host);

    const wrapper = mount(
      {
        components: { LogsPane },
        template: '<v-app><LogsPane :project="{ built: false }" name="shop" /></v-app>',
      },
      { attachTo: host, global: { plugins: [vuetify, i18n] } }
    );

    expect(wrapper.text()).toContain(en.detail.notBuilt);
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });
});

describe('axe over the extracted Settings panes', () => {
  const replies = {};

  async function renderPane(name, seed) {
    vi.resetModules();
    Object.keys(replies).forEach((k) => delete replies[k]);
    Object.assign(replies, seed);

    vi.doMock('@/lib/ipc', () => ({
      StackvoError: class extends Error {},
      call: vi.fn(),
      asList: (value) => (Array.isArray(value) ? value : []),
      api: new Proxy({}, { get: (_t, key) => () => Promise.resolve(replies[key]) }),
    }));

    const { createPinia } = await import('pinia');
    const pane = (await import(`@/components/settings/${name}.vue`)).default;

    const host = document.createElement('div');
    document.body.appendChild(host);

    // Inside `v-app`, like the view. Vuetify's drawers and sheets resolve an
    // injected layout from it, and a pane that holds one throws without it.
    const wrapper = mount(
      { components: { Pane: pane }, template: '<v-app><Pane /></v-app>' },
      { attachTo: host, global: { plugins: [createPinia(), vuetify, i18n] } }
    );

    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    return wrapper;
  }

  it('CertificatesPane has no violations', async () => {
    const wrapper = await renderPane('CertificatesPane', {
      certStatus: {
        sslEnabled: true,
        stale: true,
        caTrusted: false,
        mkcertAvailable: true,
        notAfter: 1_800_000_000,
        daysRemaining: 90,
        missing: ['new.loc'],
        rejected: [],
        covered: ['stackvo.loc'],
        certPath: '/ws/certs/wildcard.pem',
        caPath: '/ws/ca/rootCA.pem',
      },
      certPlan: { remove: ['gone.loc'] },
    });

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('DomainPane has no violations', async () => {
    const wrapper = await renderPane('DomainPane', {
      envGet: { DEFAULT_TLD_SUFFIX: 'stackvo.loc', DOCKER_DEFAULT_NETWORK: 'stackvo-net' },
      envDefaults: {},
      hostsOverview: {
        entries: [
          { domain: 'shop.loc', configured: true },
          { domain: 'blog.loc', configured: false },
        ],
        stale: ['deleted.loc'],
      },
      containerInspect: { running: true, image: 'traefik:v3', ports: [{ host: 80 }] },
    });

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it.each([
    [
      'PreferencesPane',
      { prefsGet: {}, appsAvailable: { terminals: [], editors: [], browsers: [] } },
    ],
    ['DiagnosticsPane', { logsInfo: { directory: '/logs', newestFile: null, totalBytes: 0 } }],
  ])('%s has no violations', async (name, seed) => {
    const wrapper = await renderPane(name, seed);
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('ServerLimitsPane has no violations', async () => {
    const wrapper = await renderPane('ServerLimitsPane', {
      envGet: { SERVER_MAX_BODY_SIZE: '64m', SERVER_GZIP: 'on' },
      envDefaults: {},
      catalogGet: { runtimes: [], servers: ['nginx', 'caddy'] },
      serverConfigGet: '# nginx\n',
    });
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('LocalisationPane has no violations', async () => {
    const wrapper = await renderPane('LocalisationPane', {});
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('AgentsPane has no violations', async () => {
    const wrapper = await renderPane('AgentsPane', {
      agentsStatus: {
        binary: '/opt/stackvo/stackvo-mcp',
        source: 'build',
        root: '/Users/x/.stackvo',
        clients: [
          {
            id: 'cursor',
            label: 'Cursor',
            path: '/Users/x/.cursor/mcp.json',
            present: true,
            exists: true,
            parseable: true,
            command: null,
            current: false,
          },
        ],
      },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('PhpPane has no violations', async () => {
    const wrapper = await renderPane('PhpPane', {
      envGet: { SUPPORTED_LANGUAGES_PHP_DEFAULT: '8.4' },
      envDefaults: {},
      catalogGet: { runtimes: [{ id: 'php', versions: ['8.3', '8.4'] }], servers: ['nginx'] },
    });
    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('AppearancePane has no violations', async () => {
    const wrapper = await renderPane('AppearancePane', {});
    expect(await scan(wrapper, SLIDER_HOST)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('WorkspacePane has no violations', async () => {
    const wrapper = await renderPane('WorkspacePane', {
      presetExport: { name: 'team', services: { mysql: { enabled: true } } },
      templatesList: [{ path: 'core/servers/nginx.conf', overridden: true }],
    });

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });

  it('TemplateOverridesPane has no violations', async () => {
    const wrapper = await renderPane('TemplateOverridesPane', {
      templatesList: [
        { path: 'core/servers/nginx.conf', overridden: true },
        { path: 'services/redis/docker-compose.redis.tpl', overridden: false },
      ],
    });

    expect(await scan(wrapper)).toHaveNoViolations();
    document.body.innerHTML = '';
  });
});
