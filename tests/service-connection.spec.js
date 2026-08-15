import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { i18n } from '@/i18n';
import ServiceDetailSheet from '@/components/ServiceDetailSheet.vue';

/**
 * The connection string section, and the one thing it exists to prevent.
 *
 * The sheet already showed `stackvo-mongo` as the container name, so that is
 * what people pasted into Compass — and it cannot resolve from the host,
 * because the name only exists on the Docker network. Showing one address
 * would have picked a side; these tests are mostly about the fact that it
 * shows two and says which is which.
 */

const api = vi.hoisted(() => ({
  containerInspect: vi.fn(),
  containerStats: vi.fn(),
  serviceConnection: vi.fn(),
  serviceDbClients: vi.fn(),
  serviceOpenInClient: vi.fn(),
  mailStatus: vi.fn(),
  mailMessages: vi.fn(),
  dbTargets: vi.fn(),
  // G-4's list, which the sheet reads on mount.
  dbInstances: vi.fn(),
  terminalOpenExternal: vi.fn(),
  envReveal: vi.fn(),
  openInBrowser: vi.fn(),
}));

// `asList` is the real guard, not a stub — see views-render.spec.js. The sheet
// reads it since the instance list arrived (G-4), and a mock without it fails
// at import rather than in an assertion.
vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));
vi.mock('@/lib/events', () => ({ listenAll: vi.fn(async () => () => {}) }));

const vuetify = createVuetify({ components, directives });

/** A row as `services_list` builds it — every field, so the fixture cannot
 *  quietly disagree with the boundary about what a service is. */
const MONGO = {
  id: 'mongo',
  containerName: 'stackvo-mongo',
  enabled: true,
  running: true,
  built: true,
  health: null,
  url: null,
  hostPort: null,
  ports: [{ container: 27017, host: 27017, protocol: 'tcp' }],
  declaredPorts: [],
  aliases: ['stackvo-mongo'],
  support: null,
  eolDate: null,
  companions: [],
  credentials: [],
  required: [],
  optional: [],
  unmetDependencies: [],
};

const MASKED = {
  service: 'mongo',
  kind: 'mongo',
  fromHost: {
    uri: 'mongodb://root:••••••••@127.0.0.1:27017/stackvo?authSource=admin',
    host: '127.0.0.1',
    port: 27017,
  },
  fromContainer: {
    uri: 'mongodb://root:••••••••@stackvo-mongo:27017/stackvo?authSource=admin',
    host: 'stackvo-mongo',
    port: 27017,
  },
  masked: true,
  passwordKey: 'SERVICE_MONGO_INITDB_ROOT_PASSWORD',
};

const REVEALED = {
  ...MASKED,
  fromHost: {
    ...MASKED.fromHost,
    uri: 'mongodb://root:root@127.0.0.1:27017/stackvo?authSource=admin',
  },
  fromContainer: {
    ...MASKED.fromContainer,
    uri: 'mongodb://root:root@stackvo-mongo:27017/stackvo?authSource=admin',
  },
  masked: false,
};

/**
 * What `service_db_clients` answers for a Mongo service on a machine with
 * Compass installed and DBeaver not. The system handler heads the list and has
 * the empty id — it is the absence of a choice, not a choice.
 */
const CLIENTS = [
  { id: '', name: 'System default', icon: 'mdi-open-in-app', available: true, default: true },
  { id: 'compass', name: 'MongoDB Compass', icon: 'mdi-database', available: true, default: false },
  { id: 'dbeaver', name: 'DBeaver', icon: 'mdi-database', available: false, default: false },
];

/**
 * Wrapped in a `v-app`, because the sheet is a `v-navigation-drawer` underneath
 * and Vuetify's layout composable throws without one. The wrapper is the
 * component under test's real surroundings, not a stub of them.
 */
const mountSheet = (service = MONGO) =>
  mount(
    {
      components: { ServiceDetailSheet },
      props: ['service'],
      template: `
        <v-app>
          <ServiceDetailSheet :service="service" tld="stackvo.loc" :model-value="true" />
        </v-app>`,
    },
    {
      props: { service },
      // `teleport: true` renders the sheet in place. Without it the panel lands
      // on `document.body`, outside the wrapper, and every assertion reads an
      // empty string as "the section is missing".
      global: { plugins: [vuetify, i18n], stubs: { teleport: true } },
    }
  );

