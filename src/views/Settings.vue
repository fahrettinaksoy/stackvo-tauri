<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { checkForUpdate, updatesConfigured } from '@/lib/updates';
import { getVersion } from '@tauri-apps/api/app';
import ErrorAlert from '@/components/ErrorAlert.vue';
import CertificatesPane from '@/components/settings/CertificatesPane.vue';
import { useCertificates } from '@/composables/useCertificates';
import { useEnvEditor, provideEnvEditor } from '@/composables/useEnvEditor';
import DomainPane from '@/components/settings/DomainPane.vue';
import WorkspacePane from '@/components/settings/WorkspacePane.vue';
import AppearancePane from '@/components/settings/AppearancePane.vue';
import PhpPane from '@/components/settings/PhpPane.vue';
import LocalisationPane from '@/components/settings/LocalisationPane.vue';
import ServerLimitsPane from '@/components/settings/ServerLimitsPane.vue';
import ServicesPane from '@/components/settings/ServicesPane.vue';
import CataloguePane from '@/components/settings/CataloguePane.vue';
import PreferencesPane from '@/components/settings/PreferencesPane.vue';
import { usePreferences } from '@/composables/usePreferences';
import DiagnosticsPane from '@/components/settings/DiagnosticsPane.vue';
import PolicyNotice from '@/components/settings/PolicyNotice.vue';
import SecretsPane from '@/components/settings/SecretsPane.vue';
import AgentsPane from '@/components/settings/AgentsPane.vue';
import PageLayout from '@/components/PageLayout.vue';
import SettingsSection from '@/components/SettingsSection.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';

const { t, locale } = useI18n();
const app = useAppStore();

/**
 * The `.env` editor, shared by six panes on this screen.
 *
 * Lifted into `useEnvEditor` under §14.16: it is the machinery that kept those
 * panes from being extracted one at a time, because they all edit one file
 * through one set of refs. Destructured here so the existing template bindings
 * keep their names.
 */
const envEditor = provideEnvEditor(useEnvEditor());
const {
  error: envError,
  lastSaved,
  loadDefaults: loadEnvDefaults,
  load: loadEnv,
  save: saveEnv,
  clearPending,
} = envEditor;

// Module-scoped, so the window and the pane that edits them agree.
const preferences = usePreferences();
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

const tab = ref('appearance');

/**
 * The panes, listed once.
 *
 * A side rail rather than a tab strip: five entries with icons and full names
 * do not fit a toolbar without truncating or scrolling, and a settings page is
 * navigated by name — you come here looking for "the .env file", not for the
 * fourth tab. The list also has room to grow, which a tab strip does not.
 */
/**
 * The panes, in four groups.
 *
 * Thirteen entries in one column was a list of everything the app can be told,
 * with no signal about which of them belong together — appearance sat beside
 * the Docker engine, and the two panes that both configure the stack were
 * separated by five that do not. The grouping is the answer to "where would I
 * look for this", which is a different question from "what does this do".
 *
 * The order inside each group is deliberate: the thing you set first comes
 * first. A workspace has to exist before it has a domain, and a domain before
 * a certificate covers it.
 */
const SECTION_GROUPS = [
  { key: 'app', label: 'settings.groups.app' },
  { key: 'workspace', label: 'settings.groups.workspace' },
  { key: 'stack', label: 'settings.groups.stack' },
  { key: 'help', label: 'settings.groups.help' },
];

