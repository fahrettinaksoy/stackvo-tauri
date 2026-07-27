<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useTheme } from 'vuetify';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useAppStore } from '@/stores/app';
import { useMetricsStore } from '@/stores/metrics';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { setLocale } from '@/i18n';
import { api } from '@/lib/ipc';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import OperationConsole from '@/components/OperationConsole.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import TerminalPanel from '@/components/TerminalPanel.vue';
import NewProjectDialog from '@/components/NewProjectDialog.vue';
import CloseDialog from '@/components/CloseDialog.vue';

const app = useAppStore();
const metrics = useMetricsStore();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const theme = useTheme();
const route = useRoute();
const router = useRouter();
const { t, locale } = useI18n();

/**
 * Which of the two left drawers is expanded, if either.
 *
 * One value rather than a boolean per drawer: the two are mutually exclusive,
 * and with two flags "both expanded" is a representable state that four
 * separate click handlers would each have to remember to prevent. Here it
 * cannot be expressed at all. Both start collapsed — the window opens on the
 * content, not on two sidebars.
 */
const expandedDrawer = ref(null);

const rail = computed(() => expandedDrawer.value !== 'nav');
const railProjects = computed(() => expandedDrawer.value !== 'projects');

/** Expand this drawer, collapsing the other; clicking the open one closes it. */
function toggleDrawer(which) {
  expandedDrawer.value = expandedDrawer.value === which ? null : which;
}
const projectSearch = ref('');
const stackError = ref(null);
const commandLoading = ref(false);

const terminalTarget = ref(null);
const showNewProject = ref(false);
const showCloseDialog = ref(false);

// Opened through the opener plugin rather than <a href>: a webview that
// navigates away from the app has no way back.
const SOCIAL = [
  { icon: 'mdi-youtube', title: 'YouTube', url: 'https://www.youtube.com/stackvo' },
  { icon: 'mdi-mastodon', title: 'Mastodon', url: 'https://fosstodon.org/@stackvo' },
  { icon: 'mdi-linkedin', title: 'LinkedIn', url: 'https://www.linkedin.com/company/stackvo' },
  { icon: 'mdi-reddit', title: 'Reddit', url: 'https://reddit.com/r/stackvo' },
  { icon: 'mdi-cloud', title: 'Bluesky', url: 'https://bsky.app/profile/stackvo' },
  { icon: 'mdi-twitter', title: 'Twitter/X', url: 'https://twitter.com/stackvo' },
  { icon: 'mdi-discord', title: 'Discord', url: 'https://discord.gg/stackvo' },
];

const LANGUAGES = [
  { value: 'tr', title: 'Türkçe' },
  { value: 'en', title: 'English' },
];

const NAV = [
  { key: 'dashboard', to: '/', icon: 'mdi-view-dashboard-outline', label: 'nav.dashboard' },
  { key: 'projects', to: '/projects', icon: 'mdi-folder-multiple-outline', label: 'nav.projects' },
  { key: 'services', to: '/services', icon: 'mdi-server-outline', label: 'nav.services' },
  { key: 'settings', to: '/settings', icon: 'mdi-cog-outline', label: 'nav.settings' },
];

const isDark = computed(() => theme.global.current.value.dark);

const filteredProjects = computed(() => {
  const needle = projectSearch.value?.trim().toLowerCase() ?? '';
  if (!needle) return inventory.projects;
  return inventory.projects.filter(
    (p) => p.name.toLowerCase().includes(needle) || (p.domain || '').toLowerCase().includes(needle)
  );
});

const containerCount = computed(
  () => inventory.runningProjects.length + inventory.runningServices.length
);

function toggleTheme() {
  theme.global.name.value = isDark.value ? 'light' : 'dark';
}

async function stackAction(fn) {
  stackError.value = null;
  commandLoading.value = true;
  try {
    await fn();
  } catch (e) {
    stackError.value = e;
  } finally {
    commandLoading.value = false;
  }
}

async function projectAction(name, fn) {
  stackError.value = null;
  ops.markBusy(name, true);
  try {
    await fn(name);
  } catch (e) {
    stackError.value = e;
    ops.markBusy(name, false);
  }
}

