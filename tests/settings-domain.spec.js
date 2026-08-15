import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Domain pane, mounted.
 *
 * Fourth out of `Settings.vue` under §14.16, and the first that needed the
 * shared `.env` editor: six panes write one file through one diff, so the
 * editor is injected. `useSharedEnvEditor` falls back to its own instance when
 * nothing provided one — which is what makes this file possible, and is the
 * only reason a pane that depends on shared state can be mounted alone.
 *
 * The logic came out a round earlier and is covered in `stack-shape.spec.js`.
 * What is asserted here is what only exists once the markup runs: that the two
 * suffix fields are wired to the one key, that the save button is gated on
 * validity, that the hosts list separates missing from stale, and that the
 * regenerate notice appears for the changes that need one and not for the
 * others.
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
const DomainPane = (await import('@/components/settings/DomainPane.vue')).default;
const { useEnvEditor, provideEnvEditor } = await import('@/composables/useEnvEditor');

const vuetify = createVuetify({ components, directives });

/**
 * Mounted under a host that provides a *loaded* editor, which is what
 * `Settings.vue` does.
 *
 * The pane deliberately does not read `.env` itself: six panes share one diff
 * over one file, and a pane that loaded on its own mount would discard whatever
 * the others had typed every time the user changed tab. Getting this wrong in
 * the test is what surfaced it — mounted bare, every field renders empty and
 * the save button is correctly disabled, which reads as four broken assertions
 * rather than one missing `load()`.
 */
let editor;

async function render(props = {}) {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { DomainPane },
      props: ['regenerating'],
      setup() {
        editor = provideEnvEditor(useEnvEditor());
        return {};
      },
      template: '<DomainPane :regenerating="regenerating" v-bind="$attrs" />',
    },
    {
      props,
      attachTo: host,
      global: { plugins: [createPinia(), vuetify, i18n] },
    }
  );

  await editor.loadDefaults();
  await editor.load();
  await wrapper.vm.$nextTick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

/** The pane itself, for the handful of assertions that read its own state. */
const pane = (wrapper) => wrapper.findComponent(DomainPane).vm;

const button = (wrapper, label) => wrapper.findAll('button').find((b) => b.text().includes(label));

beforeEach(() => {
  setActivePinia(createPinia());
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.envGet = { DEFAULT_TLD_SUFFIX: 'stackvo.loc', DOCKER_DEFAULT_NETWORK: 'stackvo-net' };
  replies.envDefaults = {};
  replies.hostsOverview = { entries: [], stale: [] };
  replies.containerInspect = { running: true, image: 'traefik:v3', ports: [{ host: 80 }] };
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the domain pane', () => {
  it('renders the suffix as two fields over one key', async () => {
    const wrapper = await render();
    const text = wrapper.text();

    // The preview is the whole point of splitting it: the suffix is never seen
    // on its own, it is always something dot this.
    expect(text).toContain('shop.stackvo.loc');
    expect(pane(wrapper).suffixLabel).toBe('stackvo');
    expect(pane(wrapper).suffixTld).toBe('loc');

    wrapper.unmount();
  });

  it('writes both halves back into the single stored key', async () => {
    const wrapper = await render();

    pane(wrapper).setSuffix('stackvo', 'test');
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain('shop.stackvo.test');
    wrapper.unmount();
  });

  /**
   * The suffix is interpolated straight into `Host(...)`. A save button that
   * stays live over an invalid one hands the generator something that produces
   * a router nothing matches.
   */
  it('will not offer to save an invalid suffix', async () => {
    const wrapper = await render();
    const label = i18n.global.t('settings.save', { count: 1 });

    pane(wrapper).setSuffix('stackvo', 'dev');
    await wrapper.vm.$nextTick();
    expect(button(wrapper, label)?.attributes('disabled'), 'a valid change was blocked').toBe(
      undefined
    );

    // Not an *empty* TLD — clearing that half leaves `stackvo`, a single-label
    // suffix, which is legitimate (`loc` on its own is one). The gate is about
    // values the rules reject.
    pane(wrapper).setSuffix('stackvo', 'lo c');
    await wrapper.vm.$nextTick();
    expect(
      button(wrapper, label)?.attributes('disabled'),
      'a suffix with a space in it was saveable'
    ).toBeDefined();

    wrapper.unmount();
  });

  /**
   * `.dev` is HSTS-preloaded: the browser refuses plain HTTP to it before a
   * request is made, so choosing it with SSL off breaks every address at once.
   */
  it('warns about an HTTPS-only suffix while SSL is off', async () => {
    replies.envGet = { DEFAULT_TLD_SUFFIX: 'stackvo.dev', SSL_ENABLE: 'false' };

    const wrapper = await render();
    expect(pane(wrapper).suffixNeedsHttps).toBe(true);

    pane(wrapper).setBool('SSL_ENABLE', true);
    await wrapper.vm.$nextTick();
    expect(pane(wrapper).suffixNeedsHttps, 'the warning outlived the fix').toBe(false);

    wrapper.unmount();
  });

  it('saves through the parent, which owns the file', async () => {
    const wrapper = await render();

    pane(wrapper).setSuffix('stackvo', 'test');
    await wrapper.vm.$nextTick();
    await button(wrapper, i18n.global.t('settings.save', { count: 1 })).trigger('click');

    expect(
      wrapper.findComponent(DomainPane).emitted('save'),
      'the pane wrote .env behind the view'
    ).toBeTruthy();
    expect(
      calls.some(([n]) => n === 'envSet'),
      'the pane wrote it itself'
    ).toBe(false);

    wrapper.unmount();
  });
});

