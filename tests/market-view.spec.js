import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';
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
}));

vi.mock('@/lib/ipc', () => ({ api, asList: (v) => (Array.isArray(v) ? v : []) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

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
    summary: {},
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
 * A second category, so the tab strip is more than one tab.
 *
 * The catalogue fixture had exactly one, which meant every assertion about
 * grouping passed on a page that never had to choose between groups — and a
 * tab strip with one tab is indistinguishable from no tab strip at all.
 */
const REDIS = {
  service: 'redis',
  category: 'cache',
  name: { en: 'Redis' },
  summary: {},
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

function mountPage() {
  return mount(Market, { global: { plugins: [vuetify, i18n] } });
}

beforeEach(() => {
  vi.clearAllMocks();
  api.marketStatus.mockResolvedValue(STATUS);
  api.marketCatalog.mockResolvedValue(CATALOG);
  api.instanceList.mockResolvedValue(INSTANCES);
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

    // en.js's `serviceSettings.categories.databases`, which Settings already
    // uses — not the `databases` directory name the package carries.
    expect(page.text()).toContain('Databases');
    expect(page.text()).toContain('1 service(s)');
  });

  /// Hidden by default, listed behind a switch, never removed — somebody's
  /// workspace may name it.
  it('keeps an end-of-life version out of the way without withdrawing it', async () => {
    const page = mountPage();
    await flushPromises();

    expect(page.text()).not.toContain('5.7');
    // "end-of-life", not "hidden". The count is a fact about the version's
    // upstream, and "hidden" reads as something the app is withholding.
    expect(page.text()).toContain('1 end-of-life');
    // And the page says why one is published at all, next to the switch.
    expect(page.text()).toContain('upstream has stopped patching them');

    page.vm.market.showOlder.value = true;
    await flushPromises();
    expect(page.text()).toContain('5.7');
  });

  /// One tab per category, and only the open one on screen.
  ///
  /// The categories were stacked headings, which on twenty-five services made
  /// the catalogue one long scroll whatever you were looking for. The order is
  /// the repository's own — a stack is a database and a cache before it is an
  /// admin UI — so `databases` is what the page opens on.
  it('offers a tab per category and opens on the first', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    const tabs = page.findAll('.v-tab');
    expect(tabs).toHaveLength(2);
    // Translated names, and the fixed order rather than alphabetical — which
    // would have opened the page on `cache`.
    expect(tabs[0].text()).toContain('Databases');
    expect(tabs[1].text()).toContain('Cache');

    expect(page.vm.category).toBe('databases');
  });

  /// Down the side, not across the top.
  ///
  /// Horizontal was the first attempt and does not fit: eight category names in
  /// a column that is half the page either scroll behind arrows — hiding the
  /// thing the tabs were added to make visible — or wrap to three rows.
  it('runs the category rail vertically', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    expect(page.find('.v-tabs--vertical').exists()).toBe(true);
    // And no arrow affordance, which is what a horizontal strip needs and what
    // hides categories behind a scroll.
    expect(page.find('.v-slide-group__prev').exists()).toBe(false);
  });

  /// Switching tabs shows the other category's services.
  it('shows the category that is selected', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    const windows = page.findAll('.v-window-item');
    expect(windows).toHaveLength(2);

    page.vm.category = 'cache';
    await flushPromises();

    const active = page.find('.v-tab--selected');
    expect(active.text()).toContain('Cache');
    // `eager`, so both categories are in the document — a page a browser cannot
    // find text in is one you have to already know your way around.
    expect(page.text()).toContain('Redis');
    expect(page.text()).toContain('MySQL');
  });

  /// A selection that no longer exists leaves the strip with nothing active and
  /// the window blank. The groups are rebuilt on every refresh and on every
  /// change of source, so this is not a hypothetical.
  it('moves the selection when the category it named is gone', async () => {
    api.marketCatalog.mockResolvedValue([...CATALOG, REDIS]);

    const page = mountPage();
    await flushPromises();

    page.vm.category = 'cache';
    await flushPromises();
    expect(page.vm.category).toBe('cache');

    api.marketCatalog.mockResolvedValue(CATALOG);
    await page.vm.market.load();
    await flushPromises();

    expect(page.vm.category).toBe('databases');
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

    const uninstall = page.findAll('button').filter((b) => b.text() === 'Uninstall');
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
    const switches = page.findAll('input[type="checkbox"]');
    expect(switches.some((s) => s.attributes('disabled') !== undefined)).toBe(true);
  });

  /// Reported rather than assumed: no key is pinned, so nothing verifies a
  /// signature, and the page says which.
  it('says the catalogue is not signature-checked', async () => {
    const page = mountPage();
    await flushPromises();
    expect(page.text()).toContain('not signature-checked');
  });
});
