<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useTheme } from 'vuetify';
import { useAppStore } from '@/stores/app';
import { useMetricsStore } from '@/stores/metrics';
import { useInventoryStore } from '@/stores/inventory';
import PageLayout from '@/components/PageLayout.vue';
import HostsDialog from '@/components/HostsDialog.vue';
import { api } from '@/lib/ipc';
import { bytes, bytesPerSecond, percent } from '@/lib/format';

const { t } = useI18n();
const theme = useTheme();
const app = useAppStore();
const metrics = useMetricsStore();
const inventory = useInventoryStore();

const stats = computed(() => metrics.stats);
const loading = computed(() => !metrics.stats);

const colors = computed(() => theme.current.value.colors);

// ---- headline counters -----------------------------------------------------

const totalRunning = computed(
  () => inventory.runningProjects.length + inventory.runningServices.length
);
const totalContainers = computed(
  () =>
    inventory.projects.filter((p) => p.built).length +
    inventory.services.filter((s) => s.built).length
);
const totalStopped = computed(() => Math.max(0, totalContainers.value - totalRunning.value));

// ---- gauges ----------------------------------------------------------------

/**
 * CPU time split. Absent until two samples exist — the platform counters are
 * cumulative, so the first reading would describe the app's own start-up rather
 * than the interval anyone asked about.
 */
const cpuPieItems = computed(() => {
  const b = stats.value?.cpu.breakdown;
  if (!b) return [];
  return [
    {
      key: 'system',
      title: t('dashboard.system'),
      value: b.system,
      color: colors.value.error,
      display: percent(b.system),
    },
    {
      key: 'user',
      title: t('dashboard.user'),
      value: b.user,
      color: colors.value.info,
      display: percent(b.user),
    },
    {
      key: 'nice',
      title: t('dashboard.nice'),
      value: b.nice,
      color: colors.value.warning,
      display: percent(b.nice),
    },
    {
      key: 'idle',
      title: t('dashboard.idle'),
      value: b.idle,
      color: colors.value['surface-variant'],
      display: percent(b.idle),
    },
  ];
});

function gaugeColor(value) {
  if (value >= 90) return 'error';
  if (value >= 70) return 'warning';
  return 'info';
}

const memoryPie = computed(() => {
  const m = stats.value?.memory;
  if (!m) return [];
  return [
    { key: 'used', title: t('dashboard.used'), value: m.used, color: colors.value.info },
    {
      key: 'free',
      title: t('dashboard.available'),
      value: m.available,
      color: colors.value['surface-variant'],
    },
  ];
});

const storagePie = computed(() => {
  const s = stats.value?.storage;
  if (!s) return [];
  return [
    { key: 'used', title: t('dashboard.used'), value: s.used, color: colors.value.info },
    {
      key: 'free',
      title: t('dashboard.available'),
      value: s.available,
      color: colors.value['surface-variant'],
    },
  ];
});

// ---- history ---------------------------------------------------------------

const cpuHistory = computed(() => (metrics.cpuHistory.length ? metrics.cpuHistory : [0]));

const cpuHistoryStats = computed(() => {
  const h = metrics.cpuHistory;
  if (!h.length) return { min: 0, avg: 0, max: 0 };
  return {
    min: Math.min(...h),
    max: Math.max(...h),
    avg: h.reduce((a, b) => a + b, 0) / h.length,
  };
});

const readHistory = computed(() => (metrics.diskRead.length ? metrics.diskRead : [0]));
const writeHistory = computed(() => (metrics.diskWrite.length ? metrics.diskWrite : [0]));
const rxHistory = computed(() => (metrics.netRx.length ? metrics.netRx : [0]));
const txHistory = computed(() => (metrics.netTx.length ? metrics.netTx : [0]));

// ---- problems worth surfacing unasked --------------------------------------

const missingDomains = ref([]);
const showHostsFix = ref(false);

async function refreshMissing() {
  try {
    missingDomains.value = await api.hostsMissing();
  } catch {
    missingDomains.value = [];
  }
}

async function refreshAll() {
  await Promise.all([metrics.refresh(), metrics.refreshResources(), inventory.loadAll()]);
  refreshMissing();
}

onMounted(() => {
  if (app.hasWorkspace) {
    inventory.loadAll();
    refreshMissing();
  }
});
</script>