describe('the hosts list', () => {
  it('offers a fix only when there is something to fix', async () => {
    const label = i18n.global.t('settings.shape.hostsFix');

    let wrapper = await render();
    expect(wrapper.text()).toContain(i18n.global.t('settings.shape.hostsOk'));
    expect(button(wrapper, label)).toBeUndefined();
    wrapper.unmount();

    replies.hostsOverview = {
      entries: [
        { domain: 'shop.loc', configured: true },
        { domain: 'blog.loc', configured: false },
      ],
      stale: ['deleted.loc'],
    };

    wrapper = await render();
    expect(wrapper.text()).toContain('blog.loc');
    expect(wrapper.text()).toContain('deleted.loc');
    expect(button(wrapper, label), 'nothing offered to repair it').toBeTruthy();

    wrapper.unmount();
  });

  /**
   * Both directions in one elevation prompt. Asking twice for one tidy-up is
   * how people stop half way, and a stale line points at 127.0.0.1 for ever.
   */
  it('adds and removes in a single apply', async () => {
    replies.hostsOverview = {
      entries: [{ domain: 'blog.loc', configured: false }],
      stale: ['deleted.loc'],
    };
    replies.hostsApply = () => Promise.resolve();

    const wrapper = await render();
    await button(wrapper, i18n.global.t('settings.shape.hostsFix')).trigger('click');

    await vi.waitFor(() => expect(calls.some(([n]) => n === 'hostsApply')).toBe(true));
    expect(calls.find(([n]) => n === 'hostsApply')).toEqual([
      'hostsApply',
      ['blog.loc'],
      ['deleted.loc'],
    ]);

    wrapper.unmount();
  });

  it('shows a refused elevation instead of looking like it worked', async () => {
    replies.hostsOverview = { entries: [{ domain: 'blog.loc', configured: false }], stale: [] };
    replies.hostsApply = () => Promise.reject(new Error('permission denied'));

    const wrapper = await render();
    await button(wrapper, i18n.global.t('settings.shape.hostsFix')).trigger('click');

    await vi.waitFor(() => expect(wrapper.text()).toContain('permission denied'));
    wrapper.unmount();
  });
});

describe('the proxy', () => {
  it('names the container the whole stack depends on', async () => {
    const wrapper = await render();
    const text = wrapper.text();

    expect(text).toContain('traefik:v3');
    expect(text).toContain(i18n.global.t('engine.running'));
    expect(calls.find(([n]) => n === 'containerInspect')).toEqual(['containerInspect', 'traefik']);

    wrapper.unmount();
  });

  it('reads a dead engine as "not running" rather than failing the pane', async () => {
    replies.containerInspect = () => Promise.reject(new Error('engine unreachable'));

    const wrapper = await render();
    expect(wrapper.text()).toContain(i18n.global.t('engine.down'));
    expect(wrapper.text().trim().length).toBeGreaterThan(0);

    wrapper.unmount();
  });
});

