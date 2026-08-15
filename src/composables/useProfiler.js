import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * Xdebug's profiler: a mode of the existing extension rather than a second
 * switch, plus the files it records and one opened report.
 *
 * The two modes are exclusive because they want opposite start triggers —
 * stepping connects on the next request, profiling waits for `XDEBUG_TRIGGER`
 * so an idle stack does not write a multi-megabyte file per page load.
 *
 * Lifted out of `ProjectDetail.vue` with the Profiler pane under §14.16.
 */
export function useProfiler(name) {
  const status = ref(null);
  const report = ref(null);
  /**
   * The call tree for the open profile, fetched only when the flame view is
   * asked for (F-3).
   *
   * Lazily and separately, because the two answers are different sizes: the
   * table is sixty rows and the tree is thousands of nodes. A pane that opens
   * on the table should not carry the graph across the boundary to ignore it.
   */
  const tree = ref(null);
  const treeBusy = ref(false);
  const openId = ref('');
  const error = ref(null);

  /** `''` when idle, otherwise `'mode'`, `'clear'`, or the id of one file. */
  const busy = ref('');

  /**
   * Is the running container in the mode the app is set to?
   *
   * This asked `active === false`, and that never fired for the case it exists
   * for. `active` means "both Xdebug variables are present", and after
   * switching stepping to profiling they still are — with `XDEBUG_MODE=debug`
   * in them. So the page reported profiling as applied, the trigger did
   * nothing, and the recorded list stayed at zero with nothing to say why.
   *
   * The container's own mode is the answer, compared against the configured
   * one. `null` while nothing is running is not a mismatch — a stopped project
   * has no mode to disagree with.
   */
  const needsRestart = computed(() => {
    const s = status.value;
    if (!s?.xdebug?.running) return false;
    if (s.xdebug.active === false) return true;
    return !!s.xdebug.activeMode && s.xdebug.activeMode !== s.mode;
  });

  /**
   * The time unit the *file* declares — never assumed.
   *
   * Measured on a real profile: `Time_(10ns)`. Reading it as microseconds would
   * be wrong by two orders of magnitude, and the number would look plausible.
   */
  const unit = computed(() => {
    const declared = asList(report.value?.events)[0] ?? '';
    const match = String(declared).match(/\(([^)]+)\)/);
    return match ? match[1] : '';
  });

  /** Cost in the file's own unit, rendered as ms when the unit is known. */
  function cost(value) {
    const ns = { '10ns': 10, ns: 1, us: 1000, ms: 1_000_000 }[unit.value];
    if (!ns) return `${value} ${unit.value}`.trim();
    const ms = (value * ns) / 1_000_000;
    return ms >= 1 ? `${ms.toFixed(1)} ms` : `${(ms * 1000).toFixed(0)} µs`;
  }

  async function load(runtime) {
    if (runtime !== 'php') {
      status.value = null;
      return null;
    }
    try {
      status.value = await api.profilerStatus(name.value);
    } catch {
      status.value = null;
    }
    return status.value;
  }

  async function setMode(mode) {
    busy.value = 'mode';
    error.value = null;
    try {
      status.value = await api.profilerSetMode(name.value, mode);
      return status.value;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      busy.value = '';
    }
  }

  async function open(file) {
    busy.value = file.id;
    error.value = null;
    report.value = null;
    try {
      report.value = await api.profilerRead(name.value, file.id);
      // A tree belonging to the profile that was open a moment ago is worse
      // than none: it renders, and it is about a different request.
      tree.value = null;
      openId.value = file.id;
      return report.value;
    } catch (e) {
      error.value = e;
      // Nothing is open, so nothing should be highlighted as open either.
      openId.value = '';
      return null;
    } finally {
      busy.value = '';
    }
  }

  async function remove(file, runtime) {
    busy.value = file.id;
    error.value = null;
    try {
      await api.profilerDelete(name.value, file.id);
      // The open report belongs to a file that no longer exists.
      if (openId.value === file.id) {
        report.value = null;
        openId.value = '';
      }
      await load(runtime);
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = '';
    }
  }

  async function clear(runtime) {
    busy.value = 'clear';
    error.value = null;
    try {
      await api.profilerClear(name.value);
      report.value = null;
      openId.value = '';
      await load(runtime);
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = '';
    }
  }

  /** Fetch the call tree for whatever is open. */
  async function loadTree() {
    if (!openId.value || treeBusy.value) return;
    treeBusy.value = true;
    error.value = null;
    try {
      tree.value = await api.profilerTree(name.value, openId.value);
    } catch (e) {
      error.value = e;
    } finally {
      treeBusy.value = false;
    }
  }

  return {
    status,
    report,
    tree,
    treeBusy,
    loadTree,
    openId,
    busy,
    error,
    needsRestart,
    unit,
    cost,
    load,
    setMode,
    open,
    remove,
    clear,
  };
}