const SECTIONS = [
  {
    key: 'appearance',
    group: 'app',
    icon: 'mdi-palette-outline',
    label: 'settings.appearance',
    desc: 'settings.appearanceSectionDesc',
  },
  {
    key: 'localisation',
    group: 'app',
    icon: 'mdi-translate',
    label: 'settings.localisation',
    desc: 'settings.localisationDesc',
  },
  {
    key: 'preferences',
    group: 'app',
    icon: 'mdi-tune',
    label: 'settings.preferences',
    desc: 'settings.preferencesDesc',
  },
  {
    // The folder, the compose verbs and the preset were three panes for one
    // subject: this stack, where it lives, how it is run, and how it is handed
    // to somebody else. They were also three places to look before finding the
    // button you wanted.
    key: 'workspace',
    group: 'workspace',
    icon: 'mdi-folder-cog',
    label: 'settings.workspaceAndControl',
    desc: 'settings.workspaceAndControlDesc',
  },
  // Addressing and the certificate that covers it are one subject read twice:
  // the HTTPS switch is here, and what it needs issued is next.
  {
    key: 'domain',
    group: 'workspace',
    icon: 'mdi-web',
    label: 'settings.shape.title',
    desc: 'settings.shape.sectionDesc',
  },
  {
    key: 'certificates',
    group: 'workspace',
    icon: 'mdi-certificate-outline',
    label: 'settings.certificates',
    desc: 'settings.certificatesDesc',
  },
  {
    key: 'servers',
    group: 'stack',
    icon: 'mdi-web-box',
    label: 'settings.servers.title',
    desc: 'settings.servers.desc',
  },
  {
    key: 'services',
    group: 'stack',
    icon: 'mdi-cube-outline',
    label: 'serviceSettings.title',
    desc: 'serviceSettings.sectionDesc',
  },
  // Beside the services rather than under 'app': it answers where the things
  // on that pane come from. It was reachable only from the first-run gate and
  // from a folder picker on the Market page, so a person with a URL and a
  // second launch had nowhere to type it.
  {
    key: 'catalogue',
    group: 'stack',
    icon: 'mdi-storefront-outline',
    label: 'catalogueSettings.title',
    desc: 'catalogueSettings.desc',
  },
  // Runtime versions and the PHP build were two panes answering one question:
  // what does a new project start with. Split, the answer for Python lived in
  // a different place from the answer for PHP.
  {
    key: 'php',
    group: 'stack',
    icon: 'mdi-tune-vertical',
    label: 'settings.defaults.title',
    desc: 'settings.defaults.desc',
  },
  // Beside the certificate rather than under 'stack': both answer "what does
  // this workspace hold that somebody else must not have", and a credential is
  // not a setting of the stack in the way a port or a version is.
  {
    key: 'secrets',
    group: 'workspace',
    icon: 'mdi-key-chain-variant',
    label: 'settings.secrets.title',
    desc: 'settings.secrets.description',
  },
  // With the app's own settings rather than under 'help': this is a way *in*
  // to the app, like the tray and the window, not a thing to consult when
  // something is wrong.
  {
    key: 'agents',
    group: 'app',
    icon: 'mdi-robot-outline',
    label: 'settings.agents.title',
    desc: 'settings.agents.sectionDesc',
  },
  {
    key: 'doctor',
    group: 'help',
    icon: 'mdi-stethoscope',
    label: 'doctor.title',
    desc: 'doctor.sectionDesc',
  },
  {
    key: 'about',
    group: 'help',
    icon: 'mdi-information',
    label: 'settings.about',
    desc: 'settings.aboutDesc',
  },
];

/** Only groups that have panes, so an empty heading can never render. */
const groupedSections = computed(() =>
  SECTION_GROUPS.map((g) => ({ ...g, items: SECTIONS.filter((s) => s.group === g.key) })).filter(
    (g) => g.items.length
  )
);

const section = computed(() => SECTIONS.find((s) => s.key === tab.value) ?? SECTIONS[0]);

/** Which surface family to preview a neutral swatch in — they differ per mode. */
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

/**
 * The stack-shaping settings, as controls rather than as rows in a key table.
 *
 * These were editable before — every key is, in the .env pane — but a boolean
 * you set by typing the word `true` is an escape hatch, not a setting. What
 * makes this a form is that the type is known: a switch cannot be set to
 * `ture`, a list edits as chips, and the domain suffix is checked before it
 * reaches a routing label nobody would think to look at.
 */
