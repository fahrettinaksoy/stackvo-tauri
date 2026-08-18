import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createI18n } from 'vue-i18n';
import en from '@/i18n/locales/en.js';

/**
 * The overflow menu at the end of a project row.
 *
 * The eight action columns stay; this names them. Eight glyphs in a row is
 * eight things a reader learns by pressing them, and two of those glyphs are
 * the same hammer — "build" in one column and "rebuild" in the next.
 *
 * What has to hold is that the menu and the row agree. They are drawn from one
 * list precisely so they cannot drift, but "cannot drift" is a claim about
 * code that a later edit is free to break: the conditions could just as easily
 * be retyped into the template, and the failure would be silent — a menu
 * offering a restart on a stopped container fails only when somebody presses
 * it, and a menu missing a rebuild fails by never being noticed at all.
 *
 * The other half is the two acts that never had a column. Applying a changed
 * manifest and adding a hosts entry were small icons beside the domain that
 * you had to know were clickable; if they are not in here, the menu is not
 * "everything this row can do" and the reason it was asked for is gone.
 */

const vuetify = createVuetify({ components, directives });

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

vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

const Projects = (await import('@/views/Projects.vue')).default;

/**
 * Open the filter menu on the end of the search field.
 *
 * Teleported, like every Vuetify overlay, so it is reached through the document
 * rather than through the wrapper. It stays open between choices on purpose —
 * the status and the star combine — which is what lets one call to this be
 * followed by several clicks.
 */
async function openFilters(wrapper) {
  const funnel = wrapper.get(`[aria-label="${en.projectsView.filter.title}"]`);
  await funnel.trigger('click');
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
}

/** Press one entry of the open filter menu by the label it shows. */
async function clickFilter(wrapper, label) {
  if (!document.querySelector('.v-overlay .v-list-item')) await openFilters(wrapper);

  const item = [...document.querySelectorAll('.v-overlay .v-list-item')].find(
    (el) => el.textContent.trim() === label
  );
  expect(item, `no filter entry reading "${label}"`).toBeTruthy();
  item.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();
}

/** The domains the table is showing right now. */
const shown = (wrapper) => wrapper.findAll('tbody tr .domain-link').map((b) => b.text());

/** A project that is built, running and reachable — every action available. */
const project = (over = {}) => ({
  name: 'shop',
  path: '/w/projects/shop',
  domain: 'shop.loc',
  domainConfigured: true,
  runtime: 'php',
  server: 'nginx',
  built: true,
  running: true,
  manifestValid: true,
  generatedStale: false,
  containerName: 'stackvo-shop',
  manifest: { php: { version: '8.3' }, errors: [] },
  ...over,
});

