import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

/**
 * The numbers behind one project's Indicator pane.
 *
 * Everything here is *derived* from two calls — `container_stats` on a timer
 * and `container_stats_history` once — and until now none of it was covered:
 * `ProjectDetail.vue` is 3,007 lines at 0%, so the pie slices, the sparkline
 * window and the heat grid all fabricated structure that nothing had ever
 * looked at.
 *
 * Three of those derivations have a way of being quietly wrong, and each is
 * asserted below.
 */

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

const { useContainerStats, heatLevel, STATS_INTERVAL } =
  await import('@/composables/useContainerStats');

/** The i18n stub: the titles are pass-through so assertions read as keys. */
const t = (key) => key;

const SAMPLE = {
  cpuPercent: 12,
  memoryUsed: 300,
  memoryLimit: 1000,
  netRx: 500,
  netTx: 200,
  blockRead: 40,
  blockWrite: 60,
};

beforeEach(() => {
  calls.length = 0;
  for (const key of Object.keys(replies)) delete replies[key];
  replies.containerStats = { ...SAMPLE };
  replies.containerStatsHistory = [];
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('the pies', () => {
  it('are empty until there is a sample, rather than showing zeroes', async () => {
    const s = useContainerStats(t);
    expect(s.memoryPie.value).toEqual([]);
    expect(s.networkPie.value).toEqual([]);
    expect(s.diskPie.value).toEqual([]);
  });

  it('split memory into used and the remainder of the limit', async () => {
    const s = useContainerStats(t);
    s.stats.value = { ...SAMPLE };

    expect(s.memoryPie.value.map((x) => [x.key, x.value])).toEqual([
      ['used', 300],
      ['free', 700],
    ]);
  });

  /**
   * A container over its own limit is not a negative slice. Docker reports
   * usage above `memoryLimit` for a container without one set, and a negative
   * value draws a pie that wraps the wrong way.
   */
  it('never produce a negative remainder', async () => {
    const s = useContainerStats(t);
    s.stats.value = { ...SAMPLE, memoryUsed: 1500, memoryLimit: 1000 };

    expect(s.memoryPie.value.find((x) => x.key === 'free').value).toBe(0);
  });

  /**
   * `|| 1` on every slice. A pie of two zeroes has no geometry and Vuetify
   * draws nothing at all — an idle container would show a blank card rather
   * than an empty ring, which reads as broken.
   */
  it('keep their shape when every counter is still zero', async () => {
    const s = useContainerStats(t);
    s.stats.value = { ...SAMPLE, netRx: 0, netTx: 0, blockRead: 0, blockWrite: 0 };

    expect(s.networkPie.value.map((x) => x.value)).toEqual([1, 1]);
    expect(s.diskPie.value.map((x) => x.value)).toEqual([1, 1]);
  });
});

describe('the heat grid', () => {
  /** An hour nobody measured must not look like an hour that measured zero. */
  it.each([
    [null, 'empty'],
    [0, 'l0'],
    [0.9, 'l0'],
    [1, 'l1'],
    [9.9, 'l1'],
    [10, 'l2'],
    [30, 'l3'],
    [60, 'l4'],
    [100, 'l4'],
  ])('reads %s as %s', (value, level) => {
    expect(heatLevel(value)).toBe(level);
  });

  it('gives every day twenty-four cells, unmeasured ones left null', () => {
    const s = useContainerStats(t);
    const noon = new Date('2026-08-06T12:00:00');
    s.history.value = [{ t: noon.getTime() / 1000, cpu: 40 }];

    const [day] = s.heatmap.value;
    expect(day.hours).toHaveLength(24);
    expect(day.hours[noon.getHours()]).toBe(40);
    expect(day.hours.filter((h) => h === null)).toHaveLength(23);
  });

  /** Several samples land in the same hour; the peak is what the cell means. */
  it('keeps the peak when an hour has more than one sample', () => {
    const s = useContainerStats(t);
    const hour = new Date('2026-08-06T09:00:00').getTime() / 1000;
    s.history.value = [
      { t: hour, cpu: 10 },
      { t: hour + 60, cpu: 55 },
      { t: hour + 120, cpu: 30 },
    ];

    expect(s.heatmap.value[0].hours[new Date(hour * 1000).getHours()]).toBe(55);
  });

  it('shows at most the last seven days', () => {
    const s = useContainerStats(t);
    const day = 86_400;
    const base = new Date('2026-07-01T10:00:00').getTime() / 1000;
    s.history.value = Array.from({ length: 12 }, (_, i) => ({ t: base + i * day, cpu: i }));

    expect(s.heatmap.value).toHaveLength(7);
    // The newest seven, not the first seven.
    expect(s.heatmap.value.at(-1).hours.filter(Boolean)[0]).toBe(11);
  });
});

describe('polling', () => {
  it('takes a sample immediately and then on the interval', async () => {
    const s = useContainerStats(t);
    s.start('stackvo-shop', true);

    await vi.advanceTimersByTimeAsync(0);
    expect(calls.filter(([n]) => n === 'containerStats')).toHaveLength(1);
    expect(s.stats.value.cpuPercent).toBe(12);

    await vi.advanceTimersByTimeAsync(STATS_INTERVAL * 2);
    expect(calls.filter(([n]) => n === 'containerStats').length).toBeGreaterThanOrEqual(3);

    s.stop();
  });

  /**
   * A stopped container's chart is cleared, not frozen. A frozen one claims the
   * container is still doing something.
   */
  it('clears rather than freezes when the container is not running', async () => {
    const s = useContainerStats(t);
    s.stats.value = { ...SAMPLE };

    s.start('stackvo-shop', false);
    expect(s.stats.value).toBe(null);
    expect(calls.some(([n]) => n === 'containerStats')).toBe(false);
  });

  it('stops polling when asked, and does not leave a timer behind', async () => {
    const s = useContainerStats(t);
    s.start('stackvo-shop', true);
    await vi.advanceTimersByTimeAsync(0);

    s.stop();
    const seen = calls.filter(([n]) => n === 'containerStats').length;
    await vi.advanceTimersByTimeAsync(STATS_INTERVAL * 5);

    expect(calls.filter(([n]) => n === 'containerStats')).toHaveLength(seen);
  });

  /** Starting twice must not leave two timers running against one container. */
  it('replaces its own timer rather than adding one', async () => {
    const s = useContainerStats(t);
    s.start('stackvo-shop', true);
    s.start('stackvo-shop', true);
    await vi.advanceTimersByTimeAsync(0);
    calls.length = 0;

    await vi.advanceTimersByTimeAsync(STATS_INTERVAL);
    expect(
      calls.filter(([n]) => n === 'containerStats'),
      'two timers were polling the same container'
    ).toHaveLength(1);

    s.stop();
  });

  /**
   * A container that goes away mid-poll clears the reading. Keeping the last
   * one would show live-looking numbers for something that no longer exists.
   */
  it('clears the reading when a sample fails', async () => {
    const s = useContainerStats(t);
    s.start('stackvo-shop', true);
    await vi.advanceTimersByTimeAsync(0);
    expect(s.stats.value).toBeTruthy();

    replies.containerStats = () => Promise.reject(new Error('no such container'));
    await vi.advanceTimersByTimeAsync(STATS_INTERVAL);

    expect(s.stats.value).toBe(null);
    s.stop();
  });

  it('keeps only the last sixty live samples', async () => {
    const s = useContainerStats(t);
    s.start('stackvo-shop', true);

    await vi.advanceTimersByTimeAsync(STATS_INTERVAL * 80);
    expect(s.cpuSeries.value.length).toBeLessThanOrEqual(60);

    s.stop();
  });
});

describe('history', () => {
  it('seeds the sparkline from what was recorded', async () => {
    replies.containerStatsHistory = [
      { t: 1, cpu: 5 },
      { t: 2, cpu: 9 },
    ];

    const s = useContainerStats(t);
    await s.loadHistory('stackvo-shop');

    expect(s.cpuSeries.value).toEqual([5, 9]);
    expect(calls.find(([n]) => n === 'containerStatsHistory')).toEqual([
      'containerStatsHistory',
      'stackvo-shop',
    ]);
  });

  /** The boundary is untyped; a non-list must not make the grid throw. */
  it('reads a misbehaving reply as no history at all', async () => {
    replies.containerStatsHistory = null;

    const s = useContainerStats(t);
    await s.loadHistory('stackvo-shop');

    expect(s.history.value).toEqual([]);
    expect(s.heatmap.value).toEqual([]);
  });
});
