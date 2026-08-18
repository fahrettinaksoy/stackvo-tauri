import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Catalogue pane, and specifically the half that writes an offline bundle.
 *
 * §3 #31. `market::bundle` and `stackvo market-bundle` came first because the
 * person producing a bundle is usually an operator at a terminal; this is the
 * other audience, and it is the one that cannot be told to run a command —
 * somebody sitting at the machine that has the network, about to walk a folder
 * over to one that does not.
 *
 * ## Why the assertions are about what is *said*, not only about the call
 *
 * Two facts decide whether the bundle is any use on the other side, and both
 * are on the far end of a corridor by the time they matter:
 *
 * * an unsigned bundle is refused outright by a machine whose policy sets
 *   `requireSignature`;
 * * a withdrawn version travels as an index row without its files, so the
 *   catalogue over there is smaller than the one over here.
 *
 * The Rust side reports both (`signed`, `skipped`). A pane that made the call
 * correctly and printed neither would pass every test about the call and still
 * send somebody away with a folder that does not work.
 */

globalThis.visualViewport = undefined;

const replies = {};
const calls = [];
const dialog = { open: null };

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

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args) => dialog.open?.(...args),
}));

const { i18n } = await import('@/i18n');
const CataloguePane = (await import('@/components/settings/CataloguePane.vue')).default;

const vuetify = createVuetify({ components, directives });

/** A machine that has fetched a catalogue — the state the button needs. */
const FETCHED = {
  fetched: true,
  sourceLocation: 'https://packages.example/stackvo',
  packages: 12,
  installed: 3,
  signatureRequired: false,
};

const BUNDLED = {
  packages: 12,
  versions: 47,
  files: 214,
  bytes: 3_250_586,
  skipped: [],
  signed: true,
};

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);
  setActivePinia(createPinia());

  const wrapper = mount(CataloguePane, {
    attachTo: host,
    global: { plugins: [vuetify, i18n] },
  });

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

const button = (wrapper, label) => wrapper.findAll('button').find((b) => b.text().includes(label));

beforeEach(() => {
  calls.length = 0;
  dialog.open = null;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.marketStatus = FETCHED;
  replies.policyStatus = { market: {} };
  replies.marketBundle = BUNDLED;
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('writing an offline bundle', () => {
  it('is offered only once there is a catalogue to copy', async () => {
    replies.marketStatus = { fetched: false, packages: 0, installed: 0 };
    const wrapper = await render();

    const action = button(wrapper, i18n.global.t('catalogueSettings.bundleAction'));
    expect(action, 'the bundle button is rendered').toBeTruthy();
    expect(action.attributes('disabled')).toBeDefined();
    // And it says why, rather than being a dead control.
    expect(wrapper.text()).toContain(i18n.global.t('catalogueSettings.bundleNeedsCatalogue'));
  });

  it('sends the folder that was chosen, and nothing else', async () => {
    dialog.open = async () => '/Volumes/stick/stackvo';
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The destination is the only argument: the *source* is the one this
    // machine already fetched from, and letting the pane choose a second one
    // would produce a bundle whose contents nobody here has verified.
    expect(calls).toContainEqual(['marketBundle', '/Volumes/stick/stackvo']);
  });

  it('does nothing at all when the picker is dismissed', async () => {
    // `open` resolves to null on cancel. Calling the command with `null` would
    // reach Rust as a path of "null" and create a directory called that.
    dialog.open = async () => null;
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calls.map(([name]) => name)).not.toContain('marketBundle');
  });

  it('reports the size in something a person can compare to a disk', async () => {
    dialog.open = async () => '/Volumes/stick/stackvo';
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const text = wrapper.text();
    expect(text).toContain('12');
    expect(text).toContain('47');
    expect(text).toContain('214');
    // 3,250,586 bytes. A byte count does not answer "will this fit".
    expect(text).toContain('3.1 MiB');
  });

  it('says when no signature travelled, because the far end will refuse it', async () => {
    replies.marketBundle = { ...BUNDLED, signed: false };
    dialog.open = async () => '/Volumes/stick/stackvo';
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain(i18n.global.t('catalogueSettings.bundleUnsigned'));
  });

  it('names the versions that did not travel', async () => {
    replies.marketBundle = {
      ...BUNDLED,
      versions: 46,
      skipped: ['mysql@5.5 — withdrawn by its publisher: a bad image tag shipped in this build'],
    };
    dialog.open = async () => '/Volumes/stick/stackvo';
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const text = wrapper.text();
    expect(text).toContain(i18n.global.t('catalogueSettings.bundleSkipped'));
    // The publisher's own words, verbatim — a withdrawal nobody can read the
    // reason for is one people work around.
    expect(text).toContain('a bad image tag shipped in this build');
  });

  it('shows a failure rather than a bundle that was never written', async () => {
    replies.marketBundle = () =>
      Promise.reject(
        Object.assign(new Error('/Volumes/stick is not empty'), {
          code: 'ALREADY_EXISTS',
          message: '/Volumes/stick is not empty',
        })
      );
    dialog.open = async () => '/Volumes/stick';
    const wrapper = await render();

    await button(wrapper, i18n.global.t('catalogueSettings.bundleAction')).trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain('is not empty');
    expect(wrapper.text()).not.toContain(i18n.global.t('catalogueSettings.bundleNext'));
  });
});
