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
import { useAppearanceStore } from '@/stores/appearance';
import { setLocale } from '@/i18n';
import { api } from '@/lib/ipc';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import OperationConsole from '@/components/OperationConsole.vue';
import { toasts } from '@/lib/toast';
import ErrorAlert from '@/components/ErrorAlert.vue';
import RequirementsGate from '@/components/RequirementsGate.vue';
import NewProjectDrawer from '@/components/NewProjectDrawer.vue';
import CloseDialog from '@/components/CloseDialog.vue';

const app = useAppStore();
const metrics = useMetricsStore();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const appearance = useAppearanceStore();
const theme = useTheme();
const route = useRoute();

/**
 * Is this the About window rather than the main one?
 *
 * Read from the route, not from the window label: the label needs an async
 * Tauri call, and the shell would flash on screen for the frame before it
 * resolved. The route is known before the first render.
 */
const isAboutWindow = computed(() => route.name === 'About');
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
  {
    key: 'logs',
    to: '/logs',
    icon: 'mdi-text-box-multiple-outline',
    label: 'nav.logs',
  },
  { key: 'mail', to: '/mail', icon: 'mdi-email-outline', label: 'nav.mail' },
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

// Through the store rather than straight at Vuetify: the toolbar button and the
// settings page are the same setting, and a toggle that only changed the live
// theme was forgotten on the next launch.
function toggleTheme() {
  appearance.toggleTheme(isDark.value);
}

/* Which stack-wide action is in flight, so only the pressed button spins
   while all three stay disabled. */
const stackActionKey = ref(null);

async function stackAction(fn, key = null) {
  stackError.value = null;
  commandLoading.value = true;
  stackActionKey.value = key;
  try {
    await fn();
  } catch (e) {
    stackError.value = e;
  } finally {
    commandLoading.value = false;
    stackActionKey.value = null;
  }
}

