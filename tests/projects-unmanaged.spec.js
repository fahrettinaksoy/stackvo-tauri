import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';
import tr from '@/i18n/locales/tr.js';

/**
 * The unmanaged code moved off the page and behind the overflow button.
 *
 * It used to be a strip above the table — two "point at a XAMPP folder"
 * buttons and, under them, the folders in `projects/` with no `stackvo.json`.
 * Both are things you deal with once, and they were charging every reader
 * vertical space on every visit.
 *
 * The move is only safe while two things hold, and neither is visible from a
 * page that merely mounts:
 *
 * 1. **The count survives.** These folders are invisible everywhere else in
 *    the app, which is the whole reason the strip existed. Moving them behind
 *    a button with nothing on it would make eleven undeclared folders as
 *    invisible as they are in Finder. The badge is the strip's one job.
 * 2. **The panels still open.** A menu item that opens nothing, or a dialog
 *    that renders its two lists empty, looks exactly like a workspace with
 *    nothing to adopt — the failure reads as good news.
 */

const vuetify = createVuetify({ components, directives });

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

vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

const Projects = (await import('@/views/Projects.vue')).default;

const folder = (name) => ({
  name,
  hasFiles: true,
  composeFile: null,
  detected: { runtime: 'php', framework: 'laravel', evidence: ['artisan'] },
});

const site = (name, taken = false) => ({
  name,
  path: `/opt/lampp/htdocs/${name}`,
  taken,
  bytes: 1024,
  partial: false,
  domain: null,
  detected: { runtime: 'php', framework: null, evidence: [] },
});

async function render(locale = 'en') {
  const i18n = createI18n({ legacy: false, locale, messages: { en, tr } });
  const host = document.createElement('div');
  document.body.appendChild(host);

  const wrapper = mount(
    {
      components: { Page: Projects },
      template: '<v-app><Page /></v-app>',
    },
    {
      attachTo: host,
      global: {
        plugins: [
          createPinia(),
          createRouter({
            history: createMemoryHistory(),
            routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
          }),
          vuetify,
          i18n,
        ],
      },
    }
  );

  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
  return wrapper;
}

/** Menus and dialogs are teleported, so the page's own wrapper cannot see them. */
const overlays = () => document.body.textContent;

beforeEach(() => {
  setActivePinia(createPinia());
  for (const key of Object.keys(replies)) delete replies[key];
  replies.projectsList = [];
  replies.projectAdoptable = [];
  replies.importsScan = [];
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('unmanaged code, behind the overflow button', () => {
  it('does not put the adoptable folders on the page any more', async () => {
    replies.projectAdoptable = [folder('old-crm'), folder('scratch')];

    const wrapper = await render();

    // The names are the test, not the heading: the strip listed them, and the
    // point of the move is that the table is not sharing the page with them.
    expect(wrapper.text()).not.toContain('old-crm');
    expect(wrapper.text()).not.toContain('Point at a xampp folder');
    wrapper.unmount();
  });

  it('counts them on the button instead', async () => {
    replies.projectAdoptable = [folder('old-crm'), folder('scratch')];
    replies.importsScan = [
      { source: 'xampp', path: '/opt/lampp', sites: [site('shop'), site('done', true)] },
    ];

    const wrapper = await render();

    // Two folders and one site left to take. The site already imported is not
    // counted — nothing about it is outstanding, and a badge that cannot be
    // cleared is a badge people learn to read past.
    const badge = wrapper.find('.v-badge__badge');
    expect(badge.exists()).toBe(true);
    expect(badge.text()).toBe('3');
    wrapper.unmount();
  });

  it('shows no badge when there is nothing to take over', async () => {
    const wrapper = await render();
    expect(wrapper.find('.v-badge__badge').isVisible()).toBe(false);
    wrapper.unmount();
  });

  it('opens the folders and the folder pickers from the menu', async () => {
    replies.projectAdoptable = [folder('old-crm')];

    const wrapper = await render();

    const more = wrapper.get(`[aria-label="${en.unmanaged.title}"]`);
    await more.trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Both scans, and the way into the panel, all reachable in one press —
    // each of them saying what it does under its own name.
    expect(overlays()).toContain('Point at a xampp folder');
    expect(overlays()).toContain('Point at a laragon folder');
    expect(overlays()).toContain(en.unmanaged.review);
    expect(overlays()).toContain(en.unmanaged.pickExplain);

    // The count, said in words on the item rather than only as a digit.
    expect(overlays()).toContain('1 waiting.');

    const review = [...document.querySelectorAll('.v-list-item')].find((el) =>
      el.textContent.includes(en.unmanaged.review)
    );
    review.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    // The dialog, with the folder in it — the thing the strip used to show,
    // and open on arrival rather than behind a header you press by name.
    expect(overlays()).toContain('old-crm');
    expect(overlays()).toContain('Adopt');

    // And still collapsible. The panel is seeded open by a `model-value` with
    // no listener beside it, which is a default only for as long as Vuetify
    // keeps treating an unlistened model as uncontrolled — assert it rather
    // than trust the comment that says so.
    //
    // Asked of `aria-expanded` rather than of the text: a panel that has been
    // opened once keeps its body mounted and merely hides it, so the folder's
    // name is still in `textContent` after it closes.
    const title = document.querySelector('.v-expansion-panel-title');
    expect(title.getAttribute('aria-expanded')).toBe('true');
    title.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();
    expect(title.getAttribute('aria-expanded')).toBe('false');
    wrapper.unmount();
  });

  /**
   * The menu is width-bounded, and Vuetify clips a list subtitle to one line
   * with an ellipsis — so the explanations under the two folder pickers
   * shipped as "Yalnızca alışılmış kurulum yolları tarandı. Başka…". A
   * sentence cut before its verb costs a line and tells you nothing.
   *
   * Read from the source, because jsdom applies neither Vuetify's stylesheet
   * nor a scoped `<style>` block: every mount assertion above passed on the
   * truncated version. The clipping is `-webkit-line-clamp: 1`, which only
   * applies to `display: -webkit-box`, so overriding one and not the other
   * looks like a fix and is not — which is why both are named here.
   */
  it('lets the menu explanations wrap rather than clipping them', () => {
    const source = readFileSync('src/views/Projects.vue', 'utf8');
    const style = source.slice(source.indexOf('<style'));
    const rule = /\.more-menu\s+:deep\(\.v-list-item-subtitle\)\s*\{([^}]*)\}/.exec(style);

    expect(rule, '.more-menu subtitles have no rule of their own').not.toBeNull();
    expect(rule[1]).toMatch(/-webkit-line-clamp:\s*(unset|none|initial)/);
    expect(rule[1]).toMatch(/display:\s*block/);
  });

  it('says so when the scans found nothing, rather than opening blank', async () => {
    const wrapper = await render();

    const more = wrapper.get(`[aria-label="${en.unmanaged.title}"]`);
    await more.trigger('click');
    await new Promise((resolve) => setTimeout(resolve, 0));

    // On the item before you open it, short; in full once you have.
    expect(overlays()).toContain(en.unmanaged.nothing);

    const review = [...document.querySelectorAll('.v-list-item')].find((el) =>
      el.textContent.includes(en.unmanaged.review)
    );
    review.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(overlays()).toContain(en.unmanaged.none);
    wrapper.unmount();
  });
});
