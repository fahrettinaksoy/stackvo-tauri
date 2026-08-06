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

async function scan(wrapper) {
  const results = await axe(wrapper.element, { rules: RULES });
  wrapper.unmount();
  return results;
}

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

  const PAGES = ['About', 'Dumps', 'Logs', 'Services', 'Dashboard', 'Mail', 'Projects'];

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

    const results = await axe(wrapper.element, { rules: FRAMEWORK });
    wrapper.unmount();
    expect(results).toHaveNoViolations();
    document.body.innerHTML = '';
  });
});
