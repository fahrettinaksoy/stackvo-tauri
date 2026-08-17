import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * F-5's pane — the half of the workbench that is not an argv.
 *
 * `repl.rs` holds what the command is and `examples/repl_probe.rs` holds
 * whether it runs in a real container. What is left for here is what the screen
 * does with the answer, and specifically the four things this feature could get
 * wrong quietly:
 *
 * * showing a failed run as a success, because a PHP fatal arrives on **stdout**
 *   and "stderr is empty" is the wrong test;
 * * showing only one stream, which would be blank for half the languages;
 * * saying nothing when the in-container limit was unavailable, which turns a
 *   snippet still burning a CPU into a clean stop;
 * * losing the snippet, which is the whole reason this exists beside a terminal
 *   that already runs code.
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

const { i18n } = await import('@/i18n');
const en = (await import('@/i18n/locales/en.js')).default;
const ReplPane = (await import('@/components/project/ReplPane.vue')).default;

const vuetify = createVuetify({ components, directives });

const LARAVEL = {
  id: 'laravel',
  display: 'php artisan tinker --execute',
  language: 'php',
  booted: true,
  about: 'Your models, config and container, as the app sees them.',
  because: 'artisan',
};
const NODE = {
  id: 'node',
  display: 'node -e',
  language: 'javascript',
  booted: false,
  about: 'Node on its own.',
  because: 'package.json',
};

/** A run that worked. */
const OK = {
  runner: 'laravel',
  display: 'php artisan tinker --execute',
  stdout: '41',
  stderr: '',
  exitCode: 0,
  ms: 812,
  timedOut: false,
  truncated: false,
  limited: true,
};

const mountPane = (running = true) =>
  mount(
    {
      components: { ReplPane },
      props: ['running'],
      template: '<v-app><ReplPane name="shop" :running="running" /></v-app>',
    },
    { props: { running }, global: { plugins: [vuetify, i18n] } }
  );

/** Mounted, loaded, with a snippet typed in. */
async function paneWith(code = 'dump(1);', running = true) {
  const pane = mountPane(running);
  await flushPromises();
  await pane.find('textarea').setValue(code);
  return pane;
}

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.replRunners = [LARAVEL, NODE];
  replies.replHistory = [];
  replies.replHistoryClear = [];
});

describe('what it offers', () => {
  it('says whether the application is booted, because it is not a detail', async () => {
    const pane = mountPane();
    await flushPromises();
    expect(pane.text()).toContain(en.repl.booted);
    expect(pane.text()).toContain(LARAVEL.about);
  });

  /**
   * A static site has no application to boot, and that is not a fault to
   * report as one.
   */
  it('says so plainly when the project has nothing to load', async () => {
    replies.replRunners = [];
    const pane = mountPane();
    await flushPromises();
    expect(pane.text()).toContain(en.repl.noRunner);
    expect(pane.find('textarea').exists()).toBe(false);
  });

  /**
   * The surprise for anybody who knows `tinker`: `--execute` does not echo the
   * value of the last expression. Said next to the editor, not in a document.
   */
  it('warns that a booted runner prints nothing on its own', async () => {
    const pane = mountPane();
    await flushPromises();
    expect(pane.text()).toContain(en.repl.printYourself);
  });
});

describe('running one', () => {
  it('sends the runner id and the code, and shows what came back', async () => {
    replies.replRun = OK;
    const pane = await paneWith('dump(41);');

    await pane.find('button').trigger('click');
    await flushPromises();

    expect(calls).toContainEqual(['replRun', 'shop', 'laravel', 'dump(41);']);
    expect(pane.text()).toContain('41');
    expect(pane.text()).toContain(en.repl.ok);
    expect(pane.text()).toContain('812 ms');
  });

  /**
   * The one that would have shipped wrong. A PHP fatal is written to stdout, so
   * a pane that decided success by looking at stderr would draw a green chip
   * over an uncaught exception.
   */
  it('reads failure from the exit code, not from stderr being empty', async () => {
    replies.replRun = {
      ...OK,
      stdout: 'PHP Fatal error:  Uncaught RuntimeException: boom',
      stderr: '',
      exitCode: 255,
    };
    const pane = await paneWith();

    await pane.find('button').trigger('click');
    await flushPromises();

    expect(pane.text()).toContain('exit 255');
    expect(pane.text()).not.toContain(en.repl.ok);
    expect(pane.text()).toContain('Uncaught RuntimeException');
  });

  /** And the other half of the same problem: Node writes its throw to stderr. */
  it('shows stderr too, or half the languages report nothing', async () => {
    replies.replRun = { ...OK, stdout: '', stderr: 'Error: boom\n    at [eval]', exitCode: 1 };
    const pane = await paneWith();

    await pane.find('button').trigger('click');
    await flushPromises();

    expect(pane.text()).toContain('Error: boom');
  });

  /**
   * A snippet the app could not limit inside the container is one that may
   * still be running after the pane stopped waiting. Silence there would make a
   * leak look like a clean stop.
   */
  it('says when the limit could not be enforced in the container', async () => {
    replies.replRun = { ...OK, limited: false, timedOut: false };
    const pane = await paneWith();

    await pane.find('button').trigger('click');
    await flushPromises();

    expect(pane.text()).toContain(en.repl.notLimited);
  });

  it('reports a stopped run as stopped rather than as empty output', async () => {
    replies.replRun = { ...OK, stdout: '', timedOut: true, exitCode: 124 };
    const pane = await paneWith();

    await pane.find('button').trigger('click');
    await flushPromises();

    expect(pane.text()).toContain(en.repl.timedOut);
  });

  /** Nothing to run is not a request. */
  it('will not run an empty snippet or a stopped project', async () => {
    const empty = await paneWith('   ');
    expect(empty.find('button').attributes('disabled')).toBeDefined();

    const stopped = await paneWith('dump(1);', false);
    expect(stopped.find('button').attributes('disabled')).toBeDefined();
    expect(stopped.text()).toContain(en.repl.needsRunning);
  });

  /**
   * An error under the previous run's output would read as that run's result.
   */
  it('clears the last output when a run fails outright', async () => {
    replies.replRun = OK;
    const pane = await paneWith();
    await pane.find('button').trigger('click');
    await flushPromises();
    expect(pane.text()).toContain('41');

    replies.replRun = () => Promise.reject(new Error('the project is not running'));
    await pane.find('button').trigger('click');
    await flushPromises();
    expect(pane.text()).not.toContain('812 ms');
  });
});

describe('what it remembers', () => {
  it('puts a remembered snippet back in the editor with its runner', async () => {
    replies.replHistory = [{ at: 1786800000, runner: 'node', code: 'console.log(1)' }];
    const pane = mountPane();
    await flushPromises();

    expect(pane.text()).toContain('console.log(1)');
    await pane.find('.repl-past').trigger('click');
    await flushPromises();

    expect(pane.find('textarea').element.value).toBe('console.log(1)');
    // And the runner came back with it — running a Node snippet through PHP is
    // a syntax error somebody would have to work out for themselves.
    expect(pane.text()).toContain(en.repl.bare);
  });

  it('can forget them, because a snippet can hold anything somebody pasted', async () => {
    replies.replHistory = [{ at: 1786800000, runner: 'php', code: "$token = 'sk_live_x';" }];
    const pane = mountPane();
    await flushPromises();

    const forget = pane
      .findAll('button')
      .find((candidate) => candidate.text().includes(en.repl.forget));
    await forget.trigger('click');
    await flushPromises();

    expect(calls).toContainEqual(['replHistoryClear', 'shop']);
    expect(pane.text()).not.toContain('sk_live_x');
  });
});
