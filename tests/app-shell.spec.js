import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The shell renders what it claims to render.
 *
 * Written because a change to the navigation drawer shipped with its content
 * missing and nothing caught it: lint passes on markup that renders to nothing,
 * `vite build` compiles it happily, and the unit tests never mount a component.
 * The only layer that can see "the drawer came up empty" is one that actually
 * mounts it.
 */

// jsdom has neither, and Vuetify's layout uses both.
globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(), openPath: vi.fn() }));
vi.mock('@/lib/events', () => ({
  listenAll: async () => () => {},
  // The operations store spreads these at bind(); an empty object throws.
  EVENTS: { build: [], generate: [], compose: [], project: [], service: [] },
  REFRESH_TRIGGERS: [],
}));
vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
}));

const { default: App } = await import('@/App.vue');
const { i18n } = await import('@/i18n');

const vuetify = createVuetify({ components, directives });

function mountShell() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/projects', component: { template: '<div />' } },
      { path: '/services', component: { template: '<div />' } },
      { path: '/logs', component: { template: '<div />' } },
      { path: '/settings', component: { template: '<div />' } },
    ],
  });

  return mount(App, {
    global: {
      plugins: [createPinia(), router, vuetify, i18n],
      stubs: {
        // Dialogs teleport to body and are not what this file is about.
        TerminalPanel: true,
        NewProjectDialog: true,
        CloseDialog: true,
        OperationConsole: true,
      },
    },
  });
}

let wrapper;
beforeEach(() => {
  wrapper = mountShell();
});

describe('the navigation drawer', () => {
  it('renders every destination', () => {
    const text = wrapper.text();
    for (const label of [
      'nav.dashboard',
      'nav.projects',
      'nav.services',
      'nav.logs',
      'nav.settings',
    ]) {
      const title = i18n.global.t(label);
      expect(text, `${label} is missing from the drawer`).toContain(title);
    }
  });

  it('renders the quick actions in the app bar', () => {
    // The stack-wide actions moved out of the drawer and into the global app
    // bar in 100a2d4, because they act on everything rather than on a
    // destination. They are icon buttons, so their label is a `title`
    // attribute — invisible to `wrapper.text()`, which is what made the
    // previous version of this test fail for the wrong reason.
    const bar = wrapper.find('.v-app-bar');
    expect(bar.exists(), 'no app bar').toBe(true);

    for (const key of ['quickActions.startAll', 'quickActions.stopAll', 'quickActions.restart']) {
      const label = i18n.global.t(key);
      expect(bar.find(`[title="${label}"]`).exists(), `${key} is missing`).toBe(true);
    }
  });

  it('renders the engine status', () => {
    // Collapsed by default, so the row is the rail variant; either way the
    // container count has to be on screen somewhere.
    expect(wrapper.html()).toContain('mdi-docker');
  });

  it('renders the collapse control', () => {
    const html = wrapper.html();
    expect(html).toMatch(/mdi-chevron-(left|right)/);
  });

  it('pins the status and the collapse control to the floor', () => {
    // Both are fixed chrome, not content: they belong in the drawer's append
    // region, which sits on the floor and outside the scroll area. Only the
    // destinations scroll. The quick actions are no longer asserted here —
    // they live in the app bar now, covered by the test above.
    const drawer = wrapper.find('.nav-drawer');
    expect(drawer.exists()).toBe(true);

    const append = drawer.find('.v-navigation-drawer__append');
    expect(append.exists(), 'the drawer has no append region').toBe(true);

    const footer = append.html();
    expect(footer, 'engine status is not in the footer').toContain('mdi-docker');
    expect(footer, 'the collapse control is not in the footer').toMatch(/mdi-chevron-(left|right)/);
  });

  it('leaves the destinations in the scroll area', () => {
    const content = wrapper.find('.nav-drawer .v-navigation-drawer__content');
    expect(content.exists()).toBe(true);
    expect(content.text()).toContain(i18n.global.t('nav.projects'));
  });

  it('routes when a destination is clicked', async () => {
    const router = wrapper.vm.$router;
    await router.isReady();
    const push = vi.spyOn(router, 'push');

    const items = wrapper.findAll('.nav-drawer .v-list-item');
    const projects = items.find((i) => i.text().includes(i18n.global.t('nav.projects')));
    expect(projects, 'no Projects item to click').toBeTruthy();

    await projects.trigger('click');

    // Asserted on the call rather than the settled route. The handler does not
    // return the navigation promise, so waiting for the route to change means
    // guessing at a timeout — and what this test is protecting is that the
    // click reaches a handler at all, which is exactly what broke.
    expect(push).toHaveBeenCalledWith('/projects');
  });
});

describe('the two left drawers', () => {
  it('both start collapsed', () => {
    // Scoped to the left edge rather than to every drawer in the shell: the
    // "new project" panel is a drawer too now, and it is neither on the left
    // nor a rail — counting it made this assert "all drawers are rails" fail
    // for a reason that has nothing to do with what it is protecting.
    const left = wrapper
      .findAll('.v-navigation-drawer')
      .filter((d) => d.classes().includes('v-navigation-drawer--left'));

    expect(left.length, 'the shell has two left drawers').toBe(2);
    const rails = left.filter((d) => d.classes().includes('v-navigation-drawer--rail'));
    expect(rails.length, 'both drawers should open in rail mode').toBe(left.length);
  });
});