/**
 * The proxy, which the app never named.
 *
 * Traefik is not in the service catalog and should not be — it is not a thing
 * you switch on, it is how every project and admin UI is reached at all. But
 * that left the one container the whole stack depends on with no presence in
 * the app: no version, no state, and no route to its own dashboard, which the
 * generator has been writing a router for the entire time.
 */
/**
 * The services pane: every service grouped by the category the catalog already
 * assigns, with its `.env` settings behind a sheet.
 *
 * Grouped rather than listed flat because twenty services in one column is a
 * scroll, not a choice — and the categories are already in the data, so
 * inventing a grouping here would be a second opinion about the same thing.
 */
const catalog = ref(null);

/**
 * The version a new project of each runtime starts on.
 *
 * The catalogs beside these — which versions exist, which servers there are —
 * are not settings and have no control here. They describe what the app can
 * build, so editing one could only ever select something it cannot: a
 * generator either exists for a runtime or it does not.
 */
/**
 * The About pane's own state.
 *
 * The one thing an About screen is actually asked for is the paragraph
 * somebody pastes into a bug report, so that is built here rather than left to
 * the reader to assemble from four separate cards. Everything in it is already
 * on screen; the button only saves the transcription.
 */
const RESOURCES = [
  { key: 'docs', icon: 'mdi-book-open-variant', url: 'https://stackvo.github.io/stackvo' },
  { key: 'source', icon: 'mdi-github', url: 'https://github.com/stackvo/stackvo' },
  { key: 'issues', icon: 'mdi-bug-outline', url: 'https://github.com/stackvo/stackvo/issues' },
  { key: 'sponsor', icon: 'mdi-coffee-outline', url: 'https://buymeacoffee.com/fahrettinaksoy' },
];

const OS_NAMES = { macos: 'macOS', windows: 'Windows', linux: 'Linux' };

const systemRows = computed(() => {
  const e = app.engine;
  return [
    { label: t('about.appVersion'), value: appVersion.value || '—' },
    { label: t('about.os'), value: OS_NAMES[app.preflight?.os] ?? app.preflight?.os ?? '—' },
    {
      label: t('about.docker'),
      value: e?.version ? `${e.version} (API ${e.apiVersion || '—'})` : t('engine.down'),
    },
    { label: t('about.context'), value: e?.context || '—' },
    { label: t('about.workspace'), value: app.workspace?.root || t('workspace.none') },
  ];
});

const copied = ref(false);
async function copySystemInfo() {
  const text = systemRows.value.map((r) => `${r.label}: ${r.value}`).join('\n');
  try {
    await navigator.clipboard.writeText(text);
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  } catch (e) {
    // Not fatal and not worth an error card: the same text is on screen and
    // can be selected. Reported so a silent no-op is not mistaken for success.
    envError.value = e;
  }
}

/**
 * Which servers these limits reach.
 *
 * nginx and caddy are generated as config files, so a directive can be written
 * into them. Apache is configured by `sed` inside its own Dockerfile and
 * swoole by an inline script, so neither has a file to add a line to — shown
 * rather than hidden, because a setting that silently does nothing for two of
 * five choices is worse than one that says so.
 */

/**
 * The nginx directives the form offers, mirroring the table in the generator.
 *
 * Ports are absent on purpose and it is worth saying why: the container
 * listens on 80 and Traefik terminates TLS, so a port field here would
 * contradict the routing label pointing at it. Modules and the server root are
 * likewise the image's and the container's, not settings.
 */

async function pickWorkspace() {
  // One command: native picker, validation and persistence together, so a
  // wrong folder is rejected with a reason rather than silently accepted.
  const result = await api.workspacePick();
  if (result) {
    app.workspace = result;
    loadEnv();
  }
}

/**
 * The shipped defaults and the service catalog, read together because the two
 * panes that need them open together. The `.env` half is the composable's.
 */
