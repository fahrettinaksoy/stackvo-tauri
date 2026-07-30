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
import { useAppearanceStore } from '@/stores/appearance';
import {
  DEFAULT_APPEARANCE,
  PRIMARY_SWATCHES,
  NEUTRALS,
  FONT_FAMILIES,
  STATUS_PALETTES,
} from '@/lib/appearance';
import { api } from '@/lib/ipc';
import { bytes } from '@/lib/format';
import { setLocale } from '@/i18n';
import { checkForUpdate, updatesConfigured } from '@/lib/updates';
import { getVersion } from '@tauri-apps/api/app';
import ErrorAlert from '@/components/ErrorAlert.vue';
import PageLayout from '@/components/PageLayout.vue';
import SettingsSection from '@/components/SettingsSection.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';
import DoctorPanel from '@/components/DoctorPanel.vue';

const { t, locale } = useI18n();
const app = useAppStore();
const appearance = useAppearanceStore();
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
 * The open pane, persisted for the session only.
 *
 * Deliberately not in preferences.json: which pane you last had open is not a
 * setting, and writing the config file on every click would be noise in a file
 * the user may be reading.
 */
const tab = ref('appearance');

/**
 * The panes, listed once.
 *
 * A side rail rather than a tab strip: five entries with icons and full names
 * do not fit a toolbar without truncating or scrolling, and a settings page is
 * navigated by name — you come here looking for "the .env file", not for the
 * fourth tab. The list also has room to grow, which a tab strip does not.
 */
const SECTIONS = [
  {
    key: 'appearance',
    icon: 'mdi-palette-outline',
    label: 'settings.appearance',
    desc: 'settings.appearanceSectionDesc',
  },
  {
    key: 'localisation',
    icon: 'mdi-translate',
    label: 'settings.localisation',
    desc: 'settings.localisationDesc',
  },
  {
    key: 'workspace',
    icon: 'mdi-folder-cog',
    label: 'workspace.title',
    desc: 'settings.workspaceDesc',
  },
  {
    key: 'preferences',
    icon: 'mdi-tune',
    label: 'settings.preferences',
    desc: 'settings.preferencesDesc',
  },
  { key: 'stack', icon: 'mdi-server', label: 'settings.stack', desc: 'settings.stackDesc' },
  {
    key: 'doctor',
    icon: 'mdi-stethoscope',
    label: 'doctor.title',
    desc: 'doctor.sectionDesc',
  },
  {
    key: 'certificates',
    icon: 'mdi-certificate-outline',
    label: 'settings.certificates',
    desc: 'settings.certificatesDesc',
  },
  {
    key: 'env',
    icon: 'mdi-file-document-edit',
    label: 'settings.envFile',
    desc: 'settings.envFileDesc',
  },
  { key: 'about', icon: 'mdi-information', label: 'settings.about', desc: 'settings.aboutDesc' },
];

const section = computed(() => SECTIONS.find((s) => s.key === tab.value) ?? SECTIONS[0]);

/** Which surface family to preview a neutral swatch in — they differ per mode. */
const isDark = computed(() => theme.global.current.value.dark);

/** Name being typed for a new preset. Empty disables the save button. */
const presetName = ref('');

async function savePreset() {
  await appearance.savePreset(presetName.value);
  presetName.value = '';
}

const statusItems = computed(() =>
  STATUS_PALETTES.map((p) => ({ value: p.id, title: t(`settings.statusPalettes.${p.id}`) }))
);

const fontItems = computed(() =>
  FONT_FAMILIES.map((f) => ({ value: f.id, title: t(`settings.fonts.${f.id}`) }))
);

/** Shown next to the reset button so "back to defaults" is not a leap of faith. */
const isDefaultAppearance = computed(() =>
  Object.keys(DEFAULT_APPEARANCE).every((k) => appearance.value[k] === DEFAULT_APPEARANCE[k])
);
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

/**
 * Certificate state.
 *
 * Read on mount rather than on opening the pane: `certStale` badges the rail
 * entry, and a badge that only appears once you have already navigated to the
 * thing it is pointing at is decoration.
 */
const certs = ref(null);
const certPlan = ref(null);
const certBusy = ref(false);
const certError = ref(null);

