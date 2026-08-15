import { describe, it, expect, vi, beforeEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
import { createPinia } from 'pinia';
import { i18n } from '@/i18n';
import Market from '@/views/Market.vue';

/**
 * The market page, and the three states it has to tell apart.
 *
 * "Nothing fetched", "fetched and empty" and "fetched with packages" are three
 * different screens, and only the first one is a state a fresh install is
 * genuinely in — StackVo embeds no services at all (ADR 0011). A page that
 * showed "no services found" for all three would send somebody to reinstall
 * the app to fix a directory they had not chosen yet.
 *
 * The rest of what is asserted here is the decisions the page carries rather
 * than its layout: that an end-of-life version is hidden and not withdrawn,
 * that Uninstall is refused while an instance names a version, and that the
 * page says out loud that an instance is recorded rather than started.
 */

const api = vi.hoisted(() => ({
  marketStatus: vi.fn(),
  marketCatalog: vi.fn(),
  marketRefresh: vi.fn(),
  marketInstall: vi.fn(),
  marketUninstall: vi.fn(),
  instanceList: vi.fn(),
  handoverPreview: vi.fn(),
  handoverApply: vi.fn(),
  instanceCreate: vi.fn(),
  instanceRemove: vi.fn(),
  instancePromote: vi.fn(),
  instanceEnable: vi.fn(),
  instanceDisable: vi.fn(),
  instanceStart: vi.fn(),
  instanceStop: vi.fn(),
  instanceRestart: vi.fn(),
  // The detail button resolves its row out of the services list, which is the
  // same instance table under the other command's name.
  servicesList: vi.fn(),
  // What the detail sheet reaches for the moment it is handed a row. Stubbed
  // rather than left out: an absent one rejects inside the sheet's own watcher,
  // which reads here as the Market page failing.
  containerInspect: vi.fn(),
  serviceConnection: vi.fn(),
  dbTargets: vi.fn(),
  dbSnapshots: vi.fn(),
  mailStatus: vi.fn(),
  // C-1's three, driven per test through `mockResolvedValue`.
  packageScaffold: vi.fn(),
  packageLint: vi.fn(),
  packageSeal: vi.fn(),
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

// The detail sheet subscribes to `db:progress` on mount, and it is mounted with
// the page whether or not a row is open. Without this the subscription reaches
// a Tauri runtime that is not there and rejects — outside any test's await, so
// it lands as an unhandled rejection while every assertion still passes.
vi.mock('@/lib/events', async (importOriginal) => ({
  ...(await importOriginal()),
  listenAll: async () => () => {},
  listen: async () => () => {},
}));

const vuetify = createVuetify({ components, directives });

const STATUS = {
  fetched: true,
  sequence: 3,
  generatedAt: '2026-08-11T09:00:00Z',
  expires: null,
  sourceKind: 'local',
  sourceLocation: '/Users/me/stackvo-service-packages',
  packages: 1,
  installed: 1,
  signed: false,
};

const CATALOG = [
  {
    service: 'mysql',
    category: 'databases',
    name: { en: 'MySQL' },
    summary: { en: 'The database most PHP projects assume.' },
    // Published by the index since v1, dropped by `market_catalog` until the
    // search box needed them.
    keywords: ['database', 'sql', 'mariadb'],
    capabilities: ['sql'],
    multiple: true,
    versions: [
      {
        version: '9.4',
        recommended: false,
        support: 'supported',
        eolDate: null,
        sizeBytes: 4211,
        installed: false,
        inUse: false,
      },
      {
        version: '8.0',
        recommended: true,
        support: 'supported',
        eolDate: null,
        sizeBytes: 4211,
        installed: true,
        inUse: true,
      },
      {
        version: '5.7',
        recommended: false,
        support: 'eol',
        eolDate: '2023-10-31',
        sizeBytes: 4211,
        installed: false,
        inUse: false,
      },
    ],
  },
];

/**
 * A second category, so the tree has more than one root.
 *
 * The catalogue fixture had exactly one, which meant every assertion about
 * grouping passed on a page that never had to choose between groups — and a
 * tree with one root is indistinguishable from a flat list.
 */
const REDIS = {
  service: 'redis',
  category: 'cache',
  name: { en: 'Redis' },
  summary: {},
  keywords: ['cache', 'kv'],
  capabilities: ['cache'],
  multiple: true,
  versions: [
    {
      version: '7.0',
      recommended: true,
      support: 'supported',
      eolDate: null,
      sizeBytes: 1024,
      installed: false,
      inUse: false,
    },
  ],
};

const INSTANCES = [
  {
    id: 'mysql-8-0',
    service: 'mysql',
    version: '8.0',
    enabled: false,
    primary: true,
    container: 'stackvo-mysql-8-0',
    aliases: ['stackvo-mysql-8-0', 'stackvo-mysql'],
    ports: { main: 3306 },
    packagePresent: true,
  },
];

/**
 * The same instance as `services_list` answers it — same `id`, which is what
 * the detail button matches on, and the running/ports/credentials shape the
 * detail sheet reads.
 */
const SERVICES = [
  {
    id: 'mysql-8-0',
    category: 'databases',
    enabled: false,
    running: false,
    built: false,
    version: '8.0',
    containerName: 'stackvo-mysql-8-0',
    health: null,
    url: null,
    hostPort: 3306,
    ports: [],
    declaredPorts: [{ name: 'main', container: 3306, host: 3306, protocol: 'tcp' }],
    aliases: ['stackvo-mysql-8-0', 'stackvo-mysql'],
    support: 'eol',
    eolDate: '2026-04-30',
    companions: [],
    credentials: [],
    required: [],
    optional: [],
    unmetDependencies: [],
  },
];

/**
 * Inside a `v-app`, because the page carries a side sheet now.
 *
 * `SideSheet` is a `v-navigation-drawer` underneath, and a drawer asks the
 * layout it is in where its edges are. Mounted bare it throws "Could not find
 * injected layout" before a single assertion runs — which says nothing about
 * the page and everything about the harness.
 */
function mountPage() {
  const app = mount(
    { components: { Page: Market }, template: '<v-app><Page /></v-app>' },
    { global: { plugins: [createPinia(), vuetify, i18n] } }
  );
  // The page itself, not the wrapper around it: every assertion below reaches
  // for `page.vm.catalogueTree` or `page.text()`, and both should mean Market.
  return app.findComponent(Market);
}

/**
 * Every accessible name on the page.
 *
 * The catalogue's counts, its "runs more than one version" and the paragraph
 * about end-of-life all live on `aria-label` with a tooltip beside them, so
 * this is where those assertions have to look. It is also the stricter place:
 * a tooltip is reachable by hover, and this is what everybody else gets.
 */
const labels = (page) =>
  page
    .findAll('[aria-label]')
    .map((el) => el.attributes('aria-label'))
    .filter(Boolean);

beforeEach(() => {
  vi.clearAllMocks();
  api.marketStatus.mockResolvedValue(STATUS);
  api.marketCatalog.mockResolvedValue(CATALOG);
  api.instanceList.mockResolvedValue(INSTANCES);
  api.servicesList.mockResolvedValue(SERVICES);
  api.containerInspect.mockResolvedValue({ running: false, image: 'mysql:8.0', ports: [] });
  api.serviceConnection.mockResolvedValue(null);
  api.dbTargets.mockResolvedValue([]);
  api.dbSnapshots.mockResolvedValue([]);
  api.mailStatus.mockResolvedValue(null);
  // A workspace that has already migrated, which is what these fixtures
  // describe. Without it the composable's load() threw on an absent stub and
  // the page rendered its error line — passing tests, over a broken page.
  api.handoverPreview.mockResolvedValue({
    pending: false,
    migrated: true,
    instances: [],
    notes: [],
    blockers: [],
    backup: true,
  });
});

describe('the market page', () => {
  /// The state a fresh install is in, and the one worth getting right.
  it('says no catalogue has been fetched rather than that none exists', async () => {
    api.marketStatus.mockResolvedValue({ ...STATUS, fetched: false, packages: 0, installed: 0 });
    api.marketCatalog.mockResolvedValue([]);
    api.instanceList.mockResolvedValue([]);

    const page = mountPage();
    await flushPromises();

    const text = page.text();
    expect(text).toContain('No catalogue yet');
    // And it explains why, because "empty" and "not asked yet" look identical.
    expect(text).toContain('ships no services inside itself');
  });

  it('lists what a source publishes once one has been read', async () => {
    const page = mountPage();
    await flushPromises();

    expect(page.text()).toContain('MySQL');
    expect(api.marketCatalog).toHaveBeenCalled();
  });

  /// Grouped by category, and by the category's *name* rather than its
  /// directory slug. A flat list of twenty-five services with the category as a
  /// chip made the category something to read on every row instead of something
  /// to navigate by.
  it('groups the catalogue under its category, by name', async () => {
    const page = mountPage();
    await flushPromises();

    // en.js's `serviceCategories.databases` — not the `databases` directory
    // name the package carries.
    expect(page.text()).toContain('Databases');
    // The count is on the button's accessible name, not in the row's text: in a
    // quarter-width column it was the half that survived and the service name
    // was the half that got hyphenated. Asserting here rather than on the
    // tooltip is deliberate — the accessible name is what a reader who never
    // hovers is left with, so it is the one that has to carry the sentence.
    // A substring, because the category's button carries both halves in one
    // name — "1 service(s) · 1 end-of-life".
    expect(labels(page).join(' ')).toContain('1 service(s)');
  });

  /// Hidden by default, listed behind a switch, never removed — somebody's
  /// workspace may name it.
  ///
  /// Asserted against the tree the page builds rather than against its rendered
  /// text. Both work — `VTreeview` writes every descendant into the document
  /// and collapses it visually, which was measured rather than assumed — but
  /// this states the fact directly: the switch changes what the catalogue
  /// *contains*, not what happens to be scrolled into view.
  it('keeps an end-of-life version out of the way without withdrawing it', async () => {
    const page = mountPage();
    await flushPromises();

    const versions = () =>
      page.vm.catalogueTree
        .flatMap((c) => c.children)
        .flatMap((s) => s.children.map((v) => v.title));

    expect(versions()).not.toContain('5.7');
    // "end-of-life", not "hidden". The count is a fact about the version's
    // upstream, and "hidden" reads as something the app is withholding.
    expect(labels(page).join(' ')).toContain('1 end-of-life');
    // And the page still says why one is published at all, next to the switch —
    // on a button now rather than as a paragraph under the heading.
    expect(labels(page).join(' ')).toContain('upstream has stopped patching them');

    page.vm.market.showOlder.value = true;
    await flushPromises();
    expect(versions()).toContain('5.7');
  });

  /// The catalogue's own shape: a category holds services, a service holds
  /// versions. One tree with one idea of where you are, in place of a tab rail
  /// over expansion panels — two collapsing mechanisms, each with its own.
  it('builds the catalogue as category, service, version', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    const tree = page.vm.catalogueTree;
    expect(tree).toHaveLength(2);
    // Translated names, and the repository's fixed order rather than
    // alphabetical — a stack is a database and a cache before it is an admin UI.
    expect(tree[0].title).toBe('Databases');
    expect(tree[1].title).toBe('Cache');

    expect(tree[0].children.map((s) => s.title)).toEqual(['MySQL']);
    expect(tree[0].children[0].children.map((v) => v.title)).toEqual(['9.4', '8.0']);
  });

  /// Unique across all three depths, because two services can publish `8.0` and
  /// the tree opens and closes on this value.
  it('gives every node an id nothing else in the tree shares', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    const ids = page.vm.catalogueTree.flatMap((c) => [
      c.id,
      ...c.children.flatMap((s) => [s.id, ...s.children.map((v) => v.id)]),
    ]);
    expect(new Set(ids).size).toBe(ids.length);
  });

  /// The way in is open and the rest is a click: a tree that opened nothing is
  /// a column of category names with the catalogue behind them, and one that
  /// opened everything is the stacked headings the rail replaced.
  it('opens the first category and leaves the others closed', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    expect(page.vm.opened).toEqual(['category:databases']);
  });

  /// A refresh or a change of source rebuilds the groups. Re-seeding there
  /// would close whatever the reader had opened and move them elsewhere.
  it('does not reopen the first category when the catalogue reloads', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    page.vm.opened = ['category:cache'];
    await page.vm.market.load();
    await flushPromises();

    expect(page.vm.opened).toEqual(['category:cache']);
  });

  /// The handover panel says the problem once.
  ///
  /// It said it twice: a paragraph explaining that the version would not be
  /// migrated to a nearby one, and under it a list of what to install — the
  /// same fact in two registers, stacked. When a button can answer the whole
  /// thing, the button is the sentence.
  it('states a missing package once, with the button that fixes it', async () => {
    api.handoverPreview.mockResolvedValue({
      pending: false,
      migrated: false,
      instances: [],
      notes: [],
      blockers: [{ kind: 'versionNotInstalled', subject: 'mariadb@10.6', detail: '10.11' }],
      backup: false,
      missing: [{ service: 'mariadb', version: '10.6', installable: true }],
    });

    const page = mountPage();
    await flushPromises();

    const text = page.text();
    expect(text).toContain('mariadb@10.6');
    // The long refusal is gone; only the actionable line is left.
    expect(text).not.toContain('would be an upgrade nobody asked for');
    // And the mechanics of the undo are not on a screen where nothing can be
    // undone yet — the migration has not run and cannot.
    expect(text).not.toContain('.env.pre-market.bak');
  });

  /// A blocker no button can answer keeps its explanation.
  it('keeps the explanation when the catalogue cannot supply the package', async () => {
    api.handoverPreview.mockResolvedValue({
      pending: false,
      migrated: false,
      instances: [],
      notes: [],
      blockers: [{ kind: 'versionNotInstalled', subject: 'mariadb@10.6', detail: '10.11' }],
      backup: false,
      missing: [{ service: 'mariadb', version: '10.6', installable: false }],
    });

    const page = mountPage();
    await flushPromises();

    const text = page.text();
    expect(text).toContain('would be an upgrade nobody asked for');
    expect(text).toContain('not in the catalogue this machine has read');
  });

  /// A workspace that has already migrated is told nothing.
  ///
  /// The panel keyed on "are there blockers", and the plan behind it reads
  /// `.env` — whose service keys are deliberately left behind as a record,
  /// marked rather than deleted. So a machine that migrated, whose Services
  /// page was reading the table and whose containers were running from it, was
  /// told it "still keeps its services in .env".
  it('says nothing about the handover once the workspace has migrated', async () => {
    api.handoverPreview.mockResolvedValue({
      pending: false,
      migrated: true,
      instances: [],
      notes: [],
      // What the old preview produced from the leftover `.env` record.
      blockers: [{ kind: 'versionNotInstalled', subject: 'mariadb@10.6', detail: '10.11' }],
      backup: true,
      missing: [{ service: 'mariadb', version: '10.6', installable: true }],
    });

    const page = mountPage();
    await flushPromises();

    expect(page.text()).not.toContain('still keeps its services in .env');
    expect(page.text()).not.toContain('mariadb@10.6');
  });

  /// The catalogue and this machine, beside each other.
  ///
  /// They were stacked, so on a real catalogue the instance table was a scroll
  /// away below twenty-five services — and an empty one read as if the page had
  /// simply ended. The assertion is on the structure rather than on pixels
  /// because jsdom does no layout; what it can hold is that both panes are
  /// siblings of one container rather than one following the other.
  it('puts the catalogue and the instances in two columns', async () => {
    const page = mountPage();
    await flushPromises();

    const columns = page.find('.market-columns');
    expect(columns.exists()).toBe(true);
    expect(columns.findAll('.market-col')).toHaveLength(2);

    const [catalogue, instances] = columns.findAll('.market-col');
    expect(catalogue.text()).toContain('MySQL');
    expect(instances.text()).toContain('mysql-8-0');
    expect(catalogue.text()).not.toContain('mysql-8-0');
  });

  /// The search field stays the height of a search field.
  ///
  /// Side by side, the catalogue column is a flex column so its tree can scroll
  /// inside the card. Vuetify's `.v-input` carries `flex: 1 1 auto`, which in a
  /// column reads as "grow taller" — so with the categories collapsed the field
  /// split the unused height with the tree and stood at twice its own size,
  /// with the label floating in the middle of an empty box.
  ///
  /// Read from the source: jsdom applies neither Vuetify's stylesheet nor a
  /// scoped `<style>` block, and every assertion in this file passed while the
  /// field was 80px tall.
  it('does not let the search field grow into the column', async () => {
    const source = readFileSync('src/views/Market.vue', 'utf8');
    const style = source.slice(source.indexOf('<style'));
    const rule = /\.market-col\s+:deep\(\.group-body\)\s*>\s*\.v-input\s*\{([^}]*)\}/.exec(style);

    expect(rule, 'nothing stops .v-input from growing in the column').not.toBeNull();
    expect(rule[1]).toMatch(/flex:\s*0\s+0/);

    // Inside the two-column media query, because stacked there is no leftover
    // height to take and the rule would be answering a question nobody asked.
    const query = style.indexOf('@media (min-width: 1281px)');
    expect(query).toBeGreaterThan(-1);
    expect(style.indexOf(rule[0])).toBeGreaterThan(query);
  });

  /// Every instance is one row of one line.
  ///
  /// A service with two ports — RabbitMQ's broker and its management UI — put
  /// them on two lines, and that row stood at twice the height of the others;
  /// the container names and three of the column headings wrapped as well, so
  /// the table had rows of three different heights and the eye lost the one it
  /// was reading. The cell content is asserted here; the `nowrap` that stops
  /// the rest is read from the source, because jsdom applies no stylesheet.
  it('keeps an instance on one line however many ports it has', async () => {
    api.instanceList.mockResolvedValue([
      { ...INSTANCES[0], id: 'rabbitmq-4', ports: { main: 5672, mgmt: 15672 } },
    ]);

    const page = mountPage();
    await flushPromises();

    const cells = page.findAll('.instances-table tbody td');
    const ports = cells.find((c) => c.text().includes('5672'));
    expect(ports.text()).toContain('main: 5672');
    expect(ports.text()).toContain('mgmt: 15672');
    // Stacked, each port was a block of its own. Inline, they are spans.
    expect(ports.findAll('div')).toHaveLength(0);

    const style = readFileSync('src/views/Market.vue', 'utf8');
    const rule =
      /\.instances-table :deep\(th\),\s*\.instances-table :deep\(td\)\s*\{([^}]*)\}/.exec(style);
    expect(rule, 'nothing stops the cells wrapping').not.toBeNull();
    expect(rule[1]).toMatch(/white-space:\s*nowrap/);
  });

  /// An installed version is shown whatever its support status, or a user could
  /// not uninstall something that is on their machine.
  it('never hides a version that is installed', async () => {
    api.marketCatalog.mockResolvedValue([
      {
        ...CATALOG[0],
        versions: [{ ...CATALOG[0].versions[2], installed: true }],
      },
    ]);

    const page = mountPage();
    await flushPromises();
    expect(page.text()).toContain('5.7');
  });

  it('installs a version that is not here yet', async () => {
    api.marketInstall.mockResolvedValue(STATUS);
    const page = mountPage();
    await flushPromises();

    await page.vm.market.install('mysql', '9.4');
    expect(api.marketInstall).toHaveBeenCalledWith('mysql', '9.4');
    // And re-reads afterwards, so `installed` on the row is not stale.
    expect(api.marketCatalog).toHaveBeenCalledTimes(2);
  });

  /// The Rust side refuses this; the page refuses it earlier so the reason is
  /// visible before the click rather than after it.
  it('offers no uninstall for a version an instance is using', async () => {
    const page = mountPage();
    await flushPromises();

    // By accessible name, not by text: the action buttons are glyphs with the
    // label in a tooltip now, so `Uninstall` is on `aria-label` and matching on
    // rendered text would find nothing and pass a weaker assertion by accident.
    const uninstall = page
      .findAll('button')
      .filter((b) => b.attributes('aria-label') === 'Uninstall');
    expect(uninstall.length).toBeGreaterThan(0);
    expect(uninstall.every((b) => b.attributes('disabled') !== undefined)).toBe(true);
  });

  it('shows which instance holds the pre-package name', async () => {
    const page = mountPage();
    await flushPromises();

    expect(page.text()).toContain('mysql-8-0');
    expect(page.text()).toContain('stackvo-mysql-8-0');
    expect(page.text()).toContain('Primary');
  });

  /// The detail button resolves its row out of the services list, and the two
  /// commands agree on `id` because both take it from the instance table. A
  /// button that could not find its row would render disabled and say nothing
  /// — which is exactly what this asserts against.
  it('opens the detail sheet on the service behind the instance', async () => {
    const page = mountPage();
    await flushPromises();

    const detail = page.findAll('button').filter((b) => b.attributes('aria-label') === 'Detail');
    expect(detail).toHaveLength(1);
    expect(detail[0].attributes('disabled')).toBeUndefined();

    await detail[0].trigger('click');
    expect(page.vm.detailTarget?.id).toBe('mysql-8-0');
    // The row from `services_list`, not the one from `instance_list`: the sheet
    // reads `containerName` and `running`, which the second does not carry.
    expect(page.vm.detailTarget.containerName).toBe('stackvo-mysql-8-0');
  });

  /// A services list that has not answered yet leaves nothing to open, and the
  /// button says so by being disabled rather than opening an empty sheet.
  it('offers no detail while the services list is empty', async () => {
    api.servicesList.mockResolvedValue([]);

    const page = mountPage();
    await flushPromises();

    const detail = page.findAll('button').filter((b) => b.attributes('aria-label') === 'Detail');
    expect(detail).toHaveLength(1);
    expect(detail[0].attributes('disabled')).toBeDefined();
  });

  /// Errors arrive as errors rather than as a page that silently did nothing.
  it('surfaces a refusal from the back end', async () => {
    api.instanceRemove.mockRejectedValue({
      code: 'CONFLICT',
      message: 'mysql-8-0 is using this package',
    });

    const page = mountPage();
    await flushPromises();
    await page.vm.market.remove('mysql-8-0');
    await flushPromises();

    expect(page.text()).toContain('is using this package');
  });

  /// On and off is a different decision from installed and removed, and the
  /// page has to keep them apart: one is a switch, the other is a button that
  /// says Remove.
  it('switches an instance on without offering to delete anything', async () => {
    api.instanceEnable.mockResolvedValue('enable-1');
    const page = mountPage();
    await flushPromises();

    await page.vm.market.enable('mysql-8-0');
    expect(api.instanceEnable).toHaveBeenCalledWith('mysql-8-0');
    // And nothing on this row promises deletion — that word belongs to the
    // Remove button, and to market_uninstall behind it.
    expect(api.instanceRemove).not.toHaveBeenCalled();
  });

  it('switches one off through disable rather than through remove', async () => {
    api.instanceDisable.mockResolvedValue('disable-1');
    api.instanceList.mockResolvedValue([{ ...INSTANCES[0], enabled: true }]);

    const page = mountPage();
    await flushPromises();
    await page.vm.market.disable('mysql-8-0');

    expect(api.instanceDisable).toHaveBeenCalledWith('mysql-8-0');
    expect(api.instanceRemove).not.toHaveBeenCalled();
  });

  /// An instance whose package has gone cannot be switched on: the renderer
  /// would refuse the whole file, so the row refuses first.
  it('cannot switch on an instance whose package is missing', async () => {
    api.instanceList.mockResolvedValue([{ ...INSTANCES[0], packagePresent: false }]);
    const page = mountPage();
    await flushPromises();

    expect(page.text()).toContain('Package missing');
    // The status button, which is what switches an instance on since the table
    // gained the Services page's columns — the switch it replaced carried the
    // same refusal.
    const off = page.findAll('button').filter((b) => b.text() === 'OFF');
    expect(off).toHaveLength(1);
    expect(off[0].attributes('disabled')).toBeDefined();
  });

  /// Stop/Start is a different act from On/Off, and the row has to keep them
  /// apart: one leaves the instance enabled and the compose file alone, the
  /// other rewrites the table. The Services page makes the same distinction.
  it('stops a running instance without switching it off', async () => {
    api.instanceList.mockResolvedValue([{ ...INSTANCES[0], enabled: true }]);
    api.servicesList.mockResolvedValue([{ ...SERVICES[0], enabled: true, running: true }]);
    api.instanceStop.mockResolvedValue('stop-1');

    const page = mountPage();
    await flushPromises();

    const stop = page.findAll('button').filter((b) => b.attributes('aria-label') === 'Stop');
    expect(stop).toHaveLength(1);

    await stop[0].trigger('click');
    await flushPromises();

    expect(api.instanceStop).toHaveBeenCalledWith('mysql-8-0');
    expect(api.instanceDisable).not.toHaveBeenCalled();
  });

  /// A link to a container that is not running is a tab showing Traefik's 404,
  /// so the button needs both halves to be true.
  it('offers the browser only for a running instance that has a domain', async () => {
    const withDomain = { ...SERVICES[0], enabled: true, url: 'phpmyadmin.stackvo.loc' };
    api.instanceList.mockResolvedValue([{ ...INSTANCES[0], enabled: true }]);
    api.servicesList.mockResolvedValue([{ ...withDomain, running: false }]);

    let page = mountPage();
    await flushPromises();
    const open = () =>
      page.findAll('button').filter((b) => b.attributes('aria-label') === 'Open in browser');
    expect(open()).toHaveLength(0);

    api.servicesList.mockResolvedValue([{ ...withDomain, running: true }]);
    page = mountPage();
    await flushPromises();
    expect(open()).toHaveLength(1);
  });

  /// Reported rather than assumed: no key is pinned, so nothing verifies a
  /// signature, and the page says which.
  ///
  /// In the source menu now rather than on a permanent line above the
  /// catalogue. The line said the same three things on every visit and cost a
  /// row of the page to do it; the menu is where somebody is already asking
  /// where the catalogue comes from. Opened here because a menu renders its
  /// contents when it opens — asserting on the closed page would find nothing
  /// and prove nothing.
  it('says the catalogue is not signature-checked', async () => {
    const page = mountPage();
    await flushPromises();

    page.vm.sourceOpen = true;
    await flushPromises();

    expect(document.body.textContent).toContain('not signature-checked');
    // And which source it is talking about, beside it.
    expect(document.body.textContent).toContain('/Users/me/stackvo-service-packages');
  });
  /**
   * The catalogue had no search: twenty-five services and a hundred versions
   * behind eight collapsed categories, and finding Valkey meant knowing it is
   * filed under `cache`. `keywords` is the field that makes it work — the
   * index has published them since v1 and `market_catalog` was dropping them
   * on the floor, so there was nothing to search even if there had been a box.
   */
  describe('searching the catalogue', () => {
    // Both categories, so "it narrowed" means something: with one root a
    // filtered tree is indistinguishable from an unfiltered one.
    beforeEach(() => api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]));

    it('narrows to what matches, across categories', async () => {
      const page = mountPage();
      await flushPromises();

      expect(page.vm.market.grouped.value.map((g) => g.category)).toEqual(['databases', 'cache']);

      page.vm.market.query.value = 'redis';
      await flushPromises();

      const groups = page.vm.market.grouped.value;
      expect(groups).toHaveLength(1);
      expect(groups[0].packages.map((p) => p.service)).toEqual(['redis']);
    });

    it('finds a package by a keyword rather than by its name', async () => {
      const page = mountPage();
      await flushPromises();

      // The whole point of the field: MySQL is meant to be findable by typing
      // `database`, and by typing `mariadb`.
      page.vm.market.query.value = 'mariadb';
      await flushPromises();

      expect(page.vm.market.grouped.value[0].packages.map((p) => p.service)).toEqual(['mysql']);
    });

    it('says so plainly when nothing matches', async () => {
      const page = mountPage();
      await flushPromises();

      page.vm.market.query.value = 'nothing-is-called-this';
      await flushPromises();

      expect(page.vm.market.grouped.value).toHaveLength(0);
    });
  });
});