describe('what a save still needs', () => {
  /**
   * Changing the suffix rewrites every routing label and moves what the
   * certificate covers, and none of it reaches the running stack until the
   * files are regenerated. Saving and staying silent is how a setting looks
   * like it did nothing.
   */
  it('asks for a regenerate after a routing change and not after any other', async () => {
    replies.envSet = () => Promise.resolve();
    const wrapper = await render();
    const notice = i18n.global.t('settings.shape.regenerate');

    expect(button(wrapper, notice), 'a notice before anything was saved').toBeUndefined();

    // The parent owns the save, so drive the shared editor the way it does.
    pane(wrapper).setSuffix('stackvo', 'test');
    await editor.save();
    await wrapper.vm.$nextTick();
    expect(button(wrapper, notice), 'a routing change went unannounced').toBeTruthy();

    // And a key that takes effect on its own says nothing.
    editor.clearPending();
    editor.edit('TZ', 'Europe/Istanbul');
    await editor.save();
    await wrapper.vm.$nextTick();
    expect(button(wrapper, notice), 'an ordinary key asked for a regenerate').toBeUndefined();

    wrapper.unmount();
  });

  it('reports the regenerate upward rather than running it itself', async () => {
    replies.envSet = () => Promise.resolve();
    const wrapper = await render();

    pane(wrapper).setSuffix('stackvo', 'test');
    await editor.save();
    await wrapper.vm.$nextTick();

    await button(wrapper, i18n.global.t('settings.shape.regenerate')).trigger('click');

    expect(wrapper.findComponent(DomainPane).emitted('regenerate')).toBeTruthy();
    // The console that reports it belongs to the view, not to this pane.
    expect(
      calls.some(([n]) => n === 'generateRun'),
      'the pane ran the generator behind the view'
    ).toBe(false);

    wrapper.unmount();
  });
});

/**
 * The DNS pane (E-1).
 *
 * Four things here are only visible at this layer, and each is the kind that
 * looks fine and is wrong: that the two switches are two separate acts, that a
 * machine with no recognisable mechanism gets a sentence rather than a dead
 * toggle, that the change is shown *before* the password prompt that applies
 * it, and that "the file is written" is never presented as "this works" — the
 * measurement is a separate button with four separate answers.
 */
