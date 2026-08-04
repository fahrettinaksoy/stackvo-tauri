<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { bytes, percent } from '@/lib/format';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import DumpView from '@/components/DumpView.vue';
import LogView from '@/components/LogView.vue';
import HostsDialog from '@/components/HostsDialog.vue';
import ProjectSettingsSheet from '@/components/ProjectSettingsSheet.vue';

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
const showSettings = ref(false);

const manifestText = ref('');
const manifestDirty = ref(false);
const manifestSaving = ref(false);
const preview = ref(null);

const SECTIONS = [
  { key: 'indicator', icon: 'mdi-chart-line', label: 'projectDetail.indicator' },
  // What the project *is*: its settings, the manifest they are written to, and
  // the Dockerfile they produce. Three views of one subject that were three
  // tabs, so seeing what a setting did meant leaving the page you set it on.
  { key: 'configuration', icon: 'mdi-folder-cog', label: 'projectDetail.configuration' },
  // What is running: the container's own facts, the workers inside it, and the
  // tunnel that exposes it. All three are about the process, not the project.
  { key: 'container', icon: 'mdi-docker', label: 'projectDetail.container' },
  // A section rather than a dialog over the page: logs are something you read
  // while looking at the rest, and a modal on top of a detail page hides the
  // thing it is about.
  { key: 'logs', icon: 'mdi-text-box-outline', label: 'logs.title' },
  // Xdebug, its profiler and the dump catcher. The first two are one extension
  // in two modes and were never two decisions; the third is where the output
  // lands. PHP only.
  { key: 'debug', icon: 'mdi-bug-outline', label: 'projectDetail.debug', php: true },
  // Settings that reach the container through a compose overlay this app
  // layers rather than through the manifest: php.ini for PHP, the dev server
  // for Node. A project is one or the other, so the tab is whichever applies.
  { key: 'runtime', icon: 'mdi-tune', label: 'projectDetail.runtime', runtime: true },
  // The one artefact here that leaves the machine.
  { key: 'release', icon: 'mdi-package-variant-closed', label: 'release.title' },
];
const section = ref('indicator');

/**
 * The panes this project actually has.
 *
 * Xdebug is a PHP extension, so a node project has nothing to switch on.
 * Showing the pane and explaining that inside it would be an entry in the rail
 * whose only content is a reason it does not apply — the navigation itself
 * should carry that.
 */
const sections = computed(() => {
  const runtime = project.value?.runtime;
  return SECTIONS.filter(
    (s) =>
      (!s.php || runtime === 'php') &&
      (!s.node || runtime === 'node') &&
      // The runtime pane holds php.ini or the dev server; a Go project has
      // neither, and an empty tab is a promise the page cannot keep.
      (!s.runtime || runtime === 'php' || runtime === 'node')
  );
});

/**
 * Is this pane the one on screen, and is there anything to show yet?
 *
 * The panes were a v-else-if chain, which is what kept each of them its own
 * tab: only the first matching branch renders, so two panes could never share
 * one. They are independent conditions now and several answer to the same tab,
 * which also means each has to exclude the loading state itself — the chain
 * used to do that for them.
 */
const shows = (key) => !loading.value && section.value === key;

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
/** The project directory in the system's own file manager. */
async function openProjectFolder() {
  error.value = null;
  try {
    await api.openFolder(project.value.path);
  } catch (e) {
    error.value = e;
  }
}

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
 * Make the running project match the manifest: regenerate, then rebuild.
 *
 * Both, in that order. `compose up --build` on its own rebuilds from the
 * Dockerfile already on disk, and that file was rendered from the manifest as
 * it read before the edit — so skipping the generate step produces a build that
 * succeeds and changes nothing, which is worse than one that fails.
 */
async function applyManifest() {
  await act(async () => {
    await api.generateRun('projects');
    await api.composeUpProject(props.name);
  });
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

// Navigating from a PHP project's Xdebug pane straight to a node project keeps
// the component and the selected section, which would leave the page on a pane
// the rail no longer offers — an empty panel with no way back to it.
watch(sections, (available) => {
  if (!available.some((s) => s.key === section.value)) section.value = 'indicator';
});

/**
 * This project's tunnel sidecar, when one exists.
 *
 * The URL is Cloudflare's to assign and arrives seconds after the sidecar
 * starts, so starting polls until the status call can read it out of the
 * sidecar's log — the same place it is read from after an app restart.
 */
const tunnel = ref(null);
const tunnelBusy = ref(false);

async function loadTunnel() {
  try {
    const all = await api.tunnelStatus();
    tunnel.value = all.find((t) => t.project === props.name) ?? null;
  } catch {
    tunnel.value = null;
  }
}

async function startTunnel() {
  tunnelBusy.value = true;
  error.value = null;
  try {
    await api.tunnelStart(props.name);
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 1500));
      await loadTunnel();
      if (tunnel.value?.url) break;
    }
  } catch (e) {
    error.value = e;
  } finally {
    tunnelBusy.value = false;
  }
}

async function stopTunnel() {
  tunnelBusy.value = true;
  error.value = null;
  try {
    await api.tunnelStop(props.name);
    await loadTunnel();
  } catch (e) {
    error.value = e;
  } finally {
    tunnelBusy.value = false;
  }
}

watch(section, (key) => {
  if (key === 'share') loadTunnel();
  if (key === 'workers') loadWorkers();
});

/**
 * Worker sidecars for this project.
 *
 * `available` comes from the project's files (artisan, composer.json);
 * `workers` from the engine. Docker itself does the healing — the app only
 * starts, stops, and shows the restart count the healing produces.
 */
const workerKinds = ref([]);
const workers = ref([]);
const workerBusy = ref(null);

async function loadWorkers() {
  try {
    const [kinds, all] = await Promise.all([api.workerOptions(props.name), api.workerStatus()]);
    workerKinds.value = kinds;
    workers.value = all.filter((w) => w.project === props.name);
  } catch (e) {
    error.value = e;
  }
}

function workerFor(kind) {
  return workers.value.find((w) => w.kind === kind) ?? null;
}

async function toggleWorker(kind) {
  workerBusy.value = kind;
  error.value = null;
  try {
    if (workerFor(kind)) await api.workerStop(props.name, kind);
    else await api.workerStart(props.name, kind);
    await loadWorkers();
  } catch (e) {
    error.value = e;
  } finally {
    workerBusy.value = null;
  }
}

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

  loadXdebug();
  loadPhpIni();
  loadDevServer();
  loadQuickCommands();
  loadProfiler();
  loadRelease();

  loading.value = false;
  startStats();
}

/**
 * Xdebug, across its three layers.
 *
 * Loaded with the rest rather than when the section opens: `xdebugPending`
 * badges the rail, and the state worth badging — enabled but never rebuilt, so
 * nothing actually happens when you set a breakpoint — is precisely the one a
 * user will not go looking for.
 */
const xdebug = ref(null);
const xdebugBusy = ref(false);

async function loadXdebug() {
  // Node projects have no PHP extension to report on, and the pane is not in
  // the rail for them either.
  if (project.value?.runtime !== 'php') {
    xdebug.value = null;
    return;
  }
  try {
    xdebug.value = await api.xdebugStatus(props.name);
  } catch {
    xdebug.value = null;
  }
}

