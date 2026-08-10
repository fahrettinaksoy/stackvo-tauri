<script setup>
import { computed, onMounted, onUnmounted, ref, toRef, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useContainerStats } from '@/composables/useContainerStats';
import { useCopyTick } from '@/composables/useCopyTick';
import { useXdebug } from '@/composables/useXdebug';
import IndicatorPane from '@/components/project/IndicatorPane.vue';
import ContainerPane from '@/components/project/ContainerPane.vue';
import DockerfilePane from '@/components/project/DockerfilePane.vue';
import DumpsPane from '@/components/project/DumpsPane.vue';
import DevServerPane from '@/components/project/DevServerPane.vue';
import LogsPane from '@/components/project/LogsPane.vue';
import PhpIniPane from '@/components/project/PhpIniPane.vue';
import ManifestPane from '@/components/project/ManifestPane.vue';
import RequirementsPane from '@/components/project/RequirementsPane.vue';
import OverviewPane from '@/components/project/OverviewPane.vue';
import ProfilerPane from '@/components/project/ProfilerPane.vue';
import TunnelPane from '@/components/project/TunnelPane.vue';
import WorkersPane from '@/components/project/WorkersPane.vue';
import TerminalPane from '@/components/project/TerminalPane.vue';
import XdebugPane from '@/components/project/XdebugPane.vue';
import ReleasePane from '@/components/project/ReleasePane.vue';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { api, asList } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import HostsDialog from '@/components/HostsDialog.vue';
import ProjectSettingsSheet from '@/components/ProjectSettingsSheet.vue';

const props = defineProps({ name: { type: String, required: true } });

const { t } = useI18n();
const router = useRouter();
const ops = useOperationsStore();
const app = useAppStore();

const project = ref(null);
const details = ref(null);

// The snackbar below the page confirms a copy made in any of the panes — see
// `useCopyTick`, which is shared for exactly this reason.
const { copied } = useCopyTick();

/**
 * The rail badges Xdebug as pending — enabled but not yet doing anything — and
 * the rail is on screen whether or not the Debug section is open, so the view
 * reads the status for itself rather than waiting for a pane that may never
 * mount. `XdebugPane` keeps its own; the two are cheap and neither owns the
 * other's lifetime.
 */
const { pending: xdebugPending, load: loadXdebugBadge } = useXdebug(toRef(props, 'name'));
/**
 * Live resource use and its two hours of history.
 *
 * The timer lives here rather than in the pane: it has to start and stop with
 * the container, and a pane that polled on its own mount would keep a stopped
 * container's chart moving.
 */
const {
  stats,
  cpuSeries,
  memoryPie,
  networkPie,
  diskPie,
  heatmap,
  loadHistory,
  start: startStats,
  stop: stopStats,
} = useContainerStats(t);

const error = ref(null);
const loading = ref(true);

const showHostsFix = ref(false);
const showSettings = ref(false);

const manifestText = ref('');
const manifestDirty = ref(false);
const manifestSaving = ref(false);

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

const running = computed(() => !!project.value?.running);

/** Both URLs are shown; Traefik serves the project on HTTPS and redirects HTTP. */
const httpsUrl = computed(() => (project.value?.domain ? `https://${project.value.domain}` : null));

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
 * Bring up the services this project declares, after they were switched on.
 *
 * `custom` with the declared ids rather than the `services` profile, which
 * would start everything the workspace has enabled. A project asked for its
 * own list and starting somebody else's Kafka alongside it is not what the
 * button said.
 *
 * Regenerate first for the reason `applyManifest` gives: the compose files were
 * rendered from a `.env` that did not have these services in it, so bringing
 * them up without regenerating starts nothing and reports success.
 */