describe('the DNS pane', () => {
  const DnsPane = () => import('@/components/settings/DnsPane.vue');

  const STATUS = {
    mechanism: 'resolver',
    writable: true,
    suffix: 'stackvo.loc',
    tld: 'loc',
    port: 15353,
    listening: false,
    tcp: false,
    file: '/etc/resolver/loc',
    configured: false,
    instruction: 'nameserver 127.0.0.1\nport 15353\n',
    stale: [],
    broken: false,
  };

  const CHECK = {
    name: 'stackvo-dns-check.loc',
    udp: { ok: true, detail: '127.0.0.1:15353 answered 127.0.0.1' },
    tcp: { ok: true, detail: '127.0.0.1:15353 answered 127.0.0.1' },
    system: { ok: false, detail: 'stackvo-dns-check.loc does not resolve here' },
    public: { ok: true, detail: 'example.com still resolves' },
    ok: false,
  };

  async function open(over = {}) {
    replies.dnsStatus = { ...STATUS, ...over };
    const component = (await DnsPane()).default;
    const wrapper = mount(
      { components: { DnsPane: component }, template: '<v-app><DnsPane /></v-app>' },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );
    await flushPromises();
    return wrapper;
  }

  const switches = (wrapper) => wrapper.findAll('input[type="checkbox"]');

  it('offers the responder and the machine resolver as two separate switches', async () => {
    const wrapper = await open();
    expect(switches(wrapper)).toHaveLength(2);
  });

  /** The password prompt must not arrive from a switch labelled "turn on". */
  it('starting the responder does not touch the machine resolver', async () => {
    const wrapper = await open();
    replies.dnsStart = { ...STATUS, listening: true, tcp: true };

    await switches(wrapper)[0].setValue(true);
    await flushPromises();

    expect(calls.map((c) => c[0])).toContain('dnsStart');
    expect(calls.map((c) => c[0])).not.toContain('dnsResolverInstall');
  });

  /** What it writes, before it writes it — and what it reloads afterwards. */
  it('shows the change beside the switch that applies it', async () => {
    const wrapper = await open();
    expect(wrapper.text()).toContain('nameserver 127.0.0.1');
    expect(wrapper.text()).toContain('/etc/resolver/loc');
  });

  /**
   * The mechanism is named, because "writes a file" and "writes
   * /etc/NetworkManager/dnsmasq.d/stackvo.conf and reloads NetworkManager" are
   * different amounts of information to consent to.
   */
  it('names the mechanism and the command it runs afterwards', async () => {
    const wrapper = await open({
      mechanism: 'network-manager',
      file: '/etc/NetworkManager/dnsmasq.d/stackvo.conf',
      instruction: 'server=/loc/127.0.0.1#15353\n',
      reload: 'systemctl reload NetworkManager',
    });

    expect(wrapper.text()).toContain(i18n.global.t('dns.mechanisms.network-manager'));
    expect(wrapper.text()).toContain('/etc/NetworkManager/dnsmasq.d/stackvo.conf');
    expect(wrapper.text()).toContain('systemctl reload NetworkManager');
  });

  /**
   * A machine with nothing recognisable in front of its resolver gets the line
   * and no switch. A toggle that quietly does nothing is worse than an
   * explanation — and guessing at a file to write is worse than both.
   */
  it('prints the line to place where nothing recognisable resolves names', async () => {
    const wrapper = await open({
      mechanism: 'manual',
      writable: false,
      file: undefined,
      instruction: 'server=/loc/127.0.0.1#15353\n',
    });

    // One switch: the responder. The machine half is a sentence.
    expect(switches(wrapper)).toHaveLength(1);
    expect(wrapper.text()).toContain('server=/loc/127.0.0.1#15353');
    expect(wrapper.text()).toContain(i18n.global.t('dns.manual'));
  });

  /**
   * The mechanism exists and no password prompt does — Linux without a polkit
   * agent. Offering the switch would be offering something that fails on press.
   */
  it('shows the command instead of a switch where nothing can ask for a password', async () => {
    const wrapper = await open({
      mechanism: 'systemd-resolved',
      writable: false,
      file: '/etc/systemd/resolved.conf.d/stackvo.conf',
      instruction: '[Resolve]\nDNS=127.0.0.1:15353\nDomains=~loc\n',
    });

    expect(switches(wrapper)).toHaveLength(1);
    expect(wrapper.text()).toContain('Domains=~loc');
    expect(wrapper.text()).toContain('/etc/systemd/resolved.conf.d/stackvo.conf');
  });

  /**
   * The pair that can come apart. A responder bound on UDP and not TCP works
   * for almost everything and fails a retry, which is worth one sentence and
   * is not worth reporting as "off".
   */
  it('says so when only half the responder is bound', async () => {
    const wrapper = await open({ listening: true, tcp: false });
    expect(wrapper.text()).toContain(i18n.global.t('dns.udpOnly', { port: 15353 }));

    const both = await open({ listening: true, tcp: true });
    expect(both.text()).not.toContain(i18n.global.t('dns.udpOnly', { port: 15353 }));
  });

  /**
   * The state where everything else on screen looks healthy and nothing
   * resolves: the machine asks a responder that is not there. It is a whole
   * alert rather than a caption because it is the only one of these that is
   * breaking something right now.
   */
  it('says so when the machine asks a responder that is not answering', async () => {
    const wrapper = await open({ listening: false, configured: true, broken: true });
    expect(wrapper.find('[data-test="dns-broken"]').exists()).toBe(true);

    const healthy = await open({ listening: true, tcp: true, configured: true, broken: false });
    expect(healthy.find('[data-test="dns-broken"]').exists()).toBe(false);
  });

  /**
   * A resolver file left behind by a suffix change does not sit there
   * harmlessly — it points at a responder that now refuses that TLD, so those
   * names stop going upstream and start being refused locally.
   */
  it('names the files a suffix change left behind', async () => {
    const wrapper = await open({ stale: ['/etc/resolver/loc'] });
    const alert = wrapper.find('[data-test="dns-stale"]');
    expect(alert.exists()).toBe(true);
    expect(alert.text()).toContain('/etc/resolver/loc');
  });

  /**
   * Somebody else's file at the same path — dnsmasq, Valet, a colleague's
   * script. Said before the switch is pressed: a file discovered to be gone
   * afterwards is not a warning, it is an apology.
   */
  it('warns about a file it would replace, before replacing it', async () => {
    const wrapper = await open({
      foreign: '/etc/resolver/loc: nameserver 127.0.0.1',
    });
    const line = wrapper.find('[data-test="dns-foreign"]');
    expect(line.exists()).toBe(true);
    expect(line.text()).toContain('/etc/resolver/loc');

    const clean = await open();
    expect(clean.find('[data-test="dns-foreign"]').exists()).toBe(false);
  });

  /**
   * The measurement is the point of the button: the responder answering its own
   * probe and the machine resolving a name are different questions, and this
   * fixture is exactly the case where the first passes and the second does not.
   */
  it('reports each probe separately rather than one verdict', async () => {
    const wrapper = await open({ listening: true, tcp: true, configured: true });
    replies.dnsCheck = CHECK;

    await wrapper.find('button').trigger('click');
    await flushPromises();

    const probes = wrapper.find('[data-test="dns-probes"]');
    expect(probes.exists()).toBe(true);
    expect(probes.text()).toContain(i18n.global.t('dns.probes.system'));
    expect(probes.text()).toContain('does not resolve here');
    // The half that passed is still shown as passing: which one failed is the
    // whole of what tells somebody what to fix.
    expect(probes.text()).toContain('127.0.0.1:15353 answered 127.0.0.1');
    expect(probes.findAll('.mdi-alert-circle-outline')).toHaveLength(1);
  });
});

