import { ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * Worker sidecars for this project.
 *
 * `kinds` comes from the project's files (artisan, composer.json); `workers`
 * from the engine. Docker itself does the healing — this only starts, stops,
 * and surfaces the restart count that healing produces.
 *
 * Lifted out of `ProjectDetail.vue` with the Workers pane under §14.16.
 */
export function useWorkers(name) {
  const kinds = ref([]);
  const workers = ref([]);
  const busy = ref(null);
  const error = ref(null);

  async function load() {
    try {
      const [options, all] = await Promise.all([api.workerOptions(name.value), api.workerStatus()]);
      kinds.value = asList(options);
      workers.value = asList(all).filter((w) => w.project === name.value);
    } catch (e) {
      error.value = e;
    }
    return workers.value;
  }

  /** The running sidecar for a kind, or `null` when that kind is stopped. */
  function workerFor(kind) {
    return workers.value.find((w) => w.kind === kind) ?? null;
  }

  /**
   * One button per kind, and what it does depends on what is running now —
   * the pane has no separate start and stop controls to keep in step.
   */
  async function toggle(kind) {
    busy.value = kind;
    error.value = null;
    try {
      if (workerFor(kind)) await api.workerStop(name.value, kind);
      else await api.workerStart(name.value, kind);
      await load();
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = null;
    }
  }

  return { kinds, workers, busy, error, load, workerFor, toggle };
}
