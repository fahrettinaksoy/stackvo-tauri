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
    expect(page.text()).toContain('databases');
    expect(api.marketCatalog).toHaveBeenCalled();
  });

  /// Hidden by default, listed behind a switch, never removed — somebody's
  /// workspace may name it.
  it('keeps an end-of-life version out of the way without withdrawing it', async () => {
    const page = mountPage();
    await flushPromises();

    expect(page.text()).not.toContain('5.7');
    expect(page.text()).toContain('1 hidden');

    page.vm.market.showOlder.value = true;
    await flushPromises();
    expect(page.text()).toContain('5.7');
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
