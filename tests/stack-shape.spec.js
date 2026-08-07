import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref } from 'vue';

/**
 * How the stack is addressed.
 *
 * `DEFAULT_TLD_SUFFIX` is interpolated straight into every generated routing
 * label — `Host(\`shop.SUFFIX\`)` — and into the list of names the certificate
 * has to cover. **Nothing downstream checks it again.** A suffix with a space
 * or a leading dot produces a compose file that parses, a stack that comes up,
 * and not one address that resolves.
 *
 * The rules lived in `Settings.vue` and had no test, which is the ordinary
 * outcome for validation trapped inside a 2,816-line component: it is not that
 * nobody wanted to test it, it is that reaching it meant mounting the view.
 */

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

const { useStackShape, useHostsOverview, useProxy, splitSuffix, joinSuffix, TLD_CHOICES } =
  await import('@/composables/useStackShape');
const { useEnvEditor } = await import('@/composables/useEnvEditor');

/** Rules return `true` or a message, so the message is the failure. */
const t = (key) => key;
const passes = (rules, value) => rules.every((r) => r(value) === true);

async function shape(file = {}) {
  replies.envGet = { ...file };
  replies.envDefaults = {};
  replies.envSet = () => Promise.resolve();

  const env = useEnvEditor();
  await env.loadDefaults();
  await env.load();
  return { env, ...useStackShape(env, t) };
}

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
});

describe('splitting the suffix', () => {
  /**
   * `stackvo.loc` is a namespace and a TLD, and only the second half is what
   * someone means by "can I use .dev instead". The split is on the **last**
   * dot, so a label may itself contain dots.
   */
  it.each([
    ['stackvo.loc', 'stackvo', 'loc'],
    ['my.team.stackvo.loc', 'my.team.stackvo', 'loc'],
    ['loc', '', 'loc'],
    ['', '', ''],
    ['  stackvo.loc  ', 'stackvo', 'loc'],
  ])('splits %s into %s + %s', (input, label, tld) => {
    expect(splitSuffix(input)).toEqual({ label, tld });
  });

  it('survives a nullish value rather than throwing on a keystroke', () => {
    expect(splitSuffix(null)).toEqual({ label: '', tld: '' });
    expect(splitSuffix(undefined)).toEqual({ label: '', tld: '' });
  });

  /** Rejoining must not leave the dot an empty half would produce. */
  it.each([
    [['stackvo', 'loc'], 'stackvo.loc'],
    [['', 'loc'], 'loc'],
    [['stackvo', ''], 'stackvo'],
    [['', ''], ''],
    [[' stackvo ', ' loc '], 'stackvo.loc'],
  ])('joins %j into %s', (parts, joined) => {
    expect(joinSuffix(...parts)).toBe(joined);
  });

  it('round-trips through the two fields the form shows', () => {
    for (const suffix of ['stackvo.loc', 'my.team.dev', 'loc']) {
      const { label, tld } = splitSuffix(suffix);
      expect(joinSuffix(label, tld)).toBe(suffix);
    }
  });
});

describe('the two suffix fields', () => {
  it('reads the halves out of the stored key and writes them back as one', async () => {
    const s = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.loc' });

    expect(s.suffixLabel.value).toBe('stackvo');
    expect(s.suffixTld.value).toBe('loc');

    s.setSuffix('stackvo', 'dev');
    expect(s.env.edits.value.DEFAULT_TLD_SUFFIX).toBe('stackvo.dev');
    expect(s.suffixTld.value, 'the fields did not follow the edit').toBe('dev');
  });

  it('lets the label half be cleared, leaving a bare TLD', async () => {
    const s = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.loc' });

    s.setSuffix('', 'loc');
    expect(s.env.edits.value.DEFAULT_TLD_SUFFIX).toBe('loc');
    expect(passes(s.suffixLabelRules, ''), 'an empty label is legitimate').toBe(true);
  });

  it('offers the four common TLDs without refusing the others', async () => {
    expect(TLD_CHOICES).toEqual(['loc', 'test', 'localhost', 'dev']);

    const s = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.internal' });
    expect(passes(s.suffixTldRules, 'internal'), 'the picker is a shortcut, not a whitelist').toBe(
      true
    );
  });
});

describe('what a suffix is allowed to be', () => {
  it.each(['stackvo.loc', 'my.team.dev', 'a-b.loc', 'x1.test', 'loc'])('accepts %s', async (v) => {
    const s = await shape();
    expect(passes(s.suffixTldRules, splitSuffix(v).tld)).toBe(true);
  });

  /**
   * Each of these produces a router that silently never matches: the stack
   * comes up, every container is healthy, and no address resolves.
   */
  it.each([
    ['an empty value', ''],
    ['a leading dot', '.loc'],
    ['a trailing dot', 'loc.'],
    ['a scheme', 'https://loc'],
    ['a space', 'my loc'],
    ['a leading hyphen', '-loc'],
    ['upper case', 'LOC'],
    ['a slash', 'loc/x'],
  ])('refuses %s', async (_label, value) => {
    const s = await shape();
    expect(passes(s.suffixTldRules, value)).toBe(false);
  });
});

describe('the docker network name', () => {
  it('is laxer than a hostname, because Docker is', async () => {
    const s = await shape();

    // Upper case and underscores are legal network names and not legal
    // hostname labels; one rule for both would reject a working setup.
    expect(passes(s.networkRules, 'stackvo-net')).toBe(true);
    expect(passes(s.networkRules, 'StackVo_Net.1')).toBe(true);

    expect(passes(s.networkRules, '')).toBe(false);
    expect(passes(s.networkRules, '-leading')).toBe(false);
    expect(passes(s.networkRules, 'has space')).toBe(false);
  });
});

