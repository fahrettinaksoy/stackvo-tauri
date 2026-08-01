<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useDisplay } from 'vuetify';
import { api } from '@/lib/ipc';
import { listenAll } from '@/lib/events';
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
  /**
   * The suffix `.env` gives domains. Passed in rather than read again here:
   * the table already resolved it, and two readers of one setting drift.
   */
  tld: { type: String, default: '' },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);

const { t } = useI18n();
const display = useDisplay();

/**
 * Sized against the window rather than in fixed pixels, unlike the other
 * sheets: this one carries absolute host paths, port tables and credential
 * rows, which wrap into unreadable ribbons in a form-width panel. Floored so a
 * narrow window still gets something readable, capped so a wide one does not
 * get a sheet that may as well be a page.
 */
const width = computed(() => Math.round(Math.min(Math.max(display.width.value * 0.55, 560), 1040)));

/** The `.env` value is a hostname fragment, not a URL. */
const domain = computed(() => (props.service?.url ? `${props.service.url}.${props.tld}` : null));

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
    revealed.value = {
      ...revealed.value,
      [credential.envKey]: await api.envReveal(credential.envKey),
    };
  } catch (e) {
    error.value = e;
  }
}

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
    if (!open) return;
    // A different service is a different panel: start it on the detail tab
    // rather than on whatever the last one was left showing.
    tab.value = 'detail';
    dbLine.value = '';
    dbResult.value = '';
    openMessage.value = null;
    body.value = null;
    load(props.service);
    loadMail();
    api.dbTargets().then(
      (targets) => (dbTargets.value = targets),
      () => (dbTargets.value = [])
    );
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
onUnmounted(() => stopDbEvents?.());
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
      <v-chip
        v-if="service"
        size="small"
        variant="flat"
        :color="service.running ? 'success' : 'grey-darken-1'"
        :prepend-icon="service.running ? 'mdi-check-circle' : 'mdi-stop-circle'"
      >
        {{ service.running ? t('system.running') : t('system.stopped') }}
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
        <v-tab value="logs" prepend-icon="mdi-text-box-outline" :disabled="!service?.built">
          {{ t('logs.title') }}
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

    <!-- Streamed only while its tab is showing. -->
    <LogView
      v-if="tab === 'logs' && service"
      :container="service.containerName"
      :active="modelValue && tab === 'logs'"
    />

    <!-- Explicitly the detail tab rather than "whatever is left": the inbox
         above is its own v-if chain, so an `v-else` here would render the
         detail panel underneath it. -->
    <template v-else-if="service && tab === 'detail'">
      <ErrorAlert :error="error" type="error" class="mb-2" />

      <!-- Network ------------------------------------------------------- -->
      <div class="sheet-group">{{ t('servicesView.networkInfo') }}</div>

      <div class="row">
        <span class="row-key">{{ t('servicesView.colContainerName') }}</span>
        <v-chip size="small" variant="tonal" color="primary" class="path-chip">
          <v-icon start size="small">mdi-docker</v-icon>{{ service.containerName }}
        </v-chip>
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

      <!-- Falls back to the configured port when the container is not running:
           a stopped service still has a host port, and an empty section would
           suggest it does not. -->
      <div v-if="!service.ports.length && service.hostPort" class="row">
        <span class="row-key">{{ t('services.hostPort') }}</span>
        <v-chip size="small" variant="tonal" color="grey">
          <v-icon start size="small">mdi-lan-disconnect</v-icon>{{ service.hostPort }}
        </v-chip>
      </div>

      <div v-if="service.ports.length" class="row align-start">
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
      <div v-for="dep in service.required" :key="dep" class="row">
        <span class="row-key">{{ t('servicesView.required') }}</span>
        <v-chip
          size="small"
          label
          :color="service.unmetDependencies.includes(dep) ? 'warning' : 'success'"
          >{{ dep }}</v-chip
        >
      </div>
      <div v-for="dep in service.optional" :key="dep" class="row">
        <span class="row-key">{{ t('servicesView.optional') }}</span>
        <v-chip size="small" label>{{ dep }}</v-chip>
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

.credential-key {
  min-width: 0;
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
