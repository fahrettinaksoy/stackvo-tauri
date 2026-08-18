import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '../src/i18n/locales/en.js';

/**
 * The loopback API pane — §3 #34, ADR 0026.
 *
 * The transport is proved over a real socket in `websurface_socket.rs`. What is
 * left for this side is the one thing the socket cannot check: **what a person
 * is shown, and for how long.**
 *
 * The token is returned once, by `websurface_start`. `websurface_status` does
 * not carry it and must not — a status call that did would hand it to every
 * later caller, and the first of those is the surface itself. That makes this
 * component the only place it exists, which makes "does it stop existing when
 * the surface does" a question worth a test rather than a comment.
 */

const websurfaceStart = vi.fn();
const websurfaceStatus = vi.fn();
const websurfaceStop = vi.fn();

vi.mock('@/lib/ipc', () => ({
  api: {
    websurfaceStart: (...a) => websurfaceStart(...a),
    websurfaceStatus: (...a) => websurfaceStatus(...a),
    websurfaceStop: (...a) => websurfaceStop(...a),
  },
}));

const LocalApiPane = (await import('../src/components/settings/LocalApiPane.vue')).default;

const vuetify = createVuetify({ components, directives });
const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } });

const flush = () => new Promise((r) => setTimeout(r, 0));

async function open() {
  const wrapper = mount(LocalApiPane, { global: { plugins: [vuetify, i18n] } });
  await flush();
  await wrapper.vm.$nextTick();
  return wrapper;
}

const button = (wrapper, label) =>
  wrapper.findAll('button').find((b) => b.text().includes(label));

beforeEach(() => {
  websurfaceStart.mockReset();
  websurfaceStatus.mockReset();
  websurfaceStop.mockReset();
  websurfaceStatus.mockResolvedValue({ running: false, address: null, tools: [] });
  websurfaceStop.mockResolvedValue(true);
});

describe('before anybody asks', () => {
  it('is off, and offers to start rather than to stop', async () => {
    const wrapper = await open();
    expect(wrapper.text()).toContain('Not running');
    expect(button(wrapper, 'Start')).toBeTruthy();
    expect(button(wrapper, 'Stop')).toBeFalsy();
    // Nothing was started by opening the pane. A surface that came up because
    // somebody looked at a settings page is the listener nobody turns off.
    expect(websurfaceStart).not.toHaveBeenCalled();
  });

  it('shows no token, because there is none', async () => {
    const wrapper = await open();
    expect(wrapper.text()).not.toContain('shown once');
  });
});

describe('once it is running', () => {
  const running = {
    running: true,
    address: '127.0.0.1:51234',
    tools: ['stackvo_overview', 'stackvo_projects'],
  };

  it('shows the token once, and says that it is once', async () => {
    websurfaceStart.mockResolvedValue({
      address: running.address,
      token: 'deadbeef',
      tools: 2,
    });
    websurfaceStatus.mockResolvedValueOnce({ running: false, address: null, tools: [] });
    websurfaceStatus.mockResolvedValue(running);

    const wrapper = await open();
    await button(wrapper, 'Start').trigger('click');
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain('deadbeef');
    // The sentence matters as much as the value: a token that vanishes on
    // reload without warning is a support message.
    expect(wrapper.text()).toContain('shown once');
    expect(wrapper.text()).toContain('127.0.0.1:51234');
  });

  it('builds a request out of what is actually listening', async () => {
    websurfaceStart.mockResolvedValue({ address: running.address, token: 'tok', tools: 2 });
    websurfaceStatus.mockResolvedValueOnce({ running: false, address: null, tools: [] });
    websurfaceStatus.mockResolvedValue(running);

    const wrapper = await open();
    await button(wrapper, 'Start').trigger('click');
    await flush();
    await wrapper.vm.$nextTick();

    const text = wrapper.text();
    // The address, the token and a tool that is genuinely served — not a
    // hand-written example that can drift from what the surface answers.
    expect(text).toContain('127.0.0.1:51234/call');
    expect(text).toContain('Bearer tok');
    expect(text).toContain('stackvo_overview');
  });

  it('lists what is served, so the read-only claim is checkable', async () => {
    websurfaceStatus.mockResolvedValue(running);
    const wrapper = await open();
    expect(wrapper.text()).toContain('stackvo_projects');
    expect(wrapper.text()).toContain('2 tools served');
  });

  it('says the token is gone when this session never saw it', async () => {
    // A reload leaves the surface up and the token behind. Saying so beats
    // showing an empty box.
    websurfaceStatus.mockResolvedValue(running);
    const wrapper = await open();
    expect(wrapper.text()).toContain('Stop and start again');
  });
});

describe('stopping', () => {
  it('forgets the token here as well as there', async () => {
    // The half a socket cannot check. A token left on screen after the surface
    // it opened is gone is a value somebody copies and then cannot work out
    // why it is refused.
    websurfaceStart.mockResolvedValue({ address: '127.0.0.1:5', token: 'secret-token', tools: 1 });
    websurfaceStatus.mockResolvedValueOnce({ running: false, address: null, tools: [] });
    websurfaceStatus.mockResolvedValueOnce({
      running: true,
      address: '127.0.0.1:5',
      tools: ['stackvo_overview'],
    });
    websurfaceStatus.mockResolvedValue({ running: false, address: null, tools: [] });

    const wrapper = await open();
    await button(wrapper, 'Start').trigger('click');
    await flush();
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain('secret-token');

    await button(wrapper, 'Stop').trigger('click');
    await flush();
    await wrapper.vm.$nextTick();

    expect(websurfaceStop).toHaveBeenCalled();
    expect(wrapper.text()).not.toContain('secret-token');
    expect(wrapper.text()).toContain('Not running');
  });
});

describe('when starting fails', () => {
  it('says so instead of pretending it is up', async () => {
    // The live case: a second start while one is already running is a
    // conflict, not a second surface.
    websurfaceStart.mockRejectedValue({
      code: 'CONFLICT',
      message: 'a local API is already listening on 127.0.0.1:5',
    });
    const wrapper = await open();
    await button(wrapper, 'Start').trigger('click');
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.text()).toContain('already listening');
    expect(wrapper.text()).toContain('Not running');
  });
});
