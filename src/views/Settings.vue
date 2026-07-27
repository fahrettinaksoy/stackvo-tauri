<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useTheme } from 'vuetify';
import { openPath } from '@tauri-apps/plugin-opener';
import {
  enable as enableAutostart,
  disable as disableAutostart,
  isEnabled as autostartEnabled,
} from '@tauri-apps/plugin-autostart';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { bytes } from '@/lib/format';
import { setLocale } from '@/i18n';
import { checkForUpdate, updatesConfigured } from '@/lib/updates';
import { getVersion } from '@tauri-apps/api/app';
import ErrorAlert from '@/components/ErrorAlert.vue';
import PageLayout from '@/components/PageLayout.vue';

const { t, locale } = useI18n();
const app = useAppStore();
const theme = useTheme();

const env = ref({});
const envError = ref(null);
const search = ref('');
const edits = ref({});
const saving = ref(false);
const saved = ref(false);

const prefs = ref(null);
const generatorReport = ref(null);
const verifying = ref(false);
const engineMode = ref('bash');
const generateResult = ref(null);
const stackBusy = ref(false);

// Compose-level control lives here rather than in the sidebar: the sidebar's
// quick actions match the web UI exactly (start/stop/restart the containers
// that exist), while these regenerate and recreate them. `down` in particular
// could not exist in the web UI at all — stopping the stack would have stopped
// the container serving the dashboard.
async function stackAction(fn) {
  stackBusy.value = true;
  envError.value = null;
  try {
    await fn();
  } catch (e) {
    envError.value = e;
  } finally {
    stackBusy.value = false;
  }
}

const appVersion = ref('');
const update = ref(null);
const updateProgress = ref(null);
/** Null until asked; false means this build has no key to verify against. */
const updaterReady = ref(null);
const logs = ref(null);
const apps = ref({ terminals: [], editors: [] });

/**
 * The open tab, persisted for the session only.
 *
 * Deliberately not in preferences.json: which pane you last had open is not a
 * setting, and writing the config file on every tab click would be noise in a
 * file the user may be reading.
 */
const tab = ref('workspace');
const checkingUpdate = ref(false);

async function checkUpdate() {
  checkingUpdate.value = true;
  try {
    update.value = await checkForUpdate();
  } catch (e) {
    // A signature failure is a security event, not a network hiccup.
    envError.value = { code: 'PERMISSION_DENIED', message: e.message };
  } finally {
    checkingUpdate.value = false;
  }
}

async function installUpdate() {
  await update.value.install((p) => (updateProgress.value = p));
}

async function runGenerate() {
  verifying.value = true;
  generateResult.value = null;
  envError.value = null;
  try {
    generateResult.value = await api.generateWith('all', engineMode.value);
    await verifyGenerator();
  } catch (e) {
    envError.value = e;
  } finally {
    verifying.value = false;
  }
}
const autostart = ref(false);

const rows = computed(() => {
  const needle = search.value.trim().toUpperCase();
  return Object.entries(env.value)
    .filter(([key]) => !needle || key.includes(needle))
    .sort(([a], [b]) => a.localeCompare(b));
});

const dirty = computed(() => Object.keys(edits.value).length > 0);

/** Secret values are redacted on read; editing one would write back the mask. */
const isRedacted = (value) => value === '••••••••';

const engineRows = computed(() => {
  const e = app.engine;
  if (!e) return [];
  return [
    { label: t('engine.title'), value: e.reachable ? t('engine.running') : t('engine.down') },
    { label: 'Platform', value: t(`engine.platform.${e.platform}`) },
    { label: t('engine.version'), value: e.version || t('app.never') },
    { label: t('engine.apiVersion'), value: e.apiVersion || t('app.never') },
    { label: t('engine.context'), value: e.context || t('app.never') },
    { label: t('engine.socket'), value: e.socketPath || t('app.never') },
  ];
});

async function pickWorkspace() {
  // One command: native picker, validation and persistence together, so a
  // wrong folder is rejected with a reason rather than silently accepted.
  const result = await api.workspacePick();
  if (result) {
    app.workspace = result;
    loadEnv();
  }
}

async function loadEnv() {
  envError.value = null;
  edits.value = {};
  try {
    env.value = await api.envGet();
  } catch (e) {
    envError.value = e;
    env.value = {};
  }
}

function edit(key, value) {
  if (value === env.value[key]) delete edits.value[key];
  else edits.value[key] = value;
}

