<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import { api } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import MailRelayPane from '@/components/MailRelayPane.vue';

/**
 * The inbox as a destination — Herd's Mail page, done here.
 *
 * Three states, three layouts, none sharing a frame:
 *
 *  - **No catcher running.** A Material empty state — icon, headline, one
 *    primary action — not a banner: the page's entire content is the
 *    situation, so it is presented as content, centred, with the action that
 *    resolves it. The app never flips `.env` because a page was opened; the
 *    user clicks, and `service_enable` runs the whole chain (flag →
 *    regenerate → `up -d`).
 *  - **Running, inbox empty.** A quieter empty state: nothing is wrong,
 *    nothing has arrived.
 *  - **Running with mail.** Toolbar + master-detail split. The toolbar only
 *    exists here — a refresh button over an inbox that cannot exist yet is
 *    noise wearing a disabled state.
 *
 * HTML mail renders in a sandboxed iframe (`sandbox` with no tokens): a
 * captured mail is whatever some code under test decided to send.
 */
const { t } = useI18n();
const router = useRouter();

const status = ref(null);
const messages = ref([]);
const selected = ref(null);
const body = ref(null);
const view = ref('preview');
const error = ref(null);
const loading = ref(false);
const clearing = ref(false);
const deleting = ref(false);
const query = ref('');
const searching = ref(false);
/** The compatibility report for the open message; null until it is fetched. */
const htmlCheck = ref(null);
/** Link results, and whether the (network-touching) check has been asked for. */
const linkCheck = ref(null);
const linkChecking = ref(false);
const activating = ref(false);

const POLL_MS = 10_000;
let timer = null;

const running = computed(() => !!status.value?.running);
const catcher = computed(() => status.value?.service ?? 'mailpit');

/**
 * Search runs server-side, so Mailpit's own syntax (`from:`, `to:`,
 * `subject:`, quoted phrases) works — reimplementing a filter here would be a
 * second, worse query language over a subset of the data.
 */
