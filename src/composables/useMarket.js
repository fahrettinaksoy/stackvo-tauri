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

  const loading = ref(false);
  const error = ref(null);

  /** The service+version currently being installed or removed, or null. */
  const working = ref(null);

  /** Whether end-of-life versions are listed. */
  const showOlder = ref(false);

  const fetched = computed(() => status.value?.fetched === true);

  /**
   * Packages with their versions filtered for display.
   *
   * An installed version is always shown, whatever its support status: hiding
   * something that is on the machine would leave a user unable to uninstall it.
   */
  const visible = computed(() =>
    packages.value.map((entry) => ({
      ...entry,
      versions: entry.versions.filter((v) => showOlder.value || v.support !== 'eol' || v.installed),
      // Zero while they are being shown. Saying "1 hidden" next to a list
      // that is showing it is a count that contradicts the thing beside it.
      hidden: showOlder.value
        ? 0
        : entry.versions.filter((v) => v.support === 'eol' && !v.installed).length,
    }))
  );

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

  const create = (service, version) =>
    run(`${service}@${version}`, () => api.instanceCreate(service, version));

  const remove = (id) => run(id, () => api.instanceRemove(id));

  const promote = (id) => run(id, () => api.instancePromote(id));

  /// On and off, which is a different decision from installed and removed.
  /// Nothing is deleted by either (ADR 0012) — the volume outlives both.
  const enable = (id) => run(id, () => api.instanceEnable(id));
  const disable = (id) => run(id, () => api.instanceDisable(id));

  const start = (id) => run(id, () => api.instanceStart(id));
  const stop = (id) => run(id, () => api.instanceStop(id));
  const restart = (id) => run(id, () => api.instanceRestart(id));

  return {
    status,
    packages,
    instances,
    visible,
    instancesOf,
    anyInstalled,
    fetched,
    loading,
    error,
    working,
    showOlder,
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
  };
}
