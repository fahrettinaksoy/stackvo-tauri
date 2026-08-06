import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Certificates pane, mounted for real.
 *
 * This replaces `tests/certificates-pane.spec.js`, which could not mount it:
 * the pane was 210 lines inside a 3,433-line `Settings.vue` that needs a Tauri
 * bridge, a router and five stores. So it rebuilt a *copy* of the markup and
 * kept the copy honest by reading `Settings.vue` as text and asserting the two
 * still matched — a creative answer to an untestable component, and the exact
 * thing the readiness review's §2.3 objected to: behaviour verified in the
 * copy, not in the product, with a `toContain` string match holding them
 * together. A whitespace change broke it; a real regression escaped unless
 * somebody remembered to mirror it.
 *
 * `CertificatesPane.vue` and `useCertificates` make that unnecessary. What is
 * mounted below is what ships.
 *
 * The tooltip case is carried over verbatim in intent, because it is why the
 * old file existed: the tooltip shipped **not working**. `v-tooltip` was nested
 * inside `v-icon` alongside the icon's own name, so the slot held two things
 * and hovering reached neither — and nothing caught it, because markup that
 * renders to nothing lints clean and builds clean. Only a layer that actually
 * hovers can see it.
 */

globalThis.visualViewport = undefined;

/** What `api.certStatus()` answers, in the shape `certs.rs` sends. */
const replies = {};

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy(
    {},
    {
      get: (_t, name) => () => {
        const reply = replies[name];
        return typeof reply === 'function' ? reply() : Promise.resolve(reply);
      },
    }
  ),
}));

const { i18n } = await import('@/i18n');
const CertificatesPane = (await import('@/components/settings/CertificatesPane.vue')).default;
const { useCertificates } = await import('@/composables/useCertificates');

const vuetify = createVuetify({ components, directives });

/** A healthy certificate, with everything the pane can render present. */
function status(extra = {}) {
  return {
    sslEnabled: true,
    stale: false,
    expired: false,
    caTrusted: true,
    mkcertAvailable: true,
    error: null,
    notAfter: 1_800_000_000,
    daysRemaining: 90,
    missing: [],
    rejected: [],
    covered: ['stackvo.loc', 'shop.loc'],
    certPath: '/Users/me/.stackvo/certs/wildcard.pem',
    caPath: '/Users/me/.stackvo/ca/rootCA.pem',
    ...extra,
  };
}