async function load() {
  loading.value = true;
  try {
    status.value = await api.mailStatus();
    const q = query.value.trim();
    messages.value = !status.value?.running
      ? []
      : q
        ? await api.mailSearch(q, 100)
        : await api.mailMessages(100);
    error.value = null;
    // The selection survives a poll unless its message was cleared under it.
    if (selected.value && !messages.value.some((m) => m.id === selected.value)) {
      selected.value = null;
      body.value = null;
    }
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

/**
 * Enable the catcher and bring its container up, on the user's say-so.
 * Idempotent on the flag, and `up -d` creates-or-starts whatever state the
 * container is in — so one action serves "off" and "stopped" alike.
 */
async function activate() {
  activating.value = true;
  error.value = null;
  try {
    await api.serviceEnable(catcher.value);
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    activating.value = false;
  }
}

async function open(id) {
  selected.value = id;
  body.value = null;
  htmlCheck.value = null;
  linkCheck.value = null;
  try {
    body.value = await api.mailMessage(id);
    view.value = body.value?.html ? 'preview' : 'text';
    // Cheap and local, so it comes with the message: the compatibility report
    // is the reason a developer opens a catcher rather than a mail client.
    if (body.value?.html) htmlCheck.value = await api.mailHtmlCheck(id);
  } catch (e) {
    error.value = e;
  }
}

/**
 * Follow every link in the message.
 *
 * On demand, never automatic: this is the one action on the page that leaves
 * the machine, and a local mail catcher quietly fetching whatever a captured
 * message points at is not a thing to do behind someone's back.
 */
async function runLinkCheck() {
  if (!current.value) return;
  linkChecking.value = true;
  error.value = null;
  try {
    linkCheck.value = await api.mailLinkCheck(current.value.id);
  } catch (e) {
    error.value = e;
  } finally {
    linkChecking.value = false;
  }
}

async function saveAttachment(attachment) {
  error.value = null;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({ defaultPath: attachment.fileName });
    if (!path) return;
    await api.mailAttachmentSave(current.value.id, attachment.partId, path);
  } catch (e) {
    error.value = e;
  }
}

/** Debounced so a five-letter query is one request, not five. */
let searchTimer = null;
function onSearch() {
  clearTimeout(searchTimer);
  searching.value = true;
  searchTimer = setTimeout(async () => {
    await load();
    searching.value = false;
  }, 300);
}

/** Bytes, in the unit a human would say. */
function bytes(n) {
  if (!n) return '';
  const units = ['B', 'KB', 'MB'];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit ? 1 : 0)} ${units[unit]}`;
}

async function clearAll() {
  const { confirm } = await import('@tauri-apps/plugin-dialog');
  if (!(await confirm(t('mail.confirmClear'), { title: t('mail.clear'), kind: 'warning' }))) return;
  clearing.value = true;
  try {
    await api.mailClear();
    selected.value = null;
    body.value = null;
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    clearing.value = false;
  }
}

const current = computed(() => messages.value.find((m) => m.id === selected.value) ?? null);

/**
 * Release one message to a real address (M-2).
 *
 * The catcher goes on catching everything; this sends the one that is open. The
 * opposite shape — pointing the application at a real server — would send the
 * forty password resets a test suite generates in an hour to whatever addresses
 * the fixtures happen to contain.
 *
 * The recipient is typed rather than taken from the message's own `To`: the
 * reason to release is usually that somebody else has to look at it, and
 * re-sending to the fixture address it was addressed to is the one thing that
 * is never useful.
 */
const releaseTo = ref('');
const releasing = ref(false);
const released = ref(false);

async function releaseCurrent() {
  if (!current.value || !releaseTo.value.trim()) return;
  releasing.value = true;
  released.value = false;
  error.value = null;
  try {
    await api.mailRelease(
      current.value.id,
      releaseTo.value
        .split(',')
        .map((a) => a.trim())
        .filter(Boolean)
    );
    released.value = true;
    releaseTo.value = '';
  } catch (e) {
    // Mailpit's own sentence when no relay is configured, carried through
    // unchanged — a generic "release failed" sends somebody looking at their
    // SMTP provider for a setting that is missing here.
    error.value = e;
  } finally {
    releasing.value = false;
  }
}

/** Delete the open message. No confirm — a catcher is a bin; only the
 *  whole-inbox clear keeps its dialog. */
async function deleteCurrent() {
  if (!current.value) return;
  deleting.value = true;
  try {
    await api.mailDelete(current.value.id);
    selected.value = null;
    body.value = null;
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    deleting.value = false;
  }
}

/** The date the server sent, in the user's locale — or the raw string when it
 *  does not parse, which beats hiding a message's only timestamp. */
function when(date) {
  if (!date) return '';
  const parsed = new Date(date);
  return Number.isNaN(parsed.getTime()) ? date : parsed.toLocaleString();
}

onMounted(() => {
  load();
  timer = setInterval(() => {
    // Poll quietly: no spinner, no scroll jump — new rows simply appear.
    if (!document.hidden) load();
  }, POLL_MS);
});
onUnmounted(() => clearInterval(timer));
</script>

<template>
  <PageLayout
    top-icon="mdi-email-outline"
    :top-title="t('mail.title')"
    :top-subtitle="t('mail.subtitle')"
    hide-bar
  >
    <div class="mail-page">
      <ErrorAlert :error="error" type="error" closable class="ma-4 mb-0" @close="error = null" />

      <!-- Where a released message goes (M-2). On this page rather than in
           settings, because it is configured for a reason that happens here:
           somebody presses Release and is told no relay is set up. -->
      <MailRelayPane v-if="status?.enabled && running" class="mx-4 mt-3" />

      <!-- First load: nothing is known yet, claim nothing. -->
      <div v-if="!status" class="mail-center">
        <v-progress-circular indeterminate color="primary" :aria-label="t('a11y.loading')" />
      </div>

      <!-- The catcher is off or stopped: the situation IS the content. -->
      <div v-else-if="!status.enabled || !running" class="mail-center">
        <v-empty-state
          :icon="status.enabled ? 'mdi-email-sync-outline' : 'mdi-email-off-outline'"
          :headline="status.enabled ? t('mail.stoppedHeadline') : t('mail.offHeadline')"
          :text="status.enabled ? t('mail.notRunning') : t('mail.enablePrompt')"
        >
          <template #actions>
            <div class="d-flex flex-column align-center ga-3">
              <div class="d-flex ga-2">
                <v-btn
                  color="primary"
                  variant="flat"
                  prepend-icon="mdi-email-check-outline"
                  :loading="activating"
                  @click="activate"
                >
                  {{
                    status.enabled
                      ? t('mail.startAction', { service: catcher })
                      : t('mail.enableAction', { service: catcher })
                  }}
                </v-btn>
                <v-btn variant="text" @click="router.push('/market')">
                  {{ t('nav.market') }}
                </v-btn>
              </div>
              <div v-if="activating" class="text-caption text-medium-emphasis mail-enabling">
                <v-progress-linear
                  indeterminate
                  color="primary"
                  height="2"
                  rounded
                  class="mb-2"
                  :aria-label="t('a11y.loading')"
                />
                {{ t('mail.enabling') }}
              </div>
            </div>
          </template>
        </v-empty-state>
      </div>

      <!-- Running. -->
      <template v-else>
        <!-- Nothing has arrived: quiet, nothing is wrong. -->
        <div v-if="!messages.length" class="mail-center">
          <v-empty-state
            icon="mdi-email-open-outline"
            :headline="t('mail.emptyHeadline')"
            :text="t('mail.empty')"
          />
        </div>

        <div v-else class="mail-split">
          <div class="mail-list">
            <div class="mail-list-header">
              <span class="text-subtitle-2 font-weight-bold">{{ t('mail.inbox') }}</span>
              <v-chip size="x-small" variant="tonal" color="success">{{ status.service }}</v-chip>
              <v-spacer />
              <v-btn
                icon
                size="x-small"
                variant="text"
                :loading="loading"
                :aria-label="t('app.refresh')"
                @click="load"
              >
                <v-icon>mdi-refresh</v-icon>
                <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
              </v-btn>
              <v-btn
                icon
                size="x-small"
                variant="text"
                color="error"
                :loading="clearing"
                :aria-label="t('mail.clear')"
                @click="clearAll"
              >
                <v-icon>mdi-delete-sweep-outline</v-icon>
                <v-tooltip activator="parent" location="bottom">{{ t('mail.clear') }}</v-tooltip>
              </v-btn>
            </div>
            <div class="px-3 pb-2">
              <v-text-field
                v-model="query"
                density="compact"
                variant="solo-filled"
                flat
                hide-details
                clearable
                prepend-inner-icon="mdi-magnify"
                :placeholder="t('mail.searchPlaceholder')"
                :loading="searching"
                @update:model-value="onSearch"
                @click:clear="onSearch('')"
              />
              <div class="text-caption text-medium-emphasis mt-1">
                {{ t('mail.count', { n: status.total ?? 0 }) }}
                <template v-if="status.unread">
                  · {{ t('mail.unread', { n: status.unread }) }}
                </template>
                <template v-if="query.trim()">
                  · {{ t('mail.matching', { n: messages.length }) }}
                </template>
              </div>
            </div>

            <div class="mail-list-items">
              <button
                v-for="m in messages"
                :key="m.id"
                class="mail-item"
                :class="{ selected: m.id === selected, unread: !m.read }"
                @click="open(m.id)"
              >
                <span class="mail-line">
                  <span class="mail-from">{{ m.from }}</span>
                  <span v-if="m.date" class="mail-date">{{ when(m.date) }}</span>
                </span>
                <span class="mail-subject">{{ m.subject || t('mail.noSubject') }}</span>
                <span v-if="m.snippet" class="mail-snippet">{{ m.snippet }}</span>
              </button>
            </div>
          </div>

          <div class="mail-detail">
            <div v-if="!current" class="mail-center">
              <v-empty-state icon="mdi-email-open-outline" :text="t('mail.select')" />
            </div>

            <template v-else>
              <div class="px-4 pt-3 pb-1">
                <div class="d-flex align-baseline ga-2">
                  <div class="text-subtitle-2">
                    {{ current.subject || t('mail.noSubject') }}
                  </div>
                  <v-spacer />
                  <span v-if="current.date" class="text-caption text-medium-emphasis">
                    {{ when(current.date) }}
                  </span>
                  <v-btn
                    icon
                    size="x-small"
                    variant="text"
                    color="error"
                    :loading="deleting"
                    :aria-label="t('mail.deleteOne')"
                    @click="deleteCurrent"
                  >
                    <v-icon>mdi-delete-outline</v-icon>
                    <v-tooltip activator="parent" location="bottom">{{
                      t('mail.deleteOne')
                    }}</v-tooltip>
                  </v-btn>
                </div>

                <!-- M-2. Typed rather than pre-filled with the message's own
                     recipient: the reason to release is that somebody else has
                     to see it, and re-sending to the fixture address is the one
                     thing that is never useful. -->
                <div class="d-flex align-center ga-2 mb-3" data-test="mail-release">
                  <v-text-field
                    v-model="releaseTo"
                    :label="t('mail.releaseTo')"
                    :hint="t('mail.releaseHint')"
                    persistent-hint
                    density="compact"
                    variant="outlined"
                    style="max-width: 360px"
                  />
                  <v-btn
                    size="small"
                    variant="tonal"
                    :disabled="!releaseTo.trim()"
                    :loading="releasing"
                    prepend-icon="mdi-send-outline"
                    @click="releaseCurrent"
                  >
                    {{ t('mail.release') }}
                  </v-btn>
                  <span v-if="released" class="text-caption text-success">
                    {{ t('mail.released') }}
                  </span>
                </div>

                <!-- Every recipient field the catcher reported, labelled and
                     only when present — an empty "Bcc:" row is a question, not
                     an answer. Bcc exists at all because Mailpit reads the
                     SMTP envelope; that is the whole point of a catcher. -->
                <div class="mail-headers text-caption text-medium-emphasis">
                  <template
                    v-for="row in [
                      [t('mail.fromLabel'), [current.from]],
                      [t('mail.toLabel'), current.to],
                      ['Cc', current.cc],
                      ['Bcc', current.bcc],
                      [t('mail.replyToLabel'), current.replyTo],
                    ]"
                    :key="row[0]"
                  >
                    <template v-if="row[1]?.length">
                      <span class="mail-header-key">{{ row[0] }}</span>
                      <span class="mail-header-val">{{ row[1].join(', ') }}</span>
                    </template>
                  </template>
                </div>

                <v-tabs v-model="view" density="compact" class="mt-1">
                  <v-tab value="preview" :disabled="!body?.html">{{ t('mail.preview') }}</v-tab>
                  <v-tab value="text" :disabled="!body?.text">{{ t('mail.text') }}</v-tab>
                  <v-tab value="source" :disabled="!body?.html">{{ t('mail.source') }}</v-tab>
                  <v-tab value="headers" :disabled="!body?.headers?.length">
                    {{ t('mail.headersTab') }}
                  </v-tab>
                  <v-tab v-if="body?.attachments?.length" value="attachments">
                    {{ t('mail.attachmentsTab') }}
                    <v-chip size="x-small" class="ml-2">{{ body.attachments.length }}</v-chip>
                  </v-tab>
                  <v-tab v-if="htmlCheck" value="compat">
                    {{ t('mail.compatTab') }}
                    <v-chip
                      size="x-small"
                      class="ml-2"
                      :color="
                        htmlCheck.supported >= 90
                          ? 'success'
                          : htmlCheck.supported >= 75
                            ? 'warning'
                            : 'error'
                      "
                    >
                      {{ Math.round(htmlCheck.supported) }}%
                    </v-chip>
                  </v-tab>
                  <v-tab value="links">{{ t('mail.linksTab') }}</v-tab>
                </v-tabs>
              </div>

              <!-- sandbox with NO tokens: a captured mail is untrusted input —
                   whatever the code under test decided to send. -->
              <iframe
                v-if="view === 'preview' && body?.html"
                class="mail-frame"
                sandbox
                :srcdoc="body.html"
                :title="current.subject || t('mail.noSubject')"
              />
              <pre v-else-if="view === 'text'" class="mail-raw">{{ body?.text }}</pre>
              <pre v-else-if="view === 'source'" class="mail-raw">{{ body?.html }}</pre>
              <!-- One row per header value, exactly as the catcher reported
                   them — Received appears once per hop, and that is the point
                   of looking here. -->
              <div v-else-if="view === 'headers'" class="mail-raw mail-headers-pane">
                <template v-for="(h, i) in body?.headers ?? []" :key="i">
                  <span class="mail-header-key">{{ h.name }}</span>
                  <span class="mail-header-val">{{ h.value }}</span>
                </template>
              </div>

              <!-- Attachments: what the recipient was sent to open. -->
              <div v-else-if="view === 'attachments'" class="mail-pane">
                <v-list density="compact" bg-color="transparent">
                  <v-list-item
                    v-for="a in body?.attachments ?? []"
                    :key="a.partId"
                    :title="a.fileName"
                    :subtitle="`${a.contentType} · ${bytes(a.size)}`"
                    prepend-icon="mdi-paperclip"
                  >
                    <template #append>
                      <v-btn
                        size="small"
                        variant="tonal"
                        prepend-icon="mdi-download"
                        @click="saveAttachment(a)"
                      >
                        {{ t('mail.save') }}
                      </v-btn>
                    </template>
                  </v-list-item>
                </v-list>
              </div>

              <!-- The report a developer opens a catcher for: how this markup
                   survives real mail clients. -->
              <div v-else-if="view === 'compat'" class="mail-pane">
                <div class="d-flex align-center ga-4 mb-3 flex-wrap">
                  <div>
                    <div class="text-h5 font-weight-medium">
                      {{ htmlCheck.supported.toFixed(1) }}%
                    </div>
                    <div class="text-caption text-medium-emphasis">
                      {{ t('mail.compatSupported', { n: htmlCheck.tests }) }}
                    </div>
                  </div>
                  <!-- A meter, not a spinner: it reports the share of email
                       clients this markup survives, and without a name a screen
                       reader reads the percentage with nothing attached. -->
                  <v-progress-linear
                    :aria-label="t('mail.compatTab')"
                    :model-value="htmlCheck.supported"
                    :buffer-value="htmlCheck.supported + htmlCheck.partial"
                    color="success"
                    buffer-color="warning"
                    buffer-opacity="1"
                    bg-color="error"
                    bg-opacity="0.7"
                    height="10"
                    rounded
                    class="flex-1-1"
                  />
                </div>
                <div class="text-caption text-medium-emphasis mb-2">
                  {{ t('mail.compatLegend') }}
                </div>

                <v-list density="compact" bg-color="transparent">
                  <v-list-item
                    v-for="(w, i) in htmlCheck.warnings"
                    :key="i"
                    :title="w.title"
                    :subtitle="t('mail.compatWarning', { category: w.category, found: w.found })"
                  >
                    <template #append>
                      <v-chip
                        size="x-small"
                        :color="
                          w.unsupported >= 25 ? 'error' : w.unsupported > 0 ? 'warning' : 'success'
                        "
                      >
                        {{ Math.round(w.unsupported) }}% ✗
                      </v-chip>
                    </template>
                  </v-list-item>
                  <v-list-item v-if="!htmlCheck.warnings.length" :title="t('mail.compatClean')" />
                </v-list>
              </div>

              <!-- Link check: the one action here that leaves the machine, so
                   it never runs on its own. -->
              <div v-else-if="view === 'links'" class="mail-pane">
                <div class="d-flex align-center ga-3 mb-3 flex-wrap">
                  <v-btn
                    size="small"
                    color="primary"
                    variant="tonal"
                    prepend-icon="mdi-link-variant"
                    :loading="linkChecking"
                    @click="runLinkCheck"
                  >
                    {{ t('mail.checkLinks') }}
                  </v-btn>
                  <span class="text-caption text-medium-emphasis">{{ t('mail.linksHint') }}</span>
                </div>

                <v-list v-if="linkCheck" density="compact" bg-color="transparent">
                  <v-list-item
                    v-for="(l, i) in linkCheck.links"
                    :key="i"
                    :title="l.url"
                    :subtitle="l.status"
                  >
                    <template #append>
                      <v-chip
                        size="x-small"
                        :color="l.statusCode >= 200 && l.statusCode < 400 ? 'success' : 'error'"
                      >
                        {{ l.statusCode || '—' }}
                      </v-chip>
                    </template>
                  </v-list-item>
                  <v-list-item v-if="!linkCheck.links.length" :title="t('mail.noLinks')" />
                </v-list>
              </div>
              <div v-else class="mail-center">
                <v-progress-circular
                  indeterminate
                  size="20"
                  width="2"
                  color="primary"
                  :aria-label="t('a11y.loading')"
                />
              </div>
            </template>
          </div>
        </div>
      </template>
    </div>
  </PageLayout>
</template>

<style scoped>
/* Material over hairlines: panes are tonal surfaces with radius and gap;
   rows separate by spacing and shape, selection is a tonal container. The
   only line left on the page is the tabs' own indicator. */
.mail-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* One centring rule for every "the content is a state" moment. */
.mail-center {
  flex: 1;
  display: grid;
  place-items: center;
  min-height: 0;
}

.mail-enabling {
  max-width: 360px;
  text-align: center;
}

.mail-split {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: 12px;
}

.mail-list {
  width: 360px;
  min-width: 280px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: rgba(var(--v-theme-on-surface), 0.04);
  border-radius: 0 var(--app-radius, 12px) 0 0;
}

.mail-list-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px 4px;
}

.mail-list-items {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 8px 8px;
}

.mail-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  padding: 10px 14px;
  text-align: start;
  border-radius: calc(var(--app-radius, 12px) - 4px);
}

.mail-item + .mail-item {
  margin-top: 2px;
}

.mail-item:hover {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

.mail-item.selected {
  background: rgba(var(--v-theme-primary), 0.16);
}

.mail-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}

.mail-from {
  font-size: 12px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mail-item.unread .mail-from,
.mail-item.unread .mail-subject {
  font-weight: 700;
}

.mail-date {
  font-size: 11px;
  opacity: 0.6;
  white-space: nowrap;
}

.mail-subject {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mail-snippet {
  font-size: 11.5px;
  opacity: 0.6;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mail-headers {
  display: grid;
  grid-template-columns: max-content 1fr;
  column-gap: 10px;
  row-gap: 1px;
  margin-top: 2px;
}

.mail-header-key {
  font-weight: 600;
  opacity: 0.75;
}

.mail-header-val {
  overflow-wrap: anywhere;
}

.mail-detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.mail-frame {
  flex: 1;
  border: 0;
  background: #fff;
  border-radius: var(--app-radius, 12px) 0 0 0;
}

.mail-raw {
  flex: 1;
  overflow: auto;
  margin: 0;
  padding: 12px 16px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  white-space: pre-wrap;
  word-break: break-word;
}

.mail-pane {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
}

.mail-headers-pane {
  display: grid;
  grid-template-columns: max-content 1fr;
  column-gap: 14px;
  row-gap: 3px;
  align-content: start;
}
</style>