/** The terminal chosen in Settings, on this project's container. */
async function openTerminal(project) {
  stackError.value = null;
  try {
    await api.terminalOpenExternal({ kind: 'container', name: project.containerName });
  } catch (e) {
    stackError.value = e;
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

function onFocus() {
  appearance.refreshSystemAccent();
}

let enginePoll = null;
let teardown = null;
let offClose = null;
let offTray = null;

onMounted(async () => {
  // Before anything else that paints: the saved theme, colours and type size
  // are what the first frame should already be wearing.
  await appearance.load();

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

  // A project picked from the tray. Routing is decided here rather than in
  // Rust: the route table and the guard that waits for a workspace both live
  // on this side, and a second answer to "can this page open yet" is how the
  // two come to disagree.
  offTray = await listenAll(['tray:open_project', 'tray:navigate'], (event, payload) => {
    if (!payload) return;
    if (event === 'tray:open_project') {
      router.push({ name: 'ProjectDetail', params: { name: payload } });
    } else {
      // The payload is the route's own name, so the tray never has to know
      // what path a page lives at — that is the router's business and it has
      // moved before.
      router.push({ name: payload }).catch(() => {});
    }
  });

  enginePoll = setInterval(() => {
    if (document.visibilityState === 'visible') app.refreshEngine();
  }, 5000);

  // The desktop accent can change while the app is open; the moment the window
  // comes back to the front is when a mismatch would be noticed.
  window.addEventListener('focus', onFocus);
});

onUnmounted(() => {
  window.removeEventListener('focus', onFocus);
  metrics.stop();
  ops.unbind();
  teardown?.();
  offClose?.();
  offTray?.();
  if (enginePoll) clearInterval(enginePoll);
});
</script>

<template>
  <!-- The about window loads the same document as the main one. Rendering the
       shell around it would put a navigation rail and a stack toolbar in a box
       whose whole job is to show a version number. -->
  <v-app v-if="isAboutWindow">
    <v-main>
      <router-view />
    </v-main>
  </v-app>

  <v-app v-else>
    <!-- App bar ---------------------------------------------------------- -->
    <v-app-bar color="primary" elevation="3">
      <v-toolbar-title class="text-h4 app-title">
        <span class="font-weight-bold">Stack</span><span class="font-weight-light">Vo</span>
      </v-toolbar-title>

      <v-defaults-provider :defaults="{ VBtn: { variant: 'text', density: 'comfortable' } }">
        <!-- Stack-wide actions, next to the identity they act on behalf of:
             these operate on everything, so they live in the global bar rather
             than the navigation drawer. -->
        <v-divider vertical class="mx-4 my-3"></v-divider>
        <v-btn
          icon="mdi-play-circle-outline"
          variant="elevated"
          elevation="0"
          color="success"
          class="mr-2"
          :title="t('quickActions.startAll')"
          :disabled="commandLoading || !app.engineUp"
          :loading="stackActionKey === 'start'"
          @click="stackAction(() => api.containersStartAll(), 'start')"
        />
        <v-btn
          icon="mdi-stop-circle-outline"
          variant="elevated"
          elevation="0"
          color="error"
          class="mr-2"
          :title="t('quickActions.stopAll')"
          :disabled="commandLoading || !app.engineUp"
          :loading="stackActionKey === 'stop'"
          @click="stackAction(() => api.containersStopAll(), 'stop')"
        />
        <v-btn
          icon="mdi-restart"
          variant="elevated"
          elevation="0"
          color="warning"
          :title="t('quickActions.restart')"
          :disabled="commandLoading || !app.engineUp"
          :loading="stackActionKey === 'restart'"
          @click="stackAction(() => api.containersRestartAll(), 'restart')"
        />

        <v-spacer />

        <v-btn
          icon
          variant="tonal"
          elevation="0"
          class="mr-2"
          :title="t('app.documentation')"
          @click="openUrl('https://stackvo.github.io/stackvo')"
        >
          <v-icon>mdi-book-open-variant</v-icon>
        </v-btn>
        <v-btn
          icon
          variant="tonal"
          elevation="0"
          class="mr-2"
          title="GitHub"
          @click="openUrl('https://github.com/stackvo/stackvo')"
        >
          <v-icon>mdi-github</v-icon>
        </v-btn>
        <v-btn
          icon
          variant="tonal"
          elevation="0"
          class="mr-2"
          :title="t('app.buyMeCoffee')"
          @click="openUrl('https://buymeacoffee.com/fahrettinaksoy')"
        >
          <v-icon>mdi-coffee</v-icon>
        </v-btn>

        <v-menu>
          <template #activator="{ props }">
            <v-btn icon variant="tonal" elevation="0" v-bind="props" :title="t('app.socialMedia')">
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

        <v-divider vertical class="mx-4 my-3"></v-divider>

        <v-menu location="bottom end">
          <template #activator="{ props }">
            <v-btn
              icon
              variant="tonal"
              elevation="0"
              class="mr-2"
              v-bind="props"
              :title="t('app.language')"
            >
              <v-icon>mdi-translate</v-icon>
            </v-btn>
          </template>
          <v-list density="compact" class="px-2 py-2">
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

        <v-btn
          icon
          variant="tonal"
          elevation="0"
          class="mr-5"
          :title="t('app.toggleTheme')"
          @click="toggleTheme"
        >
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
      width="180"
      class="nav-drawer border-0 elevation-6"
      @click="toggleDrawer('nav')"
    >
      <v-list nav class="nav-list mt-2">
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

        <v-divider />
        <v-list nav>
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
      width="330"
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

      <v-list nav class="projects-scroll">
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

        <!-- No `rounded="0"` here. That marks a surface running to an edge,
             where a radius cuts a notch and shows the background through it —
             but `v-list nav` insets its items by 8px on both sides, so these
             rows reach no edge and follow the corner setting like everything
             else. -->
        <v-list-item
          v-for="project in filteredProjects"
          :key="project.name"
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
              <v-list min-width="240">
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
                  :title="t('detail.externalTerminal')"
                  @click.stop="openTerminal(project)"
                />
                <!-- Only offered when the domain resolves: opening a browser at
                     a host with no /etc/hosts entry just shows an error page. -->
                <v-list-item
                  v-if="project.domain && project.running && project.domainConfigured"
                  prepend-icon="mdi-open-in-new"
                  :title="t('projects.openSite')"
                  base-color="primary"
                  @click.stop="api.openInBrowser(`https://${project.domain}`)"
                />
              </v-list>
            </v-menu>
          </template>
        </v-list-item>
      </v-list>

      <template #append>
        <v-divider />
        <v-list nav>
          <v-list-item
            rounded="lg"
            prepend-icon="mdi-plus"
            :title="railProjects ? undefined : t('newProject.title')"
            :disabled="!app.hasWorkspace"
            @click.stop="app.newProjectOpen = true"
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

      <!-- Every prerequisite, not just the desktop-only one.
           The web UI ran inside the checkout it managed and beside the daemon
           it drove, so it could assume both; a desktop app can assume neither,
           nor a compose plugin new enough for profiles, nor the network its own
           generator declares external. -->
      <RequirementsGate v-else-if="app.preflight && !app.preflight.ready" />

      <router-view v-else />
    </v-main>

    <!-- Global overlays --------------------------------------------------- -->
    <v-snackbar :model-value="!!stackError" color="transparent" location="bottom" timeout="8000">
      <ErrorAlert :error="stackError" type="error" closable @close="stackError = null" />
    </v-snackbar>

    <OperationConsole />

    <!-- The global toast queue: operations announce their outcome here in one
         line, while the console reserves itself for output worth reading —
         which, on success, there is none of. -->
    <v-snackbar-queue v-model="toasts" closable location="top right" :timeout="4000" />

    <CloseDialog v-model="showCloseDialog" />

    <NewProjectDrawer v-model="app.newProjectOpen" @created="inventory.loadProjects()" />
  </v-app>
</template>

<style scoped>
.nav-list {
  padding-top: 2px;
}

/* `v-toolbar-title` grows by default, which would push everything after it to
   the far side. The stack actions belong beside the logo, so the title keeps
   its natural width and an explicit spacer splits the bar instead. */
.app-title {
  flex: 0 0 auto;
}

.status-panel {
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: var(--app-radius);
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

/* A railed drawer shows only the icon, but the list item's grid keeps all
   three columns — and `content` is `1fr`, so the invisible title still claims
   the width and pins the icon to the left edge. Collapse the grid to the one
   visible column and center it. The append slot (row menus, spinners) is
   hidden outright: clipped halves of buttons are not chrome, they are noise.
   Overlay menus are teleported to the body and unaffected. */
.v-navigation-drawer--rail :deep(.v-list-item) {
  grid-template-columns: min-content;
  justify-content: center;
}

.v-navigation-drawer--rail :deep(.v-list-item .v-list-item__spacer),
.v-navigation-drawer--rail :deep(.v-list-item .v-list-item__content),
.v-navigation-drawer--rail :deep(.v-list-item .v-list-item__append) {
  display: none;
}
</style>
