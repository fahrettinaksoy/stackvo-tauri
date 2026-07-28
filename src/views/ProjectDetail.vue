<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { bytes, percent } from '@/lib/format';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import LogView from '@/components/LogView.vue';
import HostsDialog from '@/components/HostsDialog.vue';

const props = defineProps({ name: { type: String, required: true } });

const { t } = useI18n();
const router = useRouter();
const ops = useOperationsStore();
const app = useAppStore();

const project = ref(null);
const details = ref(null);
const stats = ref(null);
const history = ref([]);
const cpuSeries = ref([]);
const error = ref(null);
const loading = ref(true);
const copied = ref(null);
const showHostsFix = ref(false);

const manifestText = ref('');
const manifestDirty = ref(false);
const manifestSaving = ref(false);
const preview = ref(null);

const SECTIONS = [
  { key: 'indicator', icon: 'mdi-chart-line', label: 'projectDetail.indicator' },
  { key: 'configuration', icon: 'mdi-folder-cog', label: 'projectDetail.configuration' },
  { key: 'container', icon: 'mdi-docker', label: 'projectDetail.container' },
  // A section rather than a dialog over the page: logs are something you read
  // while looking at the rest, and a modal on top of a detail page hides the
  // thing it is about.
  { key: 'logs', icon: 'mdi-text-box-outline', label: 'logs.title' },
  // Below the divider: the editing surfaces the screenshots do not show, kept
  // because retiring the dialog must not retire what it could do.
  { key: 'manifest', icon: 'mdi-code-json', label: 'detail.manifest', divide: true },
  { key: 'dockerfile', icon: 'mdi-file-document-outline', label: 'detail.dockerfile' },
];
const section = ref('indicator');

const STATS_INTERVAL = 2000;
let statsTimer = null;

const manifest = computed(() => project.value?.manifest);
const running = computed(() => !!project.value?.running);

/** Both URLs are shown; Traefik serves the project on HTTPS and redirects HTTP. */
const httpUrl = computed(() => (project.value?.domain ? `http://${project.value.domain}` : null));
const httpsUrl = computed(() => (project.value?.domain ? `https://${project.value.domain}` : null));

const memoryPie = computed(() => {
  if (!stats.value) return [];
  const free = Math.max(0, stats.value.memoryLimit - stats.value.memoryUsed);
  return [
    { key: 'used', title: t('dashboard.used'), value: stats.value.memoryUsed, color: '#1976D2' },
    { key: 'free', title: t('dashboard.available'), value: free, color: '#2A313C' },
  ];
});

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
 * cells have no reading at all. An empty cell is drawn differently from a cell
 * that measured zero — "we did not look" and "nothing happened" are not the
 * same thing, and colouring them alike would invent history.
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

  return [...byDay.values()].slice(-7);
});

function heatLevel(value) {
  if (value === null) return 'empty';
  if (value < 1) return 'l0';
  if (value < 10) return 'l1';
  if (value < 30) return 'l2';
  if (value < 60) return 'l3';
  return 'l4';
}

async function copy(value, key) {
  try {
    await navigator.clipboard.writeText(value);
    copied.value = key;
    setTimeout(() => (copied.value = null), 1200);
  } catch {
    /* clipboard unavailable */
  }
}

/** Strip the diagnostics the reader adds, so the editor shows the file itself. */
function stripDiagnostics(manifest) {
  const { valid, errors, warnings, ...rest } = manifest ?? {};
  return rest;
}

async function saveManifest() {
  error.value = null;
  manifestSaving.value = true;
  try {
    await api.projectManifestWrite(props.name, JSON.parse(manifestText.value));
    manifestDirty.value = false;
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    manifestSaving.value = false;
  }
}

/** Open the project folder in the user's editor. */
async function openInEditor() {
  error.value = null;
  try {
    await api.openInEditor(project.value.path);
  } catch (e) {
    error.value = e;
  }
}

/**
 * Hand the container off to the user's own terminal application. The HTTP
 * version of this returned 400 unconditionally in the shipped container
 * configuration — it could never succeed.
 */
