import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * The service catalogue, and what this machine has taken from it.
 *
 * Two lists that have to be read together: what a source publishes, and what is
 * installed here. A version row carries both — `installed` and `inUse` come
 * back with the catalogue — so the page can offer Install or refuse Uninstall
 * without a second round trip, and so a card cannot briefly show a state that
 * was true one request ago.
 *
 * ## Three states, not two
 *
 * `status.fetched` is false before the first refresh, and that is a different
 * screen from an empty catalogue. StackVo embeds no packages at all (ADR 0011),
 * so a fresh machine genuinely has nothing — and telling somebody "no services
 * found" when the answer is "you have not pointed me at a source yet" is the
 * kind of message that makes people reinstall.
 *
 * ## End-of-life versions are hidden, not withdrawn
 *
 * `showOlder` is false by default and the counts say what is behind it. A
 * version that upstream has stopped patching should not be the easy click; it
 * should also not disappear, because somebody's workspace may name it and
 * removing it from view is the first step to removing it from the index.
 */
export function useMarket() {
  const status = ref(null);
  const packages = ref([]);
  const instances = ref([]);

  /**
   * What the `.env` → `instances.json` migration would do.
   *
   * Loaded alongside the catalogue rather than behind a button, because a
   * workspace that has not migrated is one whose services are still described
   * somewhere the market page does not read — and a page that shows an empty
   * instance table on such a machine is telling the user something untrue.
   */
  const handover = ref(null);

  const loading = ref(false);
  const error = ref(null);

  /** The service+version currently being installed or removed, or null. */
  const working = ref(null);

  /** Whether end-of-life versions are listed. */
  const showOlder = ref(false);

  /**
   * What is being searched for, if anything.
   *
   * The catalogue is twenty-five services and a hundred versions behind eight
   * collapsed categories, and it had no search: finding Valkey meant knowing it
   * is filed under `cache`. `keywords` is what makes it work rather than a
   * name match — the index publishes them so that MySQL is findable by typing
   * `database`, and by typing `mariadb`.
   */
  const query = ref('');

  const matches = (entry, needle) =>
    [
      entry.service,
      entry.category,
      ...Object.values(entry.name ?? {}),
      ...Object.values(entry.summary ?? {}),
      ...(entry.keywords ?? []),
      ...(entry.capabilities ?? []),
    ].some((field) => String(field).toLowerCase().includes(needle));

  const fetched = computed(() => status.value?.fetched === true);

  /**
   * Packages with their versions filtered for display.
   *
   * An installed version is always shown, whatever its support status: hiding
   * something that is on the machine would leave a user unable to uninstall it.
   */
  const visible = computed(() => {
    const needle = query.value.trim().toLowerCase();
    return packages.value
      .filter((entry) => !needle || matches(entry, needle))
      .map((entry) => ({
        ...entry,
        versions: entry.versions.filter(
          (v) => showOlder.value || v.support !== 'eol' || v.installed
        ),
        // Zero while they are being shown. Saying "1 hidden" next to a list
        // that is showing it is a count that contradicts the thing beside it.
        hidden: showOlder.value
          ? 0
          : entry.versions.filter((v) => v.support === 'eol' && !v.installed).length,
      }));
  });

  /**
   * The catalogue, grouped the way the packages repository is laid out.
   *
   * A flat list of twenty-five services with the category as a chip made the
   * category something you *read* rather than something you navigate by, and
   * the chip is the same width as a word so scanning for "the databases" meant
   * reading every row. The grouping already exists — it is the directory
   * structure of `packages/`, and `env.schema.json` used the same names before
   * that — so this is showing a fact rather than inventing an arrangement.
   *
   * The order is fixed rather than alphabetical: a stack is a database and a
   * cache before it is an admin UI, and sorting by name would open the list on
   * `admin-uis`, which is the category you pick last.
   */
  const ORDER = [
    'databases',
    'cache',
    'queue',
    'search',
    'storage',
    'monitoring',
    'devtools',
    'admin-uis',
  ];

  const grouped = computed(() => {
    const by = new Map();
    for (const entry of visible.value) {
      if (!by.has(entry.category)) by.set(entry.category, []);
      by.get(entry.category).push(entry);
    }
    // Anything the app has no order for still appears, after the ones it does.
    // A category added to the repository before it is added here is a category
    // whose services would otherwise vanish from this page.
    const known = ORDER.filter((c) => by.has(c));
    const rest = [...by.keys()].filter((c) => !ORDER.includes(c)).sort();
    return [...known, ...rest].map((category) => ({
      category,
      packages: by.get(category),
      // What the end-of-life switch is holding back, per category, so the
      // count sits next to the thing it is about.
      hidden: by.get(category).reduce((n, p) => n + p.hidden, 0),
    }));
  });

  const instancesOf = computed(() => {
    const by = new Map();
    for (const instance of instances.value) {
      if (!by.has(instance.service)) by.set(instance.service, []);
      by.get(instance.service).push(instance);
    }
    return by;
  });

  /** Anything at all installed? Drives the empty state on the instances pane. */
  const anyInstalled = computed(() => instances.value.length > 0);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      status.value = await api.marketStatus();
      packages.value = asList(await api.marketCatalog());
      instances.value = asList(await api.instanceList());
      handover.value = await api.handoverPreview();
    } catch (e) {
      error.value = e;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Point at a directory and read its catalogue.
   *
   * The location is a path the user chose — an offline bundle, or a checkout of
   * the packages repository. Refusing an index older than the cached one
   * happens in Rust; here it arrives as an ordinary error with a hint, because
   * "this source is behind" is something a person can act on.
   */
  async function refresh(location) {
    if (!location) return;
    loading.value = true;
    error.value = null;
    try {
      status.value = await api.marketRefresh(location);
      packages.value = asList(await api.marketCatalog());
    } catch (e) {
      error.value = e;
    } finally {
      loading.value = false;
    }
  }

  async function run(key, action) {
    working.value = key;
    error.value = null;
    try {
      await action();
      await load();
    } catch (e) {
      error.value = e;
    } finally {
      working.value = null;
    }
  }

  const install = (service, version) =>
    run(`${service}@${version}`, () => api.marketInstall(service, version));

  const uninstall = (service, version) =>
    run(`${service}@${version}`, () => api.marketUninstall(service, version));

  /**
   * Create one, with whatever the form collected.
   *
   * `settings` and `ports` are null when the caller had no form — creating with
   * the package's own defaults, which is what the button did before the create
   * dialog existed.
   */
  const create = (service, version, settings = null, ports = null) =>
    run(`${service}@${version}`, () => api.instanceCreate(service, version, settings, ports));

  const remove = (id) => run(id, () => api.instanceRemove(id));

  const promote = (id) => run(id, () => api.instancePromote(id));

  /// On and off, which is a different decision from installed and removed.
  /// Nothing is deleted by either (ADR 0012) — the volume outlives both.
  const enable = (id) => run(id, () => api.instanceEnable(id));
  const disable = (id) => run(id, () => api.instanceDisable(id));

  const start = (id) => run(id, () => api.instanceStart(id));
  const stop = (id) => run(id, () => api.instanceStop(id));
  const restart = (id) => run(id, () => api.instanceRestart(id));

  /**
   * Carry `.env`'s services over to the instance table.
   *
   * Offered only while `handover.pending` and never while it has blockers: the
   * migration is all-or-nothing in Rust, and a button that produced the same
   * refusal every time it was pressed would be a button that lies about being
   * available.
   *
   * `.env` is copied to `.env.pre-market.bak` first and its service keys are
   * marked rather than deleted, so going back is deleting the table.
   */
  const migrate = () => run('handover', () => api.handoverApply());

  /** Something the user should read before agreeing, not after. */
  const handoverPending = computed(
    () =>
      handover.value?.migrated !== true &&
      handover.value?.pending === true &&
      handover.value?.blockers.length === 0
  );
  /**
   * Blocked, and only while there is still something to do.
   *
   * `migrated` is checked here as well as in Rust because the two answer
   * different questions: Rust decides what the preview *is*, this decides
   * whether the page says anything at all. A workspace that migrated months ago
   * has a `.env` full of the keys it was migrated from — they are marked, never
   * deleted, so the record survives — and a panel keyed on "are there blockers"
   * read that record back as an outstanding job.
   */
  const handoverBlocked = computed(
    () => handover.value?.migrated !== true && (handover.value?.blockers ?? []).length > 0
  );

  /** Packages the handover needs and this machine has not got. */
  const handoverMissing = computed(() => handover.value?.missing ?? []);

  /**
   * Install everything the handover is short of, in one act.
   *
   * Sequential rather than concurrent: each install writes into the package
   * tree and the last one to finish decides what `load()` reads, and a
   * half-installed tree is exactly the state `market::install` goes to trouble
   * to avoid producing. Slower, and the slower one is the one that is right.
   *
   * Stops at the first refusal rather than pressing on. A policy that forbids
   * one package will forbid the next, and five identical errors in a row is a
   * worse answer than one.
   */
  async function installMissing() {
    const wanted = handoverMissing.value.filter((m) => m.installable);
    if (!wanted.length) return;
    await run('handover', async () => {
      for (const { service, version } of wanted) {
        await api.marketInstall(service, version);
      }
    });
  }

  return {
    status,
    packages,
    instances,
    handover,
    handoverPending,
    handoverBlocked,
    handoverMissing,
    visible,
    grouped,
    instancesOf,
    anyInstalled,
    fetched,
    loading,
    error,
    working,
    showOlder,
    query,
    load,
    refresh,
    install,
    uninstall,
    create,
    remove,
    promote,
    enable,
    disable,
    start,
    stop,
    restart,
    migrate,
    installMissing,
  };
}
