<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useTheme } from 'vuetify';
import {
  enable as enableAutostart,
  disable as disableAutostart,
  isEnabled as autostartEnabled,
} from '@tauri-apps/plugin-autostart';
import { useAppStore } from '@/stores/app';
import { useInventoryStore } from '@/stores/inventory';
import { useAppearanceStore } from '@/stores/appearance';
import {
  DEFAULT_APPEARANCE,
  PRIMARY_SWATCHES,
  NEUTRALS,
  FONT_FAMILIES,
  STATUS_PALETTES,
} from '@/lib/appearance';
import { api, asList } from '@/lib/ipc';
import { HTTPS_ONLY_SUFFIXES } from '@/lib/manifest';
import { bytes } from '@/lib/format';
import { setLocale } from '@/i18n';
import { checkForUpdate, updatesConfigured } from '@/lib/updates';
import { getVersion } from '@tauri-apps/api/app';
import ErrorAlert from '@/components/ErrorAlert.vue';
import PageLayout from '@/components/PageLayout.vue';
import SettingsSection from '@/components/SettingsSection.vue';
import ServiceSettingsSheet from '@/components/ServiceSettingsSheet.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';
import DoctorPanel from '@/components/DoctorPanel.vue';

const { t, locale } = useI18n();
const app = useAppStore();
const inventory = useInventoryStore();
const appearance = useAppearanceStore();
const theme = useTheme();

const env = ref({});
const envError = ref(null);
const edits = ref({});
const saving = ref(false);
const saved = ref(false);

const prefs = ref(null);
const generatorReport = ref(null);
const verifying = ref(false);
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

// The diagnostic archive. `bundle` holds the last result so the pane can name
// what went in — a success toast that says "saved" leaves the user to open the
// zip to find out whether the thing they were asked for is in it.
const bundling = ref(false);
const bundle = ref(null);
const bundleError = ref(null);

/**
 * Collect the bundle to a path the user picks.
 *
 * The save dialog rather than a fixed location, for the reason
 * `mail_attachment_save` uses one: this writes a file outside everything the
 * app owns, and the only acceptable authority for that is the person at the
 * keyboard. A cancelled dialog is an answer, not a failure — it returns null
 * and nothing is reported.
 */
async function saveDiagnosticBundle() {
  bundleError.value = null;
  bundle.value = null;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({
      defaultPath: `stackvo-diagnostics.zip`,
      filters: [{ name: 'Zip archive', extensions: ['zip'] }],
    });
    if (!path) return;

    bundling.value = true;
    bundle.value = await api.diagnosticsBundle(path);
  } catch (e) {
    bundleError.value = e;
  } finally {
    bundling.value = false;
  }
}
const apps = ref({ terminals: [], editors: [], browsers: [] });

/**
 * What an app picker shows when the user has never touched it.
 *
 * An empty box said nothing about what "Open in terminal" would start, while
 * the back end has always fallen back to the first installed entry. Showing
 * that entry is honest — it is what the button does — and it is deliberately
 * not written to preferences.json: the fallback should keep tracking what is
 * installed rather than freeze the first answer it ever gave.
 */
const appDefault = (list) => list?.find((a) => a.default)?.id ?? null;
const appChoice = (stored, list) => stored ?? appDefault(list);
const appItemProps = (a) => ({
  prependIcon: a.icon,
  disabled: !a.available,
  subtitle: a.default ? t('settings.appDefault') : undefined,
});

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
const isDark = computed(() => theme.global.current.value.dark);

/** Name being typed for a new preset. Empty disables the save button. */
const presetName = ref('');

async function savePreset() {
  await appearance.savePreset(presetName.value);
  presetName.value = '';
}

/**
 * Stack presets — a different thing from the appearance presets above, which is
 * why nothing here shares their names.
 *
 * The file carries which services are enabled and at which versions. That is
 * the part of a StackVo configuration a teammate does not get from a clone:
 * `stackvo.json` is already in the repository, `.env` is not, because `.env` is
 * also where every password lives.
 *
 * Import is plan-then-apply, like the hosts file and the certificate — you see
 * the diff before anything is written over your own stack.
 */
const stackPresetName = ref('');
const stackPreset = ref(null);
const stackPresetPlan = ref(null);
const stackPresetPath = ref('');
const stackPresetBusy = ref(false);
const stackPresetApplied = ref(false);

async function loadStackPreset() {
  try {
    stackPreset.value = await api.presetExport();
  } catch (e) {
    envError.value = e;
  }
}

const stackPresetJson = computed(() =>
  stackPreset.value ? JSON.stringify(stackPreset.value, null, 2) : ''
);

/** How many services the current stack has on, for the summary line. */
const stackPresetEnabled = computed(
  () => Object.values(stackPreset.value?.services ?? {}).filter((s) => s.enabled).length
);

async function exportStackPreset() {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const suggested = stackPresetName.value.trim() || 'stack';
  const path = await save({ defaultPath: `${suggested}.stackvo-preset.json` });
  if (!path) return;

  stackPresetBusy.value = true;
  envError.value = null;
  try {
    await api.presetSave(path, stackPresetName.value.trim() || null);
  } catch (e) {
    envError.value = e;
  } finally {
    stackPresetBusy.value = false;
  }
}

async function chooseStackPreset() {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({ multiple: false, directory: false });
  if (!path) return;

  stackPresetBusy.value = true;
  envError.value = null;
  stackPresetApplied.value = false;
  try {
    stackPresetPlan.value = await api.presetPlan(path);
    stackPresetPath.value = path;
  } catch (e) {
    // A file that is not a preset is an error, not an empty plan — clear the
    // pane so a previous review cannot be mistaken for this file's.
    stackPresetPlan.value = null;
    stackPresetPath.value = '';
    envError.value = e;
  } finally {
    stackPresetBusy.value = false;
  }
}

async function applyStackPreset() {
  stackPresetBusy.value = true;
  envError.value = null;
  try {
    stackPresetPlan.value = await api.presetApply(stackPresetPath.value);
    stackPresetApplied.value = true;
    // The stack this pane describes is the stack that just changed.
    await loadStackPreset();
  } catch (e) {
    envError.value = e;
  } finally {
    stackPresetBusy.value = false;
  }
}

function clearStackPresetPlan() {
  stackPresetPlan.value = null;
  stackPresetPath.value = '';
  stackPresetApplied.value = false;
}

// Loaded when the pane is opened rather than on mount: it reads .env, and the
// other eight sections have no use for it.
watch(tab, (value) => {
  if (value === 'sharing' && !stackPreset.value) loadStackPreset();
});

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
  envError.value = null;
  try {
    await api.generateRun('all');
    await verifyGenerator();
  } catch (e) {
    envError.value = e;
  } finally {
    verifying.value = false;
  }
}
const autostart = ref(false);

const dirty = computed(() => Object.keys(edits.value).length > 0);

/**
 * The stack-shaping settings, as controls rather than as rows in a key table.
 *
 * These were editable before — every key is, in the .env pane — but a boolean
 * you set by typing the word `true` is an escape hatch, not a setting. What
 * makes this a form is that the type is known: a switch cannot be set to
 * `ture`, a list edits as chips, and the domain suffix is checked before it
 * reaches a routing label nobody would think to look at.
 */
const defaults = ref({});
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
 * The hosts file, as one list rather than one broken domain at a time.
 *
 * Every domain here reaches the browser by name through the proxy, so every
 * one of them needs a line in `/etc/hosts` — and the app only ever offered to
 * add them from whichever page happened to notice one missing. A deleted
 * project's line had no route at all: it points at 127.0.0.1 forever and
 * nothing was looking for it.
 */
const hosts = ref(null);
const hostsFixing = ref(false);
const hostsMissing = computed(() => (hosts.value?.entries ?? []).filter((e) => !e.configured));

async function loadHosts() {
  hosts.value = await api.hostsOverview().catch(() => null);
}

/** Both directions in one elevation prompt: asking twice for one tidy-up is
 *  how people stop half way. */
async function fixHosts() {
  hostsFixing.value = true;
  envError.value = null;
  try {
    await api.hostsApply(
      hostsMissing.value.map((e) => e.domain),
      hosts.value?.stale ?? []
    );
    await loadHosts();
  } catch (e) {
    envError.value = e;
  } finally {
    hostsFixing.value = false;
  }
}