async function loadCerts() {
  try {
    certs.value = await api.certStatus();
    // The plan is what says which names a reissue would *drop* — the status
    // only computes what is missing. A user who deleted a project and sees its
    // domain vanish from the certificate should have been told first.
    certPlan.value = await api.certPlan(certs.value.caTrusted !== true);
    certError.value = null;
  } catch (e) {
    // A missing workspace is reported by the requirements gate already; a
    // second copy of it here would be noise.
    certs.value = null;
    certPlan.value = null;
    if (!e.needsWorkspace) certError.value = e;
  }
}

/**
 * Reissue for the domains the projects actually have.
 *
 * The plan is shown before this runs, so there is no confirmation step here —
 * the button's label is the plan. `installCa` follows what the status reported:
 * asking for the trust store when it is already trusted would raise a password
 * prompt for nothing.
 */
/**
 * True when the certificate was reissued but the running proxy is still
 * serving the old one — see `reload_proxy` in `certs.rs`. Cleared by the next
 * reissue, because the state it describes belongs to the last one.
 */
const certNotReloaded = ref(false);

async function reissueCerts() {
  certBusy.value = true;
  certError.value = null;
  certNotReloaded.value = false;
  try {
    const applied = await api.certApply(certs.value?.caTrusted !== true);
    // A certificate nothing serves is not a certificate the user has.
    certNotReloaded.value = applied?.reloaded === false;
    await loadCerts();
  } catch (e) {
    certError.value = e;
  } finally {
    certBusy.value = false;
  }
}

/** The one fact worth surfacing outside this pane. */
const certStale = computed(() => certs.value?.sslEnabled && certs.value?.stale);

/** Expiry as a date in the user's locale — the Rust side sends epoch seconds. */
const certExpiry = computed(() => {
  const seconds = certs.value?.notAfter;
  if (!seconds) return null;
  return new Date(seconds * 1000).toLocaleDateString(locale.value);
});

