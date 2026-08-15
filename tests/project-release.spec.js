import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Release pane, mounted, and the plan behind it.
 *
 * Second pane out of `ProjectDetail.vue` under §14.16. The interesting half is
 * `useRelease`: three verbs over one plan, where two of them can be told apart
 * only by *which* button spins, and where a project with nothing to release
 * answers an error code that is not a failure.
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

const { useRelease } = await import('@/composables/useRelease');
const { i18n } = await import('@/i18n');
const ReleasePane = (await import('@/components/project/ReleasePane.vue')).default;

const vuetify = createVuetify({ components, directives });

const PLAN = {
  tag: 'shop:1.4.0',
  baseImage: 'php:8.3-fpm-alpine',
  excluded: [
    ['node_modules', 'rebuilt during the image build'],
    ['.git', 'history is not shipped'],
  ],
  warnings: ['no .dockerignore found'],
  dockerfile: 'FROM php:8.3-fpm-alpine\nCOPY . /app\n',
};

/** A deferred promise, so a call can be observed while it is still in flight. */
function pending() {
  let settle;
  const promise = new Promise((resolve) => {
    settle = resolve;
  });
  return { promise, settle };
}

const ref = (value) => ({ value });

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.releasePlan = { ...PLAN };
});

describe('reading the plan', () => {
  it('adopts the tag the build would actually use', async () => {
    const r = useRelease(ref('shop'));
    await r.load();

    expect(r.tag.value).toBe('shop:1.4.0');
    expect(calls[0]).toEqual(['releasePlan', 'shop', null]);
  });

  /**
   * A tag the user typed is what the plan is read *for*; re-reading must not
   * overwrite it with the default the back end would otherwise pick.
   */
  it('keeps a tag the user has already chosen', async () => {
    const r = useRelease(ref('shop'));
    r.tag.value = 'shop:rc1';
    await r.load();

    expect(r.tag.value).toBe('shop:rc1');
    expect(calls[0]).toEqual(['releasePlan', 'shop', 'shop:rc1']);
  });

  /**
   * `NOT_FOUND` is what an unbuilt project looks like. Reporting it would put a
   * red alert on a page the user has only just opened.
   */
  it('treats nothing-to-release as a state rather than a failure', async () => {
    replies.releasePlan = () => Promise.reject({ code: 'NOT_FOUND', message: 'no image' });

    const r = useRelease(ref('shop'));
    expect(await r.load()).toBe(false);
    expect(r.plan.value).toBe(null);
    expect(r.error.value).toBe(null);
  });

  it('still reports a real failure', async () => {
    replies.releasePlan = () => Promise.reject({ code: 'DOCKER_UNAVAILABLE', message: 'down' });

    const r = useRelease(ref('shop'));
    await r.load();

    expect(r.error.value.code).toBe('DOCKER_UNAVAILABLE');
  });
});

describe('building and saving', () => {
  /**
   * Both verbs spin, and the pane draws two separate buttons off `busy`. A
   * boolean would spin them both, so which one is running has to be nameable.
   */
  it('names the verb that is running, not merely that one is', async () => {
    const build = pending();
    replies.releaseBuild = () => build.promise;

    const r = useRelease(ref('shop'));
    const done = r.build();
    expect(r.busy.value).toBe('build');

    build.settle({ imageId: 'sha256:abc', bytes: 1024 });
    await done;
    expect(r.busy.value).toBe('');
    expect(r.result.value.imageId).toBe('sha256:abc');
  });

  it('clears a previous failure when the build is retried', async () => {
    replies.releaseBuild = () => Promise.reject({ code: 'BUILD_FAILED', message: 'boom' });
    const r = useRelease(ref('shop'));
    await r.build();
    expect(r.error.value.code).toBe('BUILD_FAILED');

    replies.releaseBuild = { imageId: 'sha256:ok' };
    await r.build();
    expect(r.error.value).toBe(null);
  });

  it('writes the image to the path the dialog returned', async () => {
    replies.releaseSave = null;
    const r = useRelease(ref('shop'));
    r.tag.value = 'shop:1.4.0';

    expect(await r.save(async () => '/Users/me/shop.tar')).toBe(true);
    expect(calls.at(-1)).toEqual(['releaseSave', 'shop', '/Users/me/shop.tar', 'shop:1.4.0']);
  });

  /** Cancelling the dialog is not an error, and must not reach the back end. */
  it('does nothing at all when the save dialog is cancelled', async () => {
    const r = useRelease(ref('shop'));

    expect(await r.save(async () => null)).toBe(false);
    expect(calls.some(([n]) => n === 'releaseSave')).toBe(false);
    expect(r.busy.value).toBe('');
    expect(r.error.value).toBe(null);
  });

  it('offers a filename rather than an empty dialog', async () => {
    const seen = [];
    const r = useRelease(ref('shop'));
    await r.save(async (name) => {
      seen.push(name);
      return null;
    });

    expect(seen).toEqual(['shop-production.tar']);
  });

  /** A failed write leaves the pane usable, not stuck spinning. */
  it('releases the spinner when the write fails', async () => {
    replies.releaseSave = () => Promise.reject({ code: 'IO', message: 'disk full' });
    const r = useRelease(ref('shop'));

    expect(await r.save(async () => '/full/shop.tar')).toBe(false);
    expect(r.busy.value).toBe('');
    expect(r.error.value.code).toBe('IO');
  });
});