const proxy = ref(null);
const proxyDashboard = computed(() => (app.tld ? `https://traefik.${app.tld}/dashboard/` : null));

async function loadProxy() {
  // Its own container name, not a catalog id: `container_inspect` prefixes
  // `stackvo-` itself, and Traefik has no catalog entry to look up.
  proxy.value = await api.containerInspect('traefik').catch(() => null);
}

/**
 * The services pane: every service grouped by the category the catalog already
 * assigns, with its `.env` settings behind a sheet.
 *
 * Grouped rather than listed flat because twenty services in one column is a
 * scroll, not a choice — and the categories are already in the data, so
 * inventing a grouping here would be a second opinion about the same thing.
 */
const serviceTab = ref('all');
const sheetService = ref(null);
const sheetOpen = ref(false);

const serviceCategories = computed(() => {
  const seen = [...new Set((inventory.services ?? []).map((s) => s.category).filter(Boolean))];
  return ['all', ...seen.sort()];
});

const servicesInTab = computed(() => {
  const list = [...(inventory.services ?? [])].sort((a, b) => a.id.localeCompare(b.id));
  return serviceTab.value === 'all' ? list : list.filter((s) => s.category === serviceTab.value);
});

/**
 * A category's name in the reader's language.
 *
 * Spelled out rather than assembled from the slug, because the i18n check only
 * sees literal `t('…')` calls — a key built by interpolation reads as
 * unreachable there and as a missing translation here. The default returns the
 * slug itself, so a category added to the contract upstream shows its own name
 * instead of a blank chip.
 */
function categoryLabel(category) {
  switch (category) {
    case 'all':
      return t('serviceSettings.all');
    case 'databases':
      return t('serviceSettings.categories.databases');
    case 'cache':
      return t('serviceSettings.categories.cache');
    case 'queue':
      return t('serviceSettings.categories.queue');
    case 'search':
      return t('serviceSettings.categories.search');
    case 'monitoring':
      return t('serviceSettings.categories.monitoring');
    case 'devtools':
      return t('serviceSettings.categories.devtools');
    case 'adminUis':
      return t('serviceSettings.categories.adminUis');
    default:
      return category;
  }
}

function openService(service) {
  sheetService.value = service;
  sheetOpen.value = true;
}

function onServiceApplied() {
  inventory.loadServices();
}

const catalog = ref(null);

/**
 * The choices come from the catalog rather than from a list typed here, so a
 * PHP release added to the binary shows up without a second edit. Falling back
 * to the current value keeps the select from rendering blank if the catalog
 * call failed — a select whose only item is missing looks like data loss.
 */
const itemsFor = (key, versions) => {
  const current = effective(key);
  const list = versions?.length ? [...versions] : [];
  if (current && !list.includes(current)) list.unshift(current);
  return list;
};
const runtimeVersions = (id) => catalog.value?.runtimes?.find((r) => r.id === id)?.versions ?? [];
const phpVersionItems = computed(() =>
  itemsFor('SUPPORTED_LANGUAGES_PHP_DEFAULT', runtimeVersions('php'))
);
const nodeVersionItems = computed(() =>
  itemsFor('PHP_TOOL_NODEJS_VERSION', runtimeVersions('node'))
);
const serverItems = computed(() => itemsFor('SUPPORTED_SERVERS_DEFAULT', catalog.value?.servers));
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
 * The per-server directive file, edited here rather than only on disk.
 *
 * A text area and not a set of fields: what goes in is nginx's own grammar,
 * and pretending otherwise would mean a form that can express a fraction of it
 * and silently drops the rest.
 */
const CONFIGURABLE_SERVERS = ['nginx', 'caddy', 'frankenphp'];
const serverTab = ref('nginx');
const serverConfig = ref('');
const serverConfigSaved = ref('');
const serverConfigBusy = ref(false);
const serverConfigDirty = computed(() => serverConfig.value !== serverConfigSaved.value);

async function loadServerConfig() {
  serverConfigBusy.value = true;
  envError.value = null;
  try {
    serverConfig.value = await api.serverConfigGet(serverTab.value);
    serverConfigSaved.value = serverConfig.value;
  } catch (e) {
    envError.value = e;
  } finally {
    serverConfigBusy.value = false;
  }
}

async function saveServerConfig() {
  serverConfigBusy.value = true;
  envError.value = null;
  try {
    await api.serverConfigSet(serverTab.value, serverConfig.value);
    serverConfigSaved.value = serverConfig.value;
    // Directives reach a container only through a regenerate, the same as the
    // limits above — saying so is the difference between a feature that worked
    // and one the user believes did nothing.
    lastSaved.value = ['SERVER_CONFIG'];
  } catch (e) {
    envError.value = e;
  } finally {
    serverConfigBusy.value = false;
  }
}

watch(serverTab, loadServerConfig);

/**
 * Which shipped templates this workspace has taken over.
 *
 * The app renders from the copies compiled into its binary and reads the
 * workspace first, so a file under `core/` is an override and nothing else —
 * installing writes none. That is what makes this list answerable, and the
 * question it answers is a real one: an edit made months ago is invisible
 * until the stack stops matching what the documentation says it does.
 */
const templates = ref([]);
const templatesBusy = ref(false);
const templatesError = ref(null);
const templateBusy = ref(null);
const templateToOverride = ref(null);
const revertTarget = ref(null);

const overriddenTemplates = computed(() => templates.value.filter((f) => f.overridden));
const shippedTemplates = computed(() => templates.value.filter((f) => !f.overridden));

/**
 * Is this particular template the one being worked on?
 *
 * The emptiness check is the point. `templateBusy === templateToOverride` read
 * correctly and was wrong for the state the pane opens in: both are null, null
 * equals null, and the button sat there spinning before anyone had chosen a
 * file — and again after every successful override, which clears the selection.
 * Idle is not a path, so it can never be the busy one.
 */
const busyWith = (path) => !!path && templateBusy.value === path;

async function loadTemplates() {
  templatesBusy.value = true;
  templatesError.value = null;
  try {
    templates.value = asList(await api.templatesList());
  } catch (e) {
    templatesError.value = e;
  } finally {
    templatesBusy.value = false;
  }
}

/**
 * Copy the shipped file in, then open it in the user's own editor.
 *
 * Not a textarea in this pane: these are compose fragments and server configs,
 * and the tool for editing YAML is the one they already have open.
 */
async function overrideTemplate() {
  const path = templateToOverride.value;
  if (!path) return;

  templateBusy.value = path;
  templatesError.value = null;
  try {
    const absolute = await api.templateOverride(path);
    await loadTemplates();
    templateToOverride.value = null;
    await api.openInEditor(absolute).catch(() => {});
  } catch (e) {
    templatesError.value = e;
  } finally {
    templateBusy.value = null;
  }
}

function openTemplate(path) {
  const root = app.workspace?.root;
  if (root) api.openInEditor(`${root}/${path}`).catch(() => {});
}

/** Deletes the user's edit. Confirmed in the dialog, not here. */
async function revertTemplate() {
  const path = revertTarget.value;
  revertTarget.value = null;
  if (!path) return;

  templateBusy.value = path;
  templatesError.value = null;
  try {
    await api.templateRevert(path);
    await loadTemplates();
  } catch (e) {
    templatesError.value = e;
  } finally {
    templateBusy.value = null;
  }
}

/**
 * The nginx directives the form offers, mirroring the table in the generator.
 *
 * Ports are absent on purpose and it is worth saying why: the container
 * listens on 80 and Traefik terminates TLS, so a port field here would
 * contradict the routing label pointing at it. Modules and the server root are
 * likewise the image's and the container's, not settings.
 */