async function openExternalTerminal() {
  error.value = null;
  try {
    await api.terminalOpenExternal({ kind: 'container', name: project.value.containerName });
  } catch (e) {
    error.value = e;
  }
}

/** Rebuild and start through compose — the right action after a manifest edit. */
async function bringUp() {
  await act(() => api.composeUpProject(props.name));
}

/**
 * Which rendering to show.
 *
 * `compat` reproduces what Bash actually writes today, `strict` refuses where
 * Bash silently drops an extension. Held as state rather than fired by two
 * unlabelled buttons: which of the two you are looking at changes what the
 * comparison chip below it means.
 */
const previewMode = ref('compat');
const previewLoading = ref(false);

/** Render the project with the Rust generator and compare against Bash. */
async function loadPreview(mode = previewMode.value) {
  previewMode.value = mode;
  preview.value = null;
  error.value = null;
  previewLoading.value = true;
  try {
    preview.value = await api.projectDockerfilePreview(props.name, mode === 'strict');
  } catch (e) {
    error.value = e;
  } finally {
    previewLoading.value = false;
  }
}

/** Numbered, because a Dockerfile is read by line as often as it is read. */
const dockerfileLines = computed(() => preview.value?.dockerfile?.split('\n') ?? []);

// Rendered as soon as the section is opened. The pane used to start empty with
// a note asking the user to pick one of two modes named "strict" and "compat" —
// a question about the generator port, put before anyone had seen the file.
watch(
  () => section.value,
  (value) => {
    if (value === 'dockerfile' && !preview.value) loadPreview();
  }
);

async function act(fn) {
  error.value = null;
  ops.markBusy(props.name, true);
  try {
    await fn(props.name);
    await load();
  } catch (e) {
    error.value = e;
    ops.markBusy(props.name, false);
  }
}

async function load() {
  error.value = null;
  try {
    project.value = await api.projectGet(props.name);
  } catch (e) {
    error.value = e;
    loading.value = false;
    return;
  }

  // Read the manifest fresh rather than trusting the list payload: the file
  // may have changed on disk since the inventory was loaded.
  try {
    const m = await api.projectManifestRead(props.name);
    manifestText.value = JSON.stringify(stripDiagnostics(m), null, 2);
    manifestDirty.value = false;
  } catch {
    manifestText.value = '';
  }

  const container = project.value.containerName;

  // A container that was never built has no inspect data; that is a state to
  // render, not an error to shout about.
  try {
    details.value = await api.containerInspect(container);
  } catch (e) {
    details.value = null;
    if (e.code && e.code !== 'NOT_FOUND') error.value = e;
  }

  try {
    history.value = await api.containerStatsHistory(container);
    cpuSeries.value = history.value.map((s) => s.cpu);
  } catch {
    history.value = [];
  }

  loading.value = false;
  startStats();
}

function startStats() {
  clearInterval(statsTimer);
  if (!running.value) {
    stats.value = null;
    return;
  }

  const tick = async () => {
    try {
      stats.value = await api.containerStats(project.value.containerName);
      cpuSeries.value = [...cpuSeries.value, stats.value.cpuPercent].slice(-60);
    } catch {
      stats.value = null;
    }
  };
  tick();
  statsTimer = setInterval(tick, STATS_INTERVAL);
}

watch(() => props.name, load);
onMounted(load);
onUnmounted(() => clearInterval(statsTimer));
</script>