async function render() {
  const i18n = createI18n({ legacy: false, locale: 'en', messages: { en } });
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

/** Open the row's overflow menu and return the titles it offers, in order. */
async function openMenu(wrapper) {
  const more = wrapper.get(`[aria-label="${en.projectsView.aria.more.replace('{name}', 'shop')}"]`);
  await more.trigger('click');
  await new Promise((resolve) => setTimeout(resolve, 0));
  await wrapper.vm.$nextTick();

  // Teleported, so it is reached through the document rather than the wrapper.
  return [...document.querySelectorAll('.v-overlay .v-list-item-title')].map((el) =>
    el.textContent.trim()
  );
}

beforeEach(() => {
  setActivePinia(createPinia());
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.projectsList = [];
  replies.projectAdoptable = [];
  replies.importsScan = [];
});

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the actions menu at the end of a project row', () => {
  it('names every act the row offers as a glyph', async () => {
    replies.projectsList = [project()];
    const wrapper = await render();

    expect(await openMenu(wrapper)).toEqual([
      en.projectsView.menu.stop,
      en.projectsView.menu.restart,
      en.projectsView.rebuild,
      en.projectsView.colOpen,
      en.detail.openInEditor,
      en.detail.openFolder,
      en.detail.externalTerminal,
      en.projectsView.colDetail,
      en.projectsView.colDelete,
    ]);
  });

  /**
   * The detail page's toolbar had two acts this row did not: opening the
   * project in an editor and opening its folder. Both act on `path`, which
   * `projects_list` has always returned — there was nothing to fetch, only
   * nothing offering them, so somebody wanting either had to open the project
   * first.
   *
   * Asserted on the call rather than only on the label: these hand a path to
   * something outside the app, and handing it the wrong one — the container
   * name, the domain — is a failure that looks like nothing happening.
   */
  it('opens the project directory the way the detail page does', async () => {
    replies.projectsList = [project({ path: '/w/projects/shop' })];
    const wrapper = await render();
    await openMenu(wrapper);

    const folder = [...document.querySelectorAll('.v-overlay .v-list-item')].find((el) =>
      el.textContent.includes(en.detail.openFolder)
    );
    folder.click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calls).toContainEqual(['openFolder', '/w/projects/shop']);
  });

  /**
   * A stopped project cannot be restarted and has nothing to open in a
   * browser — the columns already know that, and a menu that offered either
   * would be a menu that had stopped reading the same row.
   */
  it('offers a stopped project only what a stopped project can do', async () => {
    replies.projectsList = [project({ running: false })];
    const wrapper = await render();

    const titles = await openMenu(wrapper);
    expect(titles).toContain(en.projectsView.menu.start);
    expect(titles).not.toContain(en.projectsView.menu.restart);
    expect(titles).not.toContain(en.projectsView.colOpen);
    expect(titles).not.toContain(en.detail.externalTerminal);
    // Still rebuildable: the image is there whether or not it is running.
    expect(titles).toContain(en.projectsView.rebuild);
  });

  /** Nothing to rebuild before there is an image; the act is Build instead. */
  it('offers Build rather than Rebuild before the first build', async () => {
    replies.projectsList = [project({ built: false, running: false })];
    const wrapper = await render();

    const titles = await openMenu(wrapper);
    expect(titles).toContain(en.projectsView.menu.build);
    expect(titles).not.toContain(en.projectsView.rebuild);
    expect(titles).not.toContain(en.projectsView.menu.start);
  });

  /**
   * The two that never had a column. Both were icons beside the domain that
   * you had to know were pressable, which is not a way to offer the act that
   * makes a changed manifest take effect.
   */
  it('carries the acts that have no column of their own', async () => {
    replies.projectsList = [project({ generatedStale: true, domainConfigured: false })];
    const wrapper = await render();

    const titles = await openMenu(wrapper);
    expect(titles).toContain(en.projectsView.menu.apply);
    expect(titles).toContain(en.projectsView.menu.fixHosts);
  });

  /** Pressing one runs the act, rather than opening the row it sits on. */
  it('runs the act it names', async () => {
    replies.projectsList = [project()];
    const wrapper = await render();
    await openMenu(wrapper);

    const stop = [...document.querySelectorAll('.v-overlay .v-list-item')].find((el) =>
      el.textContent.includes(en.projectsView.menu.stop)
    );
    stop.click();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calls.map(([name]) => name)).toContain('projectStop');
  });

  /**
   * Delete is last and behind a rule, which is the only placement that
   * survives somebody opening this menu quickly.
   */
  it('keeps the destructive act last', async () => {
    replies.projectsList = [project()];
    const wrapper = await render();

    const titles = await openMenu(wrapper);
    expect(titles.at(-1)).toBe(en.projectsView.colDelete);
    expect(document.querySelectorAll('.v-overlay .v-divider').length).toBeGreaterThan(0);
  });
});

/**
 * Narrowing the table by what a project *is*, which search cannot do.
 *
 * The page had one control — a text box — and the two questions actually asked
 * of it are not about text: "what is running right now" and "the handful I work
 * on". Typing a name answers neither.
 *
 * The property worth protecting is not that the filters filter. It is that they
 * combine and that they can always be undone: a starred filter and a status
 * filter narrow together rather than replacing one another, and an emptied
 * table offers the way back — otherwise a filter is a mode somebody gets stuck
 * in, wondering where their projects went.
 */