beforeEach(() => {
  vi.clearAllMocks();
  api.containerInspect.mockResolvedValue({ networks: [], mounts: [], ports: [] });
  api.serviceConnection.mockResolvedValue(MASKED);
  api.serviceDbClients.mockResolvedValue(CLIENTS);
  api.serviceOpenInClient.mockResolvedValue(undefined);
  api.mailStatus.mockResolvedValue({ available: false });
  api.mailMessages.mockResolvedValue([]);
  api.dbTargets.mockResolvedValue([]);
});

describe('the two addresses', () => {
  it('shows the host address and the container one, and says which is which', async () => {
    const wrapper = mountSheet();
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain('mongodb://root:••••••••@127.0.0.1:27017/stackvo?authSource=admin');
    expect(text).toContain('mongodb://root:••••••••@stackvo-mongo:27017/stackvo?authSource=admin');
    expect(text).toContain('From this machine');
    expect(text).toContain('From another container');
  });

  /**
   * The sentence is the fix. Two URIs with no explanation is the same puzzle
   * the container name and the port table already were.
   */
  it('says why the container name does not work from here', async () => {
    const wrapper = mountSheet();
    await flushPromises();

    expect(wrapper.text()).toContain('only resolves inside the Docker network');
  });

  /**
   * A running container that publishes nothing has no host address. Inventing
   * `127.0.0.1` for it would be the same class of wrong answer as the container
   * name — a string that looks right and reaches nothing.
   */
  it('drops the host row rather than inventing an address for it', async () => {
    api.serviceConnection.mockResolvedValue({ ...MASKED, fromHost: null });
    const wrapper = mountSheet();
    await flushPromises();

    expect(wrapper.text()).not.toContain('127.0.0.1');
    expect(wrapper.text()).toContain('publishes no port to the host');
  });

  /** Most rows are admin UIs opened at a domain; the section is not for them. */
  it('is absent for a service with no connection string', async () => {
    api.serviceConnection.mockResolvedValue(null);
    const wrapper = mountSheet({ ...MONGO, id: 'mongo-express' });
    await flushPromises();

    expect(wrapper.text()).not.toContain('Connection string');
  });
});

describe('the password', () => {
  it('is bullets until it is asked for', async () => {
    const wrapper = mountSheet();
    await flushPromises();

    expect(api.serviceConnection).toHaveBeenCalledWith('mongo', false);
    expect(wrapper.text()).toContain('••••••••');
    expect(wrapper.text()).not.toContain('root:root@');
  });

  it('asks Rust again with reveal set, rather than unmasking a string it holds', async () => {
    const wrapper = mountSheet();
    await flushPromises();

    api.serviceConnection.mockResolvedValue(REVEALED);
    const reveal = wrapper
      .findAll('button')
      .find((button) => button.text().includes('Reveal the value'));
    await reveal.trigger('click');
    await flushPromises();

    expect(api.serviceConnection).toHaveBeenLastCalledWith('mongo', true);
    expect(wrapper.text()).toContain(
      'mongodb://root:root@127.0.0.1:27017/stackvo?authSource=admin'
    );
  });

  /**
   * Copying is a different intention from revealing: one is "let me use it",
   * the other "let me read it", and only the second belongs in a screenshot.
   * A masked URI on the clipboard is a string that fails to connect.
   */
  it('copies the working string while the screen keeps showing bullets', async () => {
    const writeText = vi.fn(async () => {});
    vi.stubGlobal('navigator', { clipboard: { writeText } });

    const wrapper = mountSheet();
    await flushPromises();

    api.serviceConnection.mockResolvedValue(REVEALED);
    const copy = wrapper
      .findAll('button')
      .find((button) => button.attributes('aria-label') === 'Copy');
    await copy.trigger('click');
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith(
      'mongodb://root:root@127.0.0.1:27017/stackvo?authSource=admin'
    );
    // And the sheet is still masked — the reveal button was never pressed.
    expect(wrapper.text()).toContain('••••••••');

    vi.unstubAllGlobals();
  });
});