async function toggleXdebug(enabled) {
  xdebugBusy.value = true;
  error.value = null;
  try {
    xdebug.value = await api.xdebugSet(props.name, enabled);
    // The manifest changed on disk, so the editor above it is now stale.
    const m = await api.projectManifestRead(props.name);
    manifestText.value = JSON.stringify(stripDiagnostics(m), null, 2);
    manifestDirty.value = false;
  } catch (e) {
    error.value = e;
  } finally {
    xdebugBusy.value = false;
  }
}

/** Enabled, but not yet doing anything — the state that needs saying out loud. */
const xdebugPending = computed(
  () => xdebug.value?.enabled && (xdebug.value.needsRebuild || xdebug.value.active === false)
);

/**
 * PHP overrides — `memory_limit` and the rest.
 *
 * The form is four text fields over a real ini file. It is deliberately not a
 * manifest form: these are not manifest keys and cannot become them (the schema
 * is `additionalProperties: false`), and the file the docs pointed at was
 * mounted by nothing until the compose overlay behind this shipped.
 *
 * Local edit state rather than binding straight at `phpIni.values`: an empty
 * field has to mean "remove this directive", and a v-model onto the status
 * object would make every keystroke look like a pending removal.
 */
const PHP_INI_FIELDS = [
  'memory_limit',
  'upload_max_filesize',
  'post_max_size',
  'max_execution_time',
];

const phpIni = ref(null);
const phpIniBusy = ref(false);
const phpIniDraft = ref({});

function resetPhpIniDraft() {
  const values = phpIni.value?.values ?? {};
  phpIniDraft.value = Object.fromEntries(PHP_INI_FIELDS.map((k) => [k, values[k] ?? '']));
}

async function loadPhpIni() {
  if (project.value?.runtime !== 'php') {
    phpIni.value = null;
    return;
  }
  try {
    phpIni.value = await api.phpIniStatus(props.name);
  } catch {
    phpIni.value = null;
  }
  resetPhpIniDraft();
}

const phpIniDirty = computed(() => {
  const values = phpIni.value?.values ?? {};
  return PHP_INI_FIELDS.some((k) => (phpIniDraft.value[k] ?? '') !== (values[k] ?? ''));
});

/** Every field cleared and nothing unmanaged left — the whole file goes. */
const phpIniWouldRemoveFile = computed(
  () =>
    PHP_INI_FIELDS.every((k) => !(phpIniDraft.value[k] ?? '').trim()) &&
    !Object.keys(phpIni.value?.unmanaged ?? {}).length
);

/**
 * A deployable image, built from the one this project already runs.
 *
 * Reviewed before it is built, like the hosts file and the certificate — a
 * production image is the one thing here that leaves the machine. And verified
 * after: the built image is run and asked whether it leaked an `.env`, because
 * that guarantee is easy to state in a Dockerfile and quietly wrong in the
 * result.
 */
const release = ref(null);
const releaseTag = ref('');
const releaseBusy = ref('');
const releaseResult = ref(null);

async function loadRelease() {
  try {
    release.value = await api.releasePlan(props.name, releaseTag.value || null);
    if (!releaseTag.value) releaseTag.value = release.value.tag;
  } catch (e) {
    release.value = null;
    if (e.code && e.code !== 'NOT_FOUND') error.value = e;
  }
}

async function buildRelease() {
  releaseBusy.value = 'build';
  error.value = null;
  releaseResult.value = null;
  try {
    releaseResult.value = await api.releaseBuild(props.name, releaseTag.value || null);
  } catch (e) {
    error.value = e;
  } finally {
    releaseBusy.value = '';
  }
}

async function saveRelease() {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const suggested = `${props.name}-production.tar`;
  const path = await save({ defaultPath: suggested });
  if (!path) return;

  releaseBusy.value = 'save';
  error.value = null;
  try {
    await api.releaseSave(props.name, path, releaseTag.value || null);
  } catch (e) {
    error.value = e;
  } finally {
    releaseBusy.value = '';
  }
}

/**
 * Xdebug's profiler.
 *
 * A mode of the existing Xdebug toggle rather than a second switch: the
 * extension has to be compiled in either way. The two modes are exclusive
 * because they want opposite start triggers — stepping connects on the next
 * request, profiling waits for `XDEBUG_TRIGGER` so an idle stack does not write
 * a multi-megabyte file per page load.
 */
const profiler = ref(null);
const profilerBusy = ref('');

/**
 * Is the running container in the mode the app is set to?
 *
 * The warning under the mode switch asked `active === false`, and that never
 * fired for the case it exists for. `active` means "both Xdebug variables are
 * present", and after switching stepping to profiling they still are — with
 * `XDEBUG_MODE=debug` in them. So the page reported profiling as applied, the
 * trigger did nothing, and the recorded list stayed at zero with nothing on
 * screen to say why.
 *
 * The container's own mode is the answer, compared against the configured one.
 * `null` while nothing is running or nothing has been read yet, which is not a
 * mismatch — a stopped project has no mode to disagree with.
 */
/**
 * Put the overlay's settings into the running container.
 *
 * **Not a restart**, and the difference is the whole bug. `project_restart`
 * calls Docker's `restart`, which restarts the process inside the container it
 * already has — and a container's environment and mounts are fixed when it is
 * *created*. So the "restart the project to apply them" that both the profiler
 * and the dumps panes have been telling people to do could never work: the
 * warning stayed up, `dd()` kept rendering into the response, and the profile
 * list stayed at zero, no matter how many times it was clicked.
 *
 * `compose up -d` is the operation that can: compose compares each service
 * against its definition and recreates the ones whose definition changed, which
 * after an overlay is written is exactly this project.
 */
async function applyToContainer() {
  await act(api.composeUpProject);
}

const profilerNeedsRestart = computed(() => {
  const status = profiler.value;
  if (!status?.xdebug?.running) return false;
  if (status.xdebug.active === false) return true;
  return !!status.xdebug.activeMode && status.xdebug.activeMode !== status.mode;
});
const profileReport = ref(null);
const profileOpenId = ref('');

async function loadProfiler() {
  if (project.value?.runtime !== 'php') {
    profiler.value = null;
    return;
  }
  try {
    profiler.value = await api.profilerStatus(props.name);
  } catch {
    profiler.value = null;
  }
}

async function setProfilerMode(mode) {
  profilerBusy.value = 'mode';
  error.value = null;
  try {
    profiler.value = await api.profilerSetMode(props.name, mode);
  } catch (e) {
    error.value = e;
  } finally {
    profilerBusy.value = '';
  }
}

async function openProfile(file) {
  profilerBusy.value = file.id;
  error.value = null;
  profileReport.value = null;
  try {
    profileReport.value = await api.profilerRead(props.name, file.id);
    profileOpenId.value = file.id;
  } catch (e) {
    error.value = e;
    profileOpenId.value = '';
  } finally {
    profilerBusy.value = '';
  }
}

async function deleteProfile(file) {
  profilerBusy.value = file.id;
  error.value = null;
  try {
    await api.profilerDelete(props.name, file.id);
    // The open report belongs to a file that no longer exists.
    if (profileOpenId.value === file.id) {
      profileReport.value = null;
      profileOpenId.value = '';
    }
    await loadProfiler();
  } catch (e) {
    error.value = e;
  } finally {
    profilerBusy.value = '';
  }
}

async function clearProfiles() {
  profilerBusy.value = 'clear';
  error.value = null;
  try {
    await api.profilerClear(props.name);
    profileReport.value = null;
    profileOpenId.value = '';
    await loadProfiler();
  } catch (e) {
    error.value = e;
  } finally {
    profilerBusy.value = '';
  }
}

