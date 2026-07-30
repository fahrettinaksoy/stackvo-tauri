import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/lib/ipc';

const HISTORY_LENGTH = 60;

/**
 * Host metrics, sampled from the host itself.
 *
 * The values behind this store used to come from `/proc` read inside a
 * container — a constrained view on Linux and, on macOS, no view at all
 * (the old code silently estimated CPU from loadavg). These are the real
 * machine's numbers.
 */
export const useMetricsStore = defineStore('metrics', () => {
  const stats = ref(null);
  const resources = ref(null);
  const cpuHistory = ref([]);
  const memoryHistory = ref([]);
  // Throughput series for the sparklines. Rates, not totals — a cumulative
  // counter drawn as a sparkline is always a straight rising line.
  const diskRead = ref([]);
  const diskWrite = ref([]);
  const netRx = ref([]);
  const netTx = ref([]);
  const loading = ref(false);
  /** The first sample has no previous one to diff against, so no rate yet. */
  const hasRate = ref(false);

  let timer = null;

  function push(series, value) {
    series.value.push(value);
    if (series.value.length > HISTORY_LENGTH) series.value.shift();
  }

  async function refresh() {
    loading.value = true;
    try {
      const sample = await api.hostStats();
      // A poll that comes back empty is a skipped tick, not a reason to poison
      // the history with `undefined` — refresh runs on a timer, so throwing
      // here surfaces as an unhandled rejection with no caller to catch it.
      if (!sample) return;
      stats.value = sample;
      push(cpuHistory, sample.cpu.percent);
      push(memoryHistory, sample.memory.percent);
      push(diskRead, sample.disk.readRate);
      push(diskWrite, sample.disk.writeRate);
      push(netRx, sample.network.rxRate);
      push(netTx, sample.network.txRate);
      if (cpuHistory.value.length > 1) hasRate.value = true;
    } finally {
      loading.value = false;
    }
  }

  async function refreshResources() {
    try {
      resources.value = await api.dockerSystemResources();
    } catch {
      // Docker being down is normal; the host metrics above still work, which
      // is precisely the separation the container-hosted UI could not express.
      resources.value = null;
    }
  }

  /** Poll while the window is visible; stop when it is not. */
  function start(intervalMs = 2000) {
    stop();
    refresh();
    refreshResources();
    timer = setInterval(() => {
      if (document.visibilityState === 'visible') refresh();
    }, intervalMs);
  }

  function stop() {
    if (timer) clearInterval(timer);
    timer = null;
  }

  return {
    stats,
    resources,
    cpuHistory,
    memoryHistory,
    diskRead,
    diskWrite,
    netRx,
    netTx,
    loading,
    hasRate,
    refresh,
    refreshResources,
    start,
    stop,
  };
});
