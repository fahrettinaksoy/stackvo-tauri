import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Container section: three panes over one container.
 *
 * `ContainerPane` is read-only and takes everything as props, so what is worth
 * covering is the two that talk to the engine — and in both the interesting
 * behaviour is a *timing* one that a screenshot would never show: the tunnel
 * URL does not exist when `tunnel_start` returns, and the worker button's
 * meaning depends on what is running at the moment it is pressed.
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

const { useTunnel, TUNNEL_POLL_MS } = await import('@/composables/useTunnel');
const { useWorkers } = await import('@/composables/useWorkers');
const { useCopyTick, COPY_HOLD } = await import('@/composables/useCopyTick');
const { i18n } = await import('@/i18n');
const ContainerPane = (await import('@/components/project/ContainerPane.vue')).default;

const vuetify = createVuetify({ components, directives });
const ref = (value) => ({ value });

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
});

describe('the tunnel', () => {
  /** The status call answers for every project; only this one's row matters. */
  it('picks this project out of a status covering all of them', async () => {
    replies.tunnelStatus = [
      { project: 'blog', running: true, url: 'https://blog.example' },
      { project: 'shop', running: true, url: 'https://shop.example' },
    ];

    const tn = useTunnel(ref('shop'));
    await tn.load();
    expect(tn.tunnel.value.url).toBe('https://shop.example');
  });

  it('reads no row for this project as no tunnel, not as an error', async () => {
    replies.tunnelStatus = [{ project: 'blog', running: true }];

    const tn = useTunnel(ref('shop'));
    await tn.load();
    expect(tn.tunnel.value).toBe(null);
    expect(tn.error.value).toBe(null);
  });

  describe('starting', () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    /**
     * The URL is Cloudflare's to assign and arrives seconds after the sidecar
     * starts. Returning at `tunnel_start` would show a running tunnel with no
     * address, which is the state users report as "it did nothing".
     */
    it('keeps polling until the URL appears', async () => {
      let polls = 0;
      replies.tunnelStart = null;
      replies.tunnelStatus = () => {
        polls += 1;
        return Promise.resolve([
          { project: 'shop', running: true, url: polls >= 3 ? 'https://shop.example' : null },
        ]);
      };

      const tn = useTunnel(ref('shop'));
      const done = tn.start();

      await vi.advanceTimersByTimeAsync(TUNNEL_POLL_MS * 4);
      await done;

      expect(polls, 'stopped as soon as the URL was there').toBe(3);
      expect(tn.tunnel.value.url).toBe('https://shop.example');
      expect(tn.busy.value).toBe(false);
    });

    /**
     * The sidecar is up either way. A button left spinning forever would say
     * otherwise, and there is nothing further the user could press.
     */
    it('gives up spinning when the URL never arrives', async () => {
      replies.tunnelStart = null;
      replies.tunnelStatus = [{ project: 'shop', running: true, url: null }];

      const tn = useTunnel(ref('shop'));
      const done = tn.start();
      await vi.advanceTimersByTimeAsync(TUNNEL_POLL_MS * 25);
      await done;

      expect(tn.busy.value).toBe(false);
      expect(tn.error.value).toBe(null);
    });

    it('reports a refused start rather than polling for nothing', async () => {
      replies.tunnelStart = () => Promise.reject({ code: 'DOCKER_UNAVAILABLE', message: 'down' });

      const tn = useTunnel(ref('shop'));
      await tn.start();

      expect(tn.error.value.code).toBe('DOCKER_UNAVAILABLE');
      expect(calls.some(([n]) => n === 'tunnelStatus'), 'polled after a failed start').toBe(false);
      expect(tn.busy.value).toBe(false);
    });
  });

  it('re-reads the status after stopping, rather than assuming', async () => {
    replies.tunnelStop = null;
    replies.tunnelStatus = [];

    const tn = useTunnel(ref('shop'));
    tn.tunnel.value = { project: 'shop', running: true, url: 'https://shop.example' };

    expect(await tn.stop()).toBe(true);
    expect(calls.map(([n]) => n)).toEqual(['tunnelStop', 'tunnelStatus']);
    expect(tn.tunnel.value).toBe(null);
  });
});

