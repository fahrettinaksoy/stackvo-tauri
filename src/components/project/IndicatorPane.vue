<script setup>
import { useI18n } from 'vue-i18n';
import { bytes, percent } from '@/lib/format';
import { heatLevel } from '@/composables/useContainerStats';

/**
 * One project's live resource use.
 *
 * First pane out of `ProjectDetail.vue` under §14.16 — the view `Settings.vue`
 * used to sit beside at 0% coverage, and the last one still there.
 *
 * It takes its numbers as props rather than fetching them: the polling timer
 * belongs to the view, which starts and stops it with the container, and a pane
 * that polled on its own mount would keep a stopped container's chart moving.
 */
defineProps({
  stats: { type: Object, default: null },
  running: { type: Boolean, default: false },
  cpuSeries: { type: Array, default: () => [] },
  memoryPie: { type: Array, default: () => [] },
  networkPie: { type: Array, default: () => [] },
  diskPie: { type: Array, default: () => [] },
  heatmap: { type: Array, default: () => [] },
});

const { t } = useI18n();
</script>

<template>
  <v-card variant="flat" class="pane">
    <v-alert
      type="success"
      variant="tonal"
      class="mb-4"
      :icon="running ? 'mdi-pulse' : 'mdi-pause'"
    >
      {{ running ? t('projectDetail.live') : t('projects.stopped') }}
    </v-alert>

    <v-row>
      <v-col cols="12" sm="6" lg="3">
        <v-card rounded="lg" class="pa-4 metric-tile">
          <div class="d-flex align-center mb-2">
            <v-icon size="18" color="info" class="mr-2">mdi-cpu-64-bit</v-icon>
            <span class="tile-label">{{ t('stats.cpu') }}</span>
            <v-spacer />
            <span class="text-h6 text-success">{{ percent(stats?.cpuPercent, 0) }}</span>
          </div>
          <v-sparkline
            :model-value="cpuSeries.length > 1 ? cpuSeries : [0, 0]"
            :gradient="['#1976D2', '#4CAF50']"
            line-width="3"
            smooth="8"
            height="46"
          />
          <div class="tile-foot">{{ stats?.onlineCpus ?? '—' }} {{ t('stats.cores') }}</div>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" lg="3">
        <v-card rounded="lg" class="pa-4 metric-tile">
          <div class="d-flex align-center mb-2">
            <v-icon size="18" color="info" class="mr-2">mdi-memory</v-icon>
            <span class="tile-label">{{ t('stats.memory') }}</span>
            <v-spacer />
            <span class="text-h6 text-success">{{ percent(stats?.memoryPercent) }}</span>
          </div>
          <!-- Named: Vuetify emits `role="progressbar"` with a value and no
               name, so a screen reader read a bare number off a page with four
               of these on it. -->
          <v-progress-linear
            :aria-label="t('stats.memory')"
            :model-value="stats?.memoryPercent ?? 0"
            color="success"
            height="6"
            rounded
            class="my-4"
          />
          <div class="tile-foot">
            {{ bytes(stats?.memoryUsed) }} / {{ bytes(stats?.memoryLimit) }}
          </div>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" lg="3">
        <v-card rounded="lg" class="pa-4 metric-tile">
          <div class="d-flex align-center mb-2">
            <v-icon size="18" color="info" class="mr-2">mdi-harddisk</v-icon>
            <span class="tile-label">{{ t('projectDetail.disk') }}</span>
            <v-spacer />
            <span class="text-h6">{{ bytes(stats?.blockRead) }}</span>
          </div>
          <!-- No bar here, and that is the fix rather than the omission.
               This was `model-value="12"` — a hardcoded constant drawn beside
               the real read/write figures, so it read as a measurement and was
               a decoration. Block I/O is a pair of counters, not a ratio; there
               is no percentage to draw. -->
          <div class="my-4" />
          <div class="tile-foot">
            R {{ bytes(stats?.blockRead) }} / W {{ bytes(stats?.blockWrite) }}
          </div>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" lg="3">
        <v-card rounded="lg" class="pa-4 metric-tile">
          <div class="d-flex align-center mb-2">
            <v-icon size="18" color="info" class="mr-2">mdi-lan</v-icon>
            <span class="tile-label">{{ t('stats.network') }}</span>
            <v-spacer />
            <span class="text-body-2">
              <span class="text-success">↓{{ bytes(stats?.netRx) }}</span>
              <span class="text-warning ml-1">↑{{ bytes(stats?.netTx) }}</span>
            </span>
          </div>
          <v-divider color="success" thickness="2" class="my-4" />
          <div class="tile-foot">
            {{ stats?.pids ?? '—' }} pids · ↓{{ bytes(stats?.netRx) }} ↑{{ bytes(stats?.netTx) }}
          </div>
        </v-card>
      </v-col>
    </v-row>

    <v-card rounded="lg" class="pa-4 mt-4">
      <div class="section-head">
        <v-icon size="18" class="mr-2">mdi-chart-donut</v-icon>{{ t('projectDetail.composition') }}
      </div>

      <v-row v-if="stats" class="mt-2">
        <v-col
          v-for="c in [
            {
              key: 'mem',
              title: t('stats.memory'),
              items: memoryPie,
              foot: `${percent(stats.memoryPercent, 0)} ${t('projectDetail.usedShort')}`,
            },
            {
              key: 'net',
              title: t('stats.network'),
              items: networkPie,
              foot: `↓${bytes(stats.netRx)} / ↑${bytes(stats.netTx)}`,
            },
            {
              key: 'disk',
              title: t('dashboard.diskIo'),
              items: diskPie,
              foot: `R${bytes(stats.blockRead)} / W${bytes(stats.blockWrite)}`,
            },
          ]"
          :key="c.key"
          cols="12"
          md="4"
          class="text-center"
        >
          <div class="text-body-2 mb-2">{{ c.title }}</div>
          <v-pie
            :items="c.items"
            :size="150"
            inner-cut="55"
            gap="1"
            :legend="false"
            animation
            class="justify-center"
          />
          <div class="tile-foot mt-2">{{ c.foot }}</div>
        </v-col>
      </v-row>

      <div v-else class="text-caption text-medium-emphasis py-8 text-center">
        {{ t('projects.stopped') }}
      </div>
    </v-card>

    <v-card rounded="lg" class="pa-4 mt-4">
      <div class="section-head">
        <v-icon size="18" class="mr-2">mdi-calendar</v-icon>{{ t('projectDetail.cpuActivity') }}
      </div>

      <div v-if="!heatmap.length" class="text-caption text-medium-emphasis py-6 text-center">
        {{ t('projectDetail.noHistory') }}
      </div>

      <div v-else class="heatmap mt-3">
        <div class="heat-hours">
          <span v-for="h in [0, 6, 12, 18]" :key="h" :style="{ left: `${(h / 24) * 100}%` }">{{
            h
          }}</span>
        </div>
        <div v-for="day in heatmap" :key="day.label.toDateString()" class="heat-row">
          <span class="heat-day">
            {{ day.label.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' }) }}
          </span>
          <div class="heat-cells">
            <v-tooltip v-for="(value, hour) in day.hours" :key="hour" location="top">
              <template #activator="{ props: tip }">
                <span v-bind="tip" class="heat-cell" :class="heatLevel(value)" />
              </template>
              <span class="text-caption">
                {{ hour }}:00 —
                {{ value === null ? t('projectDetail.noSample') : percent(value) }}
              </span>
            </v-tooltip>
          </div>
        </div>
        <div class="heat-legend">
          <span class="text-caption">{{ t('projectDetail.less') }}</span>
          <span v-for="l in ['l0', 'l1', 'l2', 'l3', 'l4']" :key="l" class="heat-cell" :class="l" />
          <span class="text-caption">{{ t('projectDetail.more') }}</span>
        </div>
      </div>
    </v-card>
  </v-card>
</template>