<template>
  <PageLayout
    top-icon="mdi-view-dashboard"
    :top-title="t('dashboard.title')"
    :top-subtitle="t('dashboard.subtitle')"
    :bar-title="t('dashboard.overview')"
  >
    <template #bar-append>
      <v-btn 
      icon 
      variant="tonal"
      elevation="0"
      size="small"
      class="mr-2"
      :aria-label="t('app.refresh')" 
      :loading="metrics.loading" 
      @click="refreshAll"
      >
        <v-icon>mdi-refresh</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
      </v-btn>
    </template>

    <div class="dash-body">
      <!-- Counters -->
      <v-row class="dash-row">
        <v-col cols="12" md="3">
          <v-card elevation="1" hover>
            <v-card-text style="min-height: 100px">
              <div class="d-flex align-center">
                <v-icon color="info" size="48" class="mr-4">mdi-heart-pulse</v-icon>
                <div class="flex-grow-1">
                  <div class="text-h4">{{ totalContainers }}</div>
                  <div class="text-subtitle-1 text-grey">{{ t('dashboard.health') }}</div>
                  <div class="text-caption text-grey">
                    <v-icon size="12" color="success">mdi-circle</v-icon>
                    {{ totalRunning }} {{ t('dashboard.running') }}
                    <v-icon size="12" color="error" class="ml-2">mdi-circle</v-icon>
                    {{ totalStopped }} {{ t('dashboard.stopped') }}
                  </div>
                </div>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <v-col cols="12" md="3">
          <v-card
            elevation="1"
            hover
            class="cursor-pointer"
            @click="$router.push('/projects')"
          >
            <v-card-text style="min-height: 100px">
              <div class="d-flex align-center">
                <v-icon color="info" size="48" class="mr-4">mdi-folder-multiple</v-icon>
                <div class="flex-grow-1">
                  <div class="text-h4">{{ inventory.projects.length }}</div>
                  <div class="text-subtitle-1 text-grey">{{ t('dashboard.projects') }}</div>
                  <div class="text-caption text-grey">
                    <v-icon size="12" color="success">mdi-circle</v-icon>
                    {{ inventory.runningProjects.length }} {{ t('dashboard.active') }}
                    <v-icon size="12" color="grey" class="ml-2">mdi-circle</v-icon>
                    {{ inventory.projects.length - inventory.runningProjects.length }}
                    {{ t('dashboard.inactive') }}
                  </div>
                </div>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <v-col cols="12" md="3">
          <v-card
            elevation="1"
            hover
            class="cursor-pointer"
            @click="$router.push('/services')"
          >
            <v-card-text style="min-height: 100px">
              <div class="d-flex align-center">
                <v-icon color="info" size="48" class="mr-4">mdi-server</v-icon>
                <div class="flex-grow-1">
                  <div class="text-h4">{{ inventory.services.length }}</div>
                  <div class="text-subtitle-1 text-grey">{{ t('dashboard.services') }}</div>
                  <div class="text-caption text-grey">
                    <v-icon size="12" color="success">mdi-circle</v-icon>
                    {{ inventory.runningServices.length }} {{ t('dashboard.active') }}
                    <v-icon size="12" color="grey" class="ml-2">mdi-circle</v-icon>
                    {{ inventory.services.length - inventory.runningServices.length }}
                    {{ t('dashboard.inactive') }}
                  </div>
                </div>
              </div>
            </v-card-text>
          </v-card>
        </v-col>

        <v-col cols="12" md="3">
          <v-card elevation="1" hover>
            <v-card-text style="min-height: 100px">
              <div class="d-flex align-center">
                <v-icon color="info" size="48" class="mr-4">mdi-layers</v-icon>
                <div class="flex-grow-1">
                  <div class="text-h4">{{ metrics.resources?.images.total ?? '—' }}</div>
                  <div class="text-subtitle-1 text-grey">{{ t('dashboard.images') }}</div>
                  <div v-if="metrics.resources" class="text-caption text-grey">
                    <v-icon size="12" color="success">mdi-circle</v-icon>
                    {{ metrics.resources.images.inUse }} {{ t('stats.inUse') }}
                    <v-icon size="12" color="grey" class="ml-2">mdi-circle</v-icon>
                    {{ metrics.resources.images.unused }} {{ t('stats.unused') }}
                  </div>
                </div>
              </div>
            </v-card-text>
          </v-card>
        </v-col>
      </v-row>

      <!-- Gauges. These read the host directly; the web UI read /proc from
           inside a container, which on macOS meant no /proc at all. -->
      <v-row class="dash-row">
        <v-col cols="12" md="6" lg="3">
          <v-card elevation="1" class="pa-4 d-flex flex-column metric-card">
            <div class="text-subtitle-2 text-grey mb-2">{{ t('dashboard.cpuLoad') }}</div>

            <div v-if="loading" class="flex-grow-1 d-flex align-center justify-center">
              <v-progress-circular indeterminate size="28" />
            </div>

            <div
              v-else-if="!cpuPieItems.length"
              class="flex-grow-1 d-flex align-center justify-center text-caption text-medium-emphasis text-center px-4"
            >
              {{ t('dashboard.breakdownPending') }}
            </div>

            <div v-else class="flex-grow-1 d-flex align-center justify-center">
              <v-pie
                :items="cpuPieItems"
                :size="130"
                inner-cut="70"
                gap="2"
                rounded="2"
                :legend="{ position: 'right' }"
                animation
              >
                <template #center>
                  <div class="text-center">
                    <div class="text-h5 font-weight-bold">{{ percent(stats.cpu.percent) }}</div>
                    <div class="text-caption text-grey">{{ t('dashboard.cpu') }}</div>
                  </div>
                </template>
                <template #legend="{ items, toggle, isActive }">
                  <v-list class="py-0 bg-transparent metric-legend">
                    <v-list-item
                      v-for="item in items"
                      :key="item.key"
                      :class="['my-0', { 'opacity-40': !isActive(item) }]"
                      :title="item.title"
                      rounded="lg"
                      link
                      @click="toggle(item)"
                    >
                      <template #prepend>
                        <v-avatar :color="item.color" :size="10" class="mr-2" />
                      </template>
                      <template #append>
                        <span class="text-caption font-weight-bold ml-3">{{
                          item.raw.display
                        }}</span>
                      </template>
                    </v-list-item>
                  </v-list>
                </template>
              </v-pie>
            </div>
          </v-card>
        </v-col>

        <v-col cols="12" md="6" lg="3">
          <v-card elevation="1" class="pa-4 d-flex flex-column metric-card">
            <div class="d-flex align-center mb-2">
              <span class="text-subtitle-2 text-grey">{{ t('dashboard.cpuHistory') }}</span>
              <v-spacer />
              <span class="text-h6">{{ percent(stats?.cpu.percent) }}</span>
            </div>

            <div class="flex-grow-1 d-flex flex-column justify-center">
              <v-sparkline
                :model-value="cpuHistory"
                :gradient="['#4CAF50', '#4CAF50']"
                line-width="2"
                smooth="8"
                auto-draw
                height="70"
              />
              <div class="d-flex justify-space-around mt-4">
                <div v-for="k in ['min', 'avg', 'max']" :key="k" class="text-center">
                  <div class="text-caption text-grey">{{ t(`dashboard.${k}`) }}</div>
                  <div class="text-body-2 font-weight-bold">{{ percent(cpuHistoryStats[k]) }}</div>
                </div>
              </div>
            </div>
          </v-card>
        </v-col>

        <v-col cols="12" md="6" lg="3">
          <v-card elevation="1" class="pa-4 d-flex flex-column metric-card">
            <div class="text-subtitle-2 text-grey mb-2">{{ t('stats.memory') }}</div>
            <div v-if="loading" class="flex-grow-1 d-flex align-center justify-center">
              <v-progress-circular indeterminate size="28" />
            </div>
            <div v-else class="flex-grow-1 d-flex align-center justify-center">
              <v-pie
                :items="memoryPie"
                :size="130"
                inner-cut="70"
                gap="2"
                rounded="2"
                :legend="false"
                animation
              >
                <template #center>
                  <div class="text-center">
                    <div
                      class="text-h5 font-weight-bold"
                      :class="`text-${gaugeColor(stats.memory.percent)}`"
                    >
                      {{ percent(stats.memory.percent) }}
                    </div>
                    <div class="text-caption text-grey">{{ bytes(stats.memory.total) }}</div>
                  </div>
                </template>
              </v-pie>

              <v-list class="py-0 bg-transparent metric-legend ml-2">
                <v-list-item :title="t('dashboard.used')">
                  <template #prepend
                    ><v-avatar :color="colors.info" :size="10" class="mr-2"
                  /></template>
                  <template #append>
                    <span class="text-caption font-weight-bold ml-3">{{
                      bytes(stats.memory.used)
                    }}</span>
                  </template>
                </v-list-item>
                <v-list-item :title="t('dashboard.available')">
                  <template #prepend>
                    <v-avatar :color="colors['surface-variant']" :size="10" class="mr-2" />
                  </template>
                  <template #append>
                    <span class="text-caption font-weight-bold ml-3">{{
                      bytes(stats.memory.available)
                    }}</span>
                  </template>
                </v-list-item>
              </v-list>
            </div>
          </v-card>
        </v-col>

        <v-col cols="12" md="6" lg="3">
          <v-card elevation="1" class="pa-4 d-flex flex-column metric-card">
            <div class="text-subtitle-2 text-grey mb-2">{{ t('stats.storage') }}</div>
            <div v-if="loading" class="flex-grow-1 d-flex align-center justify-center">
              <v-progress-circular indeterminate size="28" />
            </div>
            <div v-else class="flex-grow-1 d-flex align-center justify-center">
              <v-pie
                :items="storagePie"
                :size="130"
                inner-cut="70"
                gap="2"
                rounded="2"
                :legend="false"
                animation
              >
                <template #center>
                  <div class="text-center">
                    <div
                      class="text-h5 font-weight-bold"
                      :class="`text-${gaugeColor(stats.storage.percent)}`"
                    >
                      {{ percent(stats.storage.percent, 0) }}
                    </div>
                    <div class="text-caption text-grey">{{ bytes(stats.storage.total) }}</div>
                  </div>
                </template>
              </v-pie>

              <v-list class="py-0 bg-transparent metric-legend ml-2">
                <v-list-item :title="t('dashboard.used')">
                  <template #prepend
                    ><v-avatar :color="colors.info" :size="10" class="mr-2"
                  /></template>
                  <template #append>
                    <span class="text-caption font-weight-bold ml-3">{{
                      bytes(stats.storage.used)
                    }}</span>
                  </template>
                </v-list-item>
                <v-list-item :title="t('dashboard.available')">
                  <template #prepend>
                    <v-avatar :color="colors['surface-variant']" :size="10" class="mr-2" />
                  </template>
                  <template #append>
                    <span class="text-caption font-weight-bold ml-3">{{
                      bytes(stats.storage.available)
                    }}</span>
                  </template>
                </v-list-item>
              </v-list>
            </div>
          </v-card>
        </v-col>
      </v-row>

      <!-- Throughput -->
      <v-row class="dash-row">
        <v-col cols="12" lg="6">
          <v-card elevation="1" class="pa-4">
            <div class="d-flex align-start">
              <div class="flex-grow-1">
                <div class="text-subtitle-1 font-weight-medium">{{ t('dashboard.diskIo') }}</div>
                <div class="text-caption text-grey">{{ t('dashboard.diskIoSub') }}</div>
              </div>
              <div class="d-flex ga-6">
                <div class="text-center">
                  <v-icon size="18" color="info">mdi-download</v-icon>
                  <div class="text-caption text-grey">{{ t('dashboard.read') }}</div>
                  <div class="text-h6">{{ bytesPerSecond(stats?.disk.readRate) }}</div>
                </div>
                <div class="text-center">
                  <v-icon size="18" color="error">mdi-upload</v-icon>
                  <div class="text-caption text-grey">{{ t('dashboard.write') }}</div>
                  <div class="text-h6">{{ bytesPerSecond(stats?.disk.writeRate) }}</div>
                </div>
              </div>
            </div>

            <v-row class="mt-2">
              <v-col cols="6">
                <div class="text-caption text-grey mb-1">
                  <v-icon size="14" color="info">mdi-arrow-down</v-icon>
                  {{ t('dashboard.readHistory') }}
                </div>
                <v-sparkline
                  :model-value="readHistory"
                  :gradient="['#2196F3', '#2196F3']"
                  line-width="2"
                  smooth="8"
                  fill
                  height="40"
                />
              </v-col>
              <v-col cols="6">
                <div class="text-caption text-grey mb-1">
                  <v-icon size="14" color="error">mdi-arrow-up</v-icon>
                  {{ t('dashboard.writeHistory') }}
                </div>
                <v-sparkline
                  :model-value="writeHistory"
                  :gradient="['#FF5252', '#FF5252']"
                  line-width="2"
                  smooth="8"
                  fill
                  height="40"
                />
              </v-col>
            </v-row>
          </v-card>
        </v-col>

        <v-col cols="12" lg="6">
          <v-card elevation="1" class="pa-4">
            <div class="d-flex align-start">
              <div class="flex-grow-1">
                <div class="text-subtitle-1 font-weight-medium">{{ t('dashboard.network') }}</div>
                <div class="text-caption text-grey">{{ t('dashboard.networkSub') }}</div>
              </div>
              <div class="d-flex ga-6">
                <div class="text-center">
                  <v-icon size="18" color="success">mdi-arrow-down</v-icon>
                  <div class="text-caption text-grey">{{ t('stats.download') }}</div>
                  <div class="text-h6">{{ bytesPerSecond(stats?.network.rxRate) }}</div>
                </div>
                <div class="text-center">
                  <v-icon size="18" color="warning">mdi-arrow-up</v-icon>
                  <div class="text-caption text-grey">{{ t('stats.upload') }}</div>
                  <div class="text-h6">{{ bytesPerSecond(stats?.network.txRate) }}</div>
                </div>
              </div>
            </div>

            <v-row class="mt-2">
              <v-col cols="6">
                <div class="text-caption text-grey mb-1">
                  <v-icon size="14" color="success">mdi-arrow-down</v-icon>
                  {{ t('dashboard.downloadHistory') }}
                </div>
                <v-sparkline
                  :model-value="rxHistory"
                  :gradient="['#4CAF50', '#4CAF50']"
                  line-width="2"
                  smooth="8"
                  fill
                  height="40"
                />
              </v-col>
              <v-col cols="6">
                <div class="text-caption text-grey mb-1">
                  <v-icon size="14" color="warning">mdi-arrow-up</v-icon>
                  {{ t('dashboard.uploadHistory') }}
                </div>
                <v-sparkline
                  :model-value="txHistory"
                  :gradient="['#FB8C00', '#FB8C00']"
                  line-width="2"
                  smooth="8"
                  fill
                  height="40"
                />
              </v-col>
            </v-row>
          </v-card>
        </v-col>
      </v-row>

      <!-- Problems the web UI could detect but never showed. -->
      <v-row
        v-if="
          inventory.invalidProjects.length ||
          missingDomains.length ||
          inventory.brokenDependencies.length
        "
        class="dash-row"
      >
        <v-col cols="12">
          <v-card variant="tonal" color="warning">
            <v-card-text class="d-flex flex-column ga-2">
              <div v-if="inventory.invalidProjects.length" class="d-flex align-center ga-2">
                <v-icon size="18">mdi-file-alert-outline</v-icon>
                <span class="text-body-2">
                  {{ inventory.invalidProjects.length }} × {{ t('projects.invalidManifest') }}
                </span>
                <v-spacer />
                <v-btn size="x-small" variant="text" to="/projects">{{ t('app.projects') }}</v-btn>
              </div>

              <div v-if="missingDomains.length" class="d-flex align-center ga-2">
                <v-icon size="18">mdi-web-off</v-icon>
                <span class="text-body-2">
                  {{ missingDomains.length }} × {{ t('projects.domainMissing') }}
                </span>
                <v-spacer />
                <v-btn size="x-small" variant="text" @click="showHostsFix = true">{{
                  t('hosts.fix')
                }}</v-btn>
              </div>

              <div v-if="inventory.brokenDependencies.length" class="d-flex align-center ga-2">
                <v-icon size="18">mdi-link-variant-off</v-icon>
                <span class="text-body-2">
                  {{ inventory.brokenDependencies.length }} × {{ t('services.unmetDependency') }}
                </span>
                <v-spacer />
                <v-btn size="x-small" variant="text" to="/services">{{ t('app.services') }}</v-btn>
              </div>
            </v-card-text>
          </v-card>
        </v-col>
      </v-row>
    </div>

    <HostsDialog
      v-if="showHostsFix"
      v-model="showHostsFix"
      :add="missingDomains"
      @applied="
        () => {
          refreshMissing();
          inventory.loadProjects();
        }
      "
    />
  </PageLayout>
</template>

<style scoped>
.dash-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 16px;
}

.dash-row {
  margin-bottom: 4px;
}

/* Keep the four gauges the same height whatever their legend contains. */
.metric-card {
  min-height: 232px;
}

.metric-legend :deep(.v-list-item) {
  min-height: 26px;
  padding-inline: 6px;
}

.cursor-pointer {
  cursor: pointer;
}
</style>