/**
 * The time unit the *file* declares — never assumed.
 *
 * Measured on a real profile: `Time_(10ns)`. Reading it as microseconds would
 * be wrong by two orders of magnitude, and the number would look plausible.
 */
const profileUnit = computed(() => {
  const declared = profileReport.value?.events?.[0] ?? '';
  const match = declared.match(/\(([^)]+)\)/);
  return match ? match[1] : '';
});

/** Cost in the file's own unit, rendered as ms when the unit is known. */
function profileCost(value) {
  const unit = profileUnit.value;
  const ns = { '10ns': 10, ns: 1, us: 1000, ms: 1_000_000 }[unit];
  if (!ns) return `${value} ${unit}`.trim();
  const ms = (value * ns) / 1_000_000;
  return ms >= 1 ? `${ms.toFixed(1)} ms` : `${(ms * 1000).toFixed(0)} µs`;
}

/**
 * The commands you run in this project every day.
 *
 * Only the id crosses the boundary; the argv is built on the Rust side from a
 * fixed catalog, so nothing here can name a program to execute.
 *
 * Interactive commands open the user's own terminal and resolve to null —
 * there is nothing to stream, and an in-app REPL beside the terminal they have
 * already configured would be the worse of the two.
 */
const quickCommands = ref([]);
const quickCommandBusy = ref('');

async function loadQuickCommands() {
  try {
    quickCommands.value = await api.quickCommands(props.name);
  } catch {
    // A project with none of the marker files is the common case, not a fault.
    quickCommands.value = [];
  }
}

async function runQuickCommand(command) {
  quickCommandBusy.value = command.id;
  error.value = null;
  try {
    await api.quickCommandRun(props.name, command.id);
  } catch (e) {
    error.value = e;
  } finally {
    quickCommandBusy.value = '';
  }
}

/**
 * Hot reload for a node project.
 *
 * Not a toggle over a routing detail. Today a node project's source is COPYed
 * into the image at build time and never mounted, so editing a file on the host
 * changes nothing in the container — hot reload was impossible rather than
 * broken. Turning this on layers a compose overlay that mounts the source and
 * runs the dev server instead of the production start command.
 *
 * The third requirement is the project's own dev-server config, which lives in
 * the user's repository. It is shown as a snippet and never written.
 */
const devServer = ref(null);
const devServerBusy = ref(false);
const devServerCommand = ref('');
const snippetCopied = ref(false);

async function loadDevServer() {
  if (project.value?.runtime !== 'node') {
    devServer.value = null;
    return;
  }
  try {
    devServer.value = await api.devserverStatus(props.name);
    devServerCommand.value = devServer.value.command;
  } catch {
    devServer.value = null;
  }
}

async function toggleDevServer(enabled) {
  devServerBusy.value = true;
  error.value = null;
  try {
    devServer.value = await api.devserverSet(props.name, enabled, devServerCommand.value || null);
    devServerCommand.value = devServer.value.command;
  } catch (e) {
    error.value = e;
  } finally {
    devServerBusy.value = false;
  }
}

async function copySnippet() {
  try {
    await navigator.clipboard.writeText(devServer.value.snippet);
    snippetCopied.value = true;
    setTimeout(() => (snippetCopied.value = false), 1500);
  } catch {
    /* clipboard unavailable */
  }
}

/** On, mounted, and the project's own config still rejects the domain — the
 *  state where the container is right and the site answers 403. */
const devServerBlocked = computed(
  () => devServer.value?.enabled && devServer.value.hostAllowed === false
);

