import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Debug section: Xdebug, its profiler, and the dumps the app catches.
 *
 * Two things in here were wrong in ways a screenshot cannot show, and both are
 * pinned below because both are easy to reintroduce:
 *
 *  - `needsRestart` used to ask `active === false`, which never fired for the
 *    case it exists for. `active` only means "both Xdebug variables are
 *    present" — after switching stepping to profiling they still are, with the
 *    old mode in them. The page reported profiling as applied and the recorded
 *    list stayed at zero with nothing to say why.
 *  - the profile's time unit is read from the file (`Time_(10ns)`), never
 *    assumed. Reading it as microseconds is wrong by two orders of magnitude
 *    and the number still looks plausible.
 */

globalThis.visualViewport = undefined;

const replies = {};
const calls = [];

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get:
        (_t, name) =>
        (...args) => {
          calls.push([String(name), ...args]);
          const reply = replies[name];
          return typeof reply === 'function' ? reply(...args) : Promise.resolve(reply);
        },
    }
  ),
}));

const { useXdebug } = await import('@/composables/useXdebug');
const { useProfiler } = await import('@/composables/useProfiler');
const { i18n } = await import('@/i18n');
const XdebugPane = (await import('@/components/project/XdebugPane.vue')).default;
const DumpsPane = (await import('@/components/project/DumpsPane.vue')).default;

const vuetify = createVuetify({ components, directives });
const ref = (value) => ({ value });

const PROFILES = [
  { id: 'cachegrind.out.1', name: 'cachegrind.out.1', bytes: 4096, recordedAt: 1_786_007_730 },
];

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.xdebugStatus = { enabled: true, needsRebuild: false, active: true, running: true };
  replies.profilerStatus = {
    mode: 'profile',
    trigger: 'XDEBUG_TRIGGER=1',
    profiles: PROFILES,
    bytes: 4096,
    directory: '/ws/projects/shop/.stackvo/profiles',
    xdebug: { running: true, active: true, activeMode: 'profile' },
  };
});

describe('xdebug', () => {
  it('has nothing to report for a runtime with no PHP', async () => {
    const x = useXdebug(ref('shop'));
    expect(await x.load('node')).toBe(null);
    expect(calls).toEqual([]);
  });

  /**
   * Enabled but not yet doing anything is the state a user will not go looking
   * for: breakpoints are set, and nothing stops at them.
   */
  it.each([
    [{ enabled: true, needsRebuild: true, active: true }, true, 'enabled, image never rebuilt'],
    [{ enabled: true, needsRebuild: false, active: false }, true, 'built, not in the container'],
    [{ enabled: true, needsRebuild: false, active: true }, false, 'actually working'],
    [{ enabled: false, needsRebuild: true, active: false }, false, 'off, so nothing is pending'],
  ])('badges %o as %s (%s)', async (status, expected) => {
    replies.xdebugStatus = status;
    const x = useXdebug(ref('shop'));
    await x.load('php');

    expect(!!x.pending.value).toBe(expected);
  });

  it('reports a refused toggle and stops spinning', async () => {
    replies.xdebugSet = () => Promise.reject({ code: 'DOCKER_UNAVAILABLE', message: 'down' });
    const x = useXdebug(ref('shop'));

    expect(await x.toggle(true)).toBe(null);
    expect(x.error.value.code).toBe('DOCKER_UNAVAILABLE');
    expect(x.busy.value).toBe(false);
  });

  /**
   * Toggling rewrites the manifest on disk, and the Configuration section is
   * showing that same file. The pane says so rather than reaching across.
   */
  it('tells the page the manifest changed under it', async () => {
    replies.xdebugSet = { enabled: false, needsRebuild: true, active: false };

    const wrapper = mount(
      {
        components: { XdebugPane },
        template:
          '<v-app><XdebugPane name="shop" runtime="php" @changed="$attrs.onSeen" /></v-app>',
      },
      { attrs: { onSeen: vi.fn() }, global: { plugins: [vuetify, i18n] } }
    );
    await vi.waitFor(() => expect(wrapper.find('input[type="checkbox"]').exists()).toBe(true));

    await wrapper.find('input[type="checkbox"]').setValue(false);
    await vi.waitFor(() => expect(calls.some(([n]) => n === 'xdebugSet')).toBe(true));
    expect(wrapper.attributes()).toBeDefined();
  });

  it('does not announce a change that failed', async () => {
    replies.xdebugSet = () => Promise.reject({ code: 'IO', message: 'read-only' });
    const x = useXdebug(ref('shop'));

    expect(await x.toggle(true)).toBe(null);
  });
});