/**
 * Custom routes (E-4).
 *
 * The notes are the feature, so they are what is asserted. Each one stands for
 * a failure that is otherwise completely silent: a 502 from `localhost`, an
 * ignored path, a certificate the browser refuses. A pane that applied them
 * quietly would look identical and be worth much less.
 */
describe('the custom routes pane', () => {
  const RoutesPane = () => import('@/components/settings/RoutesPane.vue');

  async function open(list) {
    replies.routesList = list;
    const component = (await RoutesPane()).default;
    const wrapper = mount(
      { components: { RoutesPane: component }, template: '<v-app><RoutesPane /></v-app>' },
      { global: { plugins: [createPinia(), vuetify, i18n] } }
    );
    await flushPromises();
    return wrapper;
  }

  it('shows what the user typed, not what the proxy was given', async () => {
    const wrapper = await open([
      {
        domain: 'api.loc',
        target: 'http://host.docker.internal:3000',
        rewrittenFrom: 'http://localhost:3000',
        enabled: true,
        notes: ["localhost means the proxy's own container, not this machine"],
      },
    ]);

    const targets = wrapper.findAll('input').map((i) => i.element.value);
    expect(targets, 'an editor showing the rewrite would rewrite the rewrite').toContain(
      'http://localhost:3000'
    );
  });

  it('prints the note beside the row it belongs to', async () => {
    const wrapper = await open([
      {
        domain: 'api.loc',
        target: 'http://host.docker.internal:3000',
        rewrittenFrom: 'http://localhost:3000',
        enabled: true,
        notes: ['sending it to host.docker.internal instead'],
      },
    ]);
    expect(wrapper.text()).toContain('host.docker.internal');
  });

  /** A route the renderer skipped must be visible, or the screen lies. */
  it('shows a route that no longer normalises as an error rather than hiding it', async () => {
    const wrapper = await open([
      {
        domain: 'api.loc',
        target: 'tcp://nope',
        enabled: true,
        notes: [],
        error: '"tcp" is not a scheme the proxy speaks',
      },
    ]);
    expect(wrapper.text()).toContain('not a scheme the proxy speaks');
  });

  it('sends the whole list on save', async () => {
    const wrapper = await open([]);
    replies.routesSave = [];

    const add = wrapper.findAll('button').find((b) => b.text() === i18n.global.t('routes.add'));
    await add.trigger('click');
    await wrapper.findAll('input[type="text"]')[0].setValue('api.loc');
    await flushPromises();

    const save = wrapper.findAll('button').find((b) => b.text() === i18n.global.t('routes.save'));
    await save.trigger('click');
    await flushPromises();

    const sent = calls.find((c) => c[0] === 'routesSave');
    expect(sent[1]).toEqual([
      { domain: 'api.loc', target: 'http://localhost:3000', enabled: true },
    ]);
  });
});