async function loadDefaults() {
  await loadEnvDefaults();
  catalog.value = await api.catalogGet().catch(() => null);
}

/**
 * Keys the last save wrote, so the pane can say what has to happen next.
 *
 * Changing the suffix rewrites every routing label and moves what the
 * certificate has to cover, but none of that reaches the running stack until
 * the files are regenerated. Saving and staying silent is how a setting looks
 * like it did nothing.
 */
/** Clears the notice only if the regenerate actually succeeded. */
async function regenerateAfterChange() {
  await stackAction(() => api.generateRun('all'));
  if (!envError.value) clearPending();
}

/**
 * The store's cached TLD has to follow the file, or every domain the app shows
 * is the previous suffix until a reload. Passed to the composable rather than
 * done after it, so it lands before the "saved" confirmation appears.
 */
const save = () => saveEnv(() => app.refreshTld());

/**
 * The window needs one preference before any pane opens: the language.
 *
 * Applied directly rather than through `setLocale` — that persists and relabels
 * the tray, and writing back the value just read on every mount is noise. The
 * pane that *edits* preferences owns the rest; see `usePreferences`.
 */
async function loadPrefs() {
  const loaded = await preferences.load();
  if (loaded?.locale) locale.value = loaded.locale;
}

/**
 * The certificate state, shared with `CertificatesPane`.
 *
 * Only `stale` is read here — it badges the rail entry, and a badge that only
 * appears once you have navigated to the pane it points at is decoration. The
 * pane owns the loading and the two actions; see `useCertificates`.
 */
const { stale: certStale } = useCertificates();