<template>
  <PageLayout top-icon="mdi-information" :top-title="t('projectDetail.title')" hide-bar>
    <template #top-append>
      <!-- aria-label as well as the tooltip: a tooltip renders
           aria-describedby, which is a description, not a name. A screen reader
           announces an unlabelled icon button as just "button". -->
      <v-btn icon :aria-label="t('projectDetail.back')" @click="router.push('/projects')">
        <v-icon>mdi-arrow-left</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('projectDetail.back') }}</v-tooltip>
      </v-btn>
    </template>

    <v-toolbar v-if="project" class="detail-toolbar">
      <v-toolbar-title class="text-h6 font-weight-bold">{{ project.name }}</v-toolbar-title>

      <v-chip
        size="small"
        variant="tonal"
        :color="running ? 'success' : 'grey'"
        :prepend-icon="running ? 'mdi-check-circle' : 'mdi-stop-circle'"
        class="mr-2"
      >
        {{ running ? t('projects.running') : t('projects.stopped') }}
      </v-chip>

      <!-- Only when the domain resolves; otherwise the browser shows an error
           page and the user has no idea why. -->
      <v-btn
        v-if="httpsUrl && running && project.domainConfigured"
        icon
        variant="text"
        color="primary"
        :aria-label="t('projectsView.colOpen')"
        @click="openUrl(httpsUrl)"
      >
        <v-icon>mdi-open-in-new</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('projectsView.colOpen') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-if="running"
        icon
        variant="text"
        color="info"
        :aria-label="t('detail.externalTerminal')"
        @click="openExternalTerminal"
      >
        <v-icon>mdi-application-export</v-icon>
        <v-tooltip activator="parent" location="bottom">{{
          t('detail.externalTerminal')
        }}</v-tooltip>
      </v-btn>
      <v-btn icon variant="text" :aria-label="t('detail.openInEditor')" @click="openInEditor">
        <v-icon>mdi-folder-open</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('detail.openInEditor') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-if="running"
        icon
        variant="text"
        color="error"
        :aria-label="t('actions.stop')"
        :loading="ops.isBusy(name)"
        @click="act(api.projectStop)"
      >
        <v-icon>mdi-stop</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('actions.stop') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-else-if="project.built"
        icon
        variant="text"
        color="success"
        :aria-label="t('actions.start')"
        :disabled="!app.engineUp"
        :loading="ops.isBusy(name)"
        @click="act(api.projectStart)"
      >
        <v-icon>mdi-play</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('actions.start') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-else
        icon="mdi-hammer-wrench"
        variant="text"
        color="info"
        :aria-label="t('actions.build')"
        :disabled="!app.engineUp || !project.manifestValid"
        :loading="ops.isBusy(name)"
        @click="act((n) => api.projectBuild(n))"
      />
      <v-btn
        v-if="running"
        icon
        variant="text"
        color="warning"
        :aria-label="t('actions.restart')"
        :loading="ops.isBusy(name)"
        @click="act(api.projectRestart)"
      >
        <v-icon>mdi-restart</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('actions.restart') }}</v-tooltip>
      </v-btn>
      <v-btn
        icon="mdi-delete"
        variant="text"
        color="error"
        :aria-label="t('projectsView.colDelete')"
        @click="act((n) => api.projectDelete(n, false))"
      />

      <v-divider vertical class="mx-2" />

      <v-btn icon variant="text" :aria-label="t('app.refresh')" :loading="loading" @click="load">
        <v-icon>mdi-refresh</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
      </v-btn>
    </v-toolbar>

    <v-divider />

    <div class="detail-body">
      <div class="detail-content" :class="{ 'detail-content--flush': section === 'logs' }">
        <ErrorAlert :error="error" type="error" closable class="mb-4" @close="error = null" />

        <div v-if="loading" class="d-flex justify-center py-16">
          <v-progress-circular indeterminate color="primary" />
        </div>

        <!-- INDICATOR ---------------------------------------------------- -->
        <template v-else-if="section === 'indicator'">
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
                <v-progress-linear
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
                <v-progress-linear
                  model-value="12"
                  color="purple"
                  height="6"
                  rounded
                  class="my-4"
                />
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
                  {{ stats?.pids ?? '—' }} pids · ↓{{ bytes(stats?.netRx) }} ↑{{
                    bytes(stats?.netTx)
                  }}
                </div>
              </v-card>
            </v-col>
          </v-row>

          <v-card rounded="lg" class="pa-4 mt-4">
            <div class="section-head">
              <v-icon size="18" class="mr-2">mdi-chart-donut</v-icon
              >{{ t('projectDetail.composition') }}
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
              <v-icon size="18" class="mr-2">mdi-calendar</v-icon
              >{{ t('projectDetail.cpuActivity') }}
            </div>

            <div v-if="!heatmap.length" class="text-caption text-medium-emphasis py-6 text-center">
              {{ t('projectDetail.noHistory') }}
            </div>

            <div v-else class="heatmap mt-3">
              <div class="heat-hours">
                <span
                  v-for="h in [0, 6, 12, 18]"
                  :key="h"
                  :style="{ left: `${(h / 24) * 100}%` }"
                  >{{ h }}</span
                >
              </div>
              <div v-for="day in heatmap" :key="day.label.toDateString()" class="heat-row">
                <span class="heat-day">
                  {{
                    day.label.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' })
                  }}
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
                <span
                  v-for="l in ['l0', 'l1', 'l2', 'l3', 'l4']"
                  :key="l"
                  class="heat-cell"
                  :class="l"
                />
                <span class="text-caption">{{ t('projectDetail.more') }}</span>
              </div>
            </div>
          </v-card>
        </template>

        <!-- CONFIGURATION ------------------------------------------------ -->
        <template v-else-if="section === 'configuration'">
          <div class="section-head mb-4">
            <v-icon size="18" class="mr-2">mdi-folder-cog</v-icon
            >{{ t('projectDetail.configuration') }}
          </div>

          <v-row>
            <v-col cols="12" md="4">
              <div class="field">
                <span class="field-key">{{ t('projectsView.colDomain') }}</span>
                <a
                  v-if="httpUrl"
                  class="field-link"
                  @click="project.domainConfigured && openUrl(httpsUrl)"
                >
                  {{ project.domain }}
                </a>
                <span v-else class="field-val">—</span>
              </div>
              <div v-if="manifest?.php" class="field">
                <span class="field-key">{{ t('newProject.phpVersion') }}</span>
                <span class="field-val">PHP {{ manifest.php.version }}</span>
              </div>
              <div v-if="manifest?.node" class="field">
                <span class="field-key">{{ t('newProject.nodeVersion') }}</span>
                <span class="field-val">Node {{ manifest.node.version }}</span>
              </div>
              <div class="field">
                <span class="field-key">{{ t('projectDetail.containerPath') }}</span>
                <code class="field-mono">/var/www/html</code>
                <v-btn
                  icon
                  :aria-label="t('a11y.copy')"
                  size="x-small"
                  variant="text"
                  @click="copy('/var/www/html', 'cpath')"
                >
                  <v-icon>mdi-content-copy</v-icon>
                  <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                </v-btn>
              </div>
              <div class="field">
                <span class="field-key">{{ t('projectDetail.accessHttp') }}</span>
                <a
                  v-if="httpUrl"
                  class="field-link"
                  @click="project.domainConfigured && openUrl(httpUrl)"
                  >{{ httpUrl }}</a
                >
                <span v-else class="field-val">—</span>
              </div>
            </v-col>

            <v-col cols="12" md="4">
              <div class="field">
                <span class="field-key">{{ t('projectDetail.sslStatus') }}</span>
                <span class="field-val text-success">
                  <v-icon size="14" color="success">mdi-lock</v-icon>
                  {{ t('projectDetail.sslEnabled') }}
                </span>
              </div>
              <div class="field">
                <span class="field-key">{{ t('projectsView.colServer') }}</span>
                <span class="field-val">{{ manifest?.server || '—' }}</span>
              </div>
              <div class="field">
                <span class="field-key">{{ t('projectDetail.hostPath') }}</span>
                <code class="field-mono">{{ project.path }}</code>
                <v-btn
                  icon
                  :aria-label="t('a11y.copy')"
                  size="x-small"
                  variant="text"
                  @click="copy(project.path, 'hpath')"
                >
                  <v-icon>mdi-content-copy</v-icon>
                  <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                </v-btn>
              </div>
            </v-col>

            <v-col cols="12" md="4">
              <div class="field">
                <span class="field-key">{{ t('projectDetail.type') }}</span>
                <span class="field-val">
                  <v-icon size="14">mdi-cog</v-icon> {{ t('projectsView.default') }}
                </span>
              </div>
              <div v-if="manifest?.documentRoot" class="field">
                <span class="field-key">{{ t('newProject.documentRoot') }}</span>
                <code class="field-mono">{{ manifest.documentRoot }}</code>
                <v-btn
                  icon
                  :aria-label="t('a11y.copy')"
                  size="x-small"
                  variant="text"
                  @click="copy(manifest.documentRoot, 'droot')"
                >
                  <v-icon>mdi-content-copy</v-icon>
                  <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                </v-btn>
              </div>
              <div class="field">
                <span class="field-key">{{ t('projectDetail.accessHttps') }}</span>
                <a
                  v-if="httpsUrl"
                  class="field-link"
                  @click="project.domainConfigured && openUrl(httpsUrl)"
                  >{{ httpsUrl }}</a
                >
                <span v-else class="field-val">—</span>
              </div>
            </v-col>
          </v-row>

          <!-- The domain is unreachable without a hosts entry, and the fix is
               one elevated write away — so offer it here rather than only
               reporting the problem. -->
          <v-alert
            v-if="project.domain && !project.domainConfigured"
            type="warning"
            variant="tonal"
            class="mt-2"
          >
            <div class="d-flex align-center ga-2">
              <span class="text-caption">{{ t('projects.domainMissingHint') }}</span>
              <v-spacer />
              <v-btn size="x-small" variant="tonal" @click="showHostsFix = true">{{
                t('hosts.fix')
              }}</v-btn>
            </div>
          </v-alert>

          <template v-if="manifest?.php?.extensions?.length">
            <div class="section-head mt-8 mb-3">
              <v-icon size="18" class="mr-2">mdi-puzzle</v-icon
              >{{ t('projectDetail.phpExtensions') }}
              <v-chip size="x-small" class="ml-2">{{ manifest.php.extensions.length }}</v-chip>
            </div>
            <div class="d-flex flex-wrap ga-2">
              <v-chip
                v-for="ext in manifest.php.extensions"
                :key="ext"
                size="small"
                label
                variant="tonal"
              >
                {{ ext }}
              </v-chip>
            </div>
          </template>

          <!-- Contract violations, shown rather than swallowed: the Bash
               generator skips such projects without a word. -->
          <template v-if="manifest?.errors?.length || manifest?.warnings?.length">
            <div class="section-head mt-8 mb-3">
              <v-icon size="18" class="mr-2">mdi-file-alert</v-icon>{{ t('projects.problems') }}
            </div>
            <v-alert v-if="manifest.errors.length" type="error" variant="tonal" class="mb-2">
              <div v-for="(i, k) in manifest.errors" :key="k" class="text-caption">
                <strong>{{ i.code }}</strong> {{ i.path }} — {{ i.message }}
              </div>
            </v-alert>
            <v-alert v-if="manifest.warnings.length" type="warning" variant="tonal">
              <div v-for="(i, k) in manifest.warnings" :key="k" class="text-caption">
                <strong>{{ i.code }}</strong> {{ i.path }} — {{ i.message }}
              </div>
            </v-alert>
          </template>
        </template>

        <!-- MANIFEST ------------------------------------------------------ -->
        <template v-else-if="section === 'manifest'">
          <div class="d-flex align-center ga-2 mb-3">
            <div class="section-head">
              <v-icon size="18" class="mr-2">mdi-code-json</v-icon>{{ t('detail.manifest') }}
            </div>
            <span class="text-caption text-medium-emphasis">{{ t('detail.manifestHint') }}</span>
            <v-spacer />
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-play-box-outline"
              :loading="ops.isBusy(name)"
              @click="bringUp"
              >{{ t('detail.bringUp') }}</v-btn
            >
            <v-btn
              size="small"
              color="primary"
              variant="flat"
              :disabled="!manifestDirty"
              :loading="manifestSaving"
              @click="saveManifest"
              >{{ t('detail.save') }}</v-btn
            >
          </div>

          <v-textarea
            v-model="manifestText"
            variant="outlined"
            rows="24"
            class="mono-input"
            hide-details
            @update:model-value="manifestDirty = true"
          />
        </template>

        <!-- LOGS ------------------------------------------------------------ -->
        <template v-else-if="section === 'logs'">
          <LogView
            v-if="project.built"
            :container="project.containerName"
            :active="section === 'logs'"
          />
          <div v-else class="text-caption text-medium-emphasis py-8 text-center">
            {{ t('detail.notBuilt') }}
          </div>
        </template>

        <!-- DOCKERFILE ---------------------------------------------------- -->
        <template v-else-if="section === 'dockerfile'">
          <div class="section-head mb-1">
            <v-icon size="18" class="mr-2">mdi-file-document-outline</v-icon>
            {{ t('detail.dockerfile') }}
          </div>
          <div class="text-caption text-medium-emphasis mb-3">{{ t('detail.dockerfileDesc') }}</div>

          <div class="d-flex align-center ga-3 flex-wrap mb-2">
            <v-btn-toggle
              :model-value="previewMode"
              mandatory
              divided
              color="primary"
              variant="flat"
              class="bg-surface-light"
              @update:model-value="loadPreview"
            >
              <v-btn value="compat" size="small">{{ t('detail.compat') }}</v-btn>
              <v-btn value="strict" size="small">{{ t('detail.strict') }}</v-btn>
            </v-btn-toggle>

            <!-- What the chip means depends on the mode above it, so they sit
                 together rather than at opposite ends of a bar. -->
            <v-chip
              v-if="preview"
              size="small"
              :color="preview.matchesBashOutput ? 'success' : 'warning'"
              :prepend-icon="preview.matchesBashOutput ? 'mdi-check-circle' : 'mdi-alert'"
            >
              {{
                preview.matchesBashOutput ? t('detail.matchesBash') : t('detail.differsFromBash')
              }}
            </v-chip>

            <v-spacer />

            <v-btn
              v-if="preview"
              icon
              size="small"
              variant="text"
              :aria-label="t('a11y.copy')"
              @click="copy(preview.dockerfile)"
            >
              <v-icon>mdi-content-copy</v-icon>
              <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
            </v-btn>
            <v-btn
              icon
              size="small"
              variant="text"
              :loading="previewLoading"
              :aria-label="t('app.refresh')"
              @click="loadPreview()"
            >
              <v-icon>mdi-refresh</v-icon>
              <v-tooltip activator="parent">{{ t('app.refresh') }}</v-tooltip>
            </v-btn>
          </div>

          <div class="text-caption text-medium-emphasis mb-3">
            {{ previewMode === 'strict' ? t('detail.strictHint') : t('detail.compatHint') }}
          </div>

          <!-- Bash drops an unbuildable extension without a word; strict mode
               exists so the reason is visible instead. -->
          <v-alert v-if="preview?.skipped?.length" type="warning" variant="tonal" class="mb-3">
            <div class="text-caption font-weight-medium mb-1">
              {{ t('detail.silentlySkipped') }}
            </div>
            <div v-for="s in preview.skipped" :key="s.extension" class="text-caption">
              <strong>{{ s.extension }}</strong> — {{ s.reason }}
            </div>
          </v-alert>

          <div v-if="preview" class="dockerfile">
            <div v-for="(line, i) in dockerfileLines" :key="i" class="df-line">
              <span class="df-no">{{ i + 1 }}</span>
              <code class="df-code">{{ line }}</code>
            </div>
          </div>
          <div v-else-if="previewLoading" class="d-flex justify-center py-8">
            <v-progress-circular indeterminate color="primary" />
          </div>
        </template>

        <!-- CONTAINER ----------------------------------------------------- -->
        <template v-else>
          <div class="section-head mb-4">
            <v-icon size="18" class="mr-2">mdi-docker</v-icon>{{ t('projectDetail.container') }}
          </div>

          <div v-if="!details" class="text-caption text-medium-emphasis py-8 text-center">
            {{ t('projects.notBuilt') }}
          </div>

          <template v-else>
            <v-row>
              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.name') }}</span>
                  <code class="field-mono">{{ details.name }}</code>
                  <v-btn
                    icon
                    :aria-label="t('a11y.copy')"
                    size="x-small"
                    variant="text"
                    @click="copy(details.name, 'cname')"
                  >
                    <v-icon>mdi-content-copy</v-icon>
                    <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                  </v-btn>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.uptime') }}</span>
                  <span class="field-val">{{
                    details.startedAt ? new Date(details.startedAt).toLocaleString() : '—'
                  }}</span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.restartPolicy') }}</span>
                  <span class="field-val">{{ details.restartPolicy || '—' }}</span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.dnsHosts') }}</span>
                  <span
                    class="field-val"
                    :class="project.domainConfigured ? 'text-success' : 'text-warning'"
                  >
                    <v-icon size="14">{{
                      project.domainConfigured ? 'mdi-check-circle' : 'mdi-alert-circle'
                    }}</v-icon>
                    {{
                      project.domainConfigured
                        ? t('projectDetail.configured')
                        : t('projectsView.noDnsRecord')
                    }}
                  </span>
                </div>
              </v-col>

              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('detail.state') }}</span>
                  <span class="field-val" :class="details.running ? 'text-success' : ''">
                    <v-icon size="10">mdi-circle</v-icon> {{ details.state }}
                  </span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.created') }}</span>
                  <span class="field-val">{{
                    details.created ? new Date(details.created).toLocaleString() : '—'
                  }}</span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.containerId') }}</span>
                  <code class="field-mono">{{ details.id?.slice(0, 12) }}</code>
                  <v-btn
                    icon
                    :aria-label="t('a11y.copy')"
                    size="x-small"
                    variant="text"
                    @click="copy(details.id, 'cid')"
                  >
                    <v-icon>mdi-content-copy</v-icon>
                    <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                  </v-btn>
                </div>
              </v-col>

              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('detail.image') }}</span>
                  <code class="field-mono">{{ details.image }}</code>
                  <v-btn
                    icon
                    :aria-label="t('a11y.copy')"
                    size="x-small"
                    variant="text"
                    @click="copy(details.image, 'img')"
                  >
                    <v-icon>mdi-content-copy</v-icon>
                    <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                  </v-btn>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.restartCount') }}</span>
                  <span class="field-val">{{ details.restartCount }}</span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.imageSize') }}</span>
                  <span class="field-val">{{
                    details.imageSize ? bytes(details.imageSize) : '—'
                  }}</span>
                </div>
              </v-col>
            </v-row>

            <div class="section-head mt-8 mb-3">
              <v-icon size="18" class="mr-2">mdi-lan</v-icon>{{ t('stats.network') }}
            </div>

            <v-row>
              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.gateway') }}</span>
                  <span class="field-val">{{ details.gateway || '—' }}</span>
                </div>
                <div class="field">
                  <span class="field-key">{{ t('projectDetail.portMappings') }}</span>
                  <span v-if="!details.ports.length" class="field-val">—</span>
                  <span v-else class="field-val">
                    <template v-for="p in details.ports" :key="p.container">
                      <code class="field-mono">{{ p.container }}/{{ p.protocol }}</code>
                      <span v-if="p.host" class="text-success ml-1">→ {{ p.host }}</span>
                      <span v-else class="text-warning ml-1">{{
                        t('projectDetail.notPublished')
                      }}</span>
                    </template>
                  </span>
                </div>
              </v-col>
              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('stats.network') }}</span>
                  <span class="field-val">{{ details.networks.join(', ') || '—' }}</span>
                </div>
              </v-col>
            </v-row>
          </template>
        </template>
      </div>

      <!-- Section navigation ---------------------------------------------- -->
      <div class="detail-nav">
        <v-list nav class="bg-transparent">
          <template v-for="s in SECTIONS" :key="s.key">
            <v-divider v-if="s.divide" class="my-2" />
            <v-list-item
              :prepend-icon="s.icon"
              :title="t(s.label)"
              :active="section === s.key"
              class="nav-item"
              @click="section = s.key"
            />
          </template>
        </v-list>
      </div>
    </div>

    <v-snackbar :model-value="!!copied" timeout="1200" color="success" location="bottom">
      {{ t('projectDetail.copied') }}
    </v-snackbar>

    <HostsDialog
      v-if="showHostsFix && project?.domain"
      v-model="showHostsFix"
      :add="[project.domain]"
      @applied="load"
    />
  </PageLayout>