onMounted(async () => {
  loadEnv();
  loadPrefs();
  loadCerts();
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
  <PageLayout
    top-icon="mdi-cog"
    :top-title="t('app.settings')"
    :top-subtitle="t('settings.subtitle')"
    hide-bar
  >
    <div class="settings-layout">
      <div class="settings-scroll">
        <!-- One error surface for the whole page. Every action here writes to
             the same ref, and a banner that lives inside one group would be
             invisible for the four that are not open. -->
        <ErrorAlert :error="envError" type="error" class="mb-4" />

        <SettingsSection
          :icon="section.icon"
          :title="t(section.label)"
          :description="t(section.desc)"
        >
          <!-- ---- appearance ------------------------------------------------ -->
          <template v-if="tab === 'appearance'">
            <SettingsGroup
              icon="mdi-palette"
              :title="t('settings.themeColors')"
              :description="t('settings.themeColorsDesc')"
            >
              <template #append>
                <v-btn
                  size="small"
                  variant="text"
                  prepend-icon="mdi-backup-restore"
                  :disabled="isDefaultAppearance"
                  @click="appearance.reset()"
                >
                  {{ t('settings.resetAppearance') }}
                </v-btn>
              </template>

              <div class="field-label">{{ t('settings.theme') }}</div>
              <!-- Three buttons rather than a dropdown: the choice is small,
                   fixed and worth showing all of at once. `system` is Vuetify's
                   own theme name, so it tracks prefers-color-scheme live rather
                   than being read once at launch. -->
              <v-btn-toggle
                :model-value="appearance.value.theme"
                mandatory
                divided
                color="primary"
                variant="flat"
                class="mb-5 bg-surface-light"
                @update:model-value="(v) => appearance.set({ theme: v })"
              >
                <v-btn value="system" size="small" prepend-icon="mdi-theme-light-dark">
                  {{ t('settings.themeSystem') }}
                </v-btn>
                <v-btn value="light" size="small" prepend-icon="mdi-white-balance-sunny">
                  {{ t('settings.themeLight') }}
                </v-btn>
                <v-btn value="dark" size="small" prepend-icon="mdi-weather-night">
                  {{ t('settings.themeDark') }}
                </v-btn>
              </v-btn-toggle>

              <div class="d-flex align-center ga-2 mb-1">
                <div class="field-label mb-0">{{ t('settings.primaryColor') }}</div>
                <v-spacer />
                <!-- Offered only where it can be answered: on Linux there is no
                     one accent colour to read. -->
                <v-switch
                  v-if="appearance.systemAccent"
                  :model-value="appearance.value.useSystemAccent"
                  :label="t('settings.systemAccent')"
                  color="primary"
                  hide-details
                  class="flex-grow-0"
                  @update:model-value="(v) => appearance.set({ useSystemAccent: v })"
                />
              </div>
              <div
                class="swatches mb-5"
                :class="{ 'is-disabled': appearance.value.useSystemAccent }"
              >
                <button
                  v-for="c in PRIMARY_SWATCHES"
                  :key="c"
                  type="button"
                  class="swatch"
                  :class="{ 'is-active': appearance.value.primary === c }"
                  :style="{ background: c }"
                  :title="c"
                  :aria-label="c"
                  :disabled="appearance.value.useSystemAccent"
                  @click="appearance.set({ primary: c })"
                >
                  <v-icon v-if="appearance.value.primary === c" size="15" color="white">
                    mdi-check
                  </v-icon>
                </button>
              </div>

              <div class="field-label">{{ t('settings.neutralPalette') }}</div>
              <div class="swatches mb-5">
                <button
                  v-for="n in NEUTRALS"
                  :key="n.id"
                  type="button"
                  class="swatch swatch--neutral"
                  :class="{ 'is-active': appearance.value.neutral === n.id }"
                  :style="{ background: isDark ? n.dark.surface : n.light['surface-variant'] }"
                  :title="t(`settings.neutrals.${n.id}`)"
                  :aria-label="t(`settings.neutrals.${n.id}`)"
                  @click="appearance.set({ neutral: n.id })"
                >
                  <v-icon v-if="appearance.value.neutral === n.id" size="15">mdi-check</v-icon>
                </button>
              </div>

              <div class="field-label">
                {{ t('settings.radius', { px: appearance.value.radius }) }}
              </div>
              <!-- Previewed while dragging, written when the handle is let go:
                   a slider emits on every pixel, and preferences.json is a file
                   on disk. -->
              <v-slider
                :model-value="appearance.value.radius"
                :min="0"
                :max="24"
                :step="1"
                hide-details
                @update:model-value="(v) => appearance.preview({ radius: v })"
                @end="appearance.commit()"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-format-font"
              :title="t('settings.typography')"
              :description="t('settings.typographyDesc')"
            >
              <v-select
                :model-value="appearance.value.fontFamily"
                :items="fontItems"
                :label="t('settings.fontFamily')"
                :hint="t('settings.fontFamilyHint')"
                persistent-hint
                class="mb-5"
                @update:model-value="(v) => appearance.set({ fontFamily: v })"
              />

              <div class="field-label">{{ t('settings.density') }}</div>
              <!-- One knob for the whole app: every `density` prop written on a
                   component was removed, because a prop outranks a default and
                   would have made this setting a no-op wherever one existed. -->
              <v-btn-toggle
                :model-value="appearance.value.density"
                mandatory
                divided
                color="primary"
                variant="flat"
                class="mb-5 bg-surface-light"
                @update:model-value="(v) => appearance.set({ density: v })"
              >
                <v-btn value="compact" size="small">{{ t('settings.densityCompact') }}</v-btn>
                <v-btn value="comfortable" size="small">
                  {{ t('settings.densityComfortable') }}
                </v-btn>
                <v-btn value="default" size="small">{{ t('settings.densitySpacious') }}</v-btn>
              </v-btn-toggle>

              <div class="field-label">
                {{ t('settings.uiScale', { px: appearance.value.fontSize }) }}
              </div>
              <!-- Vuetify's type scale is in rem throughout, so the root size
                   scales every label, table row and dialog with it — this is a
                   UI scale control, not just a font size. -->
              <v-slider
                :model-value="appearance.value.fontSize"
                :min="12"
                :max="20"
                :step="1"
                hide-details
                class="mb-4"
                @update:model-value="(v) => appearance.preview({ fontSize: v })"
                @end="appearance.commit()"
              />

              <v-switch
                :model-value="appearance.value.highContrast"
                :label="t('settings.highContrast')"
                color="primary"
                hide-details
                @update:model-value="(v) => appearance.set({ highContrast: v })"
              />
              <div class="text-caption text-medium-emphasis">
                {{ t('settings.highContrastHint') }}
              </div>

              <v-switch
                :model-value="appearance.value.reduceMotion"
                :label="t('settings.reduceMotion')"
                color="primary"
                hide-details
                class="mt-2"
                @update:model-value="(v) => appearance.set({ reduceMotion: v })"
              />
              <div class="text-caption text-medium-emphasis">
                {{ t('settings.reduceMotionHint') }}
              </div>
            </SettingsGroup>

            <!-- The one palette in the app that is not decoration: these four
                 colours are how a container reports what it is doing. -->
            <SettingsGroup
              icon="mdi-traffic-light-outline"
              :title="t('settings.statusColors')"
              :description="t('settings.statusColorsDesc')"
            >
              <v-select
                :model-value="appearance.value.statusPalette"
                :items="statusItems"
                :label="t('settings.statusPalette')"
                class="mb-3"
                @update:model-value="(v) => appearance.set({ statusPalette: v })"
              />

              <!-- Shown, not described: whether two states are tellable apart is
                   a question about your eyes, not about the palette's name. -->
              <div class="d-flex ga-2 flex-wrap">
                <v-chip size="small" color="success" prepend-icon="mdi-check-circle">
                  {{ t('system.running') }}
                </v-chip>
                <v-chip size="small" color="error" prepend-icon="mdi-alert-circle">
                  {{ t('system.stopped') }}
                </v-chip>
                <v-chip size="small" color="warning" prepend-icon="mdi-alert">
                  {{ t('settings.generatorDiffers') }}
                </v-chip>
                <v-chip size="small" color="info" prepend-icon="mdi-information">
                  {{ t('settings.about') }}
                </v-chip>
              </div>

              <v-switch
                :model-value="appearance.value.darkConsoles"
                :label="t('settings.darkConsoles')"
                color="primary"
                hide-details
                class="mt-3"
                @update:model-value="(v) => appearance.set({ darkConsoles: v })"
              />
              <div class="text-caption text-medium-emphasis">
                {{ t('settings.darkConsolesHint') }}
              </div>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-bookmark-multiple-outline"
              :title="t('settings.presets')"
              :description="t('settings.presetsDesc')"
            >
              <div class="d-flex ga-2 align-start">
                <v-text-field
                  v-model="presetName"
                  :label="t('settings.presetName')"
                  hide-details
                  @keyup.enter="savePreset"
                />
                <v-btn
                  color="primary"
                  variant="flat"
                  :disabled="!presetName.trim()"
                  @click="savePreset"
                >
                  {{ t('settings.savePreset') }}
                </v-btn>
              </div>

              <div v-if="appearance.presets.length" class="d-flex ga-2 flex-wrap mt-3">
                <v-chip
                  v-for="p in appearance.presets"
                  :key="p.name"
                  closable
                  prepend-icon="mdi-palette-swatch"
                  @click="appearance.applyPreset(p.name)"
                  @click:close="appearance.deletePreset(p.name)"
                >
                  {{ p.name }}
                </v-chip>
              </div>
              <div v-else class="text-caption text-medium-emphasis mt-3">
                {{ t('settings.noPresets') }}
              </div>
            </SettingsGroup>
          </template>

          <!-- ---- localisation ---------------------------------------------- -->
          <template v-if="tab === 'localisation'">
            <SettingsGroup
              icon="mdi-web"
              :title="t('settings.language')"
              :description="t('settings.languageDesc')"
            >
              <v-select
                :model-value="locale"
                :items="[
                  { value: 'tr', title: 'Türkçe' },
                  { value: 'en', title: 'English' },
                ]"
                :label="t('settings.language')"
                @update:model-value="setLocale"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-console"
              :title="t('settings.consoleLanguage')"
              :description="t('settings.consoleLanguageDesc')"
            >
              <v-select
                :model-value="appearance.value.consoleLocale"
                :items="[
                  { value: 'app', title: t('settings.consoleFollowsApp') },
                  { value: 'tr', title: 'Türkçe' },
                  { value: 'en', title: 'English' },
                ]"
                :label="t('settings.consoleLanguage')"
                :hint="t('settings.consoleLanguageHint')"
                persistent-hint
                @update:model-value="(v) => appearance.set({ consoleLocale: v })"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-format-textdirection-r-to-l"
              :title="t('settings.direction')"
              :description="t('settings.directionDesc')"
            >
              <v-switch
                :model-value="appearance.value.rtl"
                :label="t('settings.rtl')"
                color="primary"
                hide-details
                @update:model-value="(v) => appearance.set({ rtl: v })"
              />
              <div class="text-caption text-medium-emphasis">{{ t('settings.rtlHint') }}</div>
            </SettingsGroup>
          </template>

          <!-- ---- workspace ------------------------------------------------ -->
          <template v-if="tab === 'workspace'">
            <SettingsGroup
              icon="mdi-folder-open-outline"
              :title="t('settings.workspaceGroup')"
              :description="t('settings.workspaceGroupDesc')"
            >
              <div class="text-body-2 break">{{ app.workspace?.root || t('workspace.none') }}</div>
              <div v-if="app.workspace" class="text-caption text-medium-emphasis mt-1">
                {{ t(`workspace.source.${app.workspace.source}`) }}
                <template v-if="app.workspace.stackvoVersion">
                  · {{ t('workspace.version') }} {{ app.workspace.stackvoVersion }}
                </template>
              </div>

              <div class="d-flex ga-2 flex-wrap mt-3">
                <v-btn
                  size="small"
                  variant="tonal"
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
              </div>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-docker"
              :title="t('engine.title')"
              :description="t('settings.engineGroupDesc')"
            >
              <template #append>
                <v-chip size="small" :color="app.engineUp ? 'success' : 'error'">
                  {{ app.engineUp ? t('engine.running') : t('engine.down') }}
                </v-chip>
              </template>

              <div
                v-for="row in engineRows"
                :key="row.label"
                class="d-flex justify-space-between py-1 ga-4"
              >
                <span class="text-caption text-medium-emphasis">{{ row.label }}</span>
                <span class="text-caption text-right break">{{ row.value }}</span>
              </div>
              <div v-if="app.engine?.error" class="text-caption text-error mt-2">
                {{ app.engine.error }}
              </div>
            </SettingsGroup>
          </template>

          <!-- ---- preferences ---------------------------------------------- -->
          <template v-if="tab === 'preferences'">
            <SettingsGroup
              icon="mdi-application-cog-outline"
              :title="t('settings.externalApps')"
              :description="t('settings.externalAppsDesc')"
            >
              <div class="d-flex flex-column ga-3">
                <!-- Detected rather than typed. The old free-text box asked the
                     user to know the launcher name; what is actually installed
                     is something the app can find out. Missing apps stay in the
                     list but disabled — omitting them would read as lack of
                     support. -->
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
                  @update:model-value="(v) => setPref({ editorCommand: v || null })"
                />
              </div>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-power"
              :title="t('settings.startup')"
              :description="t('settings.startupDesc')"
            >
              <v-switch
                :model-value="autostart"
                :label="t('settings.autostart')"
                color="primary"
                hide-details
                @update:model-value="toggleAutostart"
              />
              <v-switch
                :model-value="prefs?.startMinimized ?? false"
                :label="t('settings.startMinimized')"
                color="primary"
                hide-details
                @update:model-value="(v) => setPref({ startMinimized: v })"
              />

              <v-divider class="my-3" />

              <div class="text-body-2">{{ t('close.behaviour') }}</div>
              <div class="text-caption text-medium-emphasis">{{ t('close.behaviourHint') }}</div>
              <v-radio-group
                :model-value="prefs?.closeBehaviour ?? 'ask'"
                hide-details
                @update:model-value="(v) => setPref({ closeBehaviour: v })"
              >
                <v-radio value="ask" :label="t('close.ask')" />
                <v-radio value="tray" :label="t('close.tray')" />
                <v-radio value="quit" :label="t('close.quit')" />
                <v-radio value="stopAndQuit" :label="t('close.stopAndQuit')" />
              </v-radio-group>
            </SettingsGroup>
          </template>

          <!-- ---- stack ----------------------------------------------------- -->
          <template v-if="tab === 'stack'">
            <SettingsGroup
              icon="mdi-play-box-multiple-outline"
              :title="t('settings.compose')"
              :description="t('settings.stackSub')"
            >
              <div class="d-flex ga-2 flex-wrap">
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
              </div>
            </SettingsGroup>

            <!-- The Rust generator runs alongside the Bash one and reports
                 whether its output is identical. This is the gate for replacing
                 it. -->
            <SettingsGroup
              icon="mdi-cog-sync-outline"
              :title="t('settings.generator')"
              :description="t('settings.generatorDesc')"
            >
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

              <template v-if="generatorReport">
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
                  class="mt-2"
                >
                  <div class="text-caption">{{ w }}</div>
                </v-alert>

                <v-divider class="my-3" />

                <!-- Bash runs in every mode. `rust` refuses to write when the
                     two disagree, so switching cannot silently change an
                     image. -->
                <v-select
                  v-model="engineMode"
                  :items="[
                    { value: 'bash', title: t('settings.engineBash') },
                    { value: 'verify', title: t('settings.engineVerify') },
                    { value: 'rust', title: t('settings.engineRust') },
                  ]"
                  :label="t('settings.engineMode')"
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
              </template>
            </SettingsGroup>
          </template>

          <!-- ---- doctor ----------------------------------------------------- -->
          <!-- Diagnosis next to its repair. The findings here used to surface
               one failed compose up at a time, each as an error about itself:
               "address already in use" with no word on by what. -->
          <template v-if="tab === 'doctor'">
            <DoctorPanel />
          </template>

          <!-- ---- certificates ---------------------------------------------- -->
          <!-- HTTPS worked before this pane existed and was invisible: the one
               question a browser warning raises — "is my domain in the
               certificate?" — had no answer anywhere in the app. -->
          <template v-if="tab === 'certificates'">
            <ErrorAlert v-if="certError" :error="certError" class="mb-4" />

            <SettingsGroup
              icon="mdi-certificate-outline"
              :title="t('certs.title')"
              :description="t('certs.subtitle')"
            >
              <template #append>
                <v-btn
                  size="x-small"
                  variant="text"
                  icon="mdi-refresh"
                  :aria-label="t('app.refresh')"
                  :loading="certBusy"
                  @click="loadCerts"
                />
              </template>

              <!-- SSL off is a choice, not a fault: without it the generator
                   emits no `websecure` entry point and nothing below applies. -->
              <v-alert v-if="certs && !certs.sslEnabled" type="info" variant="tonal" class="mb-3">
                <div class="text-caption">{{ t('certs.sslOff') }}</div>
              </v-alert>

              <template v-else-if="certs">
                <div class="d-flex align-center ga-2 mb-3 flex-wrap">
                  <v-chip size="small" :color="certs.stale ? 'warning' : 'success'">
                    {{ certs.stale ? t('certs.stale') : t('certs.current') }}
                  </v-chip>
                  <v-chip
                    size="small"
                    :color="
                      certs.caTrusted === true
                        ? 'success'
                        : certs.caTrusted === false
                          ? 'warning'
                          : undefined
                    "
                  >
                    {{
                      certs.caTrusted === true
                        ? t('certs.caTrusted')
                        : certs.caTrusted === false
                          ? t('certs.caUntrusted')
                          : t('certs.caUnknown')
                    }}
                  </v-chip>
                  <span v-if="certExpiry" class="text-caption text-medium-emphasis">
                    {{
                      certs.expired
                        ? t('certs.expiredOn', { date: certExpiry })
                        : t('certs.expiresOn', {
                            date: certExpiry,
                            days: certs.daysRemaining,
                          })
                    }}
                  </span>
                </div>

                <!-- mkcert is the whole mechanism; without it nothing here can
                     be repaired, so it is said plainly rather than left for the
                     reissue button to fail on. -->
                <v-alert v-if="!certs.mkcertAvailable" type="warning" variant="tonal" class="mb-3">
                  <div class="text-caption">{{ t('certs.noMkcert') }}</div>
                </v-alert>

                <v-alert v-if="certs.error" type="error" variant="tonal" class="mb-3">
                  <div class="text-caption">{{ certs.error }}</div>
                </v-alert>

                <!-- The point of the pane: which domains the file on disk does
                     not vouch for. -->
                <template v-if="certs.missing.length">
                  <div class="text-caption text-medium-emphasis mb-1">
                    {{ t('certs.missing') }}
                  </div>
                  <div class="mb-3">
                    <v-chip
                      v-for="d in certs.missing"
                      :key="d"
                      size="x-small"
                      color="warning"
                      class="mr-1 mb-1"
                    >
                      {{ d }}
                    </v-chip>
                  </div>
                </template>

                <template v-if="certPlan?.remove?.length">
                  <div class="text-caption text-medium-emphasis mb-1">
                    {{ t('certs.dropping') }}
                  </div>
                  <div class="mb-3">
                    <v-chip
                      v-for="d in certPlan.remove"
                      :key="d"
                      size="x-small"
                      variant="outlined"
                      class="mr-1 mb-1"
                    >
                      {{ d }}
                    </v-chip>
                  </div>
                </template>

                <template v-if="certs.rejected.length">
                  <div class="text-caption text-error mb-1">{{ t('certs.rejected') }}</div>
                  <div class="mb-3">
                    <v-chip
                      v-for="d in certs.rejected"
                      :key="d"
                      size="x-small"
                      color="error"
                      class="mr-1 mb-1"
                    >
                      {{ d }}
                    </v-chip>
                  </div>
                </template>

                <div class="text-caption text-medium-emphasis mb-1">
                  {{ t('certs.covered', { n: certs.covered.length }) }}
                </div>
                <div class="mb-3">
                  <v-chip v-for="d in certs.covered" :key="d" size="x-small" class="mr-1 mb-1">
                    {{ d }}
                  </v-chip>
                </div>

                <v-btn
                  size="small"
                  variant="tonal"
                  block
                  prepend-icon="mdi-autorenew"
                  :loading="certBusy"
                  :disabled="!certs.mkcertAvailable"
                  @click="reissueCerts"
                >
                  {{ certs.caTrusted === true ? t('certs.reissue') : t('certs.reissueAndTrust') }}
                </v-btn>

                <!-- The certificate is on disk and the browser is still getting
                     the old one. Silence here is what made this bug survive:
                     the reissue reports success either way. -->
                <v-alert v-if="certNotReloaded" type="warning" variant="tonal" class="mt-3">
                  <div class="text-caption">{{ t('certs.notReloaded') }}</div>
                </v-alert>

                <div v-if="certs.certPath" class="text-caption text-medium-emphasis mt-2">
                  {{ certs.certPath }}
                </div>
              </template>
            </SettingsGroup>
          </template>

          <!-- ---- .env ------------------------------------------------------ -->
          <template v-if="tab === 'env'">
            <!-- Writes patch lines in place: comments, section banners, trailing
                 notes and blank lines all survive. A .env is a hand-maintained
                 file, not a serialised map. -->
            <SettingsGroup
              icon="mdi-file-document-edit-outline"
              :title="t('settings.envVars')"
              :description="t('settings.envEditable')"
            >
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
                <v-chip v-else-if="saved" color="success" size="small">
                  {{ t('settings.saved') }}
                </v-chip>
              </template>

              <v-text-field
                v-model="search"
                prepend-inner-icon="mdi-magnify"
                hide-details
                clearable
                class="mb-3"
              />

              <div class="env-table">
                <div v-for="[key, value] in rows" :key="key" class="env-row">
                  <span class="text-caption font-weight-medium break">{{ key }}</span>
                  <v-text-field
                    :model-value="edits[key] ?? value"
                    :disabled="isRedacted(value)"
                    :hint="isRedacted(value) ? t('settings.secretHint') : undefined"
                    persistent-hint
                    variant="plain"
                    hide-details="auto"
                    class="env-value"
                    @update:model-value="(v) => edit(key, v)"
                  />
                </div>
              </div>
            </SettingsGroup>
          </template>

          <!-- ---- about ----------------------------------------------------- -->
          <template v-if="tab === 'about'">
            <SettingsGroup
              icon="mdi-update"
              :title="t('settings.updates')"
              :description="t('settings.updatesDesc')"
            >
              <template #append>
                <v-chip v-if="appVersion" size="small" variant="tonal">
                  {{ t('settings.version') }} {{ appVersion }}
                </v-chip>
                <v-btn
                  size="x-small"
                  variant="text"
                  icon="mdi-refresh"
                  :aria-label="t('settings.checkForUpdates')"
                  :loading="checkingUpdate"
                  @click="checkUpdate"
                />
              </template>

              <!-- Stated plainly rather than left to fail as a signature error
                   at check time: without a compiled-in public key there is
                   nothing to verify a bundle against, so updates cannot work at
                   all and that is a property of the build, not of the server. -->
              <v-alert v-if="updaterReady === false" type="warning" variant="tonal" class="mb-2">
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
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-bug-outline"
              :title="t('settings.diagnostics')"
              :description="t('settings.diagnosticsHint')"
            >
              <div v-if="!logs?.directory" class="text-caption text-medium-emphasis">
                {{ t('settings.logsUnavailable') }}
              </div>
              <template v-else>
                <div class="d-flex align-center ga-2 flex-wrap">
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
            </SettingsGroup>
          </template>
        </SettingsSection>
      </div>

      <!-- The pane list. On the right rather than the left: the app already has
           two rails on the left edge, and a third one would put three columns of
           navigation between the window edge and the thing being configured. -->
      <nav class="settings-nav">
        <v-list nav class="pa-2">
          <v-list-item
            v-for="s in SECTIONS"
            :key="s.key"
            rounded="lg"
            color="primary"
            :prepend-icon="s.icon"
            :title="t(s.label)"
            :active="tab === s.key"
            @click="tab = s.key"
          >
            <!-- The certificate going stale is silent otherwise: the first
                 sign is a browser warning on a project that worked yesterday,
                 and nothing connects that to a settings pane. -->
            <template v-if="s.key === 'certificates' && certStale" #append>
              <v-icon
                size="x-small"
                color="warning"
                icon="mdi-alert-circle"
                :aria-label="t('certs.stale')"
              />
            </template>
          </v-list-item>
        </v-list>
      </nav>
    </div>
  </PageLayout>
