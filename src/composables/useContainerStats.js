import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * One container's live resource use, and the two hours of history behind it.
 *
 * Lifted out of `ProjectDetail.vue` with the Indicator pane. Everything here is
 * *derived* from two calls — `container_stats` on a timer and
 * `container_stats_history` once — so it is the half worth testing: the pie
 * slices, the sparkline window and the heat grid all fabricate structure that
 * no test has ever looked at.
 */

/** How often the live sample is taken. */
export const STATS_INTERVAL = 2000;

/** How many live samples the sparkline keeps. */
const SERIES_LENGTH = 60;

/** How many days the heat grid shows. */
const HEAT_DAYS = 7;

export function useContainerStats(t) {
  const stats = ref(null);
  const history = ref([]);
  const cpuSeries = ref([]);

  let timer = null;

  const memoryPie = computed(() => {
    if (!stats.value) return [];
    const free = Math.max(0, stats.value.memoryLimit - stats.value.memoryUsed);
    return [
      { key: 'used', title: t('dashboard.used'), value: stats.value.memoryUsed, color: '#1976D2' },
      { key: 'free', title: t('dashboard.available'), value: free, color: '#2A313C' },
    ];
  });

  // `|| 1` on every slice: a pie of two zeroes has no geometry, and Vuetify
  // draws nothing at all rather than an empty ring. One byte is a lie small
  // enough to be invisible and large enough to keep the shape.
  const networkPie = computed(() => {
    if (!stats.value) return [];
    return [
      { key: 'rx', title: t('stats.download'), value: stats.value.netRx || 1, color: '#1976D2' },
      { key: 'tx', title: t('stats.upload'), value: stats.value.netTx || 1, color: '#2A313C' },
    ];
  });

  const diskPie = computed(() => {
    if (!stats.value) return [];
    return [
      {
        key: 'read',
        title: t('dashboard.read'),
        value: stats.value.blockRead || 1,
        color: '#1976D2',
      },
      {
        key: 'write',
        title: t('dashboard.write'),
        value: stats.value.blockWrite || 1,
        color: '#2A313C',
      },
    ];
  });

  /**
   * CPU activity as a day × hour grid, the way the web UI draws it.
   *
   * The samples are taken every 60s and capped at two hours, so early on most
   * cells have no reading at all. An empty cell is drawn differently from a
   * cell that measured zero — "we did not look" and "nothing happened" are not
   * the same thing, and colouring them alike would invent history.
   */
  const heatmap = computed(() => {
    const byDay = new Map();

    for (const sample of history.value) {
      const date = new Date(sample.t * 1000);
      const dayKey = date.toDateString();
      if (!byDay.has(dayKey)) {
        byDay.set(dayKey, { label: date, hours: Array.from({ length: 24 }, () => null) });
      }
      const hour = date.getHours();
      const row = byDay.get(dayKey).hours;
      // Several samples land in the same hour; keep the peak.
      row[hour] = row[hour] === null ? sample.cpu : Math.max(row[hour], sample.cpu);
    }

    return [...byDay.values()].slice(-HEAT_DAYS);
  });

  async function loadHistory(container) {
    history.value = asList(await api.containerStatsHistory(container));
    cpuSeries.value = history.value.map((s) => s.cpu);
  }

  /**
   * Start polling, or stop and clear if the container is not running.
   *
   * Clearing rather than freezing: a stopped container showing its last reading
   * is a chart that claims the container is still doing something.
   */
  function start(container, running) {
    stop();
    if (!running || !container) {
      stats.value = null;
      return;
    }

    const tick = async () => {
      try {
        stats.value = await api.containerStats(container);
        cpuSeries.value = [...cpuSeries.value, stats.value.cpuPercent].slice(-SERIES_LENGTH);
      } catch {
        stats.value = null;
      }
    };
    tick();
    timer = setInterval(tick, STATS_INTERVAL);
  }

  function stop() {
    clearInterval(timer);
    timer = null;
  }

  return {
    stats,
    history,
    cpuSeries,
    memoryPie,
    networkPie,
    diskPie,
    heatmap,
    loadHistory,
    start,
    stop,
  };
}

/**
 * Which shade a heat cell gets.
 *
 * `null` is its own class rather than the lowest band: an hour nobody measured
 * must not look like an hour that measured zero.
 */
export function heatLevel(value) {
  if (value === null) return 'empty';
  if (value < 1) return 'l0';
  if (value < 10) return 'l1';
  if (value < 30) return 'l2';
  if (value < 60) return 'l3';
  return 'l4';
}