</template>

<style scoped>
.detail-toolbar {
  flex: 0 0 auto;
}

.detail-body {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.detail-content {
  flex: 1 1 auto;
  min-width: 0;
  overflow-y: auto;
  padding: 16px;
}

/* The log view fills and scrolls itself; the page must not scroll it too. */
.detail-content--flush {
  overflow: hidden;
  padding: 0;
}

.detail-nav {
  flex: 0 0 240px;
  padding: 16px 8px;
  overflow-y: auto;
}

.nav-item {
  text-transform: uppercase;
  font-size: 0.78rem;
  letter-spacing: 0.05em;
}

.section-head {
  display: flex;
  align-items: center;
  font-size: 0.78rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  opacity: 0.75;
  font-weight: 600;
}

.tile-label {
  font-size: 0.72rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  opacity: 0.7;
}

.tile-foot {
  font-size: 0.72rem;
  opacity: 0.6;
}

.metric-tile {
  height: 100%;
}

.field {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 0;
  border-bottom: 1px solid rgba(var(--v-border-color), 0.06);
  font-size: 0.82rem;
}

.field-key {
  flex: 0 0 46%;
  text-transform: uppercase;
  font-size: 0.68rem;
  letter-spacing: 0.06em;
  opacity: 0.6;
}

.field-val {
  font-weight: 500;
  word-break: break-word;
}

.field-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.76rem;
  word-break: break-all;
}