</template>

<style scoped>
.settings-layout {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  align-items: stretch;
}

.settings-scroll {
  flex: 1 1 auto;
  min-width: 0;
  overflow-y: auto;
  padding: 16px;
}

.settings-nav {
  flex: 0 0 220px;
  overflow-y: auto;
}

/* Small caption above a control group. Vuetify's own labels sit inside the
   field; these name a cluster of controls that has no single field to sit in. */
.field-label {
  font-size: 12px;
  opacity: 0.72;
  margin-bottom: 6px;
}

.swatches {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.swatch {
  width: 28px;
  height: 28px;
  padding: 0;
  /* Capped: a 28px swatch at the top of the range would be a circle. */
  border-radius: min(var(--app-radius), 14px);
  border: 2px solid transparent;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

/* Neutrals are near-background by definition, so without an outline the row
   reads as empty space rather than as five choices. */
.swatch--neutral {
  border-color: rgba(var(--v-theme-on-surface), 0.22);
}

.swatches.is-disabled {
  opacity: 0.4;
  pointer-events: none;
}

.swatch.is-active {
  border-color: rgba(var(--v-theme-on-surface), 0.7);
}

/* Under about 900px the rail costs more width than it earns, so it becomes a
   strip above the pane it selects. `column-reverse` keeps the markup in reading
   order — content first — while the selector still comes first on screen. */
@media (max-width: 900px) {
  .settings-layout {
    flex-direction: column-reverse;
  }

  .settings-nav {
    flex: 0 0 auto;
  }

  .settings-nav :deep(.v-list) {
    display: flex;
    flex-wrap: wrap;
  }
}

.env-table {
  max-height: 52vh;
  overflow-y: auto;
}

.env-row {
  display: grid;
  grid-template-columns: minmax(200px, 40%) 1fr;
  gap: 12px;
  align-items: center;
  padding: 2px 0;
}

.break {
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