onMounted(async () => {
  loadEnv();
  loadDefaults();
  loadPrefs();
  appVersion.value = await getVersion().catch(() => '');
  updaterReady.value = await updatesConfigured();
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

        <!-- Above the sections rather than inside one: a managed machine is a
             fact about every pane on this page, and a policy that failed to
             parse is a fact somebody has to see wherever they happen to be. -->
        <PolicyNotice />

        <SettingsSection
          :icon="section.icon"
          :title="t(section.label)"
          :description="t(section.desc)"
        >
          <!-- ---- appearance ------------------------------------------------ -->
          <template v-if="tab === 'appearance'">
            <AppearancePane />
          </template>

          <template v-if="tab === 'localisation'">
            <LocalisationPane />
          </template>

          <template v-if="tab === 'preferences'">
            <PreferencesPane />
          </template>

          <template v-if="tab === 'domain'">
            <DomainPane
              :regenerating="stackBusy"
              @save="save"
              @regenerate="regenerateAfterChange"
            />
          </template>

          <template v-if="tab === 'php'">
            <PhpPane @save="save" />
          </template>

          <template v-if="tab === 'servers'">
            <ServerLimitsPane @save="save" @directives-saved="(keys) => (lastSaved = keys)" />
          </template>

          <template v-if="tab === 'services'">
            <ServicesPane />
          </template>

          <template v-if="tab === 'catalogue'">
            <CataloguePane />
          </template>

          <template v-if="tab === 'workspace'">
            <WorkspacePane
              :busy="stackBusy"
              @pick="pickWorkspace"
              @up="stackAction(() => api.composeUp('minimal'))"
              @restart="stackAction(() => api.composeRestart())"
              @down="stackAction(() => api.composeDown())"
            />
          </template>

          <template v-if="tab === 'doctor'">
            <DiagnosticsPane />
          </template>

          <template v-if="tab === 'secrets'">
            <SecretsPane />
          </template>

          <template v-if="tab === 'agents'">
            <AgentsPane />
          </template>

          <template v-if="tab === 'certificates'">
            <CertificatesPane />
          </template>

          <!-- ---- .env ------------------------------------------------------ -->

          <!-- ---- about ----------------------------------------------------- -->
          <template v-if="tab === 'about'">
            <!-- Identity first, and once. The version was a chip inside the
                 update card, which is the one place it is least likely to be
                 looked for — the question "what am I running" is asked far
                 more often than "is there a newer one". -->
            <v-card variant="flat" class="about-hero mb-4">
              <div class="d-flex align-center ga-4 pa-5 flex-wrap">
                <v-avatar rounded="lg" size="56" color="primary">
                  <v-icon size="32" icon="mdi-cube-outline" />
                </v-avatar>
                <div class="min-w-0">
                  <div class="text-h6">StackVo</div>
                  <div class="text-body-2 text-medium-emphasis">{{ t('about.tagline') }}</div>
                </div>
                <v-spacer />
                <div class="d-flex align-center ga-2">
                  <v-chip v-if="appVersion" size="small" variant="tonal" prepend-icon="mdi-tag">
                    {{ appVersion }}
                  </v-chip>
                  <v-chip size="small" variant="tonal" prepend-icon="mdi-scale-balance">MIT</v-chip>
                </div>
              </div>
            </v-card>

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
            <!-- What a bug report needs, in the order somebody reading one
                 wants it, and copyable in a single action. Assembling this by
                 hand from four cards is the step that gets skipped, and a
                 report without it costs a round trip. -->
            <SettingsGroup
              icon="mdi-information-outline"
              :title="t('about.system')"
              :description="t('about.systemDesc')"
            >
              <template #append>
                <v-btn
                  size="small"
                  variant="tonal"
                  :prepend-icon="copied ? 'mdi-check' : 'mdi-content-copy'"
                  @click="copySystemInfo"
                >
                  {{ copied ? t('about.copied') : t('about.copy') }}
                </v-btn>
              </template>

              <div
                v-for="row in systemRows"
                :key="row.label"
                class="d-flex justify-space-between py-1 ga-4"
              >
                <span class="text-caption text-medium-emphasis">{{ row.label }}</span>
                <span class="text-caption text-right break">{{ row.value }}</span>
              </div>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-link-variant"
              :title="t('about.resources')"
              :description="t('about.resourcesDesc')"
            >
              <v-list density="comfortable" bg-color="transparent" class="pa-0 about-links">
                <v-list-item
                  v-for="r in RESOURCES"
                  :key="r.key"
                  :prepend-icon="r.icon"
                  :title="t(`about.links.${r.key}`)"
                  rounded="lg"
                  @click="api.openInBrowser(r.url)"
                >
                  <template #append>
                    <v-icon size="x-small" icon="mdi-open-in-new" />
                  </template>
                </v-list-item>
              </v-list>
            </SettingsGroup>

            <div class="text-caption text-medium-emphasis text-center py-2">
              {{ t('about.copyright') }}
            </div>
          </template>
        </SettingsSection>
      </div>

      <!-- The pane list. On the right rather than the left: the app already has
           two rails on the left edge, and a third one would put three columns of
           navigation between the window edge and the thing being configured. -->
      <nav class="settings-nav">
        <v-list nav class="pa-2">
          <template v-for="(g, i) in groupedSections" :key="g.key">
            <v-list-subheader :class="i ? 'mt-3' : ''">{{ t(g.label) }}</v-list-subheader>
            <v-list-item
              v-for="s in g.items"
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
          </template>
        </v-list>
      </nav>
    </div>
  </PageLayout>
</template>

<style scoped>
/* The identity card reads as the page's masthead, so it sits on the surface
   rather than in a group card — a heading inside a bordered box would look
   like one more setting. */
.about-hero {
  background: rgba(var(--v-theme-primary), 0.06);
  border-radius: 12px;
}

.about-links :deep(.v-list-item) {
  background: transparent;
}
.about-links :deep(.v-list-item:hover) {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

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

.notes {
  white-space: pre-wrap;
  margin: 4px 0;
  opacity: 0.75;
}

.env-value :deep(input) {
  font-size: 12px;
}
</style>
