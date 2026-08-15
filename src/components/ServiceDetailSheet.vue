<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useDisplay } from 'vuetify';
import { api, asList } from '@/lib/ipc';
import { listenAll } from '@/lib/events';
import { bytes, duration, percent } from '@/lib/format';
import { useCopyTick } from '@/composables/useCopyTick';
import SideSheet from '@/components/SideSheet.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import LogView from '@/components/LogView.vue';

/**
 * Everything the services table knows about one service, in a side sheet.
 *
 * It used to be an expansion row. Twenty columns of table and then a
 * three-column panel inside one of its cells meant the panel was always the
 * narrowest place in the window, and opening one pushed every row below it off
 * the screen. A sheet is the same content read beside the list instead of
 * inside it.
 */
const props = defineProps({
  /** The row being read, or null when the sheet is closed. */
  service: { type: Object, default: null },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);

const { t } = useI18n();
const display = useDisplay();
const { copied, copy } = useCopyTick();

/**
 * Sized against the window rather than in fixed pixels, unlike the other
 * sheets: this one carries absolute host paths, port tables and credential
 * rows, which wrap into unreadable ribbons in a form-width panel. Floored so a
 * narrow window still gets something readable, capped so a wide one does not
 * get a sheet that may as well be a page.
 */
const width = computed(() => Math.round(Math.min(Math.max(display.width.value * 0.55, 560), 1040)));

/** Already the whole domain — the suffix is applied at the boundary, once. */
const domain = computed(() => props.service?.url ?? null);

/**
 * The one chip at the top, and what it is allowed to claim.
 *
 * Running and healthy are different questions and this used to answer only the
 * first: twenty-four packages in the catalogue declare a healthcheck, and a
 * database refusing every connection carried the same green "Running" chip as
 * one that was answering. A container with no healthcheck keeps the old two
 * states — inventing a third for it would be the same overclaim in reverse.
 */
const HEALTH_CHIP = {
  healthy: { color: 'success', icon: 'mdi-heart-pulse' },
  unhealthy: { color: 'error', icon: 'mdi-heart-broken' },
  starting: { color: 'warning', icon: 'mdi-timer-sand' },
};

const status = computed(() => {
  if (!props.service) return null;
  if (!props.service.running) {
    return { color: 'grey-darken-1', icon: 'mdi-stop-circle', label: t('system.stopped') };
  }
  const chip = HEALTH_CHIP[props.service.health];
  if (chip) return { ...chip, label: t(`servicesView.health.${props.service.health}`) };
  return { color: 'success', icon: 'mdi-check-circle', label: t('system.running') };
});

/**
 * Which half of the sheet is showing.
 *
 * Logs are a tab rather than a button that opens a dialog over the panel: a
 * dialog on top of a sheet is two modal layers deep for one container, and it
 * covers the detail you opened the sheet to read.
 */
const tab = ref('detail');

/** Container inspection: networks, gateway, address, mounts. */
const details = ref(null);
const loading = ref(false);
const error = ref(null);

/**
 * Now, but only while the sheet is open.
 *
 * Uptime is the one row here that goes stale by sitting still — everything else
 * is a fact about a container that would have to change for the row to be
 * wrong. A timer that ran with the sheet closed would be a wake-up every half
 * minute for a panel nobody is looking at, so it starts and stops with it.
 */
const now = ref(Date.now());
let clock = null;

function startClock() {
  stopClock();
  now.value = Date.now();
  clock = setInterval(() => {
    now.value = Date.now();
    loadStats(props.service);
  }, 30_000);
}

function stopClock() {
  if (clock) clearInterval(clock);
  clock = null;
}

/**
 * How long this container has been up, or null.
 *
 * Null for a stopped one: Docker leaves `startedAt` at the last start, so a
 * container that exited an hour ago would otherwise report the uptime it had
 * when it died as though it were still accumulating.
 */
const uptime = computed(() => {
  if (!details.value?.running || !details.value.startedAt) return null;
  const started = Date.parse(details.value.startedAt);
  if (Number.isNaN(started)) return null;
  return duration(Math.max(0, Math.round((now.value - started) / 1000)));
});

/**
 * The runtime rows worth a line, and only the ones that are.
 *
 * Every one of these came back from `container_inspect` already and was thrown
 * away by this panel. They are built as a list rather than written out as
 * markup because most of them are absent most of the time: a healthy container
 * has no exit code, a container that has never crashed has no restart count
 * worth reading, and a row rendered with an em dash in it is a line that costs
 * height to say nothing.
 */
const runtimeRows = computed(() => {
  const d = details.value;
  if (!d) return [];
  return [
    d.image && { key: 'image', label: t('servicesView.image'), value: d.image, mono: true },
    d.imageSize && {
      key: 'imageSize',
      label: t('servicesView.imageSize'),
      value: bytes(d.imageSize),
    },
    uptime.value && { key: 'uptime', label: t('servicesView.uptime'), value: uptime.value },
    // Only once it has happened. Zero restarts is the normal state and a row
    // reading "0" invites the reader to wonder what it would mean.
    d.restartCount > 0 && {
      key: 'restarts',
      label: t('servicesView.restarts'),
      value: d.restartPolicy
        ? t('servicesView.restartsWithPolicy', { n: d.restartCount, policy: d.restartPolicy })
        : String(d.restartCount),
      colour: 'warning',
    },
    // The single most useful number about a container that is not running, and
    // it was being dropped on the floor. 137 is the out-of-memory kill.
    !d.running &&
      d.exitCode !== null &&
      d.exitCode !== undefined && {
        key: 'exit',
        label: t('servicesView.exitCode'),
        value:
          d.exitCode === 137
            ? t('servicesView.exitOutOfMemory', { code: d.exitCode })
            : String(d.exitCode),
        colour: d.exitCode === 0 ? undefined : 'error',
      },
  ].filter(Boolean);
});

/** Values revealed by an explicit click, keyed by their `.env` name. */
const revealed = ref({});

async function load(service) {
  details.value = null;
  error.value = null;
  revealed.value = {};
  if (!service?.built) return;

  loading.value = true;
  try {
    details.value = await api.containerInspect(service.id);
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

/**
 * Open the container in the terminal chosen in Settings.
 *
 * `terminal_open_external` reads `terminalApp` from preferences and falls back
 * to the platform default, so the choice made once in Settings is the one used
 * here — no second picker, no in-app terminal panel stacked on the sheet.
 */
async function openTerminal() {
  error.value = null;
  try {
    await api.terminalOpenExternal({ kind: 'container', name: props.service.containerName });
  } catch (e) {
    error.value = e;
  }
}

const isRevealed = (credential) => revealed.value[credential.envKey] !== undefined;

/**
 * Show a masked value, or put it back.
 *
 * Hiding drops the value rather than parking it out of sight, so what the
 * component holds is exactly what is on screen. Showing it again is another
 * read of a local file, which costs nothing.
 */
async function toggleReveal(credential) {
  if (isRevealed(credential)) {
    const next = { ...revealed.value };
    delete next[credential.envKey];
    revealed.value = next;
    return;
  }

  try {
    // Keyed on `envKey` because that is what the row is keyed on, but asked for
    // by `key`: the boundary dispatches to the instance table or to `.env`, and
    // an instance's setting is called `ROOT_PASSWORD` there. Asking with the
    // `.env` spelling is what made the eye answer "not set in .env" over a
    // password a migrated workspace does have.
    revealed.value = {
      ...revealed.value,
      [credential.envKey]: await api.serviceReveal(props.service.id, credential.key),
    };
  } catch (e) {
    error.value = e;
  }
}

// ---------------------------------------------------- connection strings

/**
 * The two addresses this service answers on, or null when it has none.
 *
 * Null is the normal answer for most rows — an admin UI is opened at its
 * domain, which is a row above — so the whole section is hidden rather than
 * shown empty.
 */
const connection = ref(null);
/** Set once the user has asked for the password; dropped when the sheet moves. */
const connectionRevealed = ref(false);

async function loadConnection(service, reveal = false) {
  if (!service) return;
  try {
    connection.value = await api.serviceConnection(service.id, reveal);
    connectionRevealed.value = reveal;
  } catch (e) {
    error.value = e;
    connection.value = null;
  }
}

/**
 * The desktop clients on this machine that open this service's kind of address.
 *
 * Empty is the normal answer and the button is hidden for it — most services
 * have no connection string at all, and AMQP or SMTP has one no database client
 * takes. The list is fetched rather than derived from the URI's scheme, because
 * whether a client handles a scheme is a fact about that client: Redis Insight
 * is installed on the machine this was written on and registers only
 * `redisinsight`, so it is not offered for `redis://` and the button would have
 * launched an app that ignores the address it was given.
 */
const dbClients = ref([]);

async function loadDbClients(service) {
  if (!service) {
    dbClients.value = [];
    return;
  }
  try {
    dbClients.value = await api.serviceDbClients(service.id);
  } catch {
    // A picker that cannot be built is a picker that is not shown; the copy
    // button beside it still works and is what this replaces.
    dbClients.value = [];
  }
}

/**
 * Hand the address over.
 *
 * The password crosses in the string, because one with bullets in it does not
 * connect — the same distinction `copyUri` makes between reading a secret and
 * using one, and this is the second.
 */
async function openInClient(client) {
  try {
    await api.serviceOpenInClient(props.service.id, client ?? '');
  } catch (e) {
    error.value = e;
  }
}

/**
 * Copy the string that works, while the screen keeps showing bullets.
 *
 * A masked URI on the clipboard is a string that fails to connect, so this
 * fetches the real one — the same deliberate ask the eye makes — and does not
 * put it on screen. Revealing and copying are different intentions: one is "let
 * me read it", the other "let me use it", and only the first belongs in a
 * screenshot.
 */
async function copyUri(endpoint) {
  let value = endpoint.uri;
  if (connection.value?.masked) {
    try {
      const live = await api.serviceConnection(props.service.id, true);
      value = endpoint.key === 'host' ? live?.fromHost?.uri : live?.fromContainer?.uri;
    } catch (e) {
      error.value = e;
      return;
    }
  }
  await copy(value, `uri-${endpoint.key}`);
}

/**
 * The endpoints in the order they are read, with the label each one needs.
 *
 * The host address comes first because it is the one somebody has open a
 * client for. `fromHost` is null when a running container publishes nothing,
 * and that row is dropped rather than filled with an address that would fail.
 */
const endpoints = computed(() => {
  if (!connection.value) return [];
  return [
    connection.value.fromHost && {
      key: 'host',
      label: t('servicesView.fromHost'),
      hint: t('servicesView.fromHostHint'),
      icon: 'mdi-laptop',
      ...connection.value.fromHost,
    },
    {
      key: 'container',
      label: t('servicesView.fromContainer'),
      hint: t('servicesView.fromContainerHint'),
      icon: 'mdi-docker',
      ...connection.value.fromContainer,
    },
  ].filter(Boolean);
});

/**
 * The icon a credential gets, from what the key is rather than from a list
 * someone has to maintain per service.
 */
function credentialIcon(key) {
  if (/PASSWORD|PASS|SECRET|TOKEN/.test(key)) return { icon: 'mdi-lock', color: 'error' };
  if (/USER/.test(key)) return { icon: 'mdi-account', color: 'success' };
  if (/DATABASE|\bDB\b/.test(key)) return { icon: 'mdi-database', color: 'info' };
  if (/PORT/.test(key)) return { icon: 'mdi-ethernet', color: 'purple' };
  if (/HOST|SERVER|URL/.test(key)) return { icon: 'mdi-server-network', color: 'primary' };
  return { icon: 'mdi-information-outline', color: 'grey' };
}

/**
 * What upstream says about this version, or null.
 *
 * Only worth a chip when it is not the ordinary answer: "Supported" beside
 * every service is a word that stops being read, and the two that matter are
 * the two that change what you should do next. It was visible only in the
 * catalogue tree — on the page you install from, which is not where somebody
 * debugging an end-of-life database is standing.
 */
const support = computed(() => {
  const status = props.service?.support;
  if (status !== 'eol' && status !== 'deprecated') return null;
  return {
    color: status === 'eol' ? 'error' : 'warning',
    label: t(`marketView.support.${status}`),
    // The date is the difference between "ended two years ago" and "ends next
    // month", and both were rendering as the same three words.
    date: props.service.eolDate ?? null,
  };
});

/**
 * Live CPU and memory for this container.
 *
 * `container_stats` has been on the boundary the whole time and no service
 * screen has ever called it. Sampled rather than streamed: the endpoint reads
 * cumulative counters and needs two readings for a percentage, so one call is
 * already two round trips, and a panel that did this every second would cost
 * more than the number is worth. Refreshed on the same clock as uptime.
 */
const stats = ref(null);

async function loadStats(service) {
  if (!service?.running) {
    stats.value = null;
    return;
  }
  try {
    stats.value = await api.containerStats(service.id);
  } catch {
    // A container that stopped between the list and this call is the ordinary
    // case, and it is not worth a red panel over a detail sheet.
    stats.value = null;
  }
}

/**
 * Which container the logs tab is showing.
 *
 * The main one, unless a companion's log button says otherwise. A companion
 * with no way to read its output is a row that reports a problem and offers no
 * way to look into it — and Kafka's Zookeeper is exactly the container whose
 * log holds the answer when the broker will not start.
 */
const logTarget = ref(null);
const logContainer = computed(() => logTarget.value?.container ?? props.service?.containerName);

function openCompanionLogs(companion) {
  logTarget.value = { container: companion.containerName, label: companion.name };
  tab.value = 'logs';
}

const companionColour = (companion) => {
  if (!companion.built) return 'grey-darken-1';
  if (companion.health === 'unhealthy') return 'error';
  return companion.running ? 'success' : 'grey-darken-1';
};

function companionLabel(companion) {
  if (!companion.built) return t('servicesView.notCreatedShort');
  if (!companion.running) return t('system.stopped');
  const health = HEALTH_CHIP[companion.health] ? companion.health : null;
  return health ? t(`servicesView.health.${health}`) : t('system.running');
}

/**
 * The three states a dependency row can be in, told apart in one place.
 *
 * `provider` absent and `running` false are two different failures with two
 * different fixes — install something that provides it, or start what you
 * have — and a colour that merged them would send half the readers to the
 * wrong page. An optional dependency nothing answers is not a fault at all,
 * which is why `required` is read before anything else.
 */
function dependencyState(dep) {
  if (dep.running) return t('servicesView.depRunning');
  if (!dep.provider) return t('servicesView.depNotInstalled');
  return t('servicesView.depStopped');
}

function dependencyColour(dep) {
  if (dep.running) return 'success';
  if (!dep.required) return 'grey';
  return dep.provider ? 'warning' : 'error';
}

const dependencyIcon = (dep) =>
  dep.running ? 'mdi-check-circle' : dep.provider ? 'mdi-stop-circle' : 'mdi-package-variant';

/** A mount that lands under /var/log is the one the log section is about. */
const isLogMount = (mount) => /(^|\/)log/i.test(mount.destination);

// ------------------------------------------------------------------ mail

/**
 * The inbox, for whichever catcher this checkout has.
 *
 * `mail_status` names the service rather than this matching on "mailhog": the
 * upstream template still installs the unmaintained image, Mailpit is where the
 * ecosystem went, and which one is present is the Rust side's answer.
 */
const mail = ref(null);
const messages = ref([]);
const mailLoading = ref(false);
const openMessage = ref(null);
const body = ref(null);

const isMailService = computed(
  () => !!mail.value?.available && mail.value.service === props.service?.id
);

async function loadMail() {
  mailLoading.value = true;
  try {
    mail.value = await api.mailStatus();
    messages.value = mail.value?.running ? await api.mailMessages(50) : [];
  } catch (e) {
    error.value = e;
    messages.value = [];
  } finally {
    mailLoading.value = false;
  }
}

// Fetched when a message is opened, not with the list: a body per row would be
// fifty requests to render a screen showing subjects.
watch(openMessage, async (id) => {
  body.value = null;
  if (!id) return;
  try {
    body.value = await api.mailMessage(id);
  } catch (e) {
    error.value = e;
  }
});

async function clearMail() {
  const { confirm } = await import('@tauri-apps/plugin-dialog');
  if (!(await confirm(t('mail.confirmClear'), { title: t('mail.clear'), kind: 'warning' }))) return;
  try {
    await api.mailClear();
    await loadMail();
  } catch (e) {
    error.value = e;
  }
}

// ---------------------------------------------------------------- backup

/**
 * Backup, for the four services that have a dump tool inside their image.
 *
 * Read from `db_targets` rather than matched on the service name here: which
 * engines are supported is the Rust side's answer, and a second list in the UI
 * is a second thing to update when a fifth engine arrives.
 */
const dbTargets = ref([]);
const dbBusy = ref(null);
const dbLine = ref('');
const dbResult = ref('');

const dbTarget = computed(() =>
  dbTargets.value.find((target) => target.service === props.service?.id)
);

/**
 * A filename that sorts chronologically and is legal on every platform.
 *
 * Colons are not allowed in Windows filenames, so the obvious RFC 3339 spelling
 * would produce a backup that cannot be saved on one of the three platforms
 * this ships to.
 */
function stamp() {
  return new Date().toISOString().slice(0, 19).replace(/:/g, '-');
}

async function dumpDatabase() {
  const { save } = await import('@tauri-apps/plugin-dialog');
  const path = await save({
    defaultPath: `${dbTarget.value.service}-${stamp()}.${dbTarget.value.extension}`,
  });
  if (!path) return;

  await runDb('dump', () => api.dbDump(props.service.id, path), path);
}

async function restoreDatabase() {
  const { open, confirm } = await import('@tauri-apps/plugin-dialog');
  const path = await open({ multiple: false, directory: false });
  if (!path) return;

  // Named, not generic: "are you sure" without saying what gets replaced is a
  // dialog people learn to click through.
  const target = dbTarget.value.database ?? props.service.id;
  const ok = await confirm(t('db.confirmRestore', { db: target }), {
    title: t('db.restore'),
    kind: 'warning',
  });
  if (!ok) return;

  await runDb('restore', () => api.dbRestore(props.service.id, path), path);
}

// -------------------------------------------------------------- snapshots

/**
 * Named snapshots, beside the dump-to-a-path buttons above.
 *
 * The difference between the two is worth keeping visible rather than
 * collapsing into one control: a dump goes somewhere the user chose and is
 * theirs to look after, and a snapshot is a name this app remembers and can put
 * back. People want both, and for different reasons.
 */
const snapshots = ref([]);
const snapshotName = ref('');
const snapshotBusy = ref(null);

const serviceSnapshots = computed(() =>
  snapshots.value.filter((s) => s.service === props.service?.id)
);

/**
 * Moving this instance's data into another one (G-4).
 *
 * The plan is fetched the moment a target is chosen, not when the button is
 * pressed. The whole point of the plan is the sentence "everything in X will be
 * replaced" and the refusal for a pair that cannot work — both of which have to
 * be readable *before* the decision, not reported after it.
 */
const moveTarget = ref(null);
const movePlan = ref(null);
const moveBusy = ref(false);

/**
 * Every other database instance, which is the only set that can be a target.
 *
 * Every one of them, not only the compatible ones. A dropdown that silently
 * omitted Postgres from a MySQL sheet would leave somebody wondering whether it
 * is missing or impossible; listing it and having the plan say why is the
 * answer to both.
 */
const dbInstances = ref([]);
const moveTargets = computed(() =>
  asList(dbInstances.value)
    .filter((row) => row.id !== props.service?.id)
    .map((row) => ({
      value: row.id,
      title: `${row.id}${row.running ? '' : ' · ' + t('system.stopped')}`,
    }))
);

async function loadInstances() {
  try {
    dbInstances.value = asList(await api.dbInstances());
  } catch {
    // A workspace with no instance table has none, which is not a failure
    // worth a red panel in a sheet about something else.
    dbInstances.value = [];
  }
}

async function planMove() {
  movePlan.value = null;
  if (!moveTarget.value) return;
  try {
    movePlan.value = await api.dbMovePlan(props.service.id, moveTarget.value);
  } catch (e) {
    error.value = e;
  }
}

async function applyMove() {
  if (!movePlan.value?.possible) return;
  const { confirm } = await import('@tauri-apps/plugin-dialog');
  // Named, like every other destructive confirm in this sheet: "are you sure"
  // without saying what is replaced is a dialog people learn to click through.
  const ok = await confirm(t('dbMove.confirm', { to: moveTarget.value }), {
    title: t('dbMove.title'),
    kind: 'warning',
  });
  if (!ok) return;

  moveBusy.value = true;
  error.value = null;
  try {
    const moved = await api.dbMoveApply(props.service.id, moveTarget.value);
    dbResult.value = t('dbMove.done', { bytes: moved.bytes, to: moved.to });
  } catch (e) {
    error.value = e;
  } finally {
    moveBusy.value = false;
  }
}

async function loadSnapshots() {
  try {
    snapshots.value = asList(await api.dbSnapshots());
  } catch {
    // A workspace that has never taken one has no directory, which is not a
    // failure worth a red panel in a sheet about something else.
    snapshots.value = [];
  }
}

async function takeSnapshot() {
  const name = snapshotName.value.trim();
  if (!name) return;
  snapshotBusy.value = 'take';
  error.value = null;
  try {
    await api.dbSnapshotTake(props.service.id, name);
    snapshotName.value = '';
    await loadSnapshots();
  } catch (e) {
    error.value = e;
  } finally {
    snapshotBusy.value = null;
  }
}

async function restoreSnapshot(snapshot) {
  const { confirm } = await import('@tauri-apps/plugin-dialog');
  const target = dbTarget.value?.database ?? props.service.id;
  // Named, like the file restore above: "are you sure" without saying what is
  // replaced is a dialog people learn to click through.
  const ok = await confirm(t('db.confirmRestore', { db: target }), {
    title: t('snapshots.restore'),
    kind: 'warning',
  });
  if (!ok) return;

  snapshotBusy.value = snapshot.name;
  error.value = null;
  try {
    await api.dbSnapshotRestore(props.service.id, snapshot.name);
    dbResult.value = t('snapshots.restored', { name: snapshot.name });
  } catch (e) {
    error.value = e;
  } finally {
    snapshotBusy.value = null;
  }
}

async function deleteSnapshot(snapshot) {
  snapshotBusy.value = snapshot.name;
  error.value = null;
  try {
    await api.dbSnapshotDelete(props.service.id, snapshot.name);
    await loadSnapshots();
  } catch (e) {
    error.value = e;
  } finally {
    snapshotBusy.value = null;
  }
}

async function runDb(action, call, path) {
  dbBusy.value = action;
  dbLine.value = '';
  dbResult.value = '';
  error.value = null;
  try {
    await call();
    dbResult.value = t(action === 'dump' ? 'db.dumped' : 'db.restored', { path });
  } catch (e) {
    error.value = e;
  } finally {
    dbBusy.value = null;
  }
}

// Inspected when a row is opened, not with the list: inspecting twenty
// containers to render a sheet showing one is nineteen wasted round trips.
watch(
  () => [props.modelValue, props.service?.id],
  ([open]) => {
    if (!open) {
      stopClock();
      return;
    }
    // A different service is a different panel: start it on the detail tab
    // rather than on whatever the last one was left showing.
    tab.value = 'detail';
    // A companion's logs do not follow the sheet to the next service.
    logTarget.value = null;
    dbLine.value = '';
    dbResult.value = '';
    openMessage.value = null;
    body.value = null;
    // A revealed password does not follow the sheet to the next service.
    connection.value = null;
    connectionRevealed.value = false;
    dbClients.value = [];
    stats.value = null;
    startClock();
    load(props.service);
    loadStats(props.service);
    loadConnection(props.service);
    loadDbClients(props.service);
    loadMail();
    api.dbTargets().then(
      (targets) => (dbTargets.value = targets),
      () => (dbTargets.value = [])
    );
    loadSnapshots();
    loadInstances();
    // A target chosen for the previous service is meaningless for this one.
    moveTarget.value = null;
    movePlan.value = null;
  },
  { immediate: true }
);

// The dump tools report to stderr as they go. Shown as a single moving line
// rather than a log panel: on a healthy dump there is almost nothing to say,
// and a panel that is empty most of the time reads as broken.
let stopDbEvents = null;
onMounted(async () => {
  stopDbEvents = await listenAll(['db:progress'], (_name, payload) => {
    dbLine.value = payload?.line ?? '';
  });
});
onUnmounted(() => {
  stopDbEvents?.();
  stopClock();
});
</script>

<template>
  <SideSheet
    :model-value="modelValue"
    :title="service?.id ?? ''"
    icon="mdi-server"
    :width="width"
    :flush="tab === 'logs'"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <template #header-append>
      <!-- Only when it is not the ordinary answer. A "Supported" chip on every
           service is a word that stops being read, and it would sit beside the
           two that are worth stopping for. -->
      <v-chip
        v-if="support"
        size="small"
        variant="flat"
        :color="support.color"
        prepend-icon="mdi-clock-alert-outline"
        class="mr-2"
      >
        {{ support.date ? `${support.label} · ${support.date}` : support.label }}
      </v-chip>
      <v-chip
        v-if="status"
        size="small"
        variant="flat"
        :color="status.color"
        :prepend-icon="status.icon"
      >
        {{ status.label }}
      </v-chip>
    </template>

    <!-- On the header's own colour so the two read as one block. -->
    <template #tabs>
      <!-- `color` as well as `bg-color`: the active tab and its slider default
           to the primary colour, which on a primary bar is invisible. -->
      <v-tabs v-model="tab" bg-color="primary" color="on-primary" density="comfortable" grow>
        <v-tab value="detail" prepend-icon="mdi-information-outline">
          {{ t('servicesView.colDetail') }}
        </v-tab>
        <!-- Only on the mail catcher, and only when the checkout has one. A
             tab that is present but empty on nineteen other services is
             nineteen wrong answers to "what does this do". -->
        <v-tab v-if="isMailService" value="inbox" prepend-icon="mdi-email-outline">
          {{ t('mail.inbox') }}
          <v-chip v-if="mail?.total" size="x-small" class="ml-2">{{ mail.total }}</v-chip>
        </v-tab>
        <!-- Named when it is not the container the sheet is titled after:
             opening a companion's log from the row below and landing on a tab
             that still says "Logs" leaves no way to tell whose output is on
             screen. Enabled when *either* container exists, because a
             companion can be up while the service it belongs to is not — which
             is the state you read a Zookeeper log in. -->
        <v-tab
          value="logs"
          prepend-icon="mdi-text-box-outline"
          :disabled="!service?.built && !logTarget"
        >
          {{ logTarget ? `${t('logs.title')} · ${logTarget.label}` : t('logs.title') }}
        </v-tab>
      </v-tabs>
    </template>

    <!-- Inbox ----------------------------------------------------------- -->
    <template v-if="tab === 'inbox' && service">
      <ErrorAlert :error="error" type="error" class="mb-2" />

      <div v-if="!service.running" class="text-caption text-medium-emphasis">
        {{ t('mail.notRunning') }}
      </div>

      <!-- Up but not answering renders as an empty inbox otherwise, which
           reads as "no mail" rather than "could not ask". -->
      <v-alert v-else-if="mail?.error" type="warning" variant="tonal">
        <div class="text-caption">{{ mail.error }}</div>
      </v-alert>

      <template v-else>
        <div class="d-flex align-center ga-2 mb-3">
          <span class="text-caption text-medium-emphasis">
            {{ t('mail.count', { n: mail?.total ?? 0 }) }}
          </span>
          <v-spacer />
          <v-btn
            size="x-small"
            variant="text"
            icon="mdi-refresh"
            :aria-label="t('app.refresh')"
            :loading="mailLoading"
            @click="loadMail"
          />
          <v-btn
            size="small"
            variant="text"
            color="error"
            prepend-icon="mdi-delete-sweep-outline"
            :disabled="!messages.length"
            @click="clearMail"
          >
            {{ t('mail.clear') }}
          </v-btn>
        </div>

        <div v-if="!messages.length" class="text-caption text-medium-emphasis">
          {{ t('mail.empty') }}
        </div>

        <v-expansion-panels v-model="openMessage" variant="accordion">
          <v-expansion-panel v-for="m in messages" :key="m.id" :value="m.id">
            <v-expansion-panel-title>
              <div class="mail-row">
                <span class="mail-subject">{{ m.subject || t('mail.noSubject') }}</span>
                <span class="mail-meta">{{ m.from }} → {{ m.to.join(', ') }}</span>
                <span v-if="m.snippet" class="mail-meta">{{ m.snippet }}</span>
              </div>
            </v-expansion-panel-title>
            <v-expansion-panel-text>
              <div v-if="m.date" class="text-caption text-medium-emphasis mb-2">{{ m.date }}</div>
              <!-- The HTML part is shown as source, never rendered: a captured
                   message is untrusted input, and injecting it into this
                   document would give any application under test a script tag
                   inside the app that manages the whole stack. -->
              <pre v-if="body?.html" class="mail-body">{{ body.html }}</pre>
              <pre v-else-if="body?.text" class="mail-body">{{ body.text }}</pre>
              <div v-else class="text-caption text-medium-emphasis">{{ t('app.loading') }}</div>
            </v-expansion-panel-text>
          </v-expansion-panel>
        </v-expansion-panels>
      </template>
    </template>

    <!-- Streamed only while its tab is showing. `:key` so switching between
         the main container and a companion tears the stream down and opens a
         new one, rather than re-pointing a component that has an open handle
         to the other container's output. -->
    <LogView
      v-if="tab === 'logs' && service"
      :key="logContainer"
      :container="logContainer"
      :active="modelValue && tab === 'logs'"
    />

    <!-- Explicitly the detail tab rather than "whatever is left": the inbox
         above is its own v-if chain, so an `v-else` here would render the
         detail panel underneath it. -->
    <template v-else-if="service && tab === 'detail'">
      <ErrorAlert :error="error" type="error" class="mb-2" />

      <!-- Runtime -------------------------------------------------------- -->
      <!-- First, because it answers the question the panel is usually opened
           with. Every row here came back from `container_inspect` all along
           and was discarded — so a container killed for memory reported
           "Stopped" and nothing else, and one restarting every ten seconds
           was indistinguishable from one that had been up for a week. -->
      <template v-if="runtimeRows.length || stats">
        <div class="sheet-group">{{ t('servicesView.runtime') }}</div>
        <div v-for="row in runtimeRows" :key="row.key" class="row">
          <span class="row-key">{{ row.label }}</span>
          <span
            class="text-body-2"
            :class="[row.mono ? 'mono' : '', row.colour ? `text-${row.colour}` : '']"
          >
            {{ row.value }}
          </span>
        </div>

        <!-- What this container is costing, from a command that has been on
             the boundary since the port and that no service screen has ever
             called. One sampled figure each rather than a chart: the question
             here is "is this the thing eating the machine", and that is
             answered by a number. -->
        <div v-if="stats" class="row">
          <span class="row-key">{{ t('stats.cpu') }}</span>
          <span class="text-body-2">{{ percent(stats.cpuPercent) }}</span>
        </div>
        <div v-if="stats" class="row">
          <span class="row-key">{{ t('stats.memory') }}</span>
          <span class="text-body-2">
            {{ bytes(stats.memoryUsed) }}
            <span class="text-medium-emphasis">
              / {{ bytes(stats.memoryLimit) }} · {{ percent(stats.memoryPercent) }}
            </span>
          </span>
        </div>
      </template>

      <!-- Network ------------------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.networkInfo') }}</div>

      <div class="row">
        <span class="row-key">{{ t('servicesView.colContainerName') }}</span>
        <v-chip size="small" variant="tonal" color="primary" class="path-chip">
          <v-icon start size="small">mdi-docker</v-icon>{{ service.containerName }}
        </v-chip>
      </div>

      <!-- The compatibility name, and it is the one that matters most on this
           panel: every project written before packages says
           `DB_HOST=stackvo-mysql`, and which instance answers to that is a
           decision made on another page by a star-shaped button. The instance's
           own name is the container row above, so only the extra ones are
           listed here — a row repeating what is directly above it is a row
           that teaches the reader to skip the section. -->
      <div v-if="service.aliases.length > 1" class="row align-start">
        <span class="row-key">{{ t('servicesView.alias') }}</span>
        <div class="d-flex flex-wrap ga-1">
          <v-chip
            v-for="alias in service.aliases.slice(1)"
            :key="alias"
            size="small"
            variant="tonal"
            color="primary"
          >
            <v-icon start size="small">mdi-tag-outline</v-icon>{{ alias }}
          </v-chip>
        </div>
      </div>

      <template v-if="details">
        <div v-if="details.ipAddress" class="row">
          <span class="row-key">{{ t('servicesView.ipAddress') }}</span>
          <v-chip size="small" variant="tonal" color="success">
            <v-icon start size="small">mdi-ip-network</v-icon>{{ details.ipAddress }}
          </v-chip>
        </div>
        <div v-for="net in details.networks" :key="net" class="row">
          <span class="row-key">{{ t('servicesView.network') }}</span>
          <v-chip size="small" variant="tonal" color="info">
            <v-icon start size="small">mdi-lan</v-icon>{{ net }}
          </v-chip>
        </div>
        <div v-if="details.gateway" class="row">
          <span class="row-key">{{ t('servicesView.gateway') }}</span>
          <v-chip size="small" variant="tonal" color="warning">
            <v-icon start size="small">mdi-router-network</v-icon>{{ details.gateway }}
          </v-chip>
        </div>
      </template>

      <div v-else-if="loading" class="text-caption text-medium-emphasis">
        {{ t('app.loading') }}
      </div>
      <div v-else-if="!service.built" class="text-caption text-medium-emphasis">
        {{ t('servicesView.notCreated') }}
      </div>

      <!-- Service ------------------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.serviceInfo') }}</div>

      <div v-if="domain" class="row">
        <span class="row-key">{{ t('servicesView.colDomain') }}</span>
        <v-chip
          size="small"
          variant="tonal"
          color="primary"
          class="path-chip"
          @click="api.openInBrowser(`https://${domain}`)"
        >
          <v-icon start size="small">mdi-web</v-icon>{{ domain }}
        </v-chip>
      </div>

      <!-- The package's own ports, under the names it gives them.
           `service.ports` below is what the container publishes, which is
           nothing at all until one exists — so an installed service that had
           never been started showed no ports, and a running MinIO showed
           `9000, 9001` with nothing saying which one is the console. -->
      <div v-if="service.declaredPorts.length" class="row align-start">
        <span class="row-key">{{ t('servicesView.portMappings') }}</span>
        <div class="d-flex flex-wrap ga-1">
          <v-chip
            v-for="port in service.declaredPorts"
            :key="port.name"
            size="small"
            variant="outlined"
            :color="port.host ? 'success' : 'grey'"
          >
            <v-icon start size="small">
              {{ port.host ? 'mdi-check-network' : 'mdi-lan-disconnect' }}
            </v-icon>
            {{ port.name }}: {{ port.container }}/{{ port.protocol }}
            <template v-if="port.host"> → {{ port.host }}</template>
            <span v-else class="ml-1 text-medium-emphasis">
              {{ t('servicesView.internal') }}
            </span>
          </v-chip>
        </div>
      </div>

      <!-- Falls back to the configured port when the container is not running:
           a stopped service still has a host port, and an empty section would
           suggest it does not. -->
      <div
        v-if="!service.declaredPorts.length && !service.ports.length && service.hostPort"
        class="row"
      >
        <span class="row-key">{{ t('services.hostPort') }}</span>
        <v-chip size="small" variant="tonal" color="grey">
          <v-icon start size="small">mdi-lan-disconnect</v-icon>{{ service.hostPort }}
        </v-chip>
      </div>

      <!-- The container's own view, for a workspace that has not migrated and
           therefore has no manifest to declare anything. -->
      <div v-if="!service.declaredPorts.length && service.ports.length" class="row align-start">
        <span class="row-key">{{ t('servicesView.portMappings') }}</span>
        <div class="d-flex flex-wrap ga-1">
          <v-chip
            v-for="port in service.ports"
            :key="`${port.container}/${port.protocol}`"
            size="small"
            variant="outlined"
            :color="port.host ? 'success' : 'grey'"
          >
            <v-icon start size="small">
              {{ port.host ? 'mdi-check-network' : 'mdi-lan-disconnect' }}
            </v-icon>
            {{ port.container }}/{{ port.protocol }}
            <template v-if="port.host"> → {{ port.host }}</template>
            <span v-else class="ml-1 text-medium-emphasis">
              {{ t('servicesView.internal') }}
            </span>
          </v-chip>
        </div>
      </div>

      <!-- Connection ---------------------------------------------------- -->
      <!-- Only for the services somebody points a client at. An admin UI is
           opened at its domain, which is the row above this. -->
      <template v-if="connection">
        <div class="sheet-group">{{ t('servicesView.connection') }}</div>

        <!-- The distinction this section exists for. Without it the container
             name above is the obvious thing to paste into Compass, and it
             cannot resolve from the host. -->
        <div class="text-caption text-medium-emphasis mb-2">
          {{ t('servicesView.connectionSubtitle') }}
        </div>

        <div v-for="endpoint in endpoints" :key="endpoint.key" class="endpoint">
          <div class="endpoint-head">
            <v-icon size="small" class="mr-1">{{ endpoint.icon }}</v-icon>
            <span class="font-weight-medium">{{ endpoint.label }}</span>
            <span class="text-medium-emphasis ml-2">{{ endpoint.hint }}</span>
          </div>
          <div class="endpoint-row">
            <code class="endpoint-uri">{{ endpoint.uri }}</code>
            <!-- Copies the working string even while the screen shows bullets:
                 a masked URI on the clipboard is one that fails to connect. -->
            <v-btn
              icon
              size="x-small"
              variant="text"
              :aria-label="t('app.copy')"
              @click="copyUri(endpoint)"
            >
              <v-icon size="small">
                {{ copied === `uri-${endpoint.key}` ? 'mdi-check' : 'mdi-content-copy' }}
              </v-icon>
              <v-tooltip activator="parent">{{ t('app.copy') }}</v-tooltip>
            </v-btn>

            <!-- Only on the host row. The container address is a name on a
                 Docker network, so a client on this desktop cannot resolve it —
                 offering to open it would rebuild the exact confusion the two
                 rows exist to prevent. -->
            <v-menu v-if="endpoint.key === 'host' && dbClients.length">
              <template #activator="{ props: menu }">
                <v-btn
                  v-bind="menu"
                  icon
                  size="x-small"
                  variant="text"
                  :aria-label="t('servicesView.openInClient')"
                >
                  <v-icon size="small">mdi-open-in-app</v-icon>
                  <v-tooltip activator="parent">{{ t('servicesView.openInClient') }}</v-tooltip>
                </v-btn>
              </template>
              <v-list density="compact">
                <!-- Greyed rather than hidden, the same rule the editor picker
                     follows: an absent row reads as "this app has never heard
                     of TablePlus", which is a different statement. -->
                <v-list-item
                  v-for="client in dbClients"
                  :key="client.id || 'system'"
                  :disabled="!client.available"
                  :prepend-icon="client.icon"
                  :title="client.name"
                  @click="openInClient(client.id)"
                />
              </v-list>
            </v-menu>
          </div>
        </div>

        <!-- Offered only where a password is actually in the string. -->
        <v-btn
          v-if="connection.masked || connectionRevealed"
          size="x-small"
          variant="text"
          class="mt-1"
          :prepend-icon="connectionRevealed ? 'mdi-eye-off-outline' : 'mdi-eye-outline'"
          :aria-pressed="connectionRevealed"
          @click="loadConnection(service, !connectionRevealed)"
        >
          {{ connectionRevealed ? t('servicesView.hide') : t('servicesView.reveal') }}
        </v-btn>

        <div v-if="!connection.fromHost" class="text-caption text-warning mt-1">
          {{ t('servicesView.notPublished') }}
        </div>
      </template>

      <!-- Credentials --------------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.credentials') }}</div>

      <div v-if="!service.credentials.length" class="text-caption text-medium-emphasis">
        {{ t('servicesView.noCredentials') }}
      </div>

      <div v-for="c in service.credentials" :key="c.envKey" class="credential">
        <v-icon size="small" :color="credentialIcon(c.key).color">
          {{ credentialIcon(c.key).icon }}
        </v-icon>
        <span class="row-key credential-key">{{ c.key }}</span>
        <span class="credential-value">{{ revealed[c.envKey] ?? c.value }}</span>

        <!-- Secrets arrive masked; this asks for the one value, and puts it
             back. -->
        <v-btn
          v-if="c.secret"
          icon
          size="x-small"
          variant="text"
          :aria-label="isRevealed(c) ? t('servicesView.hide') : t('servicesView.reveal')"
          :aria-pressed="isRevealed(c)"
          @click="toggleReveal(c)"
        >
          <v-icon size="small">
            {{ isRevealed(c) ? 'mdi-eye-off-outline' : 'mdi-eye-outline' }}
          </v-icon>
          <v-tooltip activator="parent">
            {{ isRevealed(c) ? t('servicesView.hide') : t('servicesView.reveal') }}
          </v-tooltip>
        </v-btn>
      </div>

      <!-- Backup --------------------------------------------------------- -->
      <!-- Only for the four engines with a dump tool in their image; every
           other service row is unchanged. -->
      <template v-if="dbTarget">
        <div class="sheet-group">{{ t('db.title') }}</div>

        <div class="text-caption text-medium-emphasis mb-2">
          {{
            dbTarget.database ? t('db.subtitle', { db: dbTarget.database }) : t('db.subtitleAll')
          }}
        </div>

        <div v-if="!service.running" class="text-caption text-warning mb-2">
          {{ t('db.notRunning') }}
        </div>

        <div class="d-flex ga-2 flex-wrap">
          <v-btn
            size="small"
            variant="tonal"
            prepend-icon="mdi-database-export"
            :loading="dbBusy === 'dump'"
            :disabled="!service.running || !!dbBusy"
            @click="dumpDatabase"
          >
            {{ t('db.dump') }}
          </v-btn>
          <v-btn
            size="small"
            variant="tonal"
            color="warning"
            prepend-icon="mdi-database-import"
            :loading="dbBusy === 'restore'"
            :disabled="!service.running || !!dbBusy"
            @click="restoreDatabase"
          >
            {{ t('db.restore') }}
          </v-btn>
        </div>

        <!-- The tools report to stderr; the last line is the useful one. -->
        <div v-if="dbLine" class="text-caption text-medium-emphasis mt-2 mono">{{ dbLine }}</div>
        <div v-if="dbResult" class="text-caption text-success mt-2">{{ dbResult }}</div>

        <!-- Snapshots ---------------------------------------------------- -->
        <div class="sheet-group">{{ t('snapshots.title') }}</div>
        <div class="text-caption text-medium-emphasis mb-2">{{ t('snapshots.subtitle') }}</div>

        <div class="d-flex ga-2 align-start">
          <v-text-field
            v-model="snapshotName"
            :label="t('snapshots.name')"
            density="compact"
            variant="outlined"
            hide-details
            :disabled="!service.running || !!snapshotBusy"
            @keyup.enter="takeSnapshot"
          />
          <v-btn
            size="small"
            variant="tonal"
            prepend-icon="mdi-camera-outline"
            :loading="snapshotBusy === 'take'"
            :disabled="!service.running || !snapshotName.trim() || !!snapshotBusy"
            @click="takeSnapshot"
          >
            {{ t('snapshots.take') }}
          </v-btn>
        </div>

        <div v-if="!serviceSnapshots.length" class="text-caption text-medium-emphasis mt-3">
          {{ t('snapshots.none') }}
        </div>

        <v-list v-else density="compact" class="bg-transparent mt-2">
          <v-list-item v-for="snap in serviceSnapshots" :key="snap.name" class="px-0">
            <template #prepend>
              <!-- A scheduled copy is marked, because it is the only kind
                   retention deletes on its own. -->
              <v-icon
                :icon="snap.automatic ? 'mdi-clock-outline' : 'mdi-camera-outline'"
                class="mr-3"
              />
            </template>
            <v-list-item-title class="text-body-2 mono">{{ snap.name }}</v-list-item-title>
            <v-list-item-subtitle class="text-caption">
              {{ snap.takenAt }} · {{ bytes(snap.bytes) }}
              <span v-if="snap.automatic"> · {{ t('snapshots.automatic') }}</span>
            </v-list-item-subtitle>
            <template #append>
              <v-btn
                size="small"
                variant="text"
                color="warning"
                :loading="snapshotBusy === snap.name"
                :disabled="!service.running || !!snapshotBusy"
                @click="restoreSnapshot(snap)"
              >
                {{ t('snapshots.restore') }}
              </v-btn>
              <v-btn
                icon
                size="x-small"
                variant="text"
                :aria-label="t('snapshots.delete')"
                :disabled="!!snapshotBusy"
                @click="deleteSnapshot(snap)"
              >
                <v-icon>mdi-delete-outline</v-icon>
                <v-tooltip activator="parent">{{ t('snapshots.delete') }}</v-tooltip>
              </v-btn>
            </template>
          </v-list-item>
        </v-list>
      </template>

      <!-- Moving data into another instance (G-4) ------------------------ -->
      <template v-if="moveTargets.length">
        <div class="sheet-group">{{ t('dbMove.title') }}</div>
        <p class="text-caption text-medium-emphasis mb-2">{{ t('dbMove.explain') }}</p>

        <div class="d-flex align-center ga-2 mb-2">
          <v-select
            v-model="moveTarget"
            :items="moveTargets"
            :label="t('dbMove.target')"
            density="compact"
            variant="outlined"
            hide-details
            style="max-width: 260px"
            @update:model-value="planMove"
          />
          <v-btn
            size="small"
            color="warning"
            variant="flat"
            :disabled="!movePlan?.possible || moveBusy"
            :loading="moveBusy"
            @click="applyMove"
          >
            {{ t('dbMove.move') }}
          </v-btn>
        </div>

        <!-- The refusal and the warnings, before the button is worth pressing.
             A pair that cannot work says so with its reason rather than being
             absent from the list. -->
        <v-alert
          v-if="movePlan && !movePlan.possible"
          type="error"
          variant="tonal"
          density="compact"
          class="mb-2"
        >
          <div class="text-caption">{{ movePlan.refused }}</div>
        </v-alert>
        <v-alert
          v-else-if="movePlan?.warnings?.length"
          type="warning"
          variant="tonal"
          density="compact"
          class="mb-2"
        >
          <div v-for="(note, i) in movePlan.warnings" :key="i" class="text-caption">
            {{ note }}
          </div>
        </v-alert>
      </template>

      <!-- Logs and mounts ----------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.logInfo') }}</div>

      <div class="row">
        <span class="row-key">{{ t('servicesView.containerLogs') }}</span>
        <v-chip size="small" variant="tonal" color="info" class="path-chip">
          <v-icon start size="small">mdi-docker</v-icon>
          <code>docker logs {{ service.containerName }}</code>
        </v-chip>
      </div>

      <template v-if="details?.mounts?.length">
        <div v-for="mount in details.mounts" :key="mount.destination" class="row align-start">
          <span class="row-key">
            {{ isLogMount(mount) ? t('servicesView.logPath') : t('servicesView.mount') }}
          </span>
          <div class="d-flex flex-column ga-1 flex-grow-1">
            <v-chip size="small" variant="tonal" color="info" class="path-chip">
              <v-icon start size="small">mdi-docker</v-icon>
              <code>{{ mount.destination }}</code>
            </v-chip>
            <!-- A named volume has no path on the host to open. -->
            <v-chip
              v-if="mount.source"
              size="small"
              variant="tonal"
              :color="mount.kind === 'volume' ? 'grey' : 'warning'"
              class="path-chip"
            >
              <v-icon start size="small">
                {{ mount.kind === 'volume' ? 'mdi-database-outline' : 'mdi-folder' }}
              </v-icon>
              <code>{{ mount.source }}</code>
            </v-chip>
          </div>
        </div>
      </template>
      <div v-else-if="service.built && !loading" class="text-caption text-medium-emphasis">
        {{ t('servicesView.noMounts') }}
      </div>

      <!-- Companions ----------------------------------------------------- -->
      <!-- Containers that come with this instance and are not separately
           installable — Kafka's Zookeeper, the only one in the catalogue.
           They were rendered into the compose file and then invisible: no row,
           no status, no way to reach their logs. When Kafka does not come up,
           the answer is usually in one of these, and the panel about Kafka was
           the one place that did not mention it existed. -->
      <template v-if="service.companions.length">
        <div class="sheet-group">{{ t('servicesView.companions') }}</div>
        <div class="text-caption text-medium-emphasis mb-2">
          {{ t('servicesView.companionsSubtitle') }}
        </div>

        <div v-for="companion in service.companions" :key="companion.name" class="row align-start">
          <span class="row-key">{{ companion.name }}</span>
          <div class="d-flex flex-column ga-1 flex-grow-1">
            <div class="d-flex align-center ga-2 flex-wrap">
              <v-chip
                size="small"
                variant="flat"
                :color="companionColour(companion)"
                :prepend-icon="companion.running ? 'mdi-check-circle' : 'mdi-stop-circle'"
              >
                {{ companionLabel(companion) }}
              </v-chip>
              <!-- Its logs, in the same viewer as the main container's. This
                   is the row's reason for existing: a broker that cannot
                   reach its Zookeeper says so in the Zookeeper's log. -->
              <!-- Named for a reader who cannot see which row it is in: on a
                   service with two companions, "Logs" twice is two buttons
                   with the same name and different jobs. -->
              <v-btn
                size="x-small"
                variant="text"
                prepend-icon="mdi-text-box-outline"
                :aria-label="t('servicesView.companionLogs', { name: companion.name })"
                :disabled="!companion.built"
                @click="openCompanionLogs(companion)"
              >
                {{ t('logs.title') }}
              </v-btn>
            </div>
            <v-chip size="small" variant="tonal" color="primary" class="path-chip">
              <v-icon start size="small">mdi-docker</v-icon>{{ companion.containerName }}
            </v-chip>
            <span class="text-caption text-medium-emphasis mono">{{ companion.image }}</span>
          </div>
        </div>
      </template>

      <!-- Dependencies -------------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.dependencies') }}</div>

      <!-- The web UI modelled dependencies for three of twenty services and
           referenced one that does not exist, so admin UIs could be started
           against nothing. -->
      <div
        v-if="!service.required.length && !service.optional.length"
        class="text-caption text-medium-emphasis"
      >
        {{ t('servicesView.noDependencies') }}
      </div>

      <!-- Every declared dependency, answered or not.
           The unanswered one used to be dropped before it reached this
           template, so Kibana with no Elasticsearch installed rendered "No
           dependencies" — which is the opposite of true, in exactly the state
           somebody opens this panel to diagnose. What the row says now is the
           three-way answer: nothing provides it, something does and is
           stopped, or it is running. -->
      <div
        v-for="dep in [...service.required, ...service.optional]"
        :key="`${dep.capability}:${dep.service ?? ''}`"
        class="row"
      >
        <span class="row-key">
          {{ dep.required ? t('servicesView.required') : t('servicesView.optional') }}
        </span>
        <v-chip size="small" label :color="dependencyColour(dep)">
          <v-icon start size="small">{{ dependencyIcon(dep) }}</v-icon>
          {{ dep.provider ?? dep.service ?? dep.capability }}
        </v-chip>
        <!-- The capability, when the row is not already named by it. It is
             what the manifest actually asks for, and it is the reason MariaDB
             can answer a package that names no service at all. -->
        <span v-if="dep.provider || dep.service" class="text-caption text-medium-emphasis">
          {{ dep.capability }}
        </span>
        <span class="text-caption" :class="`text-${dependencyColour(dep)}`">
          {{ dependencyState(dep) }}
        </span>
      </div>
    </template>

    <template #footer>
      <v-btn
        color="primary"
        variant="flat"
        prepend-icon="mdi-console"
        :disabled="!service?.running"
        @click="openTerminal"
      >
        {{ t('detail.externalTerminal') }}
      </v-btn>
    </template>
  </SideSheet>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
  font-size: 0.8rem;
}

.row-key {
  opacity: 0.65;
  min-width: 108px;
  flex-shrink: 0;
}

.credential {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 30px;
  font-size: 0.8rem;
}

.endpoint {
  margin-bottom: 8px;
}

.endpoint-head {
  display: flex;
  align-items: center;
  font-size: 0.75rem;
}

.endpoint-row {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* A URI with a database name and a query on the end runs past the sheet in a
   narrow window; it wraps rather than scrolling sideways, because the part that
   would be cut off is the part that is easy to get wrong. */
.endpoint-uri {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  font-weight: 600;
  word-break: break-all;
  flex: 1 1 auto;
  user-select: all;
}

.credential-key {
  min-width: 0;
}

/* `.mono` is declared in settings-panes.css under `.settings-scroll`, which
   this sheet is not inside — so the uses of it here (a snapshot name, the
   dump's progress line, and now the image reference) have been rendering in
   the body font. Declared locally rather than widened there: that file's own
   comment says `.mono` is a name any page might reuse, which is exactly why it
   is scoped and why this is a copy rather than a promotion. */
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  overflow-wrap: anywhere;
}

.credential-value {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  font-weight: 600;
  word-break: break-all;
}

/* A bind-mount source is an absolute host path; without this it runs out of the
   sheet. */
.path-chip {
  max-width: 100%;
  height: auto;
  min-height: 24px;
}

.path-chip :deep(.v-chip__content) {
  white-space: normal;
  word-break: break-all;
  padding: 2px 0;
}

/* Subject on its own line above the addresses: a captured message is usually
   from one no-reply address to one developer, so the subject is the only part
   that distinguishes two rows. */
.mail-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.mail-subject {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mail-meta {
  font-size: 12px;
  opacity: 0.7;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Shown as source, never rendered — see the template. Wrapped rather than
   scrolled sideways: an HTML mail is one very long line. */
.mail-body {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 420px;
  overflow-y: auto;
  margin: 0;
}
</style>
