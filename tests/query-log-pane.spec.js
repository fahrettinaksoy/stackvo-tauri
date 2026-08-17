import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * F-1's screen, and specifically the half of it that is different per database.
 *
 * The parsing, the shapes and the N+1 counting are `querylog.rs`'s own tests,
 * and what actually runs against a live MySQL, MariaDB, Postgres and Mongo is
 * `examples/querylog_probe.rs`. What is left for here is the thing neither of
 * those can see: whether the pane offers the same two verbs on all four.
 *
 * It did not. "Start again" was hidden on Postgres, because the first version
 * of the module could not delete anything from a log the server owns — and the
 * button is not a deletion, it is "the session starts here", which a watermark
 * answers. These hold the shape after that: the same controls everywhere, and
 * the one cost that genuinely is not the same said where the switch is.
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
const QueryLogPane = (await import('@/components/project/QueryLogPane.vue')).default;

const vuetify = createVuetify({ components, directives });

/** One database running, named by the caller. */
const target = (service) => ({ service, kind: service, running: true, enabled: true });

/** A session with something in it, so the recording half of the pane renders. */
const RECORDING = {
  recording: true,
  supported: true,
  entries: [
    {
      at: 1786801017.193,
      sql: 'SELECT * FROM users WHERE id = 7',
      shape: 'SELECT * FROM users WHERE id = ?',
    },
  ],
  repeats: [],
};

const mountPane = () =>
  mount(
    { components: { QueryLogPane }, template: '<v-app><QueryLogPane /></v-app>' },
    { global: { plugins: [vuetify, i18n] } }
  );

/** Mounted, loaded, and recording on whichever database is listed. */
async function paneFor(service) {
  replies.dbTargets = [target(service)];
  replies.queryLog = RECORDING;
  const pane = mountPane();
  await flushPromises();
  return pane;
}

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.dbTargets = [];
  replies.queryLog = { recording: false, supported: true, entries: [], repeats: [] };
});

describe('start again', () => {
  /**
   * All four, and Postgres is the one this is about: it was excluded by name,
   * and the exclusion outlived the reason for it.
   */
  it.each(['mysql', 'mariadb', 'postgres', 'mongo'])('is offered on %s', async (service) => {
    const pane = await paneFor(service);
    expect(pane.text()).toContain(en.queryLog.clear);
  });

  it('is not offered before recording starts — there is no session to restart', async () => {
    replies.dbTargets = [target('postgres')];
    const pane = mountPane();
    await flushPromises();
    expect(pane.text()).not.toContain(en.queryLog.clear);
  });

  it('asks the backend and takes the session it hands back', async () => {
    const pane = await paneFor('postgres');
    const button = pane
      .findAll('button')
      .find((candidate) => candidate.text().includes(en.queryLog.clear));

    replies.queryLogClear = { ...RECORDING, entries: [] };
    await button.trigger('click');
    await flushPromises();

    expect(calls).toContainEqual(['queryLogClear', 'postgres']);
    // The pane does not empty its own list and hope: what is shown is what the
    // next read of the log actually returned.
    expect(pane.text()).toContain(en.queryLog.nothingYet);
  });
});

describe('what recording costs', () => {
  /**
   * The same warning on all four, because the write-throughput cost is the same
   * shape everywhere and forgetting to switch it off is the failure mode.
   */
  it.each(['mysql', 'postgres', 'mongo'])('is said while it is on, on %s', async (service) => {
    const pane = await paneFor(service);
    expect(pane.text()).toContain(en.queryLog.cost);
  });

  /**
   * And one sentence that is only true of Postgres: its statements land in the
   * server's own log file, and no button here takes them back out. Said beside
   * the switch rather than in a document, because that is the moment somebody
   * decides whether to record against a database holding real rows.
   */
  it('names the disk it also lands on, but only on Postgres', async () => {
    expect((await paneFor('postgres')).text()).toContain(en.queryLog.costPostgres);
    expect((await paneFor('mysql')).text()).not.toContain(en.queryLog.costPostgres);
  });
});