async function chooseWorkspace() {
  const result = await api.workspacePick();
  if (result) app.workspace = result;
}

let enginePoll = null;
let teardown = null;
let offClose = null;

onMounted(async () => {
  await app.boot();
  await ops.bind();
  metrics.start();
  if (app.hasWorkspace) inventory.loadAll();

  teardown = await listenAll(REFRESH_TRIGGERS, () => inventory.loadAll());

  // Rust prevents the close and hands the decision here when the preference is
  // "ask"; every other value is applied natively without a round trip.
  offClose = await listenAll(['app:close_requested'], () => {
    showCloseDialog.value = true;
  });

  enginePoll = setInterval(() => {
    if (document.visibilityState === 'visible') app.refreshEngine();
  }, 5000);
});

onUnmounted(() => {
  metrics.stop();
  ops.unbind();
  teardown?.();
  offClose?.();
  if (enginePoll) clearInterval(enginePoll);
});
</script>

<template>
  <v-app>
    <!-- App bar ---------------------------------------------------------- -->
    <v-app-bar color="primary" elevation="3">
      <v-toolbar-title class="text-h4">
        <span class="font-weight-bold">Stack</span><span class="font-weight-light">Vo</span>
      </v-toolbar-title>

      <v-defaults-provider :defaults="{ VBtn: { variant: 'text', density: 'comfortable' } }">
        <v-btn
          icon
          :title="t('app.documentation')"
          @click="openUrl('https://stackvo.github.io/stackvo')"
        >
          <v-icon>mdi-book-open-variant</v-icon>
        </v-btn>
        <v-btn icon title="GitHub" @click="openUrl('https://github.com/stackvo/stackvo')">
          <v-icon>mdi-github</v-icon>
        </v-btn>
        <v-btn
          icon
          :title="t('app.buyMeCoffee')"
          @click="openUrl('https://buymeacoffee.com/fahrettinaksoy')"
        >
          <v-icon>mdi-coffee</v-icon>
        </v-btn>

        <v-menu>
          <template #activator="{ props }">
            <v-btn icon v-bind="props" :title="t('app.socialMedia')">
              <v-icon>mdi-share-variant</v-icon>
            </v-btn>
          </template>
          <v-list>
            <v-list-item v-for="s in SOCIAL" :key="s.title" @click="openUrl(s.url)">
              <template #prepend
                ><v-icon>{{ s.icon }}</v-icon></template
              >
              <v-list-item-title>{{ s.title }}</v-list-item-title>
            </v-list-item>
          </v-list>
        </v-menu>

        <v-divider vertical class="mx-2" />

        <v-menu>
          <template #activator="{ props }">
            <v-btn icon v-bind="props" :title="t('app.language')">
              <v-icon>mdi-translate</v-icon>
            </v-btn>
          </template>
          <v-list density="compact">
            <v-list-item
              v-for="lang in LANGUAGES"
              :key="lang.value"
              :active="locale === lang.value"
              @click="setLocale(lang.value)"
            >
              <v-list-item-title>{{ lang.title }}</v-list-item-title>
            </v-list-item>
          </v-list>
        </v-menu>

        <v-btn icon :title="t('app.toggleTheme')" @click="toggleTheme">
          <v-icon>{{ isDark ? 'mdi-weather-sunny' : 'mdi-weather-night' }}</v-icon>
        </v-btn>
      </v-defaults-provider>
    </v-app-bar>

    <!-- Primary navigation ----------------------------------------------- -->
    <v-navigation-drawer
      location="left"
      permanent
      :rail="rail"
      rail-width="64"
      width="220"
      class="nav-drawer border-0 elevation-6"
      @click="toggleDrawer('nav')"
    >
      <v-list nav density="comfortable" class="nav-list">
        <v-list-item
          v-for="item in NAV"
          :key="item.key"
          rounded="lg"
          color="primary"
          :prepend-icon="item.icon"
          :title="t(item.label)"
          :active="route.path === item.to"
          @click.stop="router.push(item.to)"
        />
      </v-list>

      <!-- Everything below the destinations lives in the drawer's append slot,
           so it sits on the floor of the drawer rather than trailing whatever
           the list above happens to end at. The append region is also outside
           the scroll area, which is what these are: fixed chrome, not content.

           This is what the `<v-spacer />` that used to be here was reaching for
           and never achieved — `.nav-drawer` has no rule making the content a
           flex column, so there was nothing for it to grow in. -->
      <template #append>
        <!-- The engine row is the one the web UI could not show honestly: a
             container-hosted dashboard needs Docker up to render at all. -->
        <template v-if="!rail">
          <div class="status-panel mx-3 mb-2">
            <div class="status-row">
              <span class="status-dot" :class="app.engineUp ? 'is-up' : 'is-down'" />
              <span class="status-key">{{ t('system.docker') }}</span>
              <span class="status-val" :class="app.engineUp ? 'text-success' : 'text-error'">
                {{ app.engineUp ? t('system.running') : t('system.stopped') }}
              </span>
            </div>
            <div class="status-divider" />
            <div class="status-row">
              <v-icon size="15" class="status-ic">mdi-cube-outline</v-icon>
              <span class="status-key">{{ t('system.containers') }}</span>
              <span class="status-val">{{ containerCount }}</span>
            </div>
          </div>
        </template>

        <div v-else class="rail-status">
          <v-tooltip
            :text="`${t('system.docker')} · ${app.engineUp ? t('system.running') : t('system.stopped')}`"
            location="end"
          >
            <template #activator="{ props }">
              <div v-bind="props" class="rail-stat">
                <v-icon size="20" :color="app.engineUp ? 'success' : 'error'">mdi-docker</v-icon>
                <span class="rail-stat-dot" :class="app.engineUp ? 'is-up' : 'is-down'" />
              </div>
            </template>
          </v-tooltip>

          <v-tooltip :text="`${t('system.containers')}: ${containerCount}`" location="end">
            <template #activator="{ props }">
              <div v-bind="props" class="rail-stat">
                <v-badge :content="containerCount" color="info" offset-x="-2" offset-y="-2">
                  <v-icon size="20" class="text-medium-emphasis">mdi-cube-outline</v-icon>
                </v-badge>
              </div>
            </template>
          </v-tooltip>
        </div>

        <v-divider class="mx-3 mb-1" />

        <v-list nav density="compact" class="pb-1">
          <v-list-item
            rounded="lg"
            :title="t('quickActions.startAll')"
            :disabled="commandLoading || !app.engineUp"
            @click.stop="stackAction(() => api.containersStartAll())"
          >
            <template #prepend
              ><v-icon class="text-success">mdi-play-circle-outline</v-icon></template
            >
            <template v-if="commandLoading" #append>
              <v-progress-circular indeterminate size="18" width="2" />
            </template>
          </v-list-item>
          <v-list-item
            rounded="lg"
            :title="t('quickActions.stopAll')"
            :disabled="commandLoading || !app.engineUp"
            @click.stop="stackAction(() => api.containersStopAll())"
          >
            <template #prepend
              ><v-icon class="text-error">mdi-stop-circle-outline</v-icon></template
            >
          </v-list-item>
          <v-list-item
            rounded="lg"
            :title="t('quickActions.restart')"
            :disabled="commandLoading || !app.engineUp"
            @click.stop="stackAction(() => api.containersRestartAll())"
          >
            <template #prepend><v-icon class="text-warning">mdi-restart</v-icon></template>
          </v-list-item>
        </v-list>

        <v-divider />
        <v-list nav density="compact">
          <v-list-item
            rounded="lg"
            :prepend-icon="rail ? 'mdi-chevron-right' : 'mdi-chevron-left'"
            :title="rail ? t('nav.expand') : t('nav.collapse')"
            @click.stop="toggleDrawer('nav')"
          />
        </v-list>
      </template>
    </v-navigation-drawer>

    <!-- Projects rail ----------------------------------------------------- -->
    <v-navigation-drawer
      location="left"
      permanent
      :rail="railProjects"
      rail-width="66"
      width="340"
      class="elevation-6 border-0"
      @click="toggleDrawer('projects')"
    >
      <div v-if="!railProjects" class="px-3 pt-3 pb-2" @click.stop>
        <div class="d-flex align-center mb-3">
          <v-icon size="20" class="mr-2">mdi-folder-multiple</v-icon>
          <span class="text-subtitle-2 font-weight-bold">{{ t('projects.title') }}</span>
          <v-spacer />
          <v-chip size="x-small" variant="tonal" color="success" label>
            {{ inventory.runningProjects.length }} / {{ inventory.projects.length }}
          </v-chip>
        </div>
        <v-text-field
          v-model="projectSearch"
          density="compact"
          flat
          variant="plain"
          rounded="0"
          hide-details
          single-line
          clearable
          :placeholder="t('projects.searchPlaceholder')"
          prepend-inner-icon="mdi-magnify"
        />
      </div>

      <div v-else class="d-flex justify-center pt-3 pb-2">
        <v-icon>mdi-folder-multiple</v-icon>
      </div>

      <v-divider />

      <v-list nav density="compact" class="projects-scroll">
        <div
          v-if="inventory.loadingProjects && !inventory.projects.length"
          class="pa-6 text-center text-medium-emphasis"
        >
          <v-progress-circular indeterminate size="22" />
        </div>

        <div v-else-if="!filteredProjects.length" class="pa-6 text-center text-medium-emphasis">
          <v-icon size="30" class="mb-1">mdi-folder-off-outline</v-icon>
          <div class="text-caption">{{ t('projects.empty') }}</div>
        </div>

        <v-list-item
          v-for="project in filteredProjects"
          :key="project.name"
          rounded="0"
          :active="route.path === `/projects/${project.name}`"
          @click.stop="router.push(`/projects/${project.name}`)"
        >
          <template #prepend>
            <v-icon
              size="22"
              :color="project.running ? 'success' : ''"
              :class="{ 'project-icon--stopped': !project.running }"
              >{{ project.runtime === 'node' ? 'mdi-nodejs' : 'mdi-language-php' }}</v-icon
            >
          </template>

          <v-list-item-title class="text-body-2 font-weight-medium">
            {{ project.domain || project.name }}
          </v-list-item-title>

          <template #append>
            <!-- A broken manifest is visible right here rather than only in the
                 list view; the Bash generator drops such projects silently. -->
            <v-icon v-if="!project.manifestValid" size="16" color="error" class="mr-1">
              mdi-file-alert-outline
            </v-icon>

            <v-menu>
              <template #activator="{ props }">
                <v-btn
                  icon="mdi-dots-vertical"
                  variant="text"
                  size="small"
                  :aria-label="t('a11y.moreActions')"
                  v-bind="props"
                  @click.stop
                />
              </template>
              <v-list density="compact" min-width="240">
                <v-list-item
                  prepend-icon="mdi-open-in-app"
                  :title="t('projects.openDetail')"
                  @click.stop="router.push(`/projects/${project.name}`)"
                />
                <v-divider class="my-1" />
                <v-list-item
                  v-if="!project.built"
                  prepend-icon="mdi-hammer-wrench"
                  :title="t('actions.build')"
                  base-color="info"
                  :disabled="ops.isBusy(project.name) || !app.engineUp"
                  @click.stop="projectAction(project.name, (n) => api.projectBuild(n))"
                />
                <v-list-item
                  v-else-if="project.running"
                  prepend-icon="mdi-stop"
                  :title="t('actions.stop')"
                  base-color="error"
                  :disabled="ops.isBusy(project.name)"
                  @click.stop="projectAction(project.name, api.projectStop)"
                />
                <v-list-item
                  v-else
                  prepend-icon="mdi-play"
                  :title="t('actions.start')"
                  base-color="success"
                  :disabled="ops.isBusy(project.name)"
                  @click.stop="projectAction(project.name, api.projectStart)"
                />
                <v-list-item
                  v-if="project.running"
                  prepend-icon="mdi-restart"
                  :title="t('actions.restart')"
                  base-color="warning"
                  :disabled="ops.isBusy(project.name)"
                  @click.stop="projectAction(project.name, api.projectRestart)"
                />
                <v-list-item
                  v-if="project.running"
                  prepend-icon="mdi-console"
                  :title="t('projects.terminal')"
                  @click.stop="terminalTarget = { kind: 'container', name: project.containerName }"
                />
                <!-- Only offered when the domain resolves: opening a browser at
                     a host with no /etc/hosts entry just shows an error page. -->
                <v-list-item
                  v-if="project.domain && project.running && project.domainConfigured"
                  prepend-icon="mdi-open-in-new"
                  :title="t('projects.openSite')"
                  base-color="primary"
                  @click.stop="openUrl(`https://${project.domain}`)"
                />
              </v-list>
            </v-menu>
          </template>
        </v-list-item>
      </v-list>

      <template #append>
        <v-divider />
        <v-list nav density="compact">
          <v-list-item
            rounded="lg"
            prepend-icon="mdi-plus"
            :title="railProjects ? undefined : t('newProject.title')"
            :disabled="!app.hasWorkspace"
            @click.stop="showNewProject = true"
          />
          <v-list-item
            rounded="lg"
            :prepend-icon="railProjects ? 'mdi-chevron-right' : 'mdi-chevron-left'"
            :title="railProjects ? undefined : t('nav.collapse')"
            @click.stop="toggleDrawer('projects')"
          />
        </v-list>
      </template>
    </v-navigation-drawer>

    <!-- Content ----------------------------------------------------------- -->
    <v-main>
      <div v-if="app.booting" class="d-flex justify-center py-16">
        <v-progress-circular indeterminate color="primary" />
      </div>

      <!-- The desktop-only state: the web UI was mounted inside the repo it
           managed, so it could never need to ask where that repo is. -->
      <v-container v-else-if="!app.hasWorkspace" class="pa-6">
        <v-card max-width="620" class="mx-auto mt-8">
          <v-card-item>
            <template #prepend>
              <v-icon size="32" color="warning">mdi-folder-search-outline</v-icon>
            </template>
            <v-card-title>{{ t('workspace.title') }}</v-card-title>
            <v-card-subtitle>{{ t('workspace.none') }}</v-card-subtitle>
          </v-card-item>
          <v-card-text>
            <p class="text-body-2 text-medium-emphasis mb-4">{{ t('workspace.prompt') }}</p>
            <ErrorAlert :error="app.error" type="error" class="mb-4" />
            <v-btn color="primary" prepend-icon="mdi-folder-open" @click="chooseWorkspace">
              {{ t('workspace.choose') }}
            </v-btn>
          </v-card-text>
        </v-card>
      </v-container>

      <router-view v-else />
    </v-main>

    <!-- Global overlays --------------------------------------------------- -->
    <v-snackbar :model-value="!!stackError" color="transparent" location="bottom" timeout="8000">
      <ErrorAlert :error="stackError" type="error" closable @close="stackError = null" />
    </v-snackbar>

    <OperationConsole />

    <TerminalPanel
      v-if="terminalTarget"
      :target="terminalTarget"
      :model-value="!!terminalTarget"
      @update:model-value="terminalTarget = $event ? terminalTarget : null"
    />

    <CloseDialog v-model="showCloseDialog" />

    <NewProjectDialog v-model="showNewProject" @created="inventory.loadProjects()" />
  </v-app>
</template>

<style scoped>
.nav-list {
  padding-top: 2px;
}

.status-panel {
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 8px;
  padding: 8px 10px;
}

.status-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.75rem;
}

.status-key {
  opacity: 0.7;
}

.status-val {
  margin-left: auto;
  font-weight: 600;
}

.status-divider {
  height: 1px;
  margin: 6px 0;
  background: rgba(var(--v-border-color), var(--v-border-opacity));
}

.status-dot,
.rail-stat-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}

.status-dot.is-up,
.rail-stat-dot.is-up {
  background: rgb(var(--v-theme-success));
}

.status-dot.is-down,
.rail-stat-dot.is-down {
  background: rgb(var(--v-theme-error));
}

.status-ic {
  opacity: 0.6;
}

.rail-status {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 10px 0;
}

.rail-stat {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.rail-stat-dot {
  position: absolute;
  right: -2px;
  bottom: -2px;
}

.projects-scroll {
  flex: 1 1 auto;
  overflow-y: auto;
}

/* A stopped project stays legible but recedes, so running ones read first. */
.project-icon--stopped {
  opacity: 0.45;
}
</style>
