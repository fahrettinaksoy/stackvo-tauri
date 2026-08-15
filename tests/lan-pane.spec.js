import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * E-3: the name a phone on the same Wi-Fi can open the project at.
 *
 * The pane is small and most of it is one switch, so what these cover is the
 * part that is not obvious from looking at it: three different absences that
 * must not be shown as the same thing, and one warning that has to appear
 * *with* the address rather than after somebody has already met it on a phone.
 *
 * The address itself is not tested here and cannot be — it comes from this
 * machine's routing table, so a fixed expectation would be a claim about
 * whichever laptop runs the suite. `lan.rs`'s own tests hold the shape of what
 * is derived from it; this holds what the screen does with the answer.
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
const LanPane = (await import('@/components/project/LanPane.vue')).default;

const vuetify = createVuetify({ components, directives });

/** A status with this project sharing on a working address. */
const SHARING = {
  address: '192.168.1.5',
  suffix: 'sslip.io',
  projects: [{ name: 'shop', host: 'shop.192-168-1-5.sslip.io' }],
  stale: null,
};

const mountPane = (name = 'shop') =>
  mount(
    {
      components: { LanPane },
      props: ['name'],
      template: '<v-app><LanPane :name="name" /></v-app>',
    },
    { props: { name }, global: { plugins: [vuetify, i18n] } }
  );

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.lanStatus = { address: null, suffix: 'sslip.io', projects: [], stale: null };
});

describe('the switch', () => {
  /**
   * The stored intent is read out of the status, not out of a second fetch of
   * the manifest. Two sources that can disagree is a switch showing off beside
   * a live address.
   */
  it('is on when the status lists this project, and off when it does not', async () => {
    replies.lanStatus = SHARING;
    const shop = mountPane('shop');
    await flushPromises();
    expect(shop.find('input[type="checkbox"]').element.checked).toBe(true);

    const blog = mountPane('blog');
    await flushPromises();
    expect(blog.find('input[type="checkbox"]').element.checked).toBe(false);
  });

  it('writes the intent and re-reads the status, rather than assuming it took', async () => {
    const pane = mountPane();
    await flushPromises();

    replies.lanStatus = SHARING;
    await pane.find('input[type="checkbox"]').setValue(true);
    await flushPromises();

    expect(calls).toContainEqual(['projectLanShare', 'shop', true]);
    // The name is derived on the Rust side from the address at the time it is
    // asked for, so the pane cannot compute what to show — it has to ask again.
    expect(calls.filter(([name]) => name === 'lanStatus').length).toBeGreaterThan(1);
    expect(pane.text()).toContain('shop.192-168-1-5.sslip.io');
  });

  /**
   * The name only reaches the router and the certificate on a regenerate, and
   * this pane deliberately does not trigger one — a switch that quietly rebuilt
   * the workspace would be doing an expensive thing behind a cheap control.
   */
  it('tells the page something changed instead of regenerating itself', async () => {
    const pane = mountPane();
    await flushPromises();
    replies.lanStatus = SHARING;

    await pane.find('input[type="checkbox"]').setValue(true);
    await flushPromises();

    expect(calls.some(([name]) => name.startsWith('generate'))).toBe(false);
    expect(pane.findComponent(LanPane).emitted('changed')).toBeTruthy();
    expect(pane.text()).toContain(en.lan.regenerateHint);
  });
});

describe('the three things that can be absent', () => {
  /**
   * On before there is an address. Two situations with one answer — offline,
   * and a machine whose address is public — and the second is a refusal rather
   * than a failure, so the sentence has to cover both without pretending they
   * are the same fault.
   */
  it('says the machine has no address to offer, rather than showing a blank name', async () => {
    replies.lanStatus = {
      address: null,
      suffix: 'sslip.io',
      projects: [{ name: 'shop', host: null }],
      stale: null,
    };
    const pane = mountPane();
    await flushPromises();

    expect(pane.text()).toContain(en.lan.noAddress);
    expect(pane.text()).not.toContain('sslip.io/');
  });

  /**
   * The one that would otherwise be found on a phone. On a phone a certificate
   * warning and a name that does not resolve look identical, and only one of
   * them is expected — so it is said next to the address, before anybody walks
   * across the room with it.
   */
  it('warns about the certificate beside the address, not after it', async () => {
    replies.lanStatus = SHARING;
    const pane = mountPane();
    await flushPromises();

    const text = pane.text();
    expect(text).toContain('shop.192-168-1-5.sslip.io');
    expect(text).toContain(en.lan.certWarning);
  });

  /** A laptop that changed networks: the copy on disk points somewhere else. */
  it('reports a name baked into the generated files from another network', async () => {
    replies.lanStatus = { ...SHARING, stale: 'shop.10-0-0-9.sslip.io' };
    const pane = mountPane();
    await flushPromises();

    expect(pane.text()).toContain('shop.10-0-0-9.sslip.io');
  });

  /** Nothing to say when nothing was ever shared. */
  it('is quiet on a project that never asked', async () => {
    const pane = mountPane();
    await flushPromises();

    const text = pane.text();
    expect(text).not.toContain(en.lan.noAddress);
    expect(text).not.toContain(en.lan.certWarning);
  });
});
