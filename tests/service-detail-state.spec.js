import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createPinia } from 'pinia';
import { i18n } from '@/i18n';
import ServiceDetailSheet from '@/components/ServiceDetailSheet.vue';

/**
 * What the detail sheet says about a container that is not simply fine.
 *
 * Three things it used to get wrong, each of them by saying nothing:
 *
 * - A container failing its own healthcheck carried the same green "Running"
 *   chip as a healthy one. Twenty-four packages in the catalogue declare a
 *   healthcheck and the answer never left the boundary.
 * - A required dependency nothing provides was dropped before it reached the
 *   template — `provider_instance` returned `None` and `filter_map` threw the
 *   row away — so Kibana with no Elasticsearch installed rendered "No
 *   dependencies", which is the opposite of true in exactly the state somebody
 *   opens the panel to diagnose.
 * - `container_inspect` returned the exit code, the restart count and the
 *   image all along, and the panel read four of its nineteen fields. A service
 *   killed for memory reported "Stopped" and stopped there.
 */

const api = vi.hoisted(() => ({
  containerInspect: vi.fn(),
  serviceConnection: vi.fn(),
  mailStatus: vi.fn(),
  mailMessages: vi.fn(),
  dbTargets: vi.fn(),
  dbSnapshots: vi.fn(),
  // G-4's three.
  dbInstances: vi.fn(),
  dbMovePlan: vi.fn(),
  dbMoveApply: vi.fn(),
  terminalOpenExternal: vi.fn(),
  serviceReveal: vi.fn(),
  openInBrowser: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));
vi.mock('@/lib/events', () => ({ listenAll: vi.fn(async () => () => {}) }));

const vuetify = createVuetify({ components, directives });

const KIBANA = {
  id: 'kibana-9-4-4',
  containerName: 'stackvo-kibana-9-4-4',
  enabled: true,
  running: true,
  built: true,
  health: null,
  url: 'kibana.stackvo.loc',
  hostPort: 5601,
  ports: [],
  declaredPorts: [],
  aliases: ['stackvo-kibana-9-4-4'],
  support: 'supported',
  eolDate: null,
  companions: [],
  credentials: [],
  required: [],
  optional: [],
  unmetDependencies: [],
};

/** As `list_services` builds it when nothing installed provides the capability. */
const UNPROVIDED = {
  capability: 'search',
  service: 'elasticsearch',
  provider: null,
  required: true,
  running: false,
};

const mountSheet = (service) =>
  mount(
    {
      components: { ServiceDetailSheet },
      props: ['service'],
      template: `
        <v-app>
          <ServiceDetailSheet :service="service" :model-value="true" />
        </v-app>`,
    },
    {
      props: { service },
      // Pinia because the logs tab mounts `LogView`, which reads the log store
      // for its follow/wrap preferences. The companion test is the one that
      // gets that far.
      global: { plugins: [vuetify, i18n, createPinia()], stubs: { teleport: true } },
    }
  );

beforeEach(() => {
  vi.clearAllMocks();
  api.containerInspect.mockResolvedValue({ networks: [], mounts: [], ports: [] });
  api.serviceConnection.mockResolvedValue(null);
  api.mailStatus.mockResolvedValue({ available: false });
  api.mailMessages.mockResolvedValue([]);
  api.dbTargets.mockResolvedValue([]);
  api.dbSnapshots.mockResolvedValue([]);
});

describe('the status chip', () => {
  it('says healthy rather than merely running', async () => {
    const wrapper = mountSheet({ ...KIBANA, health: 'healthy' });
    await flushPromises();

    expect(wrapper.text()).toContain('Healthy');
  });

  /** The state the whole field exists for: up, and failing its own check. */
  it('does not call an unhealthy container running', async () => {
    const wrapper = mountSheet({ ...KIBANA, health: 'unhealthy' });
    await flushPromises();

    expect(wrapper.text()).toContain('Unhealthy');
    expect(wrapper.text()).not.toContain('Running');
  });

  /**
   * Most containers declare no healthcheck, and inventing a fourth word for
   * them would be the same overclaim in reverse.
   */
  it('keeps the two old words when there is no healthcheck', async () => {
    const wrapper = mountSheet({ ...KIBANA, health: null });
    await flushPromises();

    expect(wrapper.text()).toContain('Running');
  });

  it('is stopped before it is anything else', async () => {
    // `health` survives a stop in Docker's own state, so a container that
    // exited while healthy must not be reported as healthy.
    const wrapper = mountSheet({ ...KIBANA, running: false, health: 'healthy' });
    await flushPromises();

    expect(wrapper.text()).toContain('Stopped');
    expect(wrapper.text()).not.toContain('Healthy');
  });
});

describe('the dependencies panel', () => {
  it('names a required dependency nothing provides', async () => {
    const wrapper = mountSheet({ ...KIBANA, required: [UNPROVIDED] });
    await flushPromises();

    const text = wrapper.text();
    expect(text).not.toContain('No dependencies');
    expect(text).toContain('elasticsearch');
    expect(text).toContain('nothing installed provides this');
  });

  /**
   * Two failures with two different fixes — install something, or start what
   * you have — and until the row carried `provider` there was nothing to tell
   * them apart with.
   */
  it('tells "not installed" apart from "installed and stopped"', async () => {
    const wrapper = mountSheet({
      ...KIBANA,
      required: [{ ...UNPROVIDED, provider: 'elasticsearch-9-4-4' }],
    });
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain('elasticsearch-9-4-4');
    expect(text).toContain('not running');
    expect(text).not.toContain('nothing installed provides this');
  });

  it('still says nothing when a package declares nothing', async () => {
    const wrapper = mountSheet(KIBANA);
    await flushPromises();

    expect(wrapper.text()).toContain('No dependencies');
  });
});

describe('what the package declares about itself', () => {
  it('warns about an end-of-life version where the service is used', async () => {
    const wrapper = mountSheet({ ...KIBANA, support: 'eol', eolDate: '2026-04-30' });
    await flushPromises();

    // The date, not just the words: "ended two years ago" and "ends next
    // month" were rendering identically, and only in the catalogue tree —
    // which is the page you install from, not the one you debug from.
    expect(wrapper.text()).toContain('End of life');
    expect(wrapper.text()).toContain('2026-04-30');
  });

  it('says nothing about a version that is simply supported', async () => {
    const wrapper = mountSheet(KIBANA);
    await flushPromises();

    // A chip on every service is a word that stops being read, and it would
    // sit next to the two that are worth stopping for.
    expect(wrapper.text()).not.toContain('Supported');
  });

  it('names the ports the package names them', async () => {
    const wrapper = mountSheet({
      ...KIBANA,
      running: false,
      built: false,
      declaredPorts: [
        { name: 'main', container: 9000, host: 9000, protocol: 'tcp' },
        { name: 'console', container: 9001, host: 9001, protocol: 'tcp' },
      ],
    });
    await flushPromises();

    // Which of 9000 and 9001 is the console was exactly what the container's
    // own port list could not say — and a service that has never been started
    // publishes nothing, so it used to say nothing at all.
    const text = wrapper.text();
    expect(text).toContain('main');
    expect(text).toContain('console');
    expect(text).toContain('9001');
  });

  it('names the alias every pre-package project points at', async () => {
    const wrapper = mountSheet({
      ...KIBANA,
      aliases: ['stackvo-mysql-8-0', 'stackvo-mysql'],
    });
    await flushPromises();

    expect(wrapper.text()).toContain('stackvo-mysql');

    // The instance's own name is the container row directly above, so
    // repeating it here would teach the reader to skip the section.
    const plain = mountSheet(KIBANA);
    await flushPromises();
    expect(plain.text()).not.toContain('Also reachable at');
  });
});

describe('companion containers', () => {
  const ZOOKEEPER = {
    name: 'zookeeper',
    containerName: 'stackvo-kafka-7-5-0-zookeeper',
    image: 'confluentinc/cp-zookeeper:7.5.0',
    built: true,
    running: false,
    health: null,
  };

  /**
   * The container that has been rendered into every Kafka compose file and
   * shown on no screen. When the broker will not start, the reason is usually
   * in this container's log, and the panel about the broker was the one place
   * that did not mention it exists.
   */
  it('shows a companion, its container name and its state', async () => {
    const wrapper = mountSheet({ ...KIBANA, companions: [ZOOKEEPER] });
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain('zookeeper');
    expect(text).toContain('stackvo-kafka-7-5-0-zookeeper');
    expect(text).toContain('confluentinc/cp-zookeeper:7.5.0');
    expect(text).toContain('Stopped');
  });

  it('opens the companion’s own log rather than the service’s', async () => {
    const wrapper = mountSheet({ ...KIBANA, companions: [{ ...ZOOKEEPER, running: true }] });
    await flushPromises();

    // By accessible name, not by text: Vuetify's `v-tab` *is* a `v-btn`, so
    // "the button that says Logs" is the tab as well as the companion's
    // button — and clicking the tab switches to the log panel without
    // changing whose log it shows, which is a green test over a broken
    // feature.
    await wrapper.get('button[aria-label="zookeeper log"]').trigger('click');
    await flushPromises();

    // The tab is named after whose output is on screen: landing on a tab that
    // still says "Logs" leaves no way to tell.
    expect(wrapper.text()).toContain('Logs · zookeeper');
  });

  it('says nothing at all for the services that have none', async () => {
    const wrapper = mountSheet(KIBANA);
    await flushPromises();

    expect(wrapper.text()).not.toContain('Companion containers');
  });
});

describe('the runtime rows', () => {
  it('reports the exit code of a container that stopped', async () => {
    api.containerInspect.mockResolvedValue({
      networks: [],
      mounts: [],
      ports: [],
      running: false,
      exitCode: 137,
      restartCount: 0,
    });

    const wrapper = mountSheet({ ...KIBANA, running: false });
    await flushPromises();

    // 137 is SIGKILL, and on a developer machine it is nearly always the
    // engine's memory limit rather than anything the service did.
    expect(wrapper.text()).toContain('137');
    expect(wrapper.text()).toContain('out of memory');
  });

  it('reports restarts once there have been any, and the policy behind them', async () => {
    api.containerInspect.mockResolvedValue({
      networks: [],
      mounts: [],
      ports: [],
      running: true,
      restartCount: 14,
      restartPolicy: 'unless-stopped',
      image: 'docker.elastic.co/kibana/kibana:9.4.4',
    });

    const wrapper = mountSheet(KIBANA);
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain('14');
    expect(text).toContain('unless-stopped');
    // Which image is *actually* running, as opposed to which one the manifest
    // names — the two diverging is a thing that happens and was invisible.
    expect(text).toContain('docker.elastic.co/kibana/kibana:9.4.4');
  });

  /**
   * Zero restarts is the normal state, and a row reading "0" invites the
   * reader to wonder what it would mean. Same for an exit code on something
   * that is running: there isn't one.
   */
  it('leaves out the rows that would only say nothing', async () => {
    api.containerInspect.mockResolvedValue({
      networks: [],
      mounts: [],
      ports: [],
      running: true,
      restartCount: 0,
      exitCode: 0,
    });

    const wrapper = mountSheet(KIBANA);
    await flushPromises();

    expect(wrapper.text()).not.toContain('Restarts');
    expect(wrapper.text()).not.toContain('Exit code');
  });
});

/**
 * Moving one instance's data into another (G-4).
 *
 * The plan is the feature, so the assertions are about it being *readable
 * before the button is worth pressing*: a refused pair says why rather than
 * being absent from the list, and the sentence about the target being replaced
 * is on screen without anybody pressing anything.
 */
describe('moving a database', () => {
  const MYSQL = {
    ...KIBANA,
    id: 'mysql-8-0',
    containerName: 'stackvo-mysql-8-0',
    url: null,
  };

  const instances = [
    { id: 'mysql-8-0', service: 'mysql', version: '8.0', running: true },
    { id: 'mysql-8-4', service: 'mysql', version: '8.4', running: true },
    { id: 'postgres-16', service: 'postgres', version: '16', running: false },
  ];

  beforeEach(() => {
    api.dbInstances.mockResolvedValue(instances);
  });

  /**
   * Read off the select's items rather than the rendered text: Vuetify paints
   * the options only once the menu opens, and what is *offered* is the claim —
   * not what a closed dropdown happens to have drawn.
   */
  const offered = (wrapper) => wrapper.findComponent({ name: 'VSelect' }).props('items');

  /** Listing only the compatible ones would leave "missing or impossible?" */
  it('offers every other instance, including the ones a move would refuse', async () => {
    const wrapper = mountSheet(MYSQL);
    await flushPromises();

    const values = offered(wrapper).map((item) => item.value);
    expect(values).toContain('mysql-8-4');
    expect(values, 'a Postgres target must be offered and refused, not hidden').toContain(
      'postgres-16'
    );
    expect(values, 'the source must not be a target').not.toContain('mysql-8-0');
  });

  it('marks an instance that is not running', async () => {
    const wrapper = mountSheet(MYSQL);
    await flushPromises();

    const stopped = offered(wrapper).find((item) => item.value === 'postgres-16');
    expect(stopped.title).toContain(i18n.global.t('system.stopped'));
  });

  /** The plan runs on choosing, not on pressing. */
  it('asks for the plan as soon as a target is chosen', async () => {
    api.dbMovePlan.mockResolvedValue({
      from: 'mysql-8-0',
      to: 'mysql-8-4',
      possible: true,
      warnings: ['everything in mysql-8-4 will be replaced'],
    });
    const wrapper = mountSheet(MYSQL);
    await flushPromises();

    const select = wrapper.findComponent({ name: 'VSelect' });
    await select.setValue('mysql-8-4');
    await flushPromises();

    expect(api.dbMovePlan).toHaveBeenCalledWith('mysql-8-0', 'mysql-8-4');
    expect(wrapper.text()).toContain('will be replaced');
  });

  it('shows a refusal with its reason and keeps the button disabled', async () => {
    api.dbMovePlan.mockResolvedValue({
      from: 'mysql-8-0',
      to: 'postgres-16',
      possible: false,
      refused: 'a mysql dump is not postgres input',
      warnings: [],
    });
    const wrapper = mountSheet(MYSQL);
    await flushPromises();

    const select = wrapper.findComponent({ name: 'VSelect' });
    await select.setValue('postgres-16');
    await flushPromises();

    expect(wrapper.text()).toContain('is not postgres input');
    const move = wrapper.findAll('button').find((b) => b.text() === i18n.global.t('dbMove.move'));
    expect(move.attributes('disabled')).toBeDefined();
  });
});