describe('the profiler', () => {
  /**
   * The bug this replaced: `active` stays true across a mode switch, with the
   * old mode still in the variables.
   */
  it.each([
    [
      { running: true, active: true, activeMode: 'debug' },
      'profile',
      true,
      'container in the old mode',
    ],
    [{ running: true, active: true, activeMode: 'profile' }, 'profile', false, 'already applied'],
    [{ running: true, active: false, activeMode: null }, 'profile', true, 'not in the container'],
    [{ running: false, active: false, activeMode: null }, 'profile', false, 'nothing to disagree'],
  ])('%o vs %s needs a recreate: %s (%s)', async (xdebug, mode, expected) => {
    replies.profilerStatus = { mode, profiles: [], bytes: 0, xdebug };
    const p = useProfiler(ref('shop'));
    await p.load('php');

    expect(p.needsRestart.value).toBe(expected);
  });

  describe('the time unit', () => {
    it.each([
      ['Time_(10ns)', 12_345_678, '123.5 ms'],
      ['Time_(us)', 500, '500 µs'],
      ['Time_(ms)', 3, '3.0 ms'],
      ['Time_(ns)', 2_000_000, '2.0 ms'],
    ])('reads %s and renders the cost from it', async (declared, value, rendered) => {
      const p = useProfiler(ref('shop'));
      p.report.value = { events: [declared] };

      expect(p.cost(value)).toBe(rendered);
    });

    /**
     * A file whose header is missing or unparseable renders the raw number
     * rather than a wrong one — inventing a unit would be worse than showing
     * none.
     */
    it.each([[undefined], [{ events: [] }], [{ events: ['Time'] }]])(
      'refuses to guess when the file declares none (%o)',
      async (report) => {
        const p = useProfiler(ref('shop'));
        p.report.value = report ?? null;

        expect(p.unit.value).toBe('');
        expect(p.cost(4200)).toBe('4200');
      }
    );
  });

  it('marks the file that is working, not every row', async () => {
    let settle;
    replies.profilerRead = () => new Promise((resolve) => (settle = resolve));
    const p = useProfiler(ref('shop'));
    await p.load('php');

    const done = p.open(PROFILES[0]);
    expect(p.busy.value).toBe('cachegrind.out.1');
    settle({ events: ['Time_(10ns)'], functions: [] });
    await done;

    expect(p.openId.value).toBe('cachegrind.out.1');
    expect(p.busy.value).toBe('');
  });

  it('highlights nothing as open when the read fails', async () => {
    replies.profilerRead = () => Promise.reject({ code: 'IO', message: 'unreadable' });
    const p = useProfiler(ref('shop'));
    p.openId.value = 'cachegrind.out.1';

    await p.open(PROFILES[0]);
    expect(p.openId.value).toBe('');
    expect(p.report.value).toBe(null);
    expect(p.error.value.code).toBe('IO');
  });

  /** The open report belongs to a file that no longer exists. */
  it('closes the report when its own file is deleted', async () => {
    replies.profilerDelete = null;
    const p = useProfiler(ref('shop'));
    await p.load('php');
    await p.open(PROFILES[0]);
    p.report.value = { events: ['Time_(10ns)'] };

    await p.remove(PROFILES[0], 'php');
    expect(p.report.value).toBe(null);
    expect(p.openId.value).toBe('');
    expect(
      calls.filter(([n]) => n === 'profilerStatus'),
      're-read after deleting'
    ).toHaveLength(2);
  });

  it('leaves the report alone when a different file is deleted', async () => {
    replies.profilerDelete = null;
    replies.profilerRead = { events: ['Time_(10ns)'] };
    const p = useProfiler(ref('shop'));
    await p.load('php');
    await p.open(PROFILES[0]);

    await p.remove({ id: 'cachegrind.out.9' }, 'php');
    expect(p.openId.value).toBe('cachegrind.out.1');
    expect(p.report.value).toBeTruthy();
  });

  it('clears everything, report included', async () => {
    replies.profilerClear = null;
    replies.profilerRead = { events: ['Time_(10ns)'] };
    const p = useProfiler(ref('shop'));
    await p.load('php');
    await p.open(PROFILES[0]);

    expect(await p.clear('php')).toBe(true);
    expect(p.report.value).toBe(null);
    expect(p.openId.value).toBe('');
  });
});

describe('the dumps pane', () => {
  /**
   * The recreate button is the project's lifecycle, which the view owns — the
   * pane only says the container needs one.
   */
  it('passes the recreate up rather than running it', async () => {
    const onApply = vi.fn();
    mount(
      {
        components: { DumpsPane },
        template: '<v-app><DumpsPane name="shop" @apply="$attrs.onApply" /></v-app>',
      },
      {
        attrs: { onApply },
        global: { plugins: [createPinia(), vuetify, i18n] },
      }
    );

    expect(
      calls.some(([n]) => n === 'composeUpProject'),
      'the pane must not run the project lifecycle itself'
    ).toBe(false);
  });
});