describe('the workers', () => {
  beforeEach(() => {
    replies.workerOptions = ['queue', 'scheduler'];
    replies.workerStatus = [
      { project: 'shop', kind: 'queue', restarts: 2 },
      { project: 'blog', kind: 'queue', restarts: 0 },
    ];
  });

  it("shows only this project's sidecars, from a status covering all of them", async () => {
    const w = useWorkers(ref('shop'));
    await w.load();

    expect(w.workers.value).toHaveLength(1);
    expect(w.workerFor('queue').restarts).toBe(2);
    expect(w.workerFor('scheduler'), 'an offered kind that is not running').toBe(null);
  });

  /** One button per kind, and what it does depends on what is running now. */
  it('stops a running kind and starts a stopped one', async () => {
    replies.workerStop = null;
    replies.workerStart = null;

    const w = useWorkers(ref('shop'));
    await w.load();
    calls.length = 0;

    await w.toggle('queue');
    expect(calls[0]).toEqual(['workerStop', 'shop', 'queue']);

    calls.length = 0;
    await w.toggle('scheduler');
    expect(calls[0]).toEqual(['workerStart', 'shop', 'scheduler']);
  });

  /** The busy key is the kind, so only the pressed row's button spins. */
  it('marks the kind that is working, not every row', async () => {
    let settle;
    replies.workerStart = () => new Promise((resolve) => (settle = resolve));

    const w = useWorkers(ref('shop'));
    await w.load();

    const done = w.toggle('scheduler');
    expect(w.busy.value).toBe('scheduler');
    settle(null);
    await done;
    expect(w.busy.value).toBe(null);
  });

  /** The boundary is untyped; a non-list must not make the pane throw. */
  it('reads a misbehaving reply as no workers at all', async () => {
    replies.workerOptions = null;
    replies.workerStatus = null;

    const w = useWorkers(ref('shop'));
    await w.load();

    expect(w.kinds.value).toEqual([]);
    expect(w.workers.value).toEqual([]);
  });

  it('reports a refused start and stops spinning', async () => {
    replies.workerStart = () => Promise.reject({ code: 'DOCKER_UNAVAILABLE', message: 'down' });

    const w = useWorkers(ref('shop'));
    await w.load();

    expect(await w.toggle('scheduler')).toBe(false);
    expect(w.error.value.code).toBe('DOCKER_UNAVAILABLE');
    expect(w.busy.value).toBe(null);
  });
});

describe('the copy tick', () => {
  /** A page has several copy buttons; a bare boolean would tick all of them. */
  it('ticks the button that was pressed, and only that one', async () => {
    vi.useFakeTimers();
    // jsdom ships no clipboard at all, so the success path has to be given one.
    const written = [];
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText: (v) => (written.push(v), Promise.resolve()) },
      configurable: true,
    });

    const { copied, copy, reset } = useCopyTick();
    reset();

    expect(await copy('stackvo-shop', 'cname')).toBe(true);
    expect(copied.value).toBe('cname');

    expect(written).toEqual(['stackvo-shop']);

    await vi.advanceTimersByTimeAsync(COPY_HOLD);
    expect(copied.value).toBe(null);
    vi.useRealTimers();
  });

  it('does not tick when there is no clipboard', async () => {
    const original = globalThis.navigator.clipboard;
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText: () => Promise.reject(new Error('denied')) },
      configurable: true,
    });

    const { copied, copy, reset } = useCopyTick();
    reset();
    expect(await copy('x', 'cname')).toBe(false);
    expect(copied.value).toBe(null);

    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: original,
      configurable: true,
    });
  });
});

describe('the container pane', () => {
  it('reports what Docker says, without inspecting again', async () => {
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
            imageSize: 512 * 1024 * 1024,
            state: 'running',
            // Shaped from `engine::ContainerDetails`: the `Vec` fields are
            // never absent, so a defensively minimal fixture would be testing
            // a payload the back end cannot send.
            ports: [{ container: 80, host: 8080 }],
            networks: ['stackvo-net'],
            gateway: '172.20.0.1',
            restartCount: 0,
            restartPolicy: 'unless-stopped',
            startedAt: '2026-08-06T09:00:00Z',
            created: '2026-08-01T12:00:00Z',
            running: true,
          },
          running: true,
        },
        global: { plugins: [vuetify, i18n] },
      }
    );

    expect(calls, 'a read-only pane must not talk to the back end').toEqual([]);
    expect(wrapper.text()).toContain('stackvo-shop');
    expect(wrapper.text()).toContain('512');
  });

  it('says the project is not built rather than showing empty fields', async () => {
    const wrapper = mount(
      {
        components: { ContainerPane },
        template: '<v-app><ContainerPane :project="{ built: false }" /></v-app>',
      },
      { global: { plugins: [vuetify, i18n] } }
    );

    expect(wrapper.text()).toContain(i18n.global.t('projects.notBuilt'));
  });
});