async function render() {
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(CertificatesPane, {
    attachTo: host,
    global: { plugins: [vuetify, i18n] },
  });

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

beforeEach(() => {
  // The composable's state is module-scoped so the rail badge and the pane
  // share one fetch; a test must not inherit the previous one's.
  useCertificates().reset();
  for (const key of Object.keys(replies)) delete replies[key];
  replies.certStatus = status();
  replies.certPlan = { remove: [] };
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the certificates pane', () => {
  /** The case the old file existed for. */
  it('has an icon that opens the explanation on hover', async () => {
    const wrapper = await render();

    const icon = wrapper.find('.why-separate');
    expect(icon.exists(), 'no information icon').toBe(true);

    // Vuetify renders the overlay's content up front and toggles its
    // visibility, so "is the text in the DOM" proves nothing either way — the
    // broken version had it there too. What changes on hover is the overlay
    // becoming active, and that is what is asserted.
    const active = () => document.querySelectorAll('.v-overlay--active').length;
    expect(active(), 'the tooltip was open before anything hovered it').toBe(0);

    await icon.trigger('mouseenter');
    await vi.waitFor(() => expect(active(), 'hovering the icon opened nothing').toBe(1));

    expect(document.body.textContent).toContain(i18n.global.t('certs.whySeparate'));

    wrapper.unmount();
  });

  it('shows what the certificate covers and where both files are', async () => {
    const wrapper = await render();
    const text = wrapper.text();

    expect(text).toContain('stackvo.loc');
    expect(text).toContain('shop.loc');
    // Two paths with two different jobs. They were reported as "the
    // certificate is in two places" three times, because only one was shown.
    expect(text).toContain('/Users/me/.stackvo/certs/wildcard.pem');
    expect(text).toContain('/Users/me/.stackvo/ca/rootCA.pem');

    wrapper.unmount();
  });

  /**
   * The point of the pane: which domains the file on disk does not vouch for,
   * and which a reissue would drop. The second was invisible before the plan
   * was fetched alongside the status — a user who deleted a project watched its
   * domain vanish from the certificate with no warning.
   */
  it('names the domains that are missing and the ones a reissue would drop', async () => {
    replies.certStatus = status({ stale: true, missing: ['new.loc'] });
    replies.certPlan = { remove: ['deleted.loc'] };

    const wrapper = await render();
    const text = wrapper.text();

    expect(text).toContain('new.loc');
    expect(text).toContain('deleted.loc');
    expect(text).toContain(i18n.global.t('certs.stale'));

    wrapper.unmount();
  });

  /**
   * SSL off is a choice, not a fault: without it the generator emits no
   * `websecure` entry point and nothing else in the pane applies.
   */
  it('says nothing applies when SSL is switched off', async () => {
    replies.certStatus = status({ sslEnabled: false });

    const wrapper = await render();
    expect(wrapper.text()).toContain(i18n.global.t('certs.sslOff'));
    expect(wrapper.text()).not.toContain('stackvo.loc');

    wrapper.unmount();
  });

  /**
   * mkcert is the whole mechanism. Without it nothing here can be repaired, so
   * it is said plainly rather than left for the reissue button to fail on —
   * and the button that cannot work is disabled rather than merely useless.
   */
  it('disables reissue and explains why when mkcert is missing', async () => {
    replies.certStatus = status({ mkcertAvailable: false });

    const wrapper = await render();
    expect(wrapper.text()).toContain(i18n.global.t('certs.noMkcert'));

    const reissue = wrapper
      .findAll('button')
      .find((b) => b.text().includes(i18n.global.t('certs.reissue')));
    expect(reissue, 'no reissue button').toBeTruthy();
    expect(reissue.attributes('disabled')).toBeDefined();

    wrapper.unmount();
  });

  /**
   * The trust button exists only when the CA is not trusted, and on macOS it is
   * the one thing this app cannot do for itself — `sudo` needs a terminal and
   * root through AppleScript is refused.
   */
  it('offers the terminal trust step only while the CA is untrusted', async () => {
    const label = i18n.global.t('certs.trustInTerminal');

    let wrapper = await render();
    expect(wrapper.text()).not.toContain(label);
    wrapper.unmount();

    useCertificates().reset();
    replies.certStatus = status({ caTrusted: false });
    wrapper = await render();
    expect(wrapper.text()).toContain(label);
    wrapper.unmount();
  });

  /**
   * A certificate nothing serves is not a certificate the user has. The reissue
   * reports success either way, which is what let this go unnoticed.
   */
  it('warns when the reissue succeeded but the proxy kept the old certificate', async () => {
    replies.certApply = { reloaded: false };

    const wrapper = await render();
    const reissue = wrapper
      .findAll('button')
      .find((b) => b.text().includes(i18n.global.t('certs.reissue')));

    await reissue.trigger('click');
    await vi.waitFor(() => expect(wrapper.text()).toContain(i18n.global.t('certs.notReloaded')));

    wrapper.unmount();
  });

  /**
   * A missing workspace is already reported by the requirements gate. A second
   * copy of it here would be noise — but any *other* failure has to be shown,
   * or the pane silently renders nothing.
   */
  it('reports a real failure and stays quiet about a missing workspace', async () => {
    replies.certStatus = () => Promise.reject(new Error('mkcert exploded'));

    let wrapper = await render();
    expect(wrapper.text()).toContain('mkcert exploded');
    wrapper.unmount();

    useCertificates().reset();
    const gated = new Error('no workspace');
    gated.needsWorkspace = true;
    replies.certStatus = () => Promise.reject(gated);

    wrapper = await render();
    expect(wrapper.text()).not.toContain('no workspace');
    wrapper.unmount();
  });
});