/**
 * G-3. The string had been right and copyable since this section was written;
 * what nobody had written was the step that hands it to the application it was
 * built for.
 */
/**
 * A second way in, for the menu only.
 *
 * `mountSheet` stubs `teleport` so the panel renders inside the wrapper, and
 * that stub does not merely relocate an overlay's content — it drops it. The
 * sheet is readable and the menu inside it is not, which is invisible until
 * something asserts on a menu. So these mount the real thing and read
 * `document.body`, where both the panel and the menu land.
 */
const mountSheetForOverlays = (service = MONGO) =>
  mount(
    {
      components: { ServiceDetailSheet },
      props: ['service'],
      template: `
        <v-app>
          <ServiceDetailSheet :service="service" tld="stackvo.loc" :model-value="true" />
        </v-app>`,
    },
    { props: { service }, global: { plugins: [vuetify, i18n] }, attachTo: document.body }
  );

const overlays = () => document.body.textContent;

afterEach(() => {
  document.body.innerHTML = '';
});

describe('opening it in a client', () => {
  it('offers the clients this machine actually has, greying out the rest', async () => {
    const wrapper = mountSheetForOverlays();
    await flushPromises();

    expect(api.serviceDbClients).toHaveBeenCalledWith('mongo');

    // Queried from `document`, not from the wrapper: without the teleport stub
    // the panel itself is on `document.body` too.
    document.querySelector('[aria-label="Open in a database client"]').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const text = overlays();
    expect(text).toContain('MongoDB Compass');
    // Absent would read as "this app has never heard of DBeaver", which is a
    // different statement from "it is not installed here".
    expect(text).toContain('DBeaver');
  });

  it('hands the service and the chosen client to Rust, and nothing else', async () => {
    const wrapper = mountSheetForOverlays();
    await flushPromises();

    // Queried from `document`, not from the wrapper: without the teleport stub
    // the panel itself is on `document.body` too.
    document.querySelector('[aria-label="Open in a database client"]').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const compass = [...document.querySelectorAll('.v-list-item')].find((el) =>
      el.textContent.includes('MongoDB Compass')
    );
    compass.click();
    await flushPromises();

    // The URI is not one of the arguments. Rust re-resolves it with the real
    // password, because a string that has been through the front end is one
    // that could have been changed there.
    expect(api.serviceOpenInClient).toHaveBeenCalledWith('mongo', 'compass');
  });

  /**
   * The container address is a name on a Docker network. Offering to open it
   * would rebuild the exact confusion the two rows exist to prevent, so the
   * button appears once and on the host row.
   */
  it('is offered on the host row only', async () => {
    const wrapper = mountSheet();
    await flushPromises();

    const buttons = wrapper
      .findAll('button')
      .filter((button) => button.attributes('aria-label') === 'Open in a database client');
    expect(buttons).toHaveLength(1);
  });

  /**
   * Most services have no connection string, and AMQP or SMTP has one no
   * desktop database client takes. An empty list hides the button rather than
   * opening an empty menu.
   */
  it('is absent when nothing on this machine opens that kind of address', async () => {
    api.serviceDbClients.mockResolvedValue([]);
    const wrapper = mountSheet();
    await flushPromises();

    const open = wrapper
      .findAll('button')
      .find((button) => button.attributes('aria-label') === 'Open in a database client');
    expect(open).toBeUndefined();
  });

  /**
   * A picker that cannot be built is a picker that is not shown. The copy
   * button beside it still works, and it is what this replaces — so a failure
   * here must not take the section down with it.
   */
  it('leaves the rest of the section working when the list cannot be fetched', async () => {
    api.serviceDbClients.mockRejectedValue(new Error('nope'));
    const wrapper = mountSheet();
    await flushPromises();

    expect(wrapper.text()).toContain(
      'mongodb://root:••••••••@127.0.0.1:27017/stackvo?authSource=admin'
    );
    expect(
      wrapper.findAll('button').find((b) => b.attributes('aria-label') === 'Copy')
    ).toBeTruthy();
  });
});