const NGINX_FIELDS = [
  { key: 'SERVER_MAX_BODY_SIZE', kind: 'size', icon: 'mdi-upload' },
  { key: 'SERVER_CLIENT_BODY_TIMEOUT', kind: 'seconds', icon: 'mdi-timer-sand' },
  { key: 'SERVER_KEEPALIVE_TIMEOUT', kind: 'seconds', icon: 'mdi-lan-connect' },
  { key: 'SERVER_FASTCGI_CONNECT_TIMEOUT', kind: 'seconds', icon: 'mdi-transit-connection' },
  { key: 'SERVER_FASTCGI_SEND_TIMEOUT', kind: 'seconds', icon: 'mdi-upload-network' },
  { key: 'SERVER_FASTCGI_TIMEOUT', kind: 'seconds', icon: 'mdi-timer-outline' },
];
const NGINX_SWITCHES = [
  { key: 'SERVER_TCP_NODELAY', on: 'on', off: 'off' },
  { key: 'SERVER_GZIP', on: 'on', off: 'off' },
];

const onOff = (key) => effective(key) === 'on';
const setOnOff = (key, value) => edit(key, value ? 'on' : 'off');
const gzipOn = computed(() => onOff('SERVER_GZIP'));

/**
 * Which servers have a generated config file, and so can take directives.
 *
 * FrankenPHP was `false` here and it was simply wrong: it writes a `Caddyfile`
 * exactly as caddy does. Sitting greyed out beside Apache and Swoole — whose
 * exclusion the note underneath explains — made an oversight look like a
 * decision somebody had made.
 *
 * Not the same question as the request limits above, which reach nginx and
 * caddy only; the note says which is which rather than this map pretending one
 * flag answers both.
 */
const SERVER_SUPPORT = {
  nginx: true,
  caddy: true,
  frankenphp: true,
  apache: false,
  swoole: false,
};

const sizeRules = [
  (v) =>
    !String(v ?? '').trim() ||
    /^\d+[kKmMgG]?$/.test(String(v).trim()) ||
    t('settings.servers.sizeInvalid'),
];
const secondsRules = [
  (v) =>
    !String(v ?? '').trim() ||
    /^\d+$/.test(String(v).trim()) ||
    t('settings.servers.secondsInvalid'),
];

const RUNTIME_DEFAULTS = [
  { id: 'python', key: 'SUPPORTED_LANGUAGES_PYTHON_DEFAULT', icon: 'mdi-language-python' },
  { id: 'go', key: 'SUPPORTED_LANGUAGES_GO_DEFAULT', icon: 'mdi-language-go' },
  { id: 'ruby', key: 'SUPPORTED_LANGUAGES_RUBY_DEFAULT', icon: 'mdi-language-ruby' },
  { id: 'rust', key: 'SUPPORTED_LANGUAGES_RUST_DEFAULT', icon: 'mdi-language-rust' },
  { id: 'node', key: 'SUPPORTED_LANGUAGES_NODEJS_DEFAULT', icon: 'mdi-nodejs' },
];
const runtimeItems = (runtime) => itemsFor(runtime.key, runtimeVersions(runtime.id));

const effective = (key) => edits.value[key] ?? env.value[key] ?? defaults.value[key] ?? '';
const isDefault = (key) => effective(key) === defaults.value[key];
const resetToDefault = (key) => edit(key, defaults.value[key] ?? '');

const boolOf = (key) => effective(key) === 'true';
const setBool = (key, on) => edit(key, on ? 'true' : 'false');

const listOf = (key) =>
  effective(key)
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
const setList = (key, items) =>
  edit(
    key,
    items
      .map((s) => String(s).trim())
      .filter(Boolean)
      .join(',')
  );

/**
 * A suffix, not a URL. It is concatenated straight into `Host(\`x.SUFFIX\`)`,
 * so a leading dot or a scheme produces a route that silently never matches —
 * the stack comes up and nothing resolves.
 */
const suffixRules = [
  (v) => !!String(v ?? '').trim() || t('settings.shape.suffixRequired'),
  (v) =>
    /^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$/.test(String(v ?? '').trim()) ||
    t('settings.shape.suffixInvalid'),
];

/**
 * The suffix, split where people actually think about it.
 *
 * `stackvo.loc` is a namespace and a TLD, and only the second half is what
 * someone means by "can I use .dev instead". Split on the last dot: everything
 * before it is the label, which may itself contain dots.
 */
const TLD_CHOICES = ['loc', 'test', 'localhost', 'dev'];
const splitSuffix = (value) => {
  const text = String(value ?? '').trim();
  const at = text.lastIndexOf('.');
  return at === -1
    ? { label: '', tld: text }
    : { label: text.slice(0, at), tld: text.slice(at + 1) };
};
const suffixLabel = computed(() => splitSuffix(effective('DEFAULT_TLD_SUFFIX')).label);
const suffixTld = computed(() => splitSuffix(effective('DEFAULT_TLD_SUFFIX')).tld);
const setSuffix = (label, tld) => {
  const parts = [String(label ?? '').trim(), String(tld ?? '').trim()].filter(Boolean);
  edit('DEFAULT_TLD_SUFFIX', parts.join('.'));
};

const PART = /^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$/;
const suffixLabelRules = [
  (v) =>
    !String(v ?? '').trim() || PART.test(String(v).trim()) || t('settings.shape.suffixInvalid'),
];
const suffixTldRules = [
  (v) => !!String(v ?? '').trim() || t('settings.shape.suffixRequired'),
  (v) => PART.test(String(v ?? '').trim()) || t('settings.shape.suffixInvalid'),
];

/**
 * Choosing an HSTS-preloaded TLD for the whole stack with HTTPS off breaks
 * every address at once, not just one project's — so it is said here too.
 */
const suffixNeedsHttps = computed(
  () => HTTPS_ONLY_SUFFIXES.includes(suffixTld.value.toLowerCase()) && !boolOf('SSL_ENABLE')
);
const networkRules = [
  (v) => !!String(v ?? '').trim() || t('settings.shape.networkRequired'),
  (v) =>
    /^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(String(v ?? '').trim()) ||
    t('settings.shape.networkInvalid'),
];
const shapeValid = computed(
  () =>
    suffixRules.every((r) => r(effective('DEFAULT_TLD_SUFFIX')) === true) &&
    networkRules.every((r) => r(effective('DOCKER_DEFAULT_NETWORK')) === true)
);

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