/**
 * The package authoring dialog (C-1).
 *
 * The property worth a test is the one that is silent when wrong: a report
 * carrying problems means **nothing was written**, so the screen must not show
 * a package as valid beside a list of refusals. That is exactly the mistake a
 * later "tidy the alerts" change would make.
 */
describe('writing a package', () => {
  /**
   * Queried through `document`, not the wrapper: `v-dialog` teleports its card
   * to the body, so `wrapper.find` reaches an element that is not there.
   */
  async function open() {
    document.body.innerHTML = '';
    const component = (await import('@/components/PackageAuthorDialog.vue')).default;
    mount(
      {
        components: { PackageAuthorDialog: component },
        template: '<v-app><PackageAuthorDialog :model-value="true" /></v-app>',
      },
      { global: { plugins: [createPinia(), vuetify, i18n] }, attachTo: document.body }
    );
    await flushPromises();
  }

  const type = (placeholder, value) => {
    const input = document.querySelector(`input[placeholder="${placeholder}"]`);
    input.value = value;
    input.dispatchEvent(new Event('input'));
    return flushPromises();
  };

  const press = (label) => {
    const button = [...document.querySelectorAll('button')].find(
      (b) => b.textContent.trim() === label
    );
    button.click();
    return flushPromises();
  };

  it('reports refusals and does not also call the package valid', async () => {
    api.packageSeal.mockResolvedValue({
      service: 'widget',
      version: '1.0',
      dir: '/tmp/widget',
      resealed: ['compose.yml'],
      problems: ['widget@1.0: privileged — Container escape, in one word.'],
    });
    await open();

    await type('widget', 'widget');
    await type('1.0', '1.0');
    await press(i18n.global.t('authoring.seal'));

    expect(document.body.textContent).toContain('privileged');
    expect(document.body.textContent).not.toContain(
      i18n.global.t('authoring.valid', { service: 'widget', version: '1.0' })
    );
  });

  it('names the files it rewrote and the directory to edit', async () => {
    api.packageScaffold.mockResolvedValue({
      service: 'widget',
      version: '1.0',
      dir: '/tmp/market/packages/databases/widget/versions/1.0',
      resealed: ['compose.yml'],
      problems: [],
    });
    await open();

    await type('widget', 'widget');
    await type('1.0', '1.0');
    await type('widget:1.0', 'widget:1.0');
    await press(i18n.global.t('authoring.create'));

    expect(document.body.textContent).toContain('compose.yml');
    expect(document.body.textContent).toContain(
      '/tmp/market/packages/databases/widget/versions/1.0'
    );
  });
});
