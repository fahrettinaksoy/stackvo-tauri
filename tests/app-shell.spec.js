import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
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
/**
 * The one command whose answer changes what the shell renders. Everything else
 * can stay null; `preflight` decides between the app and the gate, so it needs
 * to be steerable from a test.
 */
const ipc = vi.hoisted(() => ({ preflight: null }));

/**
 * Per-command stubs, keyed by the Rust command name.
 *
 * Set one to assert on *which* command a button reaches for. The hosts row
 * offers a narrower set than the dashboard does, and the difference between
 * them is only visible at this layer — both return a list of domains.
 */
const calls = vi.hoisted(() => ({}));

vi.mock('@/lib/ipc', () => ({
  // The real guard, not a stub — see views-render.spec.js.
  asList: (value) => (Array.isArray(value) ? value : []),
  StackvoError: class extends Error {},
  call: vi.fn(),
  api: new Proxy(
    {},
    {
      get:
        (_target, name) =>
        (...args) => {
          // `api.hostsMissingCore` → `hosts_missing_core`, the name the stub
          // table is keyed by and the one the Rust side registers.
          const command = String(name).replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
          if (calls[command]) return Promise.resolve(calls[command](...args));
          return Promise.resolve(command === 'preflight' ? ipc.preflight : null);
        },
    }
  ),
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

  it('keeps the empty-projects sentence out of the rail', () => {
    // Vuetify hides a `v-list-item-title` in a rail; this empty state is plain
    // markup inside the list, so nothing hid it. "Henüz proje yok." wrapped to
    // three lines inside 66px minus 48px of padding and read as a broken
    // render. The icon stays and the words move to a tooltip.
    const sentence = i18n.global.t('projects.empty');
    expect(wrapper.text(), 'the sentence is in the collapsed rail').not.toContain(sentence);
    expect(wrapper.html(), 'the empty state lost its icon too').toContain('mdi-folder-off-outline');
  });
});

/**
 * What the window shows when the app cannot run yet.
 *
 * The gate used to render *inside* the full shell, so a machine with no
 * workspace and no daemon still got a start-everything button, a stop button
 * and an empty project list around the card explaining why none of them could
 * work. Every one of those controls acts on the thing that is missing.
 */
describe('the requirements gate', () => {
  const REPORT = {
    os: 'macos',
    ready: false,
    requirements: [
      { id: 'workspace', state: 'fail', detail: null, fixable: true },
      {
        id: 'engine',
        state: 'fail',
        detail: 'No Docker socket found on this machine.',
        fixable: true,
      },
      { id: 'compose', state: 'ok', detail: '5.3.1', fixable: false },
      { id: 'network', state: 'unknown', detail: 'stackvo-net', fixable: false },
      { id: 'hosts', state: 'unknown', detail: null, fixable: false },
    ],
  };

  let gate;
  beforeEach(async () => {
    ipc.preflight = REPORT;
    gate = mountShell();
    await flushPromises();
  });

  afterEach(() => {
    ipc.preflight = null;
    for (const key of Object.keys(calls)) delete calls[key];
  });

  it('is the only thing on the window', () => {
    expect(gate.find('.gate').exists(), 'the gate did not render').toBe(true);
    expect(gate.find('.v-app-bar').exists(), 'the app bar is still up').toBe(false);
    expect(gate.findAll('.v-navigation-drawer').length, 'a rail is still up').toBe(0);
  });

  it('numbers the steps and marks the first actionable one', () => {
    const steps = gate.findAll('.step');
    expect(steps.length).toBe(REPORT.requirements.length);

    // Position 1 failed, so it carries the "next step" marker; the settled
    // compose row shows a tick instead of its number.
    expect(steps[0].classes()).toContain('is-current');
    expect(steps[0].text()).toContain(i18n.global.t('preflight.nextStep'));
    expect(steps[1].classes()).not.toContain('is-current');
    expect(steps[2].classes()).toContain('is-done');
  });

  it('offers a button for every step the app can settle itself', () => {
    const steps = gate.findAll('.step');
    for (const [index, requirement] of REPORT.requirements.entries()) {
      const action = requirement.fixable && requirement.state === 'fail';
      expect(
        steps[index].find('button').exists(),
        `${requirement.id} button should be ${action}`
      ).toBe(action);
    }
  });

  it('repeats the current action as the primary call to action', () => {
    // Two buttons, one label: the row's and the big one under the card. A
    // person scanning for "what do I press" should not have to read five rows
    // to find it.
    const label = i18n.global.t('preflight.workspaceAction');
    const matching = gate.findAll('button').filter((b) => b.text().includes(label));
    expect(matching.length, 'the call to action is missing').toBeGreaterThanOrEqual(2);
  });

  it('says which failures nobody can press a button for', async () => {
    ipc.preflight = {
      os: 'macos',
      ready: false,
      requirements: [{ id: 'compose', state: 'fail', detail: '1.29.2', fixable: false }],
    };
    const manual = mountShell();
    await flushPromises();

    expect(manual.text()).toContain(i18n.global.t('preflight.manual'));
    expect(manual.text()).toContain(i18n.global.t('preflight.composeHint.macos'));

    // "Install it yourself" without the page that says how is not an
    // instruction. Docker's own, not StackVo's — it is their plugin.
    expect(manual.text()).toContain(i18n.global.t('preflight.help'));
  });

  it('explains a warning and offers its button', async () => {
    // `ready` ignores warnings, so one can be outstanding while the screen is
    // up for another reason. Rendered as a bare title with no hint and no
    // button — which is how mkcert was listed — the row states a problem and
    // withholds the fix.
    ipc.preflight = {
      os: 'macos',
      ready: false,
      requirements: [
        { id: 'workspace', state: 'fail', detail: null, fixable: true },
        { id: 'hosts', state: 'warn', detail: 'stackvo.loc, traefik.stackvo.loc', fixable: true },
      ],
    };
    const warned = mountShell();
    await flushPromises();

    const row = warned.findAll('.step')[1];
    expect(row.text(), 'no explanation').toContain(i18n.global.t('preflight.hostsHint.macos'));
    expect(row.text(), 'no button').toContain(i18n.global.t('preflight.hostsAction'));
    expect(row.text(), 'the domains themselves are missing').toContain('traefik.stackvo.loc');

    // A warning is not the next step — the failure above it is.
    expect(row.classes()).not.toContain('is-current');
  });

  /// A prompt opened for two entries must not write four.
  it('offers only the entries its own row blocks on', async () => {
    // The row blocks on the two names the stack is addressed through. Its
    // button used to ask for `hosts_missing`, which is every missing name
    // anywhere — so a machine that needed `stackvo.loc` and
    // `traefik.stackvo.loc` had `phpmyadmin.stackvo.loc` and
    // `rabbitmq.stackvo.loc` written into /etc/hosts alongside them, from a
    // password prompt raised for something else.
    const asked = [];
    calls.hosts_missing = () => {
      asked.push('hosts_missing');
      return ['stackvo.loc', 'traefik.stackvo.loc', 'phpmyadmin.stackvo.loc'];
    };
    calls.hosts_missing_core = () => {
      asked.push('hosts_missing_core');
      return ['stackvo.loc', 'traefik.stackvo.loc'];
    };

    ipc.preflight = {
      os: 'macos',
      ready: false,
      requirements: [
        { id: 'hosts', state: 'fail', detail: 'stackvo.loc, traefik.stackvo.loc', fixable: true },
      ],
    };
    const gate = mountShell();
    await flushPromises();

    const button = gate
      .findAll('button')
      .find((b) => b.text().includes(i18n.global.t('preflight.hostsAction')));
    expect(button, 'no hosts button').toBeTruthy();

    await button.trigger('click');
    await flushPromises();

    expect(asked).toEqual(['hosts_missing_core']);
    expect(asked).not.toContain('hosts_missing');
  });
});

/**
 * The screen between "it can run" and running it.
 *
 * The requirements gate answers whether the machine is ready; it does not make
 * the stack exist. On the first launch that gap was the whole experience —
 * every check green, and the dashboard opening behind a proxy that had never
 * been started, on compose files that had never been written.
 */
describe('the bootstrap gate', () => {
  const READY = { os: 'macos', ready: true, requirements: [] };

  beforeEach(() => {
    // The file-level `beforeEach` has already mounted a shell, and its boot is
    // still in flight when these tests install their stubs — so it would pick
    // them up and run the bootstrap a second time. Counting calls only works
    // with one shell in the room.
    wrapper.unmount();
  });

  afterEach(() => {
    ipc.preflight = null;
    for (const key of Object.keys(calls)) delete calls[key];
  });

  it('sets the stack up before the app is shown, once', async () => {
    ipc.preflight = READY;
    const done = [];
    calls.workspace_get = () => ({
      root: '/tmp/app',
      projectsDir: '/tmp/code',
      valid: true,
      bootstrapped: false,
      source: 'stored',
    });
    calls.generate_run = () => {
      done.push('generate');
      return 'op-1';
    };
    // The stub replaces `api.composeUp` itself, so it sees that wrapper's own
    // positional arguments rather than the `{ mode }` object it would forward.
    calls.compose_up = (mode) => {
      done.push(`up:${mode}`);
      return 'op-2';
    };
    calls.cert_apply = (installCa) => {
      done.push(`cert:${installCa}`);
      return { add: [], remove: [] };
    };
    // Already trusted, so the terminal is never opened — the machine that has
    // done this once must not be asked again on the next fresh workspace.
    calls.cert_status = () => ({ caTrusted: true });
    calls.bootstrap_complete = () => {
      done.push('complete');
      return null;
    };

    // Held open so the screen can be looked at mid-run. Asserting after
    // `flushPromises` would only ever see the finished state — which is the
    // shell, correctly, and would have made the check below pass for the
    // wrong reason.
    let release;
    const pending = new Promise((resolve) => {
      release = resolve;
    });
    const generate = calls.generate_run;
    calls.generate_run = async (...args) => {
      const result = generate(...args);
      await pending;
      return result;
    };

    const shell = mountShell();
    await flushPromises();

    // Mid-run: the shell stays out of the way, for the same reason it does
    // behind the requirements gate — every control in it acts on a stack that
    // is not up yet.
    expect(shell.text()).toContain(i18n.global.t('bootstrap.title'));
    expect(shell.find('.v-app-bar').exists(), 'the bar was up during setup').toBe(false);

    release();
    await flushPromises();
    await flushPromises();

    // The files first — `up` is handed what `generate` writes — then the core
    // profile, which is the one Traefik is in, and the certificate last.
    //
    // The certificate is not decoration. Every router in `routes.yml` sits on
    // `websecure` and the TLS store points at a file that does not exist until
    // this step runs, so Traefik builds no certificate store at all: both names
    // resolve, reach the proxy and get a dropped connection. The first version
    // of this screen stopped after `up`, reported success, and left exactly
    // that.
    expect(done).toEqual(['generate', 'up:minimal', 'cert:false', 'complete']);

    // And then it gets out of the way. Asserted as the screen being gone
    // rather than as the bar being back: whether this wrapper re-renders the
    // shell depends on harness state it shares with the one the file-level
    // hook mounted, and `does not run again once the directory has been
    // generated into` already covers the bar.
    expect(shell.text()).not.toContain(i18n.global.t('bootstrap.title'));
  });

  it('does not run again once the directory has been generated into', async () => {
    ipc.preflight = READY;
    const done = [];
    calls.workspace_get = () => ({
      root: '/tmp/app',
      projectsDir: '/tmp/code',
      valid: true,
      bootstrapped: true,
      source: 'stored',
    });
    calls.generate_run = () => {
      done.push('generate');
      return 'op-1';
    };

    const shell = mountShell();
    await flushPromises();

    expect(done, 'a set-up workspace was set up again').toEqual([]);
    expect(shell.find('.v-app-bar').exists(), 'the shell never came back').toBe(true);
  });

  it('finishes when the certificate is issued but the machine will not trust it', async () => {
    ipc.preflight = READY;
    const done = [];
    calls.workspace_get = () => ({
      root: '/tmp/app',
      projectsDir: '/tmp/code',
      valid: true,
      bootstrapped: false,
      source: 'stored',
    });
    calls.generate_run = () => 'op-1';
    calls.compose_up = () => 'op-2';
    calls.cert_apply = () => ({ add: [], remove: [] });
    // Never trusted, however long this waits — somebody closed the terminal.
    calls.cert_status = () => ({ caTrusted: false });
    calls.cert_trust_in_terminal = () => {
      done.push('terminal');
      return null;
    };
    calls.bootstrap_complete = () => {
      done.push('complete');
      return null;
    };

    // The screen waits for somebody to type a password in another window, and
    // it waits in real seconds. Fake timers rather than a ninety-second test.
    vi.useFakeTimers();
    const shell = mountShell();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(95_000);
    vi.useRealTimers();
    await flushPromises();

    // The terminal was opened without anybody being sent to Settings, and the
    // setup finished anyway. Traefik serves the certificate either way — the
    // browser warns about the issuer, which is a state worth leaving somebody
    // in. Treating it as a failed setup would leave them with no stack at all.
    expect(done, 'a usable stack was reported as a failed setup').toEqual(['terminal', 'complete']);
    expect(shell.text()).not.toContain(i18n.global.t('bootstrap.retry'));
  });

  it('does not record a setup that failed, so it is offered again', async () => {
    ipc.preflight = READY;
    const done = [];
    calls.workspace_get = () => ({
      root: '/tmp/app',
      projectsDir: '/tmp/code',
      valid: true,
      bootstrapped: false,
      source: 'stored',
    });
    calls.generate_run = () => 'op-1';
    calls.compose_up = () => 'op-2';
    calls.cert_apply = () => {
      throw new Error('mkcert is not installed');
    };
    calls.bootstrap_complete = () => {
      done.push('complete');
      return null;
    };

    const shell = mountShell();
    await flushPromises();

    // The certificate is the step that leaves a stack unable to answer, and it
    // is the last one — so a marker written by any earlier step would record
    // this run as finished. Nothing is recorded, and the screen comes back.
    expect(done, 'a half-finished setup was recorded as done').toEqual([]);
    expect(shell.text()).toContain(i18n.global.t('bootstrap.retry'));
  });

  it('offers a retry rather than a dead end', async () => {
    ipc.preflight = READY;
    calls.workspace_get = () => ({
      root: '/tmp/app',
      projectsDir: '/tmp/code',
      valid: true,
      bootstrapped: false,
      source: 'stored',
    });
    calls.generate_run = () => {
      throw new Error('no space left on device');
    };

    const shell = mountShell();
    await flushPromises();

    // Everything this screen does can be done again from inside the app, so a
    // setup that will not complete must not be the end of the road.
    expect(shell.text()).toContain(i18n.global.t('bootstrap.retry'));
  });
});

describe('tray navigation', () => {
  // The tray sends a route name and the front end pushes it. A name here that
  // the router does not declare is a menu item that raises the window and then
  // does nothing — visible only by trying it.
  it('names only routes the router declares', () => {
    const rust = readFileSync(resolve(import.meta.dirname, '../src-tauri/src/tray.rs'), 'utf8');
    const block = rust.match(/const NAV_ITEMS[^=]*=\s*\[([\s\S]*?)\];/);
    expect(block, 'NAV_ITEMS not found in tray.rs').toBeTruthy();
    const named = [...block[1].matchAll(/\("([A-Za-z]+)",/g)].map((m) => m[1]);
    expect(named.length).toBeGreaterThan(0);

    const router = readFileSync(resolve(import.meta.dirname, '../src/router/index.js'), 'utf8');
    const declared = new Set([...router.matchAll(/name: '([A-Za-z]+)'/g)].map((m) => m[1]));

    expect(named.filter((n) => !declared.has(n))).toEqual([]);
  });
});