.field-link {
  color: rgb(var(--v-theme-primary));
  cursor: pointer;
  word-break: break-all;
}

.field-link:hover {
  text-decoration: underline;
}

.mono-input :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  line-height: 1.6;
}

.dockerfile {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.6;
  border-radius: var(--app-radius);
  background: rgba(var(--v-border-color), 0.06);
  padding: 8px 0;
  overflow-x: auto;
}

.df-line {
  display: flex;
  gap: 12px;
  padding: 0 12px;
}

.df-line:hover {
  background: rgba(var(--v-border-color), 0.06);
}

/* Right-aligned in a fixed gutter so the numbers form a column rather than a
   ragged edge, and unselectable so copying the file does not copy them. */
.df-no {
  flex: 0 0 32px;
  text-align: right;
  opacity: 0.38;
  user-select: none;
}

.df-code {
  white-space: pre-wrap;
  word-break: break-word;
}

/* Heatmap ------------------------------------------------------------------ */
.heatmap {
  position: relative;
}

.heat-hours {
  position: relative;
  height: 16px;
  margin-left: 64px;
  font-size: 0.68rem;
  opacity: 0.6;
}

.heat-hours span {
  position: absolute;
}

.heat-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.heat-day {
  flex: 0 0 56px;
  font-size: 0.7rem;
  opacity: 0.65;
  text-align: right;
}

.heat-cells {
  display: grid;
  grid-template-columns: repeat(24, 1fr);
  gap: 3px;
  flex: 1 1 auto;
}

.heat-cell {
  height: 26px;
  border-radius: min(var(--app-radius), 6px);
  display: block;
}

/* An hour with no sample is not an hour that measured zero — the two must not
   look alike, or the grid invents history it never had. */
.heat-cell.empty {
  background: rgba(var(--v-border-color), 0.08);
}

.heat-cell.l0 {
  background: #0e2a16;
}
.heat-cell.l1 {
  background: #1b5e20;
}
.heat-cell.l2 {
  background: #2e7d32;
}
.heat-cell.l3 {
  background: #43a047;
}
.heat-cell.l4 {
  background: #66bb6a;
}

.heat-legend {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 10px;
}

.heat-legend .heat-cell {
  width: 14px;
  height: 14px;
}
</style>