async function savePhpIni() {
  phpIniBusy.value = true;
  error.value = null;
  try {
    // Only what changed. Sending the unchanged fields too would rewrite lines
    // the user may have commented next to, for no reason.
    const values = phpIni.value?.values ?? {};
    const patch = {};
    for (const key of PHP_INI_FIELDS) {
      const next = (phpIniDraft.value[key] ?? '').trim();
      const now = values[key] ?? '';
      if (next === now) continue;
      // An empty field is a removal, not an empty value: this file is an
      // override layer, and `memory_limit =` with nothing after it is a
      // directive PHP reads as zero.
      patch[key] = next === '' ? null : next;
    }
    phpIni.value = await api.phpIniSet(props.name, patch);
    resetPhpIniDraft();
  } catch (e) {
    error.value = e;
  } finally {
    phpIniBusy.value = false;
  }
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
onUnmounted(() => {
  clearInterval(statsTimer);
});
</script>

<template>
  <PageLayout
    top-icon="mdi-information"
    :top-title="t('projectDetail.title')"
    :top-subtitle="t('projectDetail.subtitle')"
    hide-bar
  >
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
        size="large"
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
        variant="tonal"
        size="small"
        elevation="0"
        color="primary"
        class="mr-2"
        :aria-label="t('projectsView.colOpen')"
        @click="api.openInBrowser(httpsUrl)"
      >
        <v-icon>mdi-open-in-new</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('projectsView.colOpen') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-if="running"
        icon
        variant="tonal"
        size="small"
        elevation="0"
        color="info"
        class="mr-2"
        :aria-label="t('detail.externalTerminal')"
        @click="openExternalTerminal"
      >
        <v-icon>mdi-application-export</v-icon>
        <v-tooltip activator="parent" location="bottom">{{
          t('detail.externalTerminal')
        }}</v-tooltip>
      </v-btn>
      <!-- The commands the project's own files imply — artisan, composer,
           npm — in the bar rather than in a pane you had to navigate to. They
           are things you run while looking at the project, not things you go
           somewhere to read about. -->
      <v-menu location="bottom end">
        <template #activator="{ props: menu }">
          <v-btn
            v-bind="menu"
            icon
            variant="tonal"
            size="small"
            elevation="0"
            class="mr-2"
            :loading="!!quickCommandBusy"
            :aria-label="t('quickCmd.title')"
          >
            <v-icon>mdi-console-line</v-icon>
            <v-tooltip activator="parent" location="bottom">{{ t('quickCmd.title') }}</v-tooltip>
          </v-btn>
        </template>

        <v-list density="compact" class="cmd-menu">
          <v-list-subheader>{{ t('quickCmd.title') }}</v-list-subheader>
          <div class="px-4 pb-2 text-caption text-medium-emphasis">
            {{ t('quickCmd.explain') }}
          </div>

          <!-- They exec into the container, so there has to be one. Said here
               rather than left to a disabled item with no reason on it. -->
          <v-list-item v-if="!running" :subtitle="t('quickCmd.needsRunning')" />

          <!-- The button stays even with nothing to offer: hiding it would
               leave someone expecting artisan with no button and no reason. -->
          <v-list-item v-else-if="!quickCommands.length" :subtitle="t('quickCmd.none')" />

          <!-- A rule where the source file changes: artisan, then composer,
               then npm. The catalog already returns them in that order, so the
               break is where one tool's commands end rather than a grouping
               invented here. -->
          <template v-for="(command, i) in quickCommands" :key="command.id">
            <v-divider v-if="i && command.because !== quickCommands[i - 1].because" class="my-1" />

            <v-list-item
              :disabled="!running || !!quickCommandBusy"
              @click="runQuickCommand(command)"
            >
              <template #prepend>
                <v-icon
                  size="small"
                  :icon="command.interactive ? 'mdi-console' : 'mdi-play'"
                  class="mr-2"
                />
              </template>
              <v-list-item-title class="mono">{{ command.display }}</v-list-item-title>
              <v-list-item-subtitle>{{ command.about }}</v-list-item-subtitle>
              <v-list-item-subtitle class="text-disabled">
                {{ t('quickCmd.because', { file: command.because }) }}
              </v-list-item-subtitle>
              <template #append>
                <!-- Said on the row, not discovered by pressing it: one of these
                   opens a terminal window and the other prints into the
                   console below, and they look identical otherwise. -->
                <v-chip v-if="command.interactive" size="x-small" variant="tonal">
                  {{ t('quickCmd.opensTerminal') }}
                </v-chip>
                <v-progress-circular
                  v-else-if="quickCommandBusy === command.id"
                  size="14"
                  width="2"
                  indeterminate
                />
              </template>
            </v-list-item>
          </template>
        </v-list>
      </v-menu>

      <v-btn
        icon
        variant="tonal"
        size="small"
        elevation="0"
        class="mr-2"
        :aria-label="t('detail.openInEditor')"
        @click="openInEditor"
      >
        <v-icon>mdi-code-tags</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('detail.openInEditor') }}</v-tooltip>
      </v-btn>

      <!-- The folder icon now means the folder. It read as "open in editor"
           before, which is a different application and a different intention:
           one is where you write the code, the other is where you look at what
           is on disk beside it. -->
      <v-btn
        icon
        variant="tonal"
        size="small"
        elevation="0"
        class="mr-2"
        :aria-label="t('detail.openFolder')"
        @click="openProjectFolder"
      >
        <v-icon>mdi-folder-open</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('detail.openFolder') }}</v-tooltip>
      </v-btn>
      <v-btn
        v-if="running"
        icon
        variant="tonal"
        size="small"
        elevation="0"
        class="mr-2"
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
        variant="tonal"
        size="small"
        elevation="0"
        color="success"
        class="mr-2"
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
        variant="tonal"
        size="small"
        elevation="0"
        color="info"
        class="mr-2"
        :aria-label="t('actions.build')"
        :disabled="!app.engineUp || !project.manifestValid"
        :loading="ops.isBusy(name)"
        @click="act((n) => api.projectBuild(n))"
      />
      <v-btn
        v-if="running"
        icon
        variant="tonal"
        size="small"
        elevation="0"
        color="warning"
        class="mr-2"
        :aria-label="t('actions.restart')"
        :loading="ops.isBusy(name)"
        @click="act(api.projectRestart)"
      >
        <v-icon>mdi-restart</v-icon>
        <v-tooltip activator="parent" location="bottom">{{ t('actions.restart') }}</v-tooltip>
      </v-btn>
      <v-btn
        icon="mdi-delete"
        variant="tonal"
        size="small"
        elevation="0"
        color="error"
        :aria-label="t('projectsView.colDelete')"
        @click="act((n) => api.projectDelete(n, false))"
      />

      <v-divider vertical class="mx-3 my-3" />

      <v-btn
        icon
        variant="tonal"
        size="small"
        elevation="0"
        class="mr-3"
        :aria-label="t('app.refresh')"
        :loading="loading"
        @click="load"
      >
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
        <template v-if="shows('indicator')">
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

              <div
                v-if="!heatmap.length"
                class="text-caption text-medium-emphasis py-6 text-center"
              >
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
          </v-card>
        </template>

        <!-- CONFIGURATION ------------------------------------------------ -->
        <template v-if="shows('configuration')">
          <v-card variant="flat" class="pane">
            <div class="d-flex align-center ga-2 mb-4">
              <div class="section-head">
                <v-icon size="18" class="mr-2">mdi-folder-cog</v-icon
                >{{ t('projectDetail.configuration') }}
              </div>
              <v-spacer />
              <!-- Every value read below is a field in stackvo.json, so the way
                   to change one belongs beside them rather than only in the raw
                   JSON pane further down the rail. -->
              <v-btn
                size="small"
                variant="tonal"
                prepend-icon="mdi-tune-variant"
                @click="showSettings = true"
                >{{ t('projectSettings.open') }}</v-btn
              >
            </div>

            <v-row>
              <v-col cols="12" md="4">
                <div class="field">
                  <span class="field-key">{{ t('projectsView.colDomain') }}</span>
                  <a
                    v-if="httpUrl"
                    class="field-link"
                    @click="project.domainConfigured && api.openInBrowser(httpsUrl)"
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
                    @click="project.domainConfigured && api.openInBrowser(httpUrl)"
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
                    @click="project.domainConfigured && api.openInBrowser(httpsUrl)"
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
          </v-card>
        </template>

        <!-- XDEBUG --------------------------------------------------------- -->
        <!-- Three layers reported separately. Collapsing them into one "on"
             would put a switch in the UI that reads as done while nothing has
             been compiled in, which is worse than no switch. -->
        <template v-if="shows('debug')">
          <v-card variant="flat" class="pane">
            <div class="d-flex align-center ga-2 mb-3">
              <div class="section-head">
                <v-icon size="18" class="mr-2">mdi-bug-outline</v-icon>{{ t('xdebug.title') }}
              </div>
              <span class="text-caption text-medium-emphasis">{{ t('xdebug.subtitle') }}</span>
            </div>

            <template v-if="xdebug">
              <v-switch
                :model-value="xdebug.enabled"
                :loading="xdebugBusy"
                :disabled="xdebugBusy"
                color="primary"
                hide-details
                density="comfortable"
                :label="xdebug.enabled ? t('xdebug.on') : t('xdebug.off')"
                @update:model-value="toggleXdebug($event)"
              />

              <!-- The extension is compiled in, so the manifest can be ahead of
                   the image. Saying nothing here is how a toggle becomes a lie. -->
              <v-alert v-if="xdebug.needsRebuild" type="warning" variant="tonal" class="mt-3">
                <div class="text-caption">{{ t('xdebug.needsRebuild') }}</div>
              </v-alert>
              <v-alert
                v-else-if="xdebug.enabled && xdebug.running && xdebug.active === false"
                type="warning"
                variant="tonal"
                class="mt-3"
              >
                <div class="text-caption">{{ t('xdebug.notActive') }}</div>
              </v-alert>
              <v-alert
                v-else-if="xdebug.enabled && xdebug.active === true"
                type="success"
                variant="tonal"
                class="mt-3"
              >
                <div class="text-caption">{{ t('xdebug.active') }}</div>
              </v-alert>

              <!-- The path mapping is the step people get wrong, and both halves
                   are already known here. -->
              <template v-if="xdebug.enabled">
                <div class="section-head mt-5 mb-2">
                  <v-icon size="18" class="mr-2">mdi-tune</v-icon>{{ t('xdebug.ideSettings') }}
                </div>
                <v-table density="compact">
                  <tbody>
                    <tr>
                      <td class="text-medium-emphasis">{{ t('xdebug.port') }}</td>
                      <td class="mono">{{ xdebug.port }}</td>
                    </tr>
                    <tr>
                      <td class="text-medium-emphasis">{{ t('xdebug.ideKey') }}</td>
                      <td class="mono">{{ xdebug.ideKey }}</td>
                    </tr>
                    <tr v-if="xdebug.serverName">
                      <td class="text-medium-emphasis">{{ t('xdebug.serverName') }}</td>
                      <td class="mono">{{ xdebug.serverName }}</td>
                    </tr>
                    <tr>
                      <td class="text-medium-emphasis">{{ t('xdebug.pathMapping') }}</td>
                      <td class="mono">{{ xdebug.hostPath }} → {{ xdebug.containerPath }}</td>
                    </tr>
                    <tr v-if="xdebug.peclVersion">
                      <td class="text-medium-emphasis">{{ t('xdebug.version') }}</td>
                      <td class="mono">{{ xdebug.peclVersion }} (PHP {{ xdebug.phpVersion }})</td>
                    </tr>
                  </tbody>
                </v-table>

                <!-- The one thing this design cannot fix, said where it will be
                     read rather than left for someone to discover. -->
                <div class="text-caption text-medium-emphasis mt-3">
                  {{ t('xdebug.cliCaveat') }}
                </div>
              </template>
            </template>
          </v-card>
        </template>

        <!-- COMMANDS ------------------------------------------------------- -->
        <!-- A fixed catalog, filtered by the files the project actually has.
             Offering `artisan migrate` to a project with no artisan produces
             `not found` in the console, which reads as a broken app rather
             than as a button that never applied. -->

        <!-- DEV SERVER ----------------------------------------------------- -->
        <!-- Three requirements, kept apart because they fail separately: the
             source has to be mounted, the dev server has to be what is running,
             and the dev server has to accept a request for this domain. Only
             the first two are this app's to fix. -->
        <template v-if="shows('runtime')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-lightning-bolt-outline</v-icon>
              {{ t('devServer.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('devServer.explain') }}</p>

            <template v-if="devServer">
              <v-switch
                :model-value="devServer.enabled"
                :loading="devServerBusy"
                :disabled="devServerBusy"
                color="primary"
                hide-details
                density="comfortable"
                :label="devServer.enabled ? t('devServer.on') : t('devServer.off')"
                @update:model-value="toggleDevServer($event)"
              />

              <v-text-field
                v-model="devServerCommand"
                :label="t('devServer.command')"
                :hint="
                  devServer.productionCommand
                    ? t('devServer.commandHint', { production: devServer.productionCommand })
                    : ''
                "
                persistent-hint
                density="comfortable"
                variant="outlined"
                class="mt-4"
                :disabled="devServerBusy"
                @keyup.enter="devServer.enabled && toggleDevServer(true)"
              />

              <!-- On, but the container predates it — the source is not mounted
                   in the thing that is actually running. -->
              <v-alert v-if="devServer.needsRecreate" type="warning" variant="tonal" class="mt-4">
                <div class="text-caption">{{ t('devServer.needsRecreate') }}</div>
              </v-alert>
              <v-alert
                v-else-if="devServer.enabled && devServer.mounted"
                type="success"
                variant="tonal"
                class="mt-4"
              >
                <div class="text-caption">{{ t('devServer.live') }}</div>
              </v-alert>

              <!-- The half that is not ours. A .loc domain gets a flat 403 from
                   Vite unless its own config names it, which reads as "the site
                   is up and broken" with nothing pointing at the dev server. -->
              <template v-if="devServer.snippet">
                <div class="section-head mt-5 mb-1">
                  <v-icon size="18" class="mr-2">mdi-file-code-outline</v-icon>
                  {{ t('devServer.projectConfig') }}
                </div>
                <p class="text-caption text-medium-emphasis mb-2">
                  {{ t('devServer.projectConfigWhy') }}
                </p>

                <v-alert
                  v-if="devServerBlocked"
                  type="warning"
                  variant="tonal"
                  density="compact"
                  class="mb-2"
                >
                  <div class="text-caption">
                    {{ t('devServer.notAllowed', { file: devServer.configFile }) }}
                  </div>
                </v-alert>
                <v-alert
                  v-else-if="devServer.hostAllowed"
                  type="success"
                  variant="tonal"
                  density="compact"
                  class="mb-2"
                >
                  <div class="text-caption">{{ t('devServer.configured') }}</div>
                </v-alert>

                <div class="d-flex align-start ga-2">
                  <pre class="snippet flex-grow-1">{{ devServer.snippet }}</pre>
                  <v-btn
                    icon
                    size="small"
                    variant="text"
                    :aria-label="t('a11y.copy')"
                    @click="copySnippet"
                  >
                    <v-icon>{{ snippetCopied ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
                    <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                  </v-btn>
                </div>
              </template>
              <div v-else class="text-caption text-medium-emphasis mt-4">
                {{ t('devServer.noAdvice') }}
              </div>

              <div class="text-caption text-medium-emphasis mt-4">
                {{ t('devServer.modulesNote') }}
              </div>
              <div class="text-caption text-medium-emphasis mt-2">
                {{ t('devServer.cliCaveat') }}
              </div>
            </template>
          </v-card>
        </template>

        <!-- PROFILER ------------------------------------------------------- -->
        <!-- Xdebug's own profiler. Blackfire needs an account and SPX is not
             in the extension contract; this needs neither. -->
        <template v-if="shows('debug')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-speedometer</v-icon>{{ t('profiler.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('profiler.explain') }}</p>

            <template v-if="profiler">
              <!-- Compiled in first. Without the extension there is nothing to
                   switch a mode on. -->
              <v-alert v-if="!profiler.xdebug.enabled" type="info" variant="tonal" class="mb-4">
                <div class="text-caption">{{ t('profiler.needsXdebug') }}</div>
              </v-alert>

              <template v-else>
                <v-btn-toggle
                  :model-value="profiler.mode"
                  mandatory
                  density="comfortable"
                  variant="outlined"
                  divided
                  @update:model-value="setProfilerMode($event)"
                >
                  <v-btn value="debug" :disabled="!!profilerBusy" prepend-icon="mdi-bug-outline">
                    {{ t('profiler.modeDebug') }}
                  </v-btn>
                  <v-btn value="profile" :disabled="!!profilerBusy" prepend-icon="mdi-speedometer">
                    {{ t('profiler.modeProfile') }}
                  </v-btn>
                </v-btn-toggle>
                <div class="text-caption text-medium-emphasis mt-2">
                  {{ t('profiler.modesExclusive') }}
                </div>

                <!-- The step people miss. Profiling waits for a trigger, so
                     loading the page changes nothing until it carries one. -->
                <v-alert
                  v-if="profiler.mode === 'profile'"
                  type="info"
                  variant="tonal"
                  class="mt-4"
                >
                  <div class="text-caption">
                    {{ t('profiler.howToRecord', { trigger: profiler.trigger }) }}
                  </div>
                </v-alert>
                <!-- Fires for either mode, not just profiling: switching back
                     to stepping leaves the container profiling, and that is the
                     same silence pointing the other way. -->
                <v-alert v-if="profilerNeedsRestart" type="warning" variant="tonal" class="mt-3">
                  <div class="text-caption">{{ t('profiler.needsRecreate') }}</div>
                  <div v-if="profiler.xdebug.activeMode" class="text-caption mt-1">
                    {{
                      t('profiler.modeMismatch', {
                        running: profiler.xdebug.activeMode,
                        wanted: profiler.mode,
                      })
                    }}
                  </div>
                  <v-btn
                    size="small"
                    color="warning"
                    variant="tonal"
                    class="mt-2"
                    prepend-icon="mdi-autorenew"
                    :loading="ops.isBusy(name)"
                    @click="applyToContainer"
                  >
                    {{ t('projectDetail.applyToContainer') }}
                  </v-btn>
                </v-alert>
              </template>

              <div class="section-head mt-5 mb-2 d-flex align-center">
                <v-icon size="18" class="mr-2">mdi-file-chart-outline</v-icon>
                {{ t('profiler.recorded', { n: profiler.profiles.length }) }}
                <v-spacer />
                <!-- One run of a tight loop produced 10 MB. Sixty delete buttons
                     is not a disk-hygiene story. -->
                <v-btn
                  v-if="profiler.profiles.length"
                  size="x-small"
                  variant="text"
                  color="error"
                  :loading="profilerBusy === 'clear'"
                  @click="clearProfiles"
                >
                  {{ t('profiler.clear', { size: bytes(profiler.bytes) }) }}
                </v-btn>
              </div>

              <div v-if="!profiler.profiles.length" class="text-caption text-medium-emphasis">
                {{ t('profiler.noneYet') }}
              </div>

              <div v-for="file in profiler.profiles" :key="file.id" class="cmd-row">
                <div class="flex-grow-1 min-width-0">
                  <div class="mono text-body-2">{{ file.id }}</div>
                  <div class="text-caption text-medium-emphasis">
                    {{ bytes(file.bytes) }}
                    <span v-if="file.modified">
                      · {{ new Date(file.modified * 1000).toLocaleString() }}</span
                    >
                  </div>
                </div>
                <v-chip v-if="file.compressed" size="x-small" color="warning" variant="tonal">
                  {{ t('profiler.compressed') }}
                </v-chip>
                <v-btn
                  size="small"
                  variant="tonal"
                  :loading="profilerBusy === file.id"
                  :disabled="file.compressed || !!profilerBusy"
                  @click="openProfile(file)"
                >
                  {{ t('profiler.open') }}
                </v-btn>
                <!-- No confirmation: a profile is a recording you can make again
                     by reloading the page, not something to lose. -->
                <v-btn
                  icon
                  size="x-small"
                  variant="text"
                  :aria-label="t('profiler.deleteOne')"
                  :disabled="!!profilerBusy"
                  @click="deleteProfile(file)"
                >
                  <v-icon>mdi-delete-outline</v-icon>
                  <v-tooltip activator="parent">{{ t('profiler.deleteOne') }}</v-tooltip>
                </v-btn>
              </div>

              <!-- The report. Self cost, because it is the one this parser can
                   state exactly and the one that answers "what is slow". -->
              <template v-if="profileReport">
                <div class="section-head mt-5 mb-1">
                  <v-icon size="18" class="mr-2">mdi-podium</v-icon>{{ profileOpenId }}
                </div>
                <div class="text-caption text-medium-emphasis mb-2">
                  {{
                    t('profiler.summary', {
                      n: profileReport.functionCount,
                      total: profileCost(profileReport.selfTotal),
                      creator: profileReport.creator,
                    })
                  }}
                </div>
                <v-alert
                  v-if="profileReport.truncated"
                  type="warning"
                  variant="tonal"
                  density="compact"
                  class="mb-2"
                >
                  <div class="text-caption">{{ t('profiler.truncated') }}</div>
                </v-alert>

                <v-table density="compact">
                  <thead>
                    <tr>
                      <th>{{ t('profiler.colFunction') }}</th>
                      <th class="text-right">{{ t('profiler.colSelf') }}</th>
                      <th class="text-right">{{ t('profiler.colInclusive') }}</th>
                      <th class="text-right">{{ t('profiler.colCalls') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="fn in profileReport.functions" :key="fn.name">
                      <td class="min-width-0">
                        <div class="mono text-truncate">{{ fn.name }}</div>
                        <!-- The bar is the percentage, so the eye finds the hot
                             row before it reads a single number. -->
                        <v-progress-linear
                          :model-value="fn.percent"
                          height="3"
                          color="primary"
                          class="mt-1"
                        />
                      </td>
                      <td class="text-right mono">
                        {{ profileCost(fn.selfTime) }}
                        <div class="text-caption text-medium-emphasis">
                          {{ fn.percent.toFixed(1) }}%
                        </div>
                      </td>
                      <td class="text-right mono">{{ profileCost(fn.inclusiveTime) }}</td>
                      <td class="text-right mono">{{ fn.calls }}</td>
                    </tr>
                  </tbody>
                </v-table>
              </template>
            </template>
          </v-card>
        </template>

        <!-- DUMPS ---------------------------------------------------------- -->
        <!-- One renderer, two scopes: this pane and the Dumps page share
             `DumpView`, so search, the source link and the capture switch
             cannot drift between them. -->
        <template v-if="shows('debug')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-bug-check-outline</v-icon>{{ t('dumps.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('dumps.explain') }}</p>

            <DumpView :project="name" scope="project">
              <!-- The recreate button belongs to the project page: it is the
                   same operation the profiler warning offers, and this is
                   where the project's lifecycle controls live. -->
              <template #recreate>
                <v-btn
                  size="small"
                  color="warning"
                  variant="tonal"
                  class="mt-2"
                  prepend-icon="mdi-autorenew"
                  :loading="ops.isBusy(name)"
                  @click="applyToContainer"
                >
                  {{ t('projectDetail.applyToContainer') }}
                </v-btn>
              </template>
            </DumpView>
          </v-card>
        </template>

        <!-- RELEASE -------------------------------------------------------- -->
        <!-- The dev image is not a production image: for PHP it holds no
             application code at all (the source is bind-mounted) and it carries
             Xdebug. So this is a build, and the result is checked rather than
             trusted. -->
        <template v-if="shows('release')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-package-variant-closed</v-icon>
              {{ t('release.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('release.explain') }}</p>

            <template v-if="release">
              <div class="d-flex ga-2 align-start">
                <v-text-field
                  v-model="releaseTag"
                  :label="t('release.tag')"
                  :hint="t('release.tagHint', { base: release.baseImage })"
                  persistent-hint
                  density="comfortable"
                  variant="outlined"
                  :disabled="!!releaseBusy"
                />
                <v-btn
                  color="primary"
                  variant="flat"
                  :loading="releaseBusy === 'build'"
                  :disabled="!releaseTag.trim() || !!releaseBusy"
                  @click="buildRelease"
                >
                  {{ t('release.build') }}
                </v-btn>
              </div>

              <!-- Everything the result will be true of, before the build rather
                   than after. None of these is a decision to make silently. -->
              <v-alert type="info" variant="tonal" class="mt-4">
                <div v-for="line in release.warnings" :key="line" class="text-caption">
                  • {{ line }}
                </div>
              </v-alert>

              <div class="section-head mt-5 mb-2">
                <v-icon size="18" class="mr-2">mdi-eye-off-outline</v-icon>
                {{ t('release.excluded') }}
              </div>
              <v-table density="compact">
                <tbody>
                  <tr v-for="[pattern, reason] in release.excluded" :key="pattern">
                    <td class="mono">{{ pattern }}</td>
                    <td class="text-medium-emphasis text-caption">{{ reason }}</td>
                  </tr>
                </tbody>
              </v-table>

              <v-expansion-panels v-if="release.dockerfile" variant="accordion" class="mt-4">
                <v-expansion-panel :title="t('release.dockerfile')">
                  <v-expansion-panel-text>
                    <pre class="snippet">{{ release.dockerfile }}</pre>
                  </v-expansion-panel-text>
                </v-expansion-panel>
              </v-expansion-panels>

              <!-- Read out of the built image, not inferred from the Dockerfile:
                   this guarantee is easy to state and easy to get wrong. -->
              <template v-if="releaseResult">
                <div class="section-head mt-5 mb-2">
                  <v-icon size="18" class="mr-2">mdi-shield-check-outline</v-icon>
                  {{ t('release.checked') }}
                </div>

                <v-alert
                  :type="releaseResult.verification.clean ? 'success' : 'error'"
                  variant="tonal"
                >
                  <div class="text-caption">
                    {{
                      releaseResult.verification.clean
                        ? t('release.clean', { tag: releaseResult.plan.tag })
                        : t('release.notClean')
                    }}
                  </div>
                  <ul class="text-caption mt-2 pl-4">
                    <li v-if="releaseResult.verification.envFiles.length">
                      {{
                        t('release.leaked', {
                          files: releaseResult.verification.envFiles.join(', '),
                        })
                      }}
                    </li>
                    <li v-else>{{ t('release.noEnv') }}</li>
                    <li v-if="releaseResult.verification.xdebugActive === true">
                      {{ t('release.xdebugOn') }}
                    </li>
                    <li v-else-if="releaseResult.verification.xdebugActive === false">
                      {{ t('release.xdebugOff') }}
                    </li>
                    <li v-if="!releaseResult.verification.hasApp">{{ t('release.noApp') }}</li>
                  </ul>
                </v-alert>

                <v-btn
                  v-if="releaseResult.verification.clean"
                  class="mt-3"
                  variant="tonal"
                  prepend-icon="mdi-download-outline"
                  :loading="releaseBusy === 'save'"
                  :disabled="!!releaseBusy"
                  @click="saveRelease"
                >
                  {{ t('release.save') }}
                </v-btn>
              </template>
            </template>
          </v-card>
        </template>

        <!-- PHP.INI -------------------------------------------------------- -->
        <!-- Three states again, and for the same reason as Xdebug: the file on
             disk, the mount in the running container, and PHP having read it.
             They come apart in practice — the Bash CLI's `up` layers three
             compose files, not five, and recreates the container without the
             mount — so collapsing them would produce a form that saves happily
             and changes nothing. -->
        <template v-if="shows('runtime')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-language-php</v-icon>{{ t('phpIni.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('phpIni.explain') }}</p>

            <template v-if="phpIni">
              <v-row dense>
                <v-col v-for="key in PHP_INI_FIELDS" :key="key" cols="12" sm="6">
                  <!-- The placeholder is what PHP in the container reports right
                       now, not a documented default. Measured, because assuming
                       was wrong: these images ship no php.ini at all, and
                       max_execution_time is 0 under FPM rather than the 30 the
                       manual lists. Falls back to the field name when nothing is
                       running — inventing a number would be worse. -->
                  <v-text-field
                    v-model="phpIniDraft[key]"
                    :label="t(`phpIni.field.${key}`)"
                    :placeholder="phpIni.effective?.[key] || t('phpIni.notMeasured')"
                    :hint="t(`phpIni.hint.${key}`)"
                    persistent-placeholder
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    :disabled="phpIniBusy"
                  />
                </v-col>
              </v-row>

              <!-- Legal, and almost always a mistake: the upload fails at the
                   smaller of the two, which is a number the user can see they
                   have already raised. -->
              <v-alert v-if="phpIni.warning" type="warning" variant="tonal" class="mt-3">
                <div class="text-caption">{{ phpIni.warning }}</div>
              </v-alert>

              <div class="d-flex align-center ga-2 mt-4">
                <v-btn
                  color="primary"
                  variant="flat"
                  size="small"
                  :loading="phpIniBusy"
                  :disabled="!phpIniDirty"
                  @click="savePhpIni"
                >
                  {{
                    phpIniWouldRemoveFile && phpIniDirty ? t('phpIni.removeFile') : t('phpIni.save')
                  }}
                </v-btn>
                <v-btn
                  variant="text"
                  size="small"
                  :disabled="!phpIniDirty || phpIniBusy"
                  @click="resetPhpIniDraft"
                >
                  {{ t('app.cancel') }}
                </v-btn>
                <span class="text-caption text-medium-emphasis">{{
                  t('phpIni.emptyRemoves')
                }}</span>
              </div>

              <div v-if="phpIni.effective" class="text-caption text-medium-emphasis mt-2">
                {{ t('phpIni.measured') }}
              </div>

              <!-- What is true after a save, which is not the same as saved. PHP
                   reads its ini at process start, so a bind-mounted edit is on
                   disk and not yet in the process. -->
              <v-alert v-if="phpIni.needsRecreate" type="warning" variant="tonal" class="mt-4">
                <div class="text-caption">{{ t('phpIni.needsRecreate') }}</div>
              </v-alert>
              <v-alert
                v-else-if="phpIni.exists && phpIni.running"
                type="info"
                variant="tonal"
                class="mt-4"
              >
                <div class="text-caption">{{ t('phpIni.needsRestart') }}</div>
              </v-alert>

              <!-- Directives the form does not manage, shown because they are
                   preserved on every write and a form that hid them would look
                   like it had lost them. -->
              <template v-if="Object.keys(phpIni.unmanaged).length">
                <div class="section-head mt-5 mb-2">
                  <v-icon size="18" class="mr-2">mdi-file-document-edit-outline</v-icon>
                  {{ t('phpIni.unmanaged') }}
                </div>
                <v-table density="compact">
                  <tbody>
                    <tr v-for="(value, key) in phpIni.unmanaged" :key="key">
                      <td class="text-medium-emphasis mono">{{ key }}</td>
                      <td class="mono">{{ value }}</td>
                    </tr>
                  </tbody>
                </v-table>
              </template>

              <v-table density="compact" class="mt-5">
                <tbody>
                  <tr>
                    <td class="text-medium-emphasis">{{ t('phpIni.file') }}</td>
                    <td class="mono">{{ phpIni.path }}</td>
                  </tr>
                  <tr>
                    <td class="text-medium-emphasis">{{ t('phpIni.mountedAt') }}</td>
                    <td class="mono">{{ phpIni.containerPath }}</td>
                  </tr>
                </tbody>
              </v-table>

              <div class="text-caption text-medium-emphasis mt-3">{{ t('phpIni.cliCaveat') }}</div>
            </template>
          </v-card>
        </template>

        <!-- CONTAINER ----------------------------------------------------- -->
        <template v-if="shows('container')">
          <v-card variant="flat" class="pane">
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
          </v-card>
        </template>
        <!-- SHARE ---------------------------------------------------------- -->
        <template v-if="shows('container')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-earth</v-icon>{{ t('tunnel.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('tunnel.explain') }}</p>

            <!-- The tunnel forwards to the container; a stopped container would
                 serve 502s from a URL that looks like it worked. -->
            <v-alert v-if="!running" type="info" variant="tonal">
              <div class="text-caption">{{ t('tunnel.needsRunning') }}</div>
            </v-alert>

            <template v-else-if="tunnel?.running">
              <v-alert v-if="tunnel.url" type="success" variant="tonal" class="mb-3">
                <div class="d-flex align-center ga-2 flex-wrap">
                  <a class="field-link" @click="api.openInBrowser(tunnel.url)">{{ tunnel.url }}</a>
                  <v-btn
                    icon
                    :aria-label="t('a11y.copy')"
                    size="x-small"
                    variant="text"
                    @click="copy(tunnel.url, 'tunnel')"
                  >
                    <v-icon>{{ copied === 'tunnel' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
                    <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
                  </v-btn>
                  <v-spacer />
                  <v-btn
                    size="small"
                    color="error"
                    variant="tonal"
                    :loading="tunnelBusy"
                    @click="stopTunnel"
                  >
                    {{ t('tunnel.stop') }}
                  </v-btn>
                </div>
              </v-alert>
              <div v-else class="d-flex align-center ga-3">
                <v-progress-circular indeterminate size="18" width="2" color="primary" />
                <span class="text-caption text-medium-emphasis">{{ t('tunnel.connecting') }}</span>
                <v-spacer />
                <v-btn
                  size="small"
                  color="error"
                  variant="tonal"
                  :loading="tunnelBusy"
                  @click="stopTunnel"
                >
                  {{ t('tunnel.stop') }}
                </v-btn>
              </div>

              <!-- Said before anyone pastes the URL into a public issue: the
                   link is live, unauthenticated, and reaches this machine. -->
              <v-alert type="warning" variant="tonal" class="mt-3">
                <div class="text-caption">{{ t('tunnel.publicWarning') }}</div>
              </v-alert>
            </template>

            <template v-else>
              <v-btn
                color="primary"
                variant="flat"
                prepend-icon="mdi-earth"
                :loading="tunnelBusy"
                @click="startTunnel"
              >
                {{ t('tunnel.start') }}
              </v-btn>
              <div class="text-caption text-medium-emphasis mt-3">
                {{ t('tunnel.startHint') }}
              </div>
            </template>
          </v-card>
        </template>

        <!-- WORKERS --------------------------------------------------------- -->
        <template v-if="shows('container')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-cog-sync-outline</v-icon>{{ t('workers.title') }}
            </div>
            <p class="text-caption text-medium-emphasis mb-4">{{ t('workers.explain') }}</p>

            <v-alert v-if="!workerKinds.length" type="info" variant="tonal">
              <div class="text-caption">{{ t('workers.none') }}</div>
            </v-alert>

            <v-alert v-else-if="!running" type="info" variant="tonal" class="mb-3">
              <div class="text-caption">{{ t('workers.needsRunning') }}</div>
            </v-alert>

            <div v-for="kind in workerKinds" :key="kind" class="worker-row">
              <v-icon :color="workerFor(kind)?.running ? 'success' : 'grey'" size="18">
                {{ workerFor(kind)?.running ? 'mdi-check-circle' : 'mdi-stop-circle-outline' }}
              </v-icon>
              <div class="min-w-0">
                <span class="text-body-2 font-weight-medium">{{ t(`workers.${kind}`) }}</span>
                <div class="text-caption text-medium-emphasis">
                  {{ t(`workers.${kind}Desc`) }}
                </div>
                <!-- The healing made visible: 0 is healthy, a big number is a
                     crash loop wearing a green chip. -->
                <div
                  v-if="workerFor(kind)?.restarts"
                  class="text-caption"
                  :class="workerFor(kind).restarts > 3 ? 'text-error' : 'text-warning'"
                >
                  {{ t('workers.restarts', { count: workerFor(kind).restarts }) }}
                </div>
              </div>
              <v-spacer />
              <v-btn
                size="small"
                :color="workerFor(kind) ? 'error' : 'primary'"
                variant="tonal"
                :loading="workerBusy === kind"
                :disabled="!workerFor(kind) && !running"
                @click="toggleWorker(kind)"
              >
                {{ workerFor(kind) ? t('workers.stop') : t('workers.start') }}
              </v-btn>
            </div>
          </v-card>
        </template>

        <!-- MANIFEST ------------------------------------------------------ -->
        <template v-if="shows('configuration')">
          <v-card variant="flat" class="pane">
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
          </v-card>
        </template>

        <!-- LOGS ------------------------------------------------------------ -->
        <template v-if="shows('logs')">
          <!-- `project` is what unlocks the file sources: the container stream
               carries stdout, and nothing an application logs goes there. -->
          <LogView
            v-if="project.built"
            :container="project.containerName"
            :project="name"
            :active="section === 'logs'"
          />
          <div v-else class="text-caption text-medium-emphasis py-8 text-center">
            {{ t('detail.notBuilt') }}
          </div>
        </template>

        <!-- DOCKERFILE ---------------------------------------------------- -->
        <template v-if="shows('configuration')">
          <v-card variant="flat" class="pane">
            <div class="section-head mb-1">
              <v-icon size="18" class="mr-2">mdi-file-document-outline</v-icon>
              {{ t('detail.dockerfile') }}
            </div>
            <div class="text-caption text-medium-emphasis mb-3">
              {{ t('detail.dockerfileDesc') }}
            </div>

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
          </v-card>
        </template>
      </div>

      <!-- Section navigation ---------------------------------------------- -->
      <div class="detail-nav">
        <v-list nav class="bg-transparent">
          <template v-for="s in sections" :key="s.key">
            <v-divider v-if="s.divide" class="my-2" />
            <v-list-item
              :prepend-icon="s.icon"
              :title="t(s.label)"
              :active="section === s.key"
              class="nav-item"
              @click="section = s.key"
            >
              <!-- Enabled but not doing anything: a breakpoint that never
                   fires looks like an IDE fault, and nothing else on screen
                   would say otherwise. -->
              <template v-if="s.key === 'xdebug' && xdebugPending" #append>
                <v-icon
                  size="x-small"
                  color="warning"
                  icon="mdi-alert-circle"
                  :aria-label="t('xdebug.needsRebuild')"
                />
              </template>
            </v-list-item>
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

    <!-- Mounted only while open: the sheet reads the manifest when it opens, and
         one that lived in the DOM all along would hold whatever it read the
         first time this page was visited. -->
    <ProjectSettingsSheet
      v-if="showSettings"
      v-model="showSettings"
      :name="name"
      @saved="load"
      @apply="applyManifest"
    />
  </PageLayout>
</template>

<style scoped>
/* The card Settings uses, for the same reason it uses it: several groups now
   share one tab, and without a surface between them three subjects run
   together under one heading. Filled rather than outlined — the fill does the
   separating and the hairline only has to close the shape, which is what keeps
   a dark surface from reading as a white box around every group. */
.pane {
  background: rgba(var(--v-theme-surface-bright), 0.55);
  border: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
  padding: 16px;
}

.pane + .pane {
  margin-top: 16px;
}

/* A menu, not a page. Its width was whatever the longest description came to,
   which put a 1000px panel over the window; the text wraps inside a fixed
   width instead. */
.cmd-menu {
  width: 340px;
}

.cmd-menu :deep(.v-list-item-subtitle) {
  white-space: normal;
  line-height: 1.35;
}

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

.worker-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 10px 0;
}

.worker-row + .worker-row {
  border-top: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

/* Symfony renders the dump with its own alignment and box drawing, so this has
   to be monospaced and must not reflow — a wrapped `array:2 [` tree is unreadable
   in a way that a scrollbar is not. */

/* One command per row, the same shape as the worker rows above it. */
.cmd-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
}

.cmd-row + .cmd-row {
  border-top: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

/* A long `php artisan …` must not push the button off the row. */
.min-width-0 {
  min-width: 0;
}

/* Config for the user to paste into their own repository. Selectable, and it
   keeps its own line breaks — this is code, and a reflowed `hmr: { … }` block
   is code somebody has to repair before it runs. */
.snippet {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.55;
  margin: 0;
  padding: 10px 12px;
  border-radius: 6px;
  overflow-x: auto;
  background: rgb(var(--v-theme-surface-bright));
  user-select: text;
}
</style>