async function save() {
  saving.value = true;
  envError.value = null;
  saved.value = false;
  try {
    await api.envSet({ ...edits.value });
    await loadEnv();
    saved.value = true;
    setTimeout(() => (saved.value = false), 2500);
  } catch (e) {
    envError.value = e;
  } finally {
    saving.value = false;
  }
}

async function loadPrefs() {
  try {
    prefs.value = await api.prefsGet();
    if (prefs.value.theme && prefs.value.theme !== 'system') {
      theme.global.name.value = prefs.value.theme;
    }
    // Applied directly, not through setLocale: that persists and re-labels the
    // tray, and writing the value we just read back on every mount is noise.
    if (prefs.value.locale) locale.value = prefs.value.locale;
    autostart.value = await autostartEnabled();
  } catch {
    prefs.value = {};
  }
}

async function setPref(patch) {
  prefs.value = await api.prefsSet(patch);
}

async function toggleAutostart(value) {
  // Do the OS side first: if it refuses, the stored preference must not claim
  // otherwise.
  if (value) await enableAutostart();
  else await disableAutostart();
  autostart.value = await autostartEnabled();
  await setPref({ autostart: autostart.value });
}

async function verifyGenerator() {
  verifying.value = true;
  try {
    generatorReport.value = await api.generatorVerify();
  } catch (e) {
    envError.value = e;
  } finally {
    verifying.value = false;
  }
}

onMounted(async () => {
  loadEnv();
  loadPrefs();
  verifyGenerator();
  appVersion.value = await getVersion().catch(() => '');
  updaterReady.value = await updatesConfigured();
  logs.value = await api.logsInfo().catch(() => null);
  apps.value = await api.appsAvailable().catch(() => ({ terminals: [], editors: [] }));
  // No key means no check can succeed; asking anyway only produces a
  // signature error that looks like the server's fault.
  if (updaterReady.value) checkUpdate();
});
</script>