describe('the project filters', () => {
  const FLEET = [
    project({ name: 'live', domain: 'live.loc', running: true, built: true }),
    project({ name: 'idle', domain: 'idle.loc', running: false, built: true }),
    project({ name: 'fresh', domain: 'fresh.loc', running: false, built: false }),
  ];

  it('shows everything until something is asked of it', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    expect(shown(wrapper).sort()).toEqual(['fresh.loc', 'idle.loc', 'live.loc']);
  });

  it('separates running, stopped and never built', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    await clickFilter(wrapper, en.projectsView.filter.running);
    expect(shown(wrapper)).toEqual(['live.loc']);

    // Stopped is built-and-not-running. A project with no image has not
    // stopped, it has never started, and folding the two together would make
    // "stopped" the answer for a fresh checkout — the one case whose next step
    // is different.
    await clickFilter(wrapper, en.projectsView.filter.stopped);
    expect(shown(wrapper)).toEqual(['idle.loc']);

    await clickFilter(wrapper, en.projectsView.filter.unbuilt);
    expect(shown(wrapper)).toEqual(['fresh.loc']);

    await clickFilter(wrapper, en.projectsView.filter.all);
    expect(shown(wrapper)).toHaveLength(3);
  });

  /** The star narrows *with* the status rather than instead of it. */
  it('combines the star with the status', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    await clickFilter(wrapper, en.projectsView.filter.favourites);

    // Nothing is starred in a fresh profile, so the star alone empties it —
    // which is the state the empty action below has to rescue.
    expect(shown(wrapper)).toEqual([]);
    expect(wrapper.text()).toContain(en.projects.noMatchFilter);
  });

  /** An emptied table is never a place with no way out. */
  it('offers a way back out of an empty result', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    await clickFilter(wrapper, en.projectsView.filter.running);
    await clickFilter(wrapper, en.projectsView.filter.favourites);
    expect(shown(wrapper)).toEqual([]);

    const clear = wrapper.findAll('button').find((b) => b.text() === en.projects.clearSearch);
    await clear.trigger('click');
    await wrapper.vm.$nextTick();

    expect(shown(wrapper), 'clearing left a filter on').toHaveLength(3);
  });
  /**
   * A filter behind a menu is a filter nobody can see.
   *
   * That is the cost of folding four buttons and a star into one funnel, and
   * the badge is what pays it: with the menu shut, the only thing on screen
   * saying "this list is not all of it" is the count on that icon. Without it
   * the page reads as a workspace that lost half its projects.
   */
  it('says on the funnel how many narrowings are on', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    // Scoped to the funnel. The unmanaged-code button in the toolbar carries a
    // badge too, and an unscoped `find` returns that one — which sits at 0 in
    // this fixture and would have made every assertion below pass or fail for a
    // reason that has nothing to do with filters.
    const badge = () => wrapper.find('.filter-btn .v-badge__badge');
    expect(badge().isVisible(), 'a badge with nothing filtered').toBe(false);

    await clickFilter(wrapper, en.projectsView.filter.running);
    expect(badge().text()).toBe('1');

    await clickFilter(wrapper, en.projectsView.filter.favourites);
    expect(badge().text(), 'the two narrowings are counted as one').toBe('2');

    await clickFilter(wrapper, en.projectsView.filter.clear);
    expect(badge().isVisible()).toBe(false);
  });

  /**
   * The search term is deliberately *not* counted. It is typed into a box that
   * shows it back, so a badge for it would be a second indicator for something
   * already on screen — and the count would then never read zero on a page
   * somebody is searching.
   */
  it('does not count the search term, which is already visible', async () => {
    replies.projectsList = FLEET;
    const wrapper = await render();

    await wrapper.find('input').setValue('live');
    await wrapper.vm.$nextTick();

    expect(wrapper.find('.filter-btn .v-badge__badge').isVisible()).toBe(false);
  });
});