describe('the pane', () => {
  const open = () =>
    mount(
      { template: '<v-app><ReleasePane name="shop" /></v-app>', components: { ReleasePane } },
      { global: { plugins: [vuetify, i18n] } }
    );

  it('reads the plan for the project it was given', async () => {
    const wrapper = open();
    await vi.waitFor(() => expect(wrapper.find('input').exists()).toBe(true));
    expect(calls[0]).toEqual(['releasePlan', 'shop', null]);

    expect(wrapper.find('input').element.value).toBe('shop:1.4.0');
    // The two halves a user checks before shipping: what is left out, and why.
    expect(wrapper.text()).toContain('node_modules');
    expect(wrapper.text()).toContain('rebuilt during the image build');
    expect(wrapper.text()).toContain('no .dockerignore found');
  });

  it('says so rather than showing an empty plan when there is nothing to release', async () => {
    replies.releasePlan = () => Promise.reject({ code: 'NOT_FOUND', message: 'no image' });

    const wrapper = open();
    await vi.waitFor(() => expect(calls.length).toBeGreaterThan(0));
    await wrapper.vm.$nextTick();

    expect(wrapper.findAll('.v-alert[type="error"]')).toHaveLength(0);
    expect(wrapper.text()).not.toContain('node_modules');
  });
});

/**
 * Pushing, and the refusals that make it safe (H-1).
 *
 * The property worth a test is that a refusal is *readable and disabling*: a
 * pane that showed the reason and left the button live would be worse than one
 * that showed nothing, because it would look considered.
 */
describe('pushing a release', () => {
  const CLEAN = {
    plan: PLAN,
    verification: { clean: true, envFiles: [], xdebugActive: false, hasApp: true },
  };

  async function built() {
    replies.releasePlan = PLAN;
    replies.releaseBuild = CLEAN;
    const wrapper = mount(
      { template: '<v-app><ReleasePane name="shop" /></v-app>', components: { ReleasePane } },
      { global: { plugins: [vuetify, i18n] } }
    );
    await vi.waitFor(() => expect(wrapper.find('input').exists()).toBe(true));

    const build = wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('release.build'));
    await build.trigger('click');
    await flushPromises();
    return wrapper;
  }

  const button = (wrapper, key) =>
    wrapper.findAll('button').find((b) => b.text() === i18n.global.t(key));

  it('offers nothing to push until the check has been asked for', async () => {
    const wrapper = await built();
    expect(button(wrapper, 'release.push').attributes('disabled')).toBeDefined();
  });

  it('shows the refusal in full and keeps the button disabled', async () => {
    const wrapper = await built();
    replies.releasePushPlan = {
      tag: 'shop:1.4.0',
      possible: false,
      refused: 'shop:1.4.0 names no registry, so this would push to Docker Hub',
      warnings: [],
    };

    await button(wrapper, 'release.pushCheck').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('would push to Docker Hub');
    expect(button(wrapper, 'release.push').attributes('disabled')).toBeDefined();
  });

  it('enables the push once the plan says it may happen', async () => {
    const wrapper = await built();
    replies.releasePushPlan = {
      tag: 'registry.example.com/shop:1.4.0',
      registry: 'registry.example.com',
      possible: true,
      authenticated: true,
      warnings: [],
    };

    await button(wrapper, 'release.pushCheck').trigger('click');
    await flushPromises();

    expect(button(wrapper, 'release.push').attributes('disabled')).toBeUndefined();
  });

  /** The recipe is shown, not written: where it belongs is the user's call. */
  it('shows the recipe rather than saving it anywhere', async () => {
    const wrapper = await built();
    replies.releaseRecipe = 'services:\n  shop:\n    image: "registry.example.com/shop:1.4.0"\n';

    await button(wrapper, 'release.recipe').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('registry.example.com/shop:1.4.0');
    expect(calls.some((c) => c[0] === 'releaseSave')).toBe(false);
  });
});