<template>
  <PageLayout top-icon="mdi-cog" :top-title="t('app.settings')">
    <!-- In the toolbar rather than under it: the page name is already in the
         header above, so a bar that repeats it and a tab strip below it are two
         rows doing one row's work.

         Tabs rather than one long scroll, because the page had eight cards in
         two columns and the .env editor — the thing anyone opens Settings to
         use — sat below a fold on any window under about 1100px. -->
    <template #bar>
      <v-tabs v-model="tab" density="comfortable" show-arrows>
        <v-tab value="workspace" prepend-icon="mdi-folder-cog">
          {{ t('workspace.title') }}
        </v-tab>
        <v-tab value="preferences" prepend-icon="mdi-tune">
          {{ t('settings.preferences') }}
        </v-tab>
        <v-tab value="stack" prepend-icon="mdi-server">{{ t('settings.stack') }}</v-tab>
        <v-tab value="env" prepend-icon="mdi-file-document-edit">
          {{ t('settings.envFile') }}
        </v-tab>
        <v-tab value="about" prepend-icon="mdi-information">{{ t('settings.about') }}</v-tab>
      </v-tabs>
    </template>

    <div class="page-scroll">
      <v-window v-model="tab" class="settings-window">
        <v-window-item value="workspace">
          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('workspace.title') }}</v-card-title>
              <v-card-subtitle v-if="app.workspace">
                {{ t(`workspace.source.${app.workspace.source}`) }}
              </v-card-subtitle>
            </v-card-item>

            <v-card-text>
              <div class="text-body-2 mb-1" style="word-break: break-all">
                {{ app.workspace?.root || t('workspace.none') }}
              </div>
              <div v-if="app.workspace?.stackvoVersion" class="text-caption text-medium-emphasis">
                {{ t('workspace.version') }} {{ app.workspace.stackvoVersion }}
              </div>
            </v-card-text>

            <v-card-actions>
              <v-btn
                size="small"
                variant="text"
                prepend-icon="mdi-folder-open-outline"
                @click="pickWorkspace"
              >
                {{ t('workspace.change') }}
              </v-btn>
              <v-btn
                v-if="app.workspace?.root"
                size="small"
                variant="text"
                prepend-icon="mdi-open-in-new"
                @click="openPath(app.workspace.root)"
              >
                {{ t('projects.openFolder') }}
              </v-btn>
            </v-card-actions>
          </v-card>

          <v-card>
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('engine.title') }}</v-card-title>
            </v-card-item>
            <v-card-text>
              <div
                v-for="row in engineRows"
                :key="row.label"
                class="d-flex justify-space-between py-1 ga-4"
              >
                <span class="text-caption text-medium-emphasis">{{ row.label }}</span>
                <span class="text-caption text-right" style="word-break: break-all">{{
                  row.value
                }}</span>
              </div>
              <div v-if="app.engine?.error" class="text-caption text-error mt-2">
                {{ app.engine.error }}
              </div>
            </v-card-text>
          </v-card>
        </v-window-item>

        <v-window-item value="preferences">
          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.preferences') }}</v-card-title>
            </v-card-item>
            <v-card-text class="d-flex flex-column ga-3">
              <v-select
                :model-value="locale"
                :items="[
                  { value: 'tr', title: 'Türkçe' },
                  { value: 'en', title: 'English' },
                ]"
                :label="t('settings.language')"
                density="compact"
                @update:model-value="setLocale"
              />
              <v-select
                :model-value="prefs?.theme ?? 'system'"
                :items="[
                  { value: 'system', title: t('settings.themeSystem') },
                  { value: 'light', title: t('settings.themeLight') },
                  { value: 'dark', title: t('settings.themeDark') },
                ]"
                :label="t('settings.theme')"
                density="compact"
                @update:model-value="
                  (v) => {
                    if (v !== 'system') theme.global.name.value = v;
                    setPref({ theme: v });
                  }
                "
              />
              <!-- Detected rather than typed. The old free-text box asked the
                   user to know the launcher name; what is actually installed is
                   something the app can find out. Missing apps stay in the list
                   but disabled — omitting them would read as lack of support. -->
              <v-select
                :model-value="prefs?.terminalApp ?? null"
                :items="apps.terminals"
                item-title="name"
                item-value="id"
                :item-props="(a) => ({ prependIcon: a.icon, disabled: !a.available })"
                :label="t('settings.terminalApp')"
                :hint="t('settings.appsHint')"
                persistent-hint
                clearable
                density="compact"
                @update:model-value="(v) => setPref({ terminalApp: v || null })"
              />
              <v-select
                :model-value="prefs?.editorCommand ?? null"
                :items="apps.editors"
                item-title="name"
                item-value="id"
                :item-props="(a) => ({ prependIcon: a.icon, disabled: !a.available })"
                :label="t('settings.editorApp')"
                clearable
                density="compact"
                @update:model-value="(v) => setPref({ editorCommand: v || null })"
              />
              <v-switch
                :model-value="autostart"
                :label="t('settings.autostart')"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="toggleAutostart"
              />
              <v-switch
                :model-value="prefs?.startMinimized ?? false"
                :label="t('settings.startMinimized')"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v) => setPref({ startMinimized: v })"
              />

              <v-divider class="my-2" />

              <div class="text-body-2">{{ t('close.behaviour') }}</div>
              <div class="text-caption text-medium-emphasis">{{ t('close.behaviourHint') }}</div>
              <v-radio-group
                :model-value="prefs?.closeBehaviour ?? 'ask'"
                density="compact"
                hide-details
                @update:model-value="(v) => setPref({ closeBehaviour: v })"
              >
                <v-radio value="ask" :label="t('close.ask')" />
                <v-radio value="tray" :label="t('close.tray')" />
                <v-radio value="quit" :label="t('close.quit')" />
                <v-radio value="stopAndQuit" :label="t('close.stopAndQuit')" />
              </v-radio-group>
            </v-card-text>
          </v-card>
        </v-window-item>

        <v-window-item value="stack">
          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.stack') }}</v-card-title>
              <v-card-subtitle>{{ t('settings.stackSub') }}</v-card-subtitle>
            </v-card-item>
            <v-card-text class="d-flex ga-2 flex-wrap">
              <v-btn
                size="small"
                variant="tonal"
                prepend-icon="mdi-play-box-multiple-outline"
                :loading="stackBusy"
                :disabled="!app.engineUp"
                @click="stackAction(() => api.composeUp('minimal'))"
              >
                {{ t('actions.up') }}
              </v-btn>
              <v-btn
                size="small"
                variant="tonal"
                prepend-icon="mdi-refresh"
                :loading="stackBusy"
                :disabled="!app.engineUp"
                @click="stackAction(() => api.composeRestart())"
              >
                {{ t('actions.composeRestart') }}
              </v-btn>
              <v-btn
                size="small"
                variant="tonal"
                color="error"
                prepend-icon="mdi-stop-circle-outline"
                :loading="stackBusy"
                :disabled="!app.engineUp"
                @click="stackAction(() => api.composeDown())"
              >
                {{ t('actions.down') }}
              </v-btn>
            </v-card-text>
          </v-card>

          <!-- The Rust generator runs alongside the Bash one and reports whether
           its output is identical. This is the gate for replacing it. -->
          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.generator') }}</v-card-title>
              <template #append>
                <v-btn
                  size="x-small"
                  variant="text"
                  icon="mdi-refresh"
                  :aria-label="t('settings.verifyNow')"
                  :loading="verifying"
                  @click="verifyGenerator"
                />
              </template>
            </v-card-item>
            <v-card-text v-if="generatorReport">
              <div class="d-flex align-center ga-2 mb-2">
                <v-chip
                  size="small"
                  :color="generatorReport.readyToTakeOver ? 'success' : 'warning'"
                >
                  {{ generatorReport.matched }} /
                  {{ generatorReport.matched + generatorReport.differed }}
                </v-chip>
                <span class="text-caption text-medium-emphasis">
                  {{
                    generatorReport.readyToTakeOver
                      ? t('settings.generatorReady')
                      : t('settings.generatorDiffers')
                  }}
                </span>
              </div>

              <div
                v-for="f in generatorReport.files.filter((x) => x.status !== 'match')"
                :key="f.file"
                class="text-caption text-warning"
              >
                {{ f.file }} — {{ f.status }}
                <span v-if="f.firstDifferenceLine">(line {{ f.firstDifferenceLine }})</span>
              </div>

              <v-alert
                v-for="(w, i) in generatorReport.warnings"
                :key="i"
                type="warning"
                variant="tonal"
                density="compact"
                class="mt-2"
              >
                <div class="text-caption">{{ w }}</div>
              </v-alert>

              <v-divider class="my-3" />

              <!-- Bash runs in every mode. `rust` refuses to write when the two
               disagree, so switching cannot silently change an image. -->
              <v-select
                v-model="engineMode"
                :items="[
                  { value: 'bash', title: t('settings.engineBash') },
                  { value: 'verify', title: t('settings.engineVerify') },
                  { value: 'rust', title: t('settings.engineRust') },
                ]"
                :label="t('settings.engineMode')"
                density="compact"
                hide-details
                class="mb-2"
              />
              <v-btn
                size="small"
                variant="tonal"
                block
                :loading="verifying"
                :disabled="engineMode === 'rust' && !generatorReport.readyToTakeOver"
                @click="runGenerate"
              >
                {{ t('actions.generate') }}
              </v-btn>
              <div v-if="generateResult" class="text-caption text-success mt-2">
                {{ generateResult.note || generateResult.engine }}
              </div>
            </v-card-text>
          </v-card>
        </v-window-item>

        <v-window-item value="env">
          <v-card>
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.envFile') }}</v-card-title>
              <!-- Writes patch lines in place: comments, section banners, trailing
               notes and blank lines all survive. A .env is a hand-maintained
               file, not a serialised map. -->
              <v-card-subtitle>{{ t('settings.envEditable') }}</v-card-subtitle>
              <template #append>
                <v-btn
                  v-if="dirty"
                  color="primary"
                  variant="flat"
                  size="small"
                  :loading="saving"
                  @click="save"
                >
                  {{ t('settings.save', { count: Object.keys(edits).length }) }}
                </v-btn>
                <v-chip v-else-if="saved" color="success" size="small">{{
                  t('settings.saved')
                }}</v-chip>
              </template>
            </v-card-item>

            <v-card-text>
              <v-text-field
                v-model="search"
                prepend-inner-icon="mdi-magnify"
                density="compact"
                hide-details
                clearable
                class="mb-3"
              />

              <ErrorAlert :error="envError" type="error" />

              <div class="env-table">
                <div v-for="[key, value] in rows" :key="key" class="env-row">
                  <span class="text-caption font-weight-medium env-key">{{ key }}</span>
                  <v-text-field
                    :model-value="edits[key] ?? value"
                    :disabled="isRedacted(value)"
                    :hint="isRedacted(value) ? t('settings.secretHint') : undefined"
                    persistent-hint
                    density="compact"
                    variant="plain"
                    hide-details="auto"
                    class="env-value"
                    @update:model-value="(v) => edit(key, v)"
                  />
                </div>
              </div>
            </v-card-text>
          </v-card>
        </v-window-item>

        <v-window-item value="about">
          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.updates') }}</v-card-title>
              <v-card-subtitle>{{ t('settings.version') }} {{ appVersion }}</v-card-subtitle>
              <template #append>
                <v-btn
                  size="x-small"
                  variant="text"
                  icon="mdi-refresh"
                  :aria-label="t('settings.checkForUpdates')"
                  :loading="checkingUpdate"
                  @click="checkUpdate"
                />
              </template>
            </v-card-item>
            <v-card-text>
              <!-- Stated plainly rather than left to fail as a signature error
               at check time: without a compiled-in public key there is
               nothing to verify a bundle against, so updates cannot work at
               all and that is a property of the build, not of the server. -->
              <v-alert
                v-if="updaterReady === false"
                type="warning"
                variant="tonal"
                density="compact"
                class="mb-2"
              >
                <div class="text-caption">{{ t('settings.updaterUnconfigured') }}</div>
              </v-alert>

              <div v-if="!update" class="text-caption text-medium-emphasis">
                {{ checkingUpdate ? t('app.loading') : t('settings.upToDate') }}
              </div>

              <div v-else>
                <div class="text-body-2 mb-1">
                  {{ t('settings.updateAvailable', { version: update.version }) }}
                </div>
                <pre v-if="update.notes" class="text-caption notes">{{ update.notes }}</pre>

                <v-progress-linear
                  v-if="updateProgress"
                  :model-value="
                    updateProgress.total
                      ? (updateProgress.downloaded / updateProgress.total) * 100
                      : 0
                  "
                  color="primary"
                  height="4"
                  rounded
                  class="my-2"
                />

                <v-btn
                  size="small"
                  color="primary"
                  variant="flat"
                  :disabled="!!updateProgress"
                  @click="installUpdate"
                >
                  {{ t('settings.installUpdate') }}
                </v-btn>
                <!-- Tauri verifies the bundle signature against the key compiled
                 into this build before anything is written. -->
                <div class="text-caption text-medium-emphasis mt-2">
                  {{ t('settings.updateSigned') }}
                </div>
              </div>
            </v-card-text>
          </v-card>

          <v-card class="mb-4">
            <v-card-item>
              <v-card-title class="text-body-1">{{ t('settings.diagnostics') }}</v-card-title>
              <v-card-subtitle>{{ t('settings.diagnosticsHint') }}</v-card-subtitle>
            </v-card-item>
            <v-card-text>
              <div v-if="!logs?.directory" class="text-caption text-medium-emphasis">
                {{ t('settings.logsUnavailable') }}
              </div>
              <template v-else>
                <div class="d-flex align-center ga-2">
                  <code class="text-caption log-path">{{ logs.directory }}</code>
                  <v-spacer />
                  <v-chip size="x-small" variant="tonal">{{ bytes(logs.totalBytes) }}</v-chip>
                  <v-btn
                    size="small"
                    variant="tonal"
                    prepend-icon="mdi-folder-open"
                    @click="openPath(logs.directory)"
                  >
                    {{ t('settings.openLogs') }}
                  </v-btn>
                </div>
                <!-- Said out loud because the alternative is a user who assumes
                   the opposite and attaches nothing, or one who assumes it is
                   safe when it is not. -->
                <div class="text-caption text-medium-emphasis mt-2">
                  {{ t('settings.logsRedacted') }}
                </div>
              </template>
            </v-card-text>
          </v-card>
        </v-window-item>
      </v-window>
    </div>
  </PageLayout>
</template>

<style scoped>
/* The window and its active item have to pass the height through, or the
   scroll container below them collapses to content height and the page scrolls
   as a whole instead of the panel. */
.settings-window,
.settings-window :deep(.v-window__container),
.settings-window :deep(.v-window-item) {
  height: 100%;
}

.page-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 16px;
}

.env-table {
  max-height: 58vh;
  overflow-y: auto;
}

.env-row {
  display: grid;
  grid-template-columns: minmax(200px, 40%) 1fr;
  gap: 12px;
  align-items: center;
  padding: 2px 0;
}

.env-key {
  word-break: break-all;
}

.notes {
  white-space: pre-wrap;
  margin: 4px 0;
  opacity: 0.75;
}

.env-value :deep(input) {
  font-size: 12px;
}

.log-path {
  word-break: break-all;
  opacity: 0.75;
}
</style>