async function loadDefaults() {
  defaults.value = await api.envDefaults().catch(() => ({}));
  catalog.value = await api.catalogGet().catch(() => null);
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

/**
 * Keys the last save wrote, so the pane can say what has to happen next.
 *
 * Changing the suffix rewrites every routing label and moves what the
 * certificate has to cover, but none of that reaches the running stack until
 * the files are regenerated. Saving and staying silent is how a setting looks
 * like it did nothing.
 */
const lastSaved = ref([]);
const ROUTING_KEYS = [
  'DEFAULT_TLD_SUFFIX',
  'DOCKER_DEFAULT_NETWORK',
  'SSL_ENABLE',
  'REDIRECT_TO_HTTPS',
];
const routingChanged = computed(() => lastSaved.value.some((k) => ROUTING_KEYS.includes(k)));
/** Clears the notice only if the regenerate actually succeeded. */
async function regenerateAfterChange() {
  await stackAction(() => api.generateRun('all'));
  if (!envError.value) lastSaved.value = [];
}

const suffixChanged = computed(() => lastSaved.value.includes('DEFAULT_TLD_SUFFIX'));

async function save() {
  saving.value = true;
  envError.value = null;
  saved.value = false;
  try {
    const keys = Object.keys(edits.value);
    await api.envSet({ ...edits.value });
    lastSaved.value = keys;
    await loadEnv();
    await app.refreshTld();
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

/**
 * Trust the CA by opening a terminal.
 *
 * The app cannot do it: macOS grants the authorization for trust settings only
 * interactively, and a background child process of a windowed app is not
 * somewhere it will ask.
 */
async function trustCaInTerminal() {
  try {
    await api.certTrustInTerminal();
  } catch (e) {
    envError.value = e;
  }
}
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
    // `false`: the trust write is its own button now. Asking for it here made
    // every reissue return an error about something the user had not asked for.
    const applied = await api.certApply(false);
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
  loadDefaults();
  loadProxy();
  loadHosts();
  loadServerConfig();
  loadTemplates();
  loadPrefs();
  loadCerts();
  verifyGenerator();
  appVersion.value = await getVersion().catch(() => '');
  updaterReady.value = await updatesConfigured();
  logs.value = await api.logsInfo().catch(() => null);
  // Every key the pickers read has to be present in the fallback too — a
  // missing `browsers` leaves that select bound to undefined instead of empty.
  apps.value = await api
    .appsAvailable()
    .catch(() => ({ terminals: [], editors: [], browsers: [] }));
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
                  :model-value="appChoice(prefs?.terminalApp, apps.terminals)"
                  :items="apps.terminals"
                  item-title="name"
                  item-value="id"
                  :item-props="appItemProps"
                  :label="t('settings.terminalApp')"
                  :hint="t('settings.appsHint')"
                  persistent-hint
                  clearable
                  @update:model-value="(v) => setPref({ terminalApp: v || null })"
                />
                <v-select
                  :model-value="appChoice(prefs?.editorCommand, apps.editors)"
                  :items="apps.editors"
                  item-title="name"
                  item-value="id"
                  :item-props="appItemProps"
                  :label="t('settings.editorApp')"
                  clearable
                  @update:model-value="(v) => setPref({ editorCommand: v || null })"
                />
                <!-- Every "visit" button in the app goes through this. Cleared
                     means the system default, which is why the list carries an
                     explicit entry for it rather than only an empty state. -->
                <v-select
                  :model-value="appChoice(prefs?.browserCommand, apps.browsers)"
                  :items="apps.browsers"
                  item-title="name"
                  item-value="id"
                  :item-props="appItemProps"
                  :label="t('settings.browserApp')"
                  :hint="t('settings.browserAppHint')"
                  persistent-hint
                  clearable
                  @update:model-value="(v) => setPref({ browserCommand: v || null })"
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
          <!-- These shape every generated file. They live in the binary as
               defaults, so a fresh .env has none of them; changing one here
               writes the key, which is what makes a line in that file mean
               something. -->
          <template v-if="tab === 'domain'">
            <SettingsGroup
              icon="mdi-web"
              :title="t('settings.shape.addressTitle')"
              :description="t('settings.shape.addressDesc')"
            >
              <template #append>
                <v-btn
                  v-if="dirty"
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-content-save-outline"
                  :loading="saving"
                  :disabled="!shapeValid"
                  @click="save"
                >
                  {{ t('settings.save', { count: Object.keys(edits).length }) }}
                </v-btn>
                <v-chip v-else-if="saved" color="success" size="small">
                  {{ t('settings.saved') }}
                </v-chip>
              </template>

              <!-- Two fields for one key. The suffix is a namespace and a TLD
                   glued together, and only the TLD is the part people mean
                   when they ask to swap .loc for .dev — as one input the two
                   are indistinguishable from a raw .env row. -->
              <v-row dense align="start">
                <v-col cols="12" sm="6">
                  <v-text-field
                    :model-value="suffixLabel"
                    :label="t('settings.shape.suffixLabel')"
                    :hint="t('settings.shape.suffixLabelHint')"
                    :rules="suffixLabelRules"
                    prepend-inner-icon="mdi-tag-outline"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    @update:model-value="(v) => setSuffix(v, suffixTld)"
                  />
                </v-col>
                <v-col cols="12" sm="6">
                  <v-combobox
                    :model-value="suffixTld"
                    :items="TLD_CHOICES"
                    :label="t('settings.shape.suffixTld')"
                    :hint="t('settings.shape.suffixTldHint')"
                    :rules="suffixTldRules"
                    prepend-inner-icon="mdi-web"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    @update:model-value="(v) => setSuffix(suffixLabel, v ?? '')"
                  />
                </v-col>
              </v-row>

              <!-- What the two fields actually produce. The suffix is never
                   seen on its own: it is always something dot this. -->
              <div class="d-flex align-center ga-2 mt-2 flex-wrap">
                <span class="text-caption text-medium-emphasis">
                  {{ t('settings.shape.preview') }}
                </span>
                <v-chip size="small" variant="tonal" prepend-icon="mdi-folder-outline">
                  shop.{{ effective('DEFAULT_TLD_SUFFIX') }}
                </v-chip>
                <v-chip size="small" variant="tonal" prepend-icon="mdi-database-outline">
                  phpmyadmin.{{ effective('DEFAULT_TLD_SUFFIX') }}
                </v-chip>
                <v-btn
                  v-if="!isDefault('DEFAULT_TLD_SUFFIX')"
                  size="x-small"
                  variant="text"
                  prepend-icon="mdi-restore"
                  @click="resetToDefault('DEFAULT_TLD_SUFFIX')"
                >
                  {{ t('settings.shape.reset') }}
                </v-btn>
              </div>

              <v-alert
                v-if="suffixNeedsHttps"
                type="warning"
                variant="tonal"
                density="comfortable"
                class="mt-3"
                :text="t('settings.shape.suffixHsts')"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-file-document-outline"
              :title="t('settings.shape.hostsTitle')"
              :description="t('settings.shape.hostsDesc')"
            >
              <template #append>
                <v-btn
                  v-if="hostsMissing.length || hosts?.stale.length"
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-wrench-outline"
                  :loading="hostsFixing"
                  @click="fixHosts"
                >
                  {{ t('settings.shape.hostsFix') }}
                </v-btn>
                <v-chip v-else size="small" color="success" variant="tonal">
                  {{ t('settings.shape.hostsOk') }}
                </v-chip>
              </template>

              <div v-if="!hosts" class="text-caption text-medium-emphasis">
                {{ t('app.loading') }}
              </div>

              <template v-else>
                <div
                  v-for="entry in hosts.entries"
                  :key="entry.domain"
                  class="d-flex align-center ga-2 py-1"
                >
                  <v-icon
                    size="small"
                    :color="entry.configured ? 'success' : 'warning'"
                    :icon="entry.configured ? 'mdi-check-circle' : 'mdi-alert-circle'"
                  />
                  <span class="text-caption break">{{ entry.domain }}</span>
                  <!-- Whose line it is decides whether this app may remove it. -->
                  <v-chip
                    v-if="!entry.managedByStackvo && entry.configured"
                    size="x-small"
                    variant="tonal"
                  >
                    {{ t('settings.shape.hostsManual') }}
                  </v-chip>
                </div>

                <template v-if="hosts.stale.length">
                  <v-divider class="my-3" />
                  <div class="text-caption text-medium-emphasis mb-1">
                    {{ t('settings.shape.hostsStale') }}
                  </div>
                  <v-chip
                    v-for="d in hosts.stale"
                    :key="d"
                    size="x-small"
                    variant="tonal"
                    class="mr-1 mb-1"
                  >
                    {{ d }}
                  </v-chip>
                </template>
              </template>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-transit-connection-variant"
              :title="t('settings.shape.proxyTitle')"
              :description="t('settings.shape.proxyDesc')"
            >
              <template #append>
                <v-chip size="small" :color="proxy?.running ? 'success' : 'error'">
                  {{ proxy?.running ? t('engine.running') : t('engine.down') }}
                </v-chip>
              </template>

              <div class="d-flex justify-space-between py-1 ga-4">
                <span class="text-caption text-medium-emphasis">{{ t('about.docker') }}</span>
                <span class="text-caption text-right break">{{ proxy?.image || '—' }}</span>
              </div>
              <div class="d-flex justify-space-between py-1 ga-4">
                <span class="text-caption text-medium-emphasis">
                  {{ t('settings.shape.proxyPorts') }}
                </span>
                <span class="text-caption text-right break">
                  {{ (proxy?.ports ?? []).map((p) => p.host ?? p.container).join(', ') || '—' }}
                </span>
              </div>

              <!-- The dashboard needs a hosts entry like any other domain, and
                   until recently nothing offered one — which is why it is worth
                   a button rather than a sentence telling you the address. -->
              <v-btn
                v-if="proxyDashboard"
                size="small"
                variant="tonal"
                prepend-icon="mdi-view-dashboard-outline"
                class="mt-3"
                :disabled="!proxy?.running"
                @click="api.openInBrowser(proxyDashboard)"
              >
                {{ t('settings.shape.proxyDashboard') }}
              </v-btn>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-lan"
              :title="t('settings.shape.networkTitle')"
              :description="t('settings.shape.networkGroupDesc')"
            >
              <v-row dense>
                <v-col cols="12" md="6">
                  <v-text-field
                    :model-value="effective('DOCKER_DEFAULT_NETWORK')"
                    :label="t('settings.shape.network')"
                    :hint="t('settings.shape.networkHint')"
                    :rules="networkRules"
                    prepend-inner-icon="mdi-lan"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    @update:model-value="(v) => edit('DOCKER_DEFAULT_NETWORK', v)"
                  >
                    <template #append-inner>
                      <v-tooltip
                        v-if="!isDefault('DOCKER_DEFAULT_NETWORK')"
                        :text="t('settings.shape.reset')"
                        location="top"
                      >
                        <template #activator="{ props: tip }">
                          <v-btn
                            v-bind="tip"
                            size="x-small"
                            variant="text"
                            icon="mdi-restore"
                            :aria-label="t('settings.shape.reset')"
                            @click="resetToDefault('DOCKER_DEFAULT_NETWORK')"
                          />
                        </template>
                      </v-tooltip>
                    </template>
                  </v-text-field>
                </v-col>
              </v-row>

              <v-divider class="my-3" />

              <v-switch
                :model-value="boolOf('SSL_ENABLE')"
                :label="t('settings.shape.ssl')"
                :messages="t('settings.shape.sslHint')"
                color="primary"
                density="comfortable"
                hide-details="auto"
                @update:model-value="(v) => setBool('SSL_ENABLE', v)"
              />

              <!-- The generator already reports this — `traefik_routing_warning`
                   returns it and it lands in the generate report — but the
                   report is not where the decision is made. Every router the
                   generator writes targets the `websecure` entry point, and
                   that entry point is only written when this is on, so turning
                   it off produces a pair of files that disagree and a stack
                   where nothing resolves. Said beside the switch that causes
                   it. -->
              <v-alert
                v-if="!boolOf('SSL_ENABLE')"
                type="warning"
                variant="tonal"
                density="comfortable"
                class="mt-3"
                :text="t('settings.shape.sslOffBreaksRouting')"
              />

              <!-- Saved, but not yet true of the running stack. The routing
                   labels are baked into generated files, so until those are
                   rewritten the old suffix is still what Traefik matches on. -->
              <v-alert
                v-if="routingChanged"
                type="info"
                variant="tonal"
                density="comfortable"
                class="mt-4"
              >
                <div class="text-body-2">{{ t('settings.shape.thenRegenerate') }}</div>
                <div v-if="suffixChanged" class="text-caption text-medium-emphasis mt-1">
                  {{ t('settings.shape.thenCertificates') }}
                </div>
                <template #append>
                  <v-btn
                    size="small"
                    variant="tonal"
                    prepend-icon="mdi-cog-sync-outline"
                    :loading="stackBusy"
                    @click="regenerateAfterChange"
                  >
                    {{ t('settings.shape.regenerate') }}
                  </v-btn>
                </template>
              </v-alert>

              <!-- Redirecting to a scheme that is switched off is a dead end,
                   so the dependent control cannot be left on by itself. -->
              <v-switch
                :model-value="boolOf('SSL_ENABLE') && boolOf('REDIRECT_TO_HTTPS')"
                :disabled="!boolOf('SSL_ENABLE')"
                :label="t('settings.shape.redirect')"
                :messages="
                  boolOf('SSL_ENABLE')
                    ? t('settings.shape.redirectHint')
                    : t('settings.shape.redirectBlocked')
                "
                color="primary"
                density="comfortable"
                hide-details="auto"
                @update:model-value="(v) => setBool('REDIRECT_TO_HTTPS', v)"
              />
            </SettingsGroup>
          </template>

          <template v-if="tab === 'php'">
            <SettingsGroup
              icon="mdi-code-braces"
              :title="t('settings.defaults.runtimes')"
              :description="t('settings.runtimes.desc')"
            >
              <template #append>
                <v-btn
                  v-if="dirty"
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-content-save-outline"
                  :loading="saving"
                  @click="save"
                >
                  {{ t('settings.save', { count: Object.keys(edits).length }) }}
                </v-btn>
                <v-chip v-else-if="saved" color="success" size="small">
                  {{ t('settings.saved') }}
                </v-chip>
              </template>

              <v-row dense>
                <v-col v-for="r in RUNTIME_DEFAULTS" :key="r.id" cols="12" sm="6">
                  <v-select
                    :model-value="effective(r.key)"
                    :items="runtimeItems(r)"
                    :label="r.id"
                    :prepend-inner-icon="r.icon"
                    density="comfortable"
                    variant="outlined"
                    hide-details
                    @update:model-value="(v) => edit(r.key, v)"
                  />
                </v-col>
              </v-row>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-tag-outline"
              :title="t('settings.defaults.php')"
              :description="t('settings.php.versionDesc')"
            >
              <template #append>
                <v-btn
                  v-if="dirty"
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-content-save-outline"
                  :loading="saving"
                  @click="save"
                >
                  {{ t('settings.save', { count: Object.keys(edits).length }) }}
                </v-btn>
                <v-chip v-else-if="saved" color="success" size="small">
                  {{ t('settings.saved') }}
                </v-chip>
              </template>

              <v-row dense>
                <v-col cols="12" md="6">
                  <v-select
                    :model-value="effective('SUPPORTED_LANGUAGES_PHP_DEFAULT')"
                    :items="phpVersionItems"
                    :label="t('settings.php.version')"
                    :hint="t('settings.php.versionHint')"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    prepend-inner-icon="mdi-language-php"
                    @update:model-value="(v) => edit('SUPPORTED_LANGUAGES_PHP_DEFAULT', v)"
                  />
                </v-col>
                <v-col cols="12" md="6">
                  <v-select
                    :model-value="effective('SUPPORTED_SERVERS_DEFAULT')"
                    :items="serverItems"
                    :label="t('settings.php.server')"
                    :hint="t('settings.php.serverHint')"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    prepend-inner-icon="mdi-server"
                    @update:model-value="(v) => edit('SUPPORTED_SERVERS_DEFAULT', v)"
                  />
                </v-col>
              </v-row>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-hammer-wrench"
              :title="t('settings.defaults.phpTools')"
              :description="t('settings.shape.phpDesc')"
            >
              <v-row dense class="mb-1">
                <v-col cols="12" md="6">
                  <v-combobox
                    :model-value="effective('PHP_TOOL_COMPOSER_VERSION')"
                    :items="['latest']"
                    :label="t('settings.php.composer')"
                    :hint="t('settings.php.composerHint')"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    prepend-inner-icon="mdi-package-variant"
                    @update:model-value="(v) => edit('PHP_TOOL_COMPOSER_VERSION', v ?? '')"
                  />
                </v-col>
                <v-col cols="12" md="6">
                  <v-select
                    :model-value="effective('PHP_TOOL_NODEJS_VERSION')"
                    :items="nodeVersionItems"
                    :label="t('settings.php.nodejs')"
                    :hint="t('settings.php.nodejsHint')"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    prepend-inner-icon="mdi-nodejs"
                    @update:model-value="(v) => edit('PHP_TOOL_NODEJS_VERSION', v)"
                  />
                </v-col>
              </v-row>

              <v-combobox
                :model-value="listOf('PHP_DEFAULT_TOOLS')"
                :label="t('settings.shape.tools')"
                :hint="t('settings.shape.toolsHint')"
                multiple
                chips
                closable-chips
                persistent-hint
                density="comfortable"
                variant="outlined"
                class="mb-3"
                @update:model-value="(v) => setList('PHP_DEFAULT_TOOLS', v)"
              />
              <v-combobox
                :model-value="listOf('PHP_DEFAULT_APT_PACKAGES')"
                :label="t('settings.shape.apt')"
                :hint="t('settings.shape.aptHint')"
                multiple
                chips
                closable-chips
                persistent-hint
                density="comfortable"
                variant="outlined"
                @update:model-value="(v) => setList('PHP_DEFAULT_APT_PACKAGES', v)"
              />
            </SettingsGroup>
          </template>

          <!-- ---- stack preset ----------------------------------------------- -->
          <!-- The stack, made portable. `stackvo.json` is already in the
               teammate's clone; which services are on and at which versions is
               not, because that lives in .env and .env is where the passwords
               are. A preset carries the first half and, by construction, has
               nowhere to put the second. -->

          <!-- ---- runtimes --------------------------------------------------- -->

          <!-- ---- servers ---------------------------------------------------- -->
          <template v-if="tab === 'servers'">
            <SettingsGroup
              icon="mdi-web-box"
              :title="t('settings.servers.limits')"
              :description="t('settings.servers.limitsDesc')"
            >
              <template #append>
                <v-btn
                  v-if="dirty"
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-content-save-outline"
                  :loading="saving"
                  @click="save"
                >
                  {{ t('settings.save', { count: Object.keys(edits).length }) }}
                </v-btn>
                <v-chip v-else-if="saved" color="success" size="small">
                  {{ t('settings.saved') }}
                </v-chip>
              </template>

              <v-row dense>
                <v-col v-for="f in NGINX_FIELDS" :key="f.key" cols="12" sm="6" md="4">
                  <v-text-field
                    :model-value="effective(f.key)"
                    :label="t(`settings.servers.field.${f.key}`)"
                    :rules="f.kind === 'size' ? sizeRules : secondsRules"
                    :suffix="f.kind === 'seconds' ? 's' : undefined"
                    :prepend-inner-icon="f.icon"
                    density="comfortable"
                    variant="outlined"
                    hide-details="auto"
                    @update:model-value="(v) => edit(f.key, v)"
                  />
                </v-col>
              </v-row>

              <v-divider class="my-4" />

              <div class="d-flex ga-6 flex-wrap">
                <v-switch
                  v-for="sw in NGINX_SWITCHES"
                  :key="sw.key"
                  :model-value="onOff(sw.key)"
                  :label="t(`settings.servers.field.${sw.key}`)"
                  color="primary"
                  density="comfortable"
                  hide-details
                  @update:model-value="(v) => setOnOff(sw.key, v)"
                />
              </div>

              <!-- Only meaningful once compression is on, so it appears with
                   it rather than sitting greyed out asking to be understood. -->
              <v-row v-if="gzipOn" dense class="mt-2">
                <v-col cols="12" sm="4">
                  <v-text-field
                    :model-value="effective('SERVER_GZIP_COMP_LEVEL')"
                    :label="t('settings.servers.field.SERVER_GZIP_COMP_LEVEL')"
                    type="number"
                    min="1"
                    max="9"
                    density="comfortable"
                    variant="outlined"
                    hide-details="auto"
                    @update:model-value="(v) => edit('SERVER_GZIP_COMP_LEVEL', v)"
                  />
                </v-col>
                <v-col cols="12" sm="8">
                  <v-text-field
                    :model-value="effective('SERVER_GZIP_TYPES')"
                    :label="t('settings.servers.field.SERVER_GZIP_TYPES')"
                    :hint="t('settings.servers.gzipTypesHint')"
                    persistent-hint
                    density="comfortable"
                    variant="outlined"
                    @update:model-value="(v) => edit('SERVER_GZIP_TYPES', v)"
                  />
                </v-col>
              </v-row>

              <!-- The half people find last. An upload dies at whichever limit
                   is lowest, and PHP's are per project — raising one here and
                   not the other is the failure this note exists to prevent. -->
              <v-alert
                type="info"
                variant="tonal"
                density="comfortable"
                class="mt-3"
                :text="t('settings.servers.phpNote')"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-file-code-outline"
              :title="t('settings.servers.extra')"
              :description="t('settings.servers.extraDesc')"
            >
              <template #append>
                <v-btn
                  size="small"
                  variant="tonal"
                  color="primary"
                  prepend-icon="mdi-content-save-outline"
                  :disabled="!serverConfigDirty"
                  :loading="serverConfigBusy"
                  @click="saveServerConfig"
                >
                  {{ t('settings.save', { count: 1 }) }}
                </v-btn>
              </template>

              <v-tabs v-model="serverTab" density="compact" bg-color="transparent" class="mb-3">
                <v-tab v-for="srv in CONFIGURABLE_SERVERS" :key="srv" :value="srv">{{ srv }}</v-tab>
              </v-tabs>

              <v-textarea
                v-model="serverConfig"
                :placeholder="t('settings.servers.extraPlaceholder')"
                :hint="t('settings.servers.extraHint')"
                persistent-hint
                rows="12"
                variant="outlined"
                density="comfortable"
                class="server-config"
                spellcheck="false"
              />
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-server-network"
              :title="t('settings.servers.applies')"
              :description="t('settings.servers.appliesDesc')"
            >
              <div class="d-flex ga-2 flex-wrap">
                <v-chip
                  v-for="srv in catalog?.servers ?? []"
                  :key="srv"
                  size="small"
                  variant="tonal"
                  :prepend-icon="SERVER_SUPPORT[srv] ? 'mdi-check' : 'mdi-minus'"
                  :color="SERVER_SUPPORT[srv] ? 'primary' : undefined"
                >
                  {{ srv }}
                </v-chip>
              </div>
              <div class="text-caption text-medium-emphasis mt-2">
                {{ t('settings.servers.supportNote') }}
              </div>
            </SettingsGroup>
          </template>

          <!-- ---- services --------------------------------------------------- -->
          <template v-if="tab === 'services'">
            <SettingsGroup
              icon="mdi-cube-outline"
              :title="t('serviceSettings.pick')"
              :description="t('serviceSettings.desc')"
            >
              <div class="d-flex flex-column flex-md-row ga-4 align-start">
                <div class="services-main">
                  <v-list
                    v-if="servicesInTab.length"
                    density="comfortable"
                    class="pa-0 service-list"
                    bg-color="transparent"
                  >
                    <v-list-item
                      v-for="s in servicesInTab"
                      :key="s.id"
                      :title="s.id"
                      :subtitle="
                        s.version
                          ? `${categoryLabel(s.category)} · ${s.version}`
                          : categoryLabel(s.category)
                      "
                      rounded="lg"
                      class="mb-1"
                      @click="openService(s)"
                    >
                      <template #prepend>
                        <v-avatar
                          rounded="lg"
                          size="32"
                          :color="s.running ? 'success' : s.enabled ? 'warning' : 'surface-variant'"
                        >
                          <v-icon size="18" icon="mdi-cube-outline" />
                        </v-avatar>
                      </template>
                      <template #append>
                        <v-chip v-if="!s.enabled" size="x-small" variant="tonal" class="mr-2">
                          {{ t('serviceSettings.off') }}
                        </v-chip>
                        <v-icon icon="mdi-chevron-right" />
                      </template>
                    </v-list-item>
                  </v-list>

                  <v-alert
                    v-else
                    type="info"
                    variant="tonal"
                    density="comfortable"
                    :text="t('serviceSettings.empty')"
                  />
                </div>

                <!-- The filter reads as a sidebar to the thing it filters, so
                     it sits beside the list rather than above it. Sized to the
                     labels rather than to a share of the row: they are seven
                     short words, and a grid fraction gave them a third of the
                     pane to sit in. -->
                <v-tabs
                  v-model="serviceTab"
                  :direction="$vuetify.display.mdAndUp ? 'vertical' : 'horizontal'"
                  density="compact"
                  bg-color="transparent"
                  class="service-tabs"
                >
                  <v-tab v-for="c in serviceCategories" :key="c" :value="c">
                    {{ categoryLabel(c) }}
                  </v-tab>
                </v-tabs>
              </div>
            </SettingsGroup>
          </template>

          <!-- ---- doctor ----------------------------------------------------- -->
          <!-- Diagnosis next to its repair. The findings here used to surface
               one failed compose up at a time, each as an error about itself:
               "address already in use" with no word on by what. -->
          <!-- ---- workspace and stack control ------------------------------- -->
          <template v-if="tab === 'workspace'">
            <SettingsGroup
              icon="mdi-folder-open-outline"
              :title="t('settings.workspaceGroup')"
              :description="t('settings.workspaceGroupDesc')"
            >
              <!-- The one the user chose. -->
              <div class="text-body-2 break">
                {{ app.workspace?.projectsDir || t('workspace.none') }}
              </div>
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
                  v-if="app.workspace?.projectsDir"
                  size="small"
                  variant="text"
                  prepend-icon="mdi-open-in-new"
                  @click="api.openFolder(app.workspace.projectsDir)"
                >
                  {{ t('projects.openFolder') }}
                </v-btn>
              </div>

              <!-- And the one it never asks about. Shown because "where did my
                   compose file go" is a fair question, and because the answer
                   is a hidden directory nobody would find by looking. -->
              <v-divider class="my-4" />

              <div class="text-caption text-medium-emphasis">{{ t('workspace.appDir') }}</div>
              <div class="text-body-2 break mt-1">{{ app.workspace?.root }}</div>
              <div class="text-caption text-medium-emphasis mt-1">
                {{ t('workspace.appDirDesc') }}
              </div>
              <v-btn
                v-if="app.workspace?.root"
                size="small"
                variant="text"
                class="mt-2 ml-n2"
                prepend-icon="mdi-open-in-new"
                @click="api.openFolder(app.workspace.root)"
              >
                {{ t('projects.openFolder') }}
              </v-btn>
            </SettingsGroup>

            <!-- The templates are in the binary; a copy under `core/` exists
                 only because somebody made one here. That is what makes this
                 list possible at all — every workspace used to hold all thirty
                 files, so "has a copy" said nothing. -->
            <SettingsGroup
              icon="mdi-file-replace-outline"
              :title="t('settings.templates.title')"
              :description="t('settings.templates.description')"
            >
              <template #append>
                <v-btn
                  size="small"
                  variant="text"
                  prepend-icon="mdi-refresh"
                  :loading="templatesBusy"
                  @click="loadTemplates"
                >
                  {{ t('settings.templates.reload') }}
                </v-btn>
              </template>

              <v-alert
                v-if="templatesError"
                type="error"
                variant="tonal"
                density="comfortable"
                class="mb-3"
                :text="templatesError.message || String(templatesError)"
              />

              <div class="text-body-2 mb-3">
                {{
                  overriddenTemplates.length
                    ? t('settings.templates.count', {
                        count: overriddenTemplates.length,
                        total: templates.length,
                      })
                    : t('settings.templates.none', { total: templates.length })
                }}
              </div>

              <!-- Overridden ones first and always visible: they are the
                   answer to "why does my stack not match the docs", and a
                   forgotten edit is the reason that question gets asked. -->
              <v-list v-if="overriddenTemplates.length" density="compact" class="pa-0 mb-2">
                <v-list-item
                  v-for="file in overriddenTemplates"
                  :key="file.path"
                  class="px-0"
                  :title="file.path"
                >
                  <template #prepend>
                    <v-icon size="18" color="warning" class="mr-2">mdi-pencil</v-icon>
                  </template>
                  <template #append>
                    <v-btn
                      size="x-small"
                      variant="text"
                      prepend-icon="mdi-open-in-new"
                      @click="openTemplate(file.path)"
                    >
                      {{ t('settings.templates.open') }}
                    </v-btn>
                    <v-btn
                      size="x-small"
                      variant="text"
                      color="error"
                      prepend-icon="mdi-undo-variant"
                      :loading="busyWith(file.path)"
                      @click="revertTarget = file.path"
                    >
                      {{ t('settings.templates.revert') }}
                    </v-btn>
                  </template>
                </v-list-item>
              </v-list>

              <v-select
                v-model="templateToOverride"
                :items="shippedTemplates"
                item-title="path"
                item-value="path"
                :label="t('settings.templates.pick')"
                :hint="t('settings.templates.pickHint')"
                persistent-hint
                variant="outlined"
                density="comfortable"
                hide-no-data
              />
              <v-btn
                size="small"
                variant="tonal"
                color="primary"
                prepend-icon="mdi-file-edit-outline"
                class="mt-3"
                :disabled="!templateToOverride"
                :loading="busyWith(templateToOverride)"
                @click="overrideTemplate"
              >
                {{ t('settings.templates.override') }}
              </v-btn>
            </SettingsGroup>

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

                <!-- One generator, no selector: the Rust engine took over
                     after reaching byte parity on every file, and the report
                     above is now a drift check — does the disk still hold
                     what this generator would write? -->
                <v-btn size="small" variant="tonal" block :loading="verifying" @click="runGenerate">
                  {{ t('actions.generate') }}
                </v-btn>
              </template>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-export-variant"
              :title="t('stackPreset.export')"
              :description="t('stackPreset.exportDesc')"
            >
              <div class="d-flex ga-2 align-start">
                <v-text-field
                  v-model="stackPresetName"
                  :label="t('stackPreset.name')"
                  :placeholder="t('stackPreset.namePlaceholder')"
                  persistent-placeholder
                  hide-details
                />
                <v-btn
                  color="primary"
                  variant="flat"
                  :loading="stackPresetBusy"
                  @click="exportStackPreset"
                >
                  {{ t('stackPreset.saveFile') }}
                </v-btn>
              </div>

              <div v-if="stackPreset" class="text-caption text-medium-emphasis mt-3">
                {{
                  t('stackPreset.summary', {
                    enabled: stackPresetEnabled,
                    total: Object.keys(stackPreset.services).length,
                  })
                }}
              </div>

              <!-- Shown, not just written. The reason to read it is the reason
                   to trust it: there is no password in there because there is
                   nowhere in the format to put one. -->
              <v-expansion-panels v-if="stackPresetJson" variant="accordion" class="mt-3">
                <v-expansion-panel :title="t('stackPreset.preview')">
                  <v-expansion-panel-text>
                    <pre class="preset-json">{{ stackPresetJson }}</pre>
                  </v-expansion-panel-text>
                </v-expansion-panel>
              </v-expansion-panels>
            </SettingsGroup>

            <SettingsGroup
              icon="mdi-import"
              :title="t('stackPreset.import')"
              :description="t('stackPreset.importDesc')"
            >
              <v-btn variant="tonal" :loading="stackPresetBusy" @click="chooseStackPreset">
                {{ t('stackPreset.chooseFile') }}
              </v-btn>

              <template v-if="stackPresetPlan">
                <div class="text-body-2 mt-4">
                  <strong>{{ stackPresetPlan.name || t('stackPreset.untitled') }}</strong>
                  <span v-if="stackPresetPlan.description" class="text-medium-emphasis">
                    — {{ stackPresetPlan.description }}
                  </span>
                </div>

                <!-- "Nothing to do" and "everything was rejected" both produce
                     an empty change list and need opposite responses, so the
                     unchanged count is what tells them apart. -->
                <v-alert
                  v-if="stackPresetApplied"
                  type="success"
                  variant="tonal"
                  class="mt-3"
                  :text="t('stackPreset.applied')"
                />
                <v-alert
                  v-else-if="!stackPresetPlan.changes.length"
                  type="info"
                  variant="tonal"
                  class="mt-3"
                  :text="
                    stackPresetPlan.unchanged
                      ? t('stackPreset.alreadyMatches', { n: stackPresetPlan.unchanged })
                      : t('stackPreset.nothingUsable')
                  "
                />

                <v-table v-if="stackPresetPlan.changes.length" density="compact" class="mt-3">
                  <thead>
                    <tr>
                      <th>{{ t('stackPreset.colSubject') }}</th>
                      <th>{{ t('stackPreset.colFrom') }}</th>
                      <th>{{ t('stackPreset.colTo') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="change in stackPresetPlan.changes" :key="change.key">
                      <td>
                        <div>{{ change.subject }}</div>
                        <div class="text-caption text-medium-emphasis mono">{{ change.key }}</div>
                      </td>
                      <td class="mono text-medium-emphasis">
                        {{ change.from ?? t('stackPreset.absent') }}
                      </td>
                      <td class="mono">{{ change.to }}</td>
                    </tr>
                  </tbody>
                </v-table>

                <!-- Named, never silently dropped: a preset that quietly skips
                     half of what it was given is how somebody concludes it
                     worked and then loses an afternoon to the service it
                     ignored. -->
                <v-alert
                  v-if="stackPresetPlan.rejected.length"
                  type="warning"
                  variant="tonal"
                  class="mt-3"
                >
                  <div class="text-caption font-weight-medium mb-1">
                    {{ t('stackPreset.rejected') }}
                  </div>
                  <div v-for="line in stackPresetPlan.rejected" :key="line" class="text-caption">
                    {{ line }}
                  </div>
                </v-alert>

                <div class="d-flex ga-2 align-center mt-4">
                  <v-btn
                    v-if="!stackPresetApplied && stackPresetPlan.changes.length"
                    color="primary"
                    variant="flat"
                    :loading="stackPresetBusy"
                    @click="applyStackPreset"
                  >
                    {{ t('stackPreset.apply', { n: stackPresetPlan.changes.length }) }}
                  </v-btn>
                  <v-btn variant="text" :disabled="stackPresetBusy" @click="clearStackPresetPlan">
                    {{ stackPresetApplied ? t('app.close') : t('app.cancel') }}
                  </v-btn>
                </div>

                <!-- Enabling a service changes what the generator emits, so the
                     import is not live until regenerate-then-up. Saying so here
                     is the difference between a feature that worked and one the
                     user believes did nothing. -->
                <div
                  v-if="stackPresetApplied && stackPresetPlan.needsRegenerate"
                  class="text-caption text-medium-emphasis mt-2"
                >
                  {{ t('stackPreset.thenRegenerate') }}
                </div>
              </template>
            </SettingsGroup>
          </template>

          <template v-if="tab === 'doctor'">
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

            <DoctorPanel />

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
                    @click="api.openFolder(logs.directory)"
                  >
                    {{ t('settings.openLogs') }}
                  </v-btn>
                  <!-- The folder button leaves the reporter to find the right
                       file among seven and to know the doctor output is a
                       separate thing. This is the one that answers the whole
                       question. -->
                  <v-btn
                    size="small"
                    variant="flat"
                    color="primary"
                    prepend-icon="mdi-package-variant-closed"
                    :loading="bundling"
                    @click="saveDiagnosticBundle"
                  >
                    {{ t('settings.saveBundle') }}
                  </v-btn>
                </div>
                <!-- Said out loud because the alternative is a user who assumes
                     the opposite and attaches nothing, or one who assumes it is
                     safe when it is not. -->
                <div class="text-caption text-medium-emphasis mt-2">
                  {{ t('settings.logsRedacted') }}
                </div>
                <div class="text-caption text-medium-emphasis mt-1">
                  {{ t('settings.saveBundleHint') }}
                </div>
                <!-- Named, not counted. "Saved 6 files" tells nobody whether
                     the thing they were asked for is in there. -->
                <ErrorAlert v-if="bundleError" :error="bundleError" class="mt-3" />
                <v-alert
                  v-if="bundle"
                  type="success"
                  variant="tonal"
                  density="compact"
                  class="mt-3 text-caption"
                >
                  <div>{{ t('settings.saveBundleDone', { bytes: bytes(bundle.bytes) }) }}</div>
                  <code class="text-caption log-path d-block mt-1">{{ bundle.path }}</code>
                  <div class="mt-1">{{ bundle.entries.map((e) => e.name).join(', ') }}</div>
                </v-alert>
              </template>
            </SettingsGroup>
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
                  {{ t('certs.reissue') }}
                </v-btn>

                <!-- Trusting the CA is a separate button because it is a
                     separate thing, and on macOS it is the only one this app
                     cannot do for itself: `sudo` needs a terminal, root through
                     AppleScript is refused, and the user-domain write exits 0
                     and changes nothing. So it opens a terminal — which is
                     honest, and works. -->
                <v-btn
                  v-if="certs.caTrusted !== true"
                  size="small"
                  variant="tonal"
                  color="warning"
                  block
                  class="mt-2"
                  prepend-icon="mdi-console"
                  :disabled="!certs.mkcertAvailable"
                  @click="trustCaInTerminal"
                >
                  {{ t('certs.trustInTerminal') }}
                </v-btn>
                <div v-if="certs.caTrusted !== true" class="text-caption text-medium-emphasis mt-2">
                  {{ t('certs.trustInTerminalHint') }}
                </div>

                <!-- The certificate is on disk and the browser is still getting
                     the old one. Silence here is what made this bug survive:
                     the reissue reports success either way. -->
                <v-alert v-if="certNotReloaded" type="warning" variant="tonal" class="mt-3">
                  <div class="text-caption">{{ t('certs.notReloaded') }}</div>
                </v-alert>

                <!-- Both paths, each said to be what it is.
                     They were reported as "the certificate is in two places",
                     three times, because only one of them was ever shown and
                     the other was found by looking. They are two different
                     files with two different jobs, and the reason they are not
                     in one directory is the line below the second one. -->
                <div class="mt-3">
                  <div v-if="certs.certPath" class="text-caption text-medium-emphasis">
                    <strong>{{ t('certs.leafLabel') }}</strong> · {{ certs.certPath }}
                  </div>
                  <div v-if="certs.caPath" class="text-caption text-medium-emphasis mt-1">
                    <strong>{{ t('certs.caLabel') }}</strong> · {{ certs.caPath }}
                    <!-- The reason they are not one directory is worth having
                         and is not worth three lines of a settings pane. It was
                         three lines here, and read as a lecture attached to two
                         file paths.
                         The `#activator` slot rather than `activator="parent"`:
                         the first version nested the tooltip inside `v-icon`
                         alongside the icon's own name, so the slot held two
                         things and the hover reached neither. This is the shape
                         every other tooltip in this app already uses. -->
                    <v-tooltip :text="t('certs.whySeparate')" location="top" max-width="420">
                      <template #activator="{ props }">
                        <v-icon
                          v-bind="props"
                          size="14"
                          class="ml-1 why-separate"
                          icon="mdi-information-outline"
                        />
                      </template>
                    </v-tooltip>
                  </div>
                </div>
              </template>
            </SettingsGroup>
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
  <ServiceSettingsSheet v-model="sheetOpen" :service="sheetService" @applied="onServiceApplied" />

  <!-- Reverting deletes the file the user edited. There is no copy of it
       anywhere — the binary holds the shipped version, not theirs. -->
  <v-dialog :model-value="!!revertTarget" max-width="520" @update:model-value="revertTarget = null">
    <v-card v-if="revertTarget">
      <v-card-item>
        <template #prepend><v-icon color="error">mdi-undo-variant</v-icon></template>
        <v-card-title class="text-body-1">{{ t('settings.templates.revertTitle') }}</v-card-title>
      </v-card-item>
      <v-card-text>
        <p class="text-body-2 mb-2">{{ t('settings.templates.revertBody') }}</p>
        <code class="text-caption break">{{ revertTarget }}</code>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="revertTarget = null">{{ t('hosts.cancel') }}</v-btn>
        <v-btn color="error" variant="flat" @click="revertTemplate">
          {{ t('settings.templates.revert') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
/* Config is read in columns — an alignment that a proportional face destroys. */
.why-separate {
  cursor: help;
}

.server-config :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.8125rem;
  line-height: 1.6;
}

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

/* One layer, not two.
   `v-list` paints `surface` by default, and these rows sit on a group card
   already filled with a translucent `surface-bright`. In the light theme the
   two are close enough to read as one; in the dark theme the list's opaque
   fill sat visibly on top of the card's, so every row looked like a second
   panel. The list is transparent now and the row earns a background only
   under the pointer — which is also the only time it means anything. */
.service-list :deep(.v-list-item) {
  background: transparent;
}
.service-list :deep(.v-list-item:hover) {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

/* The list takes the room; the rail takes what its labels need. */
.services-main {
  flex: 1 1 auto;
  min-width: 0;
}

/* The rail is a filter, not navigation: left-aligned labels, sized to the
   text. Categories come from the catalog, so they are content — uppercasing
   them turns `adminUis` into `ADMINUIS`, which is not what the row beneath
   calls it. */
.service-tabs :deep(.v-tab) {
  justify-content: flex-start;
  min-width: 0;
  text-transform: none;
  letter-spacing: normal;
}

@media (min-width: 960px) {
  .service-tabs {
    flex: 0 0 auto;
    width: 160px;
  }
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

/* The preset, as it will be committed. Scrolls in its own box rather than
   widening the page — a twenty-service preset is longer than the pane. */
.preset-json {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.5;
  max-height: 320px;
  overflow: auto;
  margin: 0;
}

/* A key or a version, where the difference between 8.0 and 8.O matters. */
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
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
