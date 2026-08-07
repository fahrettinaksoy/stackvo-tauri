import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createVuetify } from 'vuetify';
import * as components from 'vuetify/components';
import * as directives from 'vuetify/directives';

/**
 * The Indicator pane, mounted.
 *
 * First pane out of `ProjectDetail.vue` under §14.16 — 3,007 lines at 0%
 * coverage, and the last view still there.
 *
 * It takes every number as a prop, which is the design worth pinning: the
 * polling timer belongs to the view, so it can start and stop with the
 * container. A pane that polled on its own mount would keep a stopped
 * container's chart moving.
 */

globalThis.visualViewport = undefined;

vi.mock('@/lib/ipc', () => ({
  StackvoError: class extends Error {},
  call: vi.fn(),
  asList: (value) => (Array.isArray(value) ? value : []),
  api: new Proxy({}, { get: () => () => Promise.resolve(null) }),
}));

const { i18n } = await import('@/i18n');
const IndicatorPane = (await import('@/components/project/IndicatorPane.vue')).default;

const vuetify = createVuetify({ components, directives });

const STATS = {
  cpuPercent: 12.5,
  memoryUsed: 300 * 1024 * 1024,
  memoryLimit: 1024 * 1024 * 1024,
  netRx: 5000,
  netTx: 2000,
  blockRead: 400,
  blockWrite: 600,
};

const PIE = [
  { key: 'a', title: 'A', value: 1, color: '#1976D2' },
  { key: 'b', title: 'B', value: 2, color: '#2A313C' },
];

function render(props = {}) {
  const host = document.createElement('div');
  document.body.appendChild(host);

  return mount(
    { components: { IndicatorPane }, template: '<v-app><IndicatorPane v-bind="$attrs" /></v-app>' },
    { props, attrs: props, attachTo: host, global: { plugins: [vuetify, i18n] } }
  );
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the indicator pane', () => {
  /**
   * A project that has never been started has no sample at all. The pane has to
   * render its empty state rather than throwing on `stats.cpuPercent`.
   */
  it('renders with no stats at all', () => {
    const wrapper = render({ running: false });
    expect(wrapper.text().trim().length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it('shows the live figures once there is a sample', () => {
    const wrapper = render({
      running: true,
      stats: STATS,
      cpuSeries: [10, 11, 12],
      memoryPie: PIE,
      networkPie: PIE,
      diskPie: PIE,
    });

    // The CPU reading is the headline number on the page. Rounded to whole
    // percent by `percent(…, 0)` — a tenth of a percent of CPU is noise.
    expect(wrapper.text()).toContain('13%');
    wrapper.unmount();
  });

  /**
   * The grid distinguishes an hour nobody measured from an hour that measured
   * zero, and the class is what carries that — see `heatLevel`.
   */
  it('draws an unmeasured hour differently from an idle one', () => {
    const hours = Array.from({ length: 24 }, () => null);
    hours[9] = 0;
    hours[10] = 80;

    const wrapper = render({
      running: true,
      stats: STATS,
      heatmap: [{ label: new Date('2026-08-06T00:00:00'), hours }],
    });

    const html = wrapper.html();
    expect(html).toContain('heat-cell');
    expect(html, 'an unmeasured hour is not marked').toContain('empty');
    expect(html, 'an idle hour was drawn as unmeasured').toContain('l0');
    expect(html, 'a busy hour was not drawn hot').toContain('l4');

    wrapper.unmount();
  });

  it('renders in Turkish', () => {
    i18n.global.locale.value = 'tr';
    try {
      const wrapper = render({ running: true, stats: STATS, memoryPie: PIE });
      expect(wrapper.text().trim().length).toBeGreaterThan(0);
      wrapper.unmount();
    } finally {
      i18n.global.locale.value = 'en';
    }
  });
});