describe('the save gate', () => {
  it('is closed while either key is invalid', async () => {
    const s = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.loc', DOCKER_DEFAULT_NETWORK: 'net' });
    expect(s.valid.value).toBe(true);

    s.env.edit('DEFAULT_TLD_SUFFIX', '.loc');
    expect(s.valid.value, 'a broken suffix was saveable').toBe(false);

    s.env.edit('DEFAULT_TLD_SUFFIX', 'stackvo.loc');
    s.env.edit('DOCKER_DEFAULT_NETWORK', '');
    expect(s.valid.value, 'an empty network was saveable').toBe(false);
  });
});

describe('the HTTPS-only warning', () => {
  /**
   * `.dev` is HSTS-preloaded: the browser refuses plain HTTP to it before a
   * request is made. Choosing it for the whole stack with SSL off breaks every
   * address at once, not one project's.
   */
  it('fires for a preloaded TLD with SSL off, and not otherwise', async () => {
    const off = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.dev', SSL_ENABLE: 'false' });
    expect(off.suffixNeedsHttps.value).toBe(true);

    off.env.edit('SSL_ENABLE', 'true');
    expect(off.suffixNeedsHttps.value, 'the warning outlived the fix').toBe(false);

    const safe = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.loc', SSL_ENABLE: 'false' });
    expect(safe.suffixNeedsHttps.value).toBe(false);
  });

  it('is case-insensitive about the TLD', async () => {
    const s = await shape({ DEFAULT_TLD_SUFFIX: 'stackvo.DEV', SSL_ENABLE: 'false' });
    expect(s.suffixNeedsHttps.value).toBe(true);
  });
});

describe('the hosts overview', () => {
  it('separates what is missing from what is stale', async () => {
    replies.hostsOverview = {
      entries: [
        { domain: 'shop.loc', configured: true },
        { domain: 'blog.loc', configured: false },
      ],
      stale: ['deleted.loc'],
    };

    const h = useHostsOverview();
    await h.load();

    expect(h.missing.value.map((e) => e.domain)).toEqual(['blog.loc']);
    expect(h.stale.value).toEqual(['deleted.loc']);
    expect(h.needsWork.value).toBe(true);
  });

  /**
   * Both directions in one call. Asking for an elevation prompt twice for one
   * tidy-up is how people stop half way — and a stale line points at 127.0.0.1
   * for ever with nothing looking for it.
   */
  it('adds and removes in a single elevated apply', async () => {
    replies.hostsOverview = {
      entries: [{ domain: 'blog.loc', configured: false }],
      stale: ['deleted.loc'],
    };
    replies.hostsApply = () => Promise.resolve();

    const h = useHostsOverview();
    await h.load();
    await h.fix();

    expect(calls.find(([n]) => n === 'hostsApply')).toEqual([
      'hostsApply',
      ['blog.loc'],
      ['deleted.loc'],
    ]);
    // Re-read, or the pane still offers to fix what it just fixed.
    expect(calls.filter(([n]) => n === 'hostsOverview').length).toBe(2);
  });

  it('reports a refused elevation instead of looking like it worked', async () => {
    replies.hostsOverview = { entries: [{ domain: 'blog.loc', configured: false }], stale: [] };
    replies.hostsApply = () => Promise.reject(new Error('permission denied'));

    const h = useHostsOverview();
    await h.load();
    await h.fix();

    expect(h.error.value.message).toBe('permission denied');
    expect(h.fixing.value, 'the button stayed spinning').toBe(false);
  });

  /** No workspace means no overview, and that is an empty list, not a crash. */
  it('survives the boundary answering nothing', async () => {
    replies.hostsOverview = () => Promise.reject(new Error('no workspace'));

    const h = useHostsOverview();
    await h.load();

    expect(h.missing.value).toEqual([]);
    expect(h.stale.value).toEqual([]);
    expect(h.needsWork.value).toBe(false);
  });
});

describe('the proxy', () => {
  it('builds its dashboard URL from the current suffix', async () => {
    const tld = ref('stackvo.loc');
    const p = useProxy(tld);

    expect(p.dashboard.value).toBe('https://traefik.stackvo.loc/dashboard/');

    tld.value = 'dev.loc';
    expect(p.dashboard.value, 'the link kept the old suffix').toBe(
      'https://traefik.dev.loc/dashboard/'
    );
  });

  it('offers no link at all before the suffix is known', () => {
    expect(useProxy(ref('')).dashboard.value).toBe(null);
  });

  /** Its own container name, not a catalog id — Traefik has no catalog entry. */
  it('inspects the container by name', async () => {
    replies.containerInspect = { running: true, image: 'traefik:v3', ports: [{ host: 80 }] };

    const p = useProxy(ref('loc'));
    await p.load();

    expect(calls.find(([n]) => n === 'containerInspect')).toEqual(['containerInspect', 'traefik']);
    expect(p.proxy.value.image).toBe('traefik:v3');
    expect(p.ports.value).toBe('80');
  });

  it('reads a dead engine as "no proxy" rather than failing the pane', async () => {
    replies.containerInspect = () => Promise.reject(new Error('engine unreachable'));

    const p = useProxy(ref('loc'));
    await p.load();

    expect(p.proxy.value).toBe(null);
    expect(p.ports.value).toBe('');
  });
});
