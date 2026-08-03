import { describe, it, expect } from 'vitest';
import { ref } from 'vue';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The "take over a template" button, in the state the pane opens in.
 *
 * It shipped spinning. The binding was `templateBusy === templateToOverride`,
 * which reads correctly and is wrong for exactly the state nobody thinks to
 * check: both refs start null, null equals null, and the button reported itself
 * busy before anyone had chosen a file — and again after every successful
 * override, which clears the selection back to null.
 *
 * Nothing else could catch it. The markup lints clean, the comparison is valid
 * JavaScript, and the two names in it are the right two names. Only mounting it
 * and looking at the idle state says anything.
 */

globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

const vuetify = createVuetify({ components, directives });

/**
 * The button and its two pieces of state, in the shape Settings.vue uses them.
 *
 * Copied rather than imported, as the certificates pane test is: mounting the
 * whole Settings view needs a Tauri bridge, a router and five stores, none of
 * which this is about. The last test below is what keeps the copy honest.
 */
const Pane = {
  template: `
    <v-btn class="override" :disabled="!templateToOverride" :loading="busyWith(templateToOverride)">
      take over
    </v-btn>
  `,
  setup() {
    const templateBusy = ref(null);
    const templateToOverride = ref(null);
    const busyWith = (path) => !!path && templateBusy.value === path;
    return { templateBusy, templateToOverride, busyWith };
  },
};

const loading = (wrapper) => wrapper.find('.override').classes().includes('v-btn--loading');

describe('the template override button', () => {
  it('is idle before a template is chosen', async () => {
    const wrapper = mount(Pane, { global: { plugins: [vuetify] } });

    expect(loading(wrapper), 'spinning with nothing selected').toBe(false);

    wrapper.unmount();
  });

  it('stays idle once a template is chosen but no work has started', async () => {
    const wrapper = mount(Pane, { global: { plugins: [vuetify] } });

    wrapper.vm.templateToOverride = 'services/redis/docker-compose.redis.tpl';
    await wrapper.vm.$nextTick();

    expect(loading(wrapper), 'selecting a file is not doing work on it').toBe(false);

    wrapper.unmount();
  });

  it('spins only while that template is the one being worked on', async () => {
    const wrapper = mount(Pane, { global: { plugins: [vuetify] } });
    const path = 'services/redis/docker-compose.redis.tpl';

    wrapper.vm.templateToOverride = path;
    wrapper.vm.templateBusy = path;
    await wrapper.vm.$nextTick();
    expect(loading(wrapper), 'the button did not report the work it started').toBe(true);

    // A different file being worked on is not this button's business, and the
    // finished state has to come back to idle rather than to "null is null".
    wrapper.vm.templateBusy = 'core/servers/nginx.conf';
    await wrapper.vm.$nextTick();
    expect(loading(wrapper), 'another file`s work spun this button').toBe(false);

    wrapper.vm.templateBusy = null;
    wrapper.vm.templateToOverride = null;
    await wrapper.vm.$nextTick();
    expect(loading(wrapper), 'back to both null, back to spinning').toBe(false);

    wrapper.unmount();
  });

  it('keeps the copy in step with the pane it is copied from', async () => {
    const { readFileSync } = await import('node:fs');
    const { resolve } = await import('node:path');
    const settings = readFileSync(
      resolve(import.meta.dirname, '../src/views/Settings.vue'),
      'utf8'
    );

    // Comments stripped first: the helper's own note names the broken
    // comparison in order to explain it, and prose is not markup.
    const source = settings.replace(/<!--[\s\S]*?-->/g, '').replace(/\/\*[\s\S]*?\*\//g, '');

    expect(source, 'the guard the button depends on is gone').toContain(
      'const busyWith = (path) => !!path && templateBusy.value === path'
    );
    expect(source).toContain(':loading="busyWith(templateToOverride)"');
    expect(source, 'the comparison that spun forever came back').not.toContain(
      'templateBusy === templateToOverride'
    );
  });
});