async function startRequired(services) {
  if (!services.length) return;
  await act(async () => {
    await api.generateRun('all');
    await api.composeUp('custom', services);
  });
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

// Navigating from a PHP project's Xdebug pane straight to a node project keeps
// the component and the selected section, which would leave the page on a pane
// the rail no longer offers — an empty panel with no way back to it.
watch(sections, (available) => {
  if (!available.some((s) => s.key === section.value)) section.value = 'indicator';
});

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

/**
 * Read the manifest fresh rather than trusting the list payload: the file may
 * have changed on disk since the inventory was loaded — and the Xdebug pane
 * rewrites it, which is what it reports as `changed`.
 */
async function reloadManifest() {
  try {
    const m = await api.projectManifestRead(props.name);
    manifestText.value = JSON.stringify(stripDiagnostics(m), null, 2);
    manifestDirty.value = false;
  } catch {
    manifestText.value = '';
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

  // An untyped boundary can answer `null` without rejecting, and everything
  // below reads fields off this. Treated as "no such project" — the same state
  // a rejection produces — rather than left to throw out of an async function
  // nobody awaits, which is an unhandled rejection and a blank page.
  if (!project.value) {
    loading.value = false;
    return;
  }

  await reloadManifest();

  await loadXdebugBadge(project.value.runtime);

  const container = project.value.containerName;

  // A container that was never built has no inspect data; that is a state to
  // render, not an error to shout about.
  try {
    details.value = await api.containerInspect(container);
  } catch (e) {
    details.value = null;
    if (e.code && e.code !== 'NOT_FOUND') error.value = e;
  }

  // Best effort: a project with no recorded history is the ordinary first
  // visit, not a failure worth reporting on the page.
  await loadHistory(container).catch(() => {});
  loadQuickCommands();

  loading.value = false;
  startStats(project.value?.containerName, running.value);
}

/**
 * A deployable image, built from the one this project already runs.
 *
 * Reviewed before it is built, like the hosts file and the certificate — a
 * production image is the one thing here that leaves the machine. And verified
 * after: the built image is run and asked whether it leaked an `.env`, because
 * that guarantee is easy to state in a Dockerfile and quietly wrong in the
 * result.
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
    quickCommands.value = asList(await api.quickCommands(props.name));
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

watch(() => props.name, load);
onMounted(load);
onUnmounted(() => {
  stopStats();
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
          <IndicatorPane
            :stats="stats"
            :running="running"
            :cpu-series="cpuSeries"
            :memory-pie="memoryPie"
            :network-pie="networkPie"
            :disk-pie="diskPie"
            :heatmap="heatmap"
          />
        </template>

        <!-- CONFIGURATION ------------------------------------------------ -->
        <template v-if="shows('configuration')">
          <OverviewPane
            :project="project"
            :details="details"
            @settings="showSettings = true"
            @fix-hosts="showHostsFix = true"
          />
        </template>

        <!-- XDEBUG --------------------------------------------------------- -->
        <!-- Three layers reported separately. Collapsing them into one "on"
             would put a switch in the UI that reads as done while nothing has
             been compiled in, which is worse than no switch. -->
        <template v-if="shows('debug')">
          <XdebugPane
            :name="name"
            :runtime="project?.runtime"
            :running="running"
            @changed="reloadManifest"
          />
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
          <DevServerPane :name="name" :runtime="project?.runtime" />
        </template>

        <!-- PROFILER ------------------------------------------------------- -->
        <!-- Xdebug's own profiler. Blackfire needs an account and SPX is not
             in the extension contract; this needs neither. -->
        <template v-if="shows('debug')">
          <ProfilerPane
            :name="name"
            :runtime="project?.runtime"
            :running="running"
            @apply="applyToContainer"
          />
        </template>

        <!-- DUMPS ---------------------------------------------------------- -->
        <!-- One renderer, two scopes: this pane and the Dumps page share
             `DumpView`, so search, the source link and the capture switch
             cannot drift between them. -->
        <template v-if="shows('debug')">
          <DumpsPane :name="name" @apply="applyToContainer" />
        </template>

        <!-- RELEASE -------------------------------------------------------- -->
        <!-- The dev image is not a production image: for PHP it holds no
             application code at all (the source is bind-mounted) and it carries
             Xdebug. So this is a build, and the result is checked rather than
             trusted. -->
        <template v-if="shows('release')">
          <ReleasePane :name="name" />
        </template>

        <!-- PHP.INI -------------------------------------------------------- -->
        <!-- Three states again, and for the same reason as Xdebug: the file on
             disk, the mount in the running container, and PHP having read it.
             They come apart in practice — the Bash CLI's `up` layers three
             compose files, not five, and recreates the container without the
             mount — so collapsing them would produce a form that saves happily
             and changes nothing. -->
        <template v-if="shows('runtime')">
          <PhpIniPane :name="name" :runtime="project?.runtime" />
        </template>

        <!-- CONTAINER ----------------------------------------------------- -->
        <template v-if="shows('container')">
          <ContainerPane :project="project" :details="details" :running="running" />
        </template>
        <!-- SHARE ---------------------------------------------------------- -->
        <template v-if="shows('container')">
          <TunnelPane :name="name" :running="running" />
        </template>

        <!-- WORKERS --------------------------------------------------------- -->
        <template v-if="shows('container')">
          <WorkersPane :name="name" :running="running" />
        </template>

        <!-- TERMINAL ------------------------------------------------------- -->
        <!-- Beside the container it attaches to, rather than on a page of its
             own: a shell is something you want *while* looking at the thing it
             runs in. The header still offers the system terminal for the other
             case. -->
        <template v-if="shows('container')">
          <TerminalPane :container-name="project?.containerName" :running="running" />
        </template>

        <!-- MANIFEST ------------------------------------------------------ -->
        <!-- What the project needs *around* it. Beside the manifest because
             that is the file it is written into, and above it because the
             answer to "why is this list here" is one scroll away. -->
        <template v-if="shows('configuration')">
          <RequirementsPane :name="name" @apply="startRequired" />
        </template>

        <template v-if="shows('configuration')">
          <ManifestPane
            v-model="manifestText"
            :name="name"
            :dirty="manifestDirty"
            :saving="manifestSaving"
            :project="project"
            @dirty="manifestDirty = true"
            @save="saveManifest"
            @bring-up="bringUp"
          />
        </template>

        <!-- LOGS ------------------------------------------------------------ -->
        <template v-if="shows('logs')">
          <LogsPane :project="project" :name="name" :active="section === 'logs'" />
        </template>

        <!-- DOCKERFILE ---------------------------------------------------- -->
        <template v-if="shows('configuration')">
          <DockerfilePane :name="name" />
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
                   would say otherwise.

                   Keyed `debug`, not `xdebug`. There has been no `xdebug`
                   section since Xdebug, the profiler and the dump catcher were
                   merged into one — so this badge could never render, and the
                   state it exists to warn about has been silent ever since. -->
              <template v-if="s.key === 'debug' && xdebugPending" #append>
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
</style>
