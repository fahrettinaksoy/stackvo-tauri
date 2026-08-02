import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The tooltip on the Certificates pane, mounted rather than read.
 *
 * It shipped not working. `v-tooltip` was nested inside `v-icon` alongside the
 * icon's own name, so the slot held two things and hovering reached neither —
 * and nothing caught it, because markup that renders to nothing lints clean and
 * builds clean. The only layer that can see "the hover does nothing" is one
 * that actually hovers.
 */

globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
globalThis.visualViewport = undefined;

const { i18n } = await import('@/i18n');
const vuetify = createVuetify({ components, directives });

/**
 * The pane's markup, in the shape Settings.vue uses it.
 *
 * Copied rather than imported: mounting the whole Settings view needs a Tauri
 * bridge, a router, five stores and a workspace, none of which this is about.
 * `the_pane_and_this_test_agree` below is what keeps the copy honest.
 */
const Pane = {
  template: `
    <div>
      <div class="text-caption">
        <strong>{{ t('certs.caLabel') }}</strong> · /Users/me/.stackvo/ca/rootCA.pem
        <v-tooltip :text="t('certs.whySeparate')" location="top" max-width="420">
          <template #activator="{ props }">
            <v-icon v-bind="props" size="14" class="ml-1 why-separate" icon="mdi-information-outline" />
          </template>
        </v-tooltip>
      </div>
    </div>
  `,
  setup() {
    return { t: i18n.global.t };
  },
};

describe('the certificates pane', () => {
  it('has an icon that opens the explanation on hover', async () => {
    const wrapper = mount(Pane, { global: { plugins: [vuetify, i18n] } });

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

  it('keeps the copy in step with the pane it is copied from', async () => {
    const { readFileSync } = await import('node:fs');
    const { resolve } = await import('node:path');
    const settings = readFileSync(
      resolve(import.meta.dirname, '../src/views/Settings.vue'),
      'utf8'
    );

    // Comments stripped first. The markup here carries a note explaining why
    // `activator="parent"` is not used, and searching the raw file found that
    // sentence and called it the bug — prose is not markup, twice in one file
    // now.
    //
    // And scoped to this tooltip: `activator="parent"` is used elsewhere for
    // things that work, so a file-wide search says nothing either.
    const markup = settings.replace(/<!--[\s\S]*?-->/g, '');
    const at = markup.indexOf('certs.whySeparate');
    expect(at, 'the tooltip is gone from the pane').toBeGreaterThan(-1);
    const block = markup.slice(at - 400, at + 400);

    expect(block).toContain(':text="t(\'certs.whySeparate\')"');
    expect(block).toContain('#activator');
    expect(block, 'the shape that did not work came back').not.toContain('activator="parent"');
  });
});
