import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { defineComponent, h } from 'vue';

/**
 * The offer a finished build makes about `/etc/hosts`.
 *
 * `project_build` writes nothing to that file and must not — the write is the
 * app's only elevation prompt, and it happens after somebody has read the diff.
 * So the build can finish, bring a container up, and answer on a name the
 * machine does not resolve, with the only hint being a warning icon on a row.
 *
 * What is covered here is the decision to interrupt, which has three ways of
 * being wrong: interrupting for a build that failed, interrupting when the DNS
 * responder already answers for the whole suffix, and interrupting when the
 * line is there. Each is a modal over somebody's work, so each is asserted.
 */

let handler = null;
let subscribed = null;
const off = vi.fn();

vi.mock('@/lib/events', () => ({
  listenAll: async (names, fn) => {
    subscribed = names;
    handler = fn;
    return off;
  },
}));

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

const { useHostsPrompt } = await import('@/composables/useHostsPrompt');

/** Mount something that uses the composable, and hand back what it opened. */
async function host() {
  const opened = [];
  const wrapper = mount(
    defineComponent({
      setup() {
        useHostsPrompt((domain, project) => opened.push([domain, project]));
        return () => h('div');
      },
    })
  );
  await flushPromises();
  return { wrapper, opened };
}

/** Deliver one finished-build event and let the two lookups settle. */
async function built(payload) {
  handler('build:success', payload);
  await flushPromises();
}

beforeEach(() => {
  calls.length = 0;
  handler = null;
  subscribed = null;
  off.mockClear();
  for (const key of Object.keys(replies)) delete replies[key];
  replies.dnsStatus = { listening: false, configured: false };
  replies.projectGet = { name: 'shop', domain: 'shop.loc', domainConfigured: false };
});

describe('the hosts offer a finished build makes', () => {
  it('opens the review dialog for a name the machine cannot resolve', async () => {
    const { opened } = await host();
    expect(subscribed).toEqual(['build:success']);

    await built({ subject: 'shop', success: true });
    expect(opened).toEqual([['shop.loc', 'shop']]);
  });

  it('reads the project rather than trusting the list the page is holding', async () => {
    // The same event refreshes every list on screen, so a `domainConfigured`
    // taken from a row would be a race against a refetch — decided in favour of
    // whichever resolved first, and the loser is a modal nobody asked for.
    await host();
    await built({ subject: 'shop', success: true });
    expect(calls.map(([name]) => name)).toContain('projectGet');
    expect(calls.find(([name]) => name === 'projectGet')).toEqual(['projectGet', 'shop']);
  });

  it('says nothing when the build failed', async () => {
    // `build:success` is the name of the finished event, not a claim about it:
    // a failed run is emitted under the same name with `success: false`.
    const { opened } = await host();
    await built({ subject: 'shop', success: false, error: 'no such image' });
    expect(opened).toEqual([]);
  });

  it('says nothing when the hosts line is already there', async () => {
    replies.projectGet = { name: 'shop', domain: 'shop.loc', domainConfigured: true };
    const { opened } = await host();
    await built({ subject: 'shop', success: true });
    expect(opened).toEqual([]);
  });

  it('says nothing for a project with no domain at all', async () => {
    replies.projectGet = { name: 'shop', domain: '', domainConfigured: false };
    const { opened } = await host();
    await built({ subject: 'shop', success: true });
    expect(opened).toEqual([]);
  });

  it('says nothing while the DNS responder is answering for the suffix', async () => {
    // E-1 answers for every name under the suffix, which is what makes the
    // per-project line unnecessary — and it is not even worth asking the
    // backend about the project, because the answer cannot change the outcome.
    replies.dnsStatus = { listening: true, configured: true };
    const { opened } = await host();
    await built({ subject: 'shop', success: true });

    expect(opened).toEqual([]);
    expect(calls.map(([name]) => name)).not.toContain('projectGet');
  });

  it('still offers it when the machine asks a responder that is down', async () => {
    // The broken state: configured to ask us, nothing listening. Every name
    // under the suffix fails, so the hosts line is exactly the repair.
    replies.dnsStatus = { listening: false, configured: true };
    const { opened } = await host();
    await built({ subject: 'shop', success: true });
    expect(opened).toEqual([['shop.loc', 'shop']]);
  });

  it('takes the subscription down with the page', async () => {
    const { wrapper } = await host();
    wrapper.unmount();
    expect(off).toHaveBeenCalled();
  });

  it('does not open over a page that has already gone', async () => {
    // The lookups are two IPC round trips; a route change in the middle of them
    // used to land a modal on whatever page came next.
    const { wrapper, opened } = await host();
    let resolve;
    replies.projectGet = () => new Promise((r) => (resolve = r));

    handler('build:success', { subject: 'shop', success: true });
    await flushPromises();
    wrapper.unmount();
    resolve({ name: 'shop', domain: 'shop.loc', domainConfigured: false });
    await flushPromises();

    expect(opened).toEqual([]);
  });
});
