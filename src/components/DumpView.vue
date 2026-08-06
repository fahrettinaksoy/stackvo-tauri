<script setup>
import { computed, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import { isFlat, summary, text as valueText } from '@/lib/dumpnode';
import ErrorAlert from '@/components/ErrorAlert.vue';
import DumpValue from '@/components/DumpValue.vue';

/**
 * `dump()` and `dd()`, caught out of the response — one renderer, two scopes.
 *
 * The same split the log viewer settled on, for the same reason it settled on
 * it. The per-project pane answers "what did this project dump"; it cannot
 * answer "which of my eight projects just dumped something", which is the
 * question you have *before* you know which project to open. Herd's dump
 * window is global and opens itself for exactly that; Lerd has both a per-site
 * tab and a global Debug bridge view.
 *
 * And there was a hole this closes rather than merely a nicety. Capture stays
 * on across navigation on purpose — a queue worker's dump should be caught
 * while you are looking at something else — but the *reader* only ran inside
 * one project's page, so those events piled up unseen until somebody happened
 * to navigate back. A page is where you leave it open.
 *
 * Nothing about rows, search or the source link is reimplemented per scope: a
 * second renderer is a second place for the two to drift.
 */
const props = defineProps({
  /** The project to follow. Empty in `scope="all"`, which follows every one. */
  project: { type: String, default: '' },
  scope: { type: String, default: 'project' },
});

const { t } = useI18n();
const app = useAppStore();

const rows = ref([]);
const search = ref('');
const useRegex = ref(false);
const only = ref('');
const kinds = ref([]);
const overview = ref([]);
const busy = ref('');
const error = ref(null);

/**
 * Hold the list still while it is being read.
 *
 * The same affordance the log viewer needed and for the same reason: a row you
 * are reading moves down every time another arrives, and a dump is something
 * you read rather than watch. What comes in while paused is counted on the
 * button and released when it is pressed again.
 */
const paused = ref(false);
const pending = ref([]);
const copied = ref(false);

/**
 * Which rows are open, by `seq`.
 *
 * Held beside the list rather than on the row itself, so that reading the list
 * never rewrites it — a row is data the poll owns, and what the reader has
 * opened is not something the next poll should be able to overwrite. `seq` and
 * not the timestamp: two workers can dump the same value from the same line in
 * the same microsecond, and it only ever counts up, so a row that leaves the
 * bounded buffer cannot hand its state to a later one.
 */
const open = ref(new Set());

/**
 * Per-project cursors: how many events each project had at the last poll.
 *
 * A map rather than a number even in project scope, so the two scopes run the
 * same loop. Events are only ever appended, so a cursor is the whole of what a
 * reader has to remember, and a missed tick costs nothing.
 */
let cursors = {};
let seq = 0;
let timer = null;

/** Which projects this scope polls: one, or every one that is capturing. */
const followed = computed(() =>
  props.scope === 'all'
    ? overview.value.filter((p) => p.enabled && p.mounted).map((p) => p.project)
    : [props.project].filter(Boolean)
);

/** The row for this project, in project scope — the switch and the warning. */
const mine = computed(() => overview.value.find((p) => p.project === props.project) ?? null);

/**
 * The project the toolbar's switch acts on.
 *
 * In project scope that is the only project there is; on the page it is
 * whatever the select points at, and nothing when it points at "all". One
 * name so the bar does not branch on scope for a control that means the same
 * thing in both.
 */
const target = computed(() =>
  props.scope === 'project'
    ? mine.value
    : only.value
      ? (overview.value.find((p) => p.project === only.value) ?? null)
      : null
);

const projectItems = computed(() => [
  { title: t('dumps.allProjects'), value: '' },
  ...overview.value.map((p) => ({ title: p.project, value: p.project, row: p })),
]);

/**
 * What each option says about itself before it is chosen.
 *
 * A project that cannot carry the bridge yet is the common case on a stack
 * that has not been recreated since this shipped, and leaving that to be
 * discovered by picking it and finding a disabled switch is a worse way to
 * learn it.
 */
function projectItemProps(item) {
  const row = item.row;
  if (!row) return {};
  return {
    appendIcon: row.enabled ? 'mdi-pulse' : undefined,
    prependIcon: row.mounted ? undefined : 'mdi-alert-outline',
    subtitle: row.mounted ? undefined : t('dumps.needsRecreateShort'),
  };
}

async function loadOverview() {
  try {
    overview.value = asList(await api.debugBridgeOverview());
  } catch {
    overview.value = [];
  }
}

async function poll() {
  for (const project of followed.value) {
    try {
      const since = cursors[project] ?? 0;
      const { total, events } = await api.debugBridgeEvents(project, since);
      if (total < since) {
        // Cleared or rotated underneath us. Drop what came from that project
        // and start again rather than showing a list that can never grow.
        rows.value = rows.value.filter((r) => r.project !== project);
        cursors[project] = 0;
        continue;
      }
      cursors[project] = total;
      // The value is flattened to text once, on arrival, rather than per
      // keystroke: search runs over every row on every character typed, and
      // walking five hundred trees to answer one keypress is work that only
      // ever produces the same answer.
      const arrived = events.map((e) => ({
        ...e,
        project,
        seq: seq++,
        text: valueText(e.value),
      }));
      if (paused.value) pending.value.unshift(...arrived.reverse());
      else for (const e of arrived) rows.value.unshift(e);
    } catch {
      /* A poll that fails is a poll; the next one decides. */
    }
  }
  // Newest first, and bounded: this is a pane, not an archive. The events file
  // is the archive, and it survives a restart.
  if (rows.value.length > 500) rows.value.length = 500;
  if (pending.value.length > 500) pending.value.length = 500;
}

async function tick() {
  await loadOverview();
  await poll();
}

function restart() {
  clearInterval(timer);
  rows.value = [];
  open.value.clear();
  cursors = {};
  timer = setInterval(tick, 1000);
  tick();
}

watch(() => [props.project, props.scope], restart, { immediate: true });
onUnmounted(() => clearInterval(timer));

async function setCapture(project, on) {
  busy.value = project;
  error.value = null;
  try {
    await api.debugBridgeSet(project, !!on);
    await tick();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = '';
  }
}

function resume() {
  rows.value.unshift(...pending.value);
  pending.value = [];
  paused.value = false;
  if (rows.value.length > 500) rows.value.length = 500;
}

/** Copy what is on screen — the filtered rows, not the whole buffer. */
async function copyVisible() {
  const text = visible.value
    .map((r) => {
      const where = [r.file && `${r.file}${r.line ? `:${r.line}` : ''}`, r.request]
        .filter(Boolean)
        .join('  ');
      return [`# ${clock(r.at)}  ${r.project}${where ? `  ${where}` : ''}`, r.text].join('\n');
    })
    .join('\n\n');
  await copyText(text);
}

/** One dump's value, which is the thing that gets pasted into an issue. */
async function copyRow(row) {
  await copyText(String(row.text ?? ''));
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1200);
  } catch {
    /* clipboard unavailable */
  }
}

async function clearAll() {
  for (const project of followed.value) {
    try {
      await api.debugBridgeClear(project);
    } catch (e) {
      error.value = e;
    }
  }
  rows.value = [];
  open.value.clear();
  cursors = {};
}

/**
 * Is anything the list would show actually being captured?
 *
 * Not simply `followed.length`: with one project selected and another one
 * capturing, that is true while the selected project is off — and the pane
 * then says "waiting for a dump" about something that can never produce one.
 * The question is about what is on screen, so it follows the selection.
 */
const waitingForSomething = computed(() => {
  if (props.scope === 'project') return !!mine.value?.enabled;
  return only.value ? !!target.value?.enabled : followed.value.length > 0;
});

/**
 * The search, compiled once per query rather than per row.
 *
 * An invalid regex is a query somebody is halfway through typing, so it
 * matches nothing rather than throwing on every row and blanking the pane.
 */
const matcher = computed(() => {
  const q = (search.value ?? '').trim();
  if (!q) return null;
  if (!useRegex.value) {
    const lower = q.toLowerCase();
    return (text) => text.toLowerCase().includes(lower);
  }
  try {
    const re = new RegExp(q, 'i');
    return (text) => re.test(text);
  } catch {
    return () => false;
  }
});

const visible = computed(() => {
  const match = matcher.value;
  return rows.value
    .filter((r) => !only.value || r.project === only.value)
    .filter((r) => !kinds.value.length || kinds.value.includes(sapiGroup(r)))
    .filter(
      (r) =>
        !match ||
        // Across everything on the row: "where did I dump that" is answered by
        // the request or the file as often as by the value.
        [r.text, r.label, r.file, r.request, r.project].some((f) => match(f ?? ''))
    );
});

function toggle(row) {
  if (open.value.has(row.seq)) open.value.delete(row.seq);
  else open.value.add(row.seq);
}

/**
 * Rows whose summary is already the whole value.
 *
 * `dump(503)` used to be a row saying `503` that opened onto a panel saying
 * `503`. There is nothing behind a scalar, so it gets no disclosure and no
 * second copy of itself — the row *is* the value.
 */
function flat(row) {
  return isFlat(row.value);
}

/**
 * Web, CLI or queue — the three places a dump comes from.
 *
 * Derived from the SAPI rather than reported by the bridge: `fpm-fcgi` is a
 * web request and everything else is a script, and the one distinction worth
 * drawing inside "script" is whether it was a queue worker, which is visible
 * in the command line the bridge already captured.
 */
function sapiGroup(row) {
  if ((row.sapi ?? '') === 'fpm-fcgi') return 'web';
  return /queue:(work|listen)/.test(row.request ?? '') ? 'queue' : 'cli';
}

const counts = computed(() => {
  const out = { web: 0, cli: 0, queue: 0 };
  for (const r of rows.value) out[sapiGroup(r)] += 1;
  return out;
});

function shortFile(file) {
  return String(file ?? '')
    .split('/')
    .slice(-3)
    .join('/');
}

function clock(at) {
  return new Date((at ?? 0) * 1000).toLocaleTimeString();
}

/**
 * Open the line that produced the dump.
 *
 * The path the bridge captured is the container's, so the host half is the
 * project directory — the same substitution the log viewer's file links make.
 */
function openSource(row) {
  const root = app.workspace?.projectsDir;
  if (!root || !row.file) return;
  const relative = String(row.file).replace(/^\/var\/www\/html\/?/, '');
  api.openInEditor(`${root}/${row.project}/${relative}`).catch(() => {});
}
</script>

<template>
  <div class="dump-root">
    <!-- The same bar the log viewer has, because it is the same kind of view:
         a stream you filter, hold still, copy out of, and clear. Anything it
         does not share is a control this stream has and that one does not. -->
    <v-toolbar flat class="dump-head">
      <v-icon size="20">mdi-bug-outline</v-icon>

      <!-- One select, not one switch per project: ten projects is ten switches
           wrapping onto a second row, which reads as a settings screen rather
           than a filter. The select answers "which project" and the switch
           beside it answers "should it be capturing". -->
      <!-- Named, because it has no visible label at all: the selected value is
           the only text in it, so a screen reader had nothing to announce the
           control *as*. -->
      <v-select
        v-if="scope === 'all'"
        v-model="only"
        :items="projectItems"
        :item-props="projectItemProps"
        :aria-label="t('dumps.allProjects')"
        density="compact"
        variant="solo-filled"
        flat
        hide-details
        class="dump-picker"
      />

      <!-- Capture is a per-project fact, so it follows the selection. `All
           projects` has nothing to switch — it is a view over what is on. -->
      <v-switch
        v-if="target"
        :model-value="target.enabled"
        color="primary"
        density="compact"
        hide-details
        :disabled="!target.mounted"
        :loading="busy === target.project"
        class="dump-switch"
        :aria-label="t('dumps.capture')"
        @update:model-value="(v) => setCapture(target.project, v)"
      >
        <template #label>
          <span class="text-caption">{{ t('dumps.capture') }}</span>
        </template>
      </v-switch>

      <v-tooltip
        v-if="target && !target.mounted"
        :text="t('dumps.needsRecreate')"
        location="bottom"
      >
        <template #activator="{ props: tip }">
          <v-icon v-bind="tip" size="18" color="warning">mdi-alert-outline</v-icon>
        </template>
      </v-tooltip>

      <v-text-field
        v-model="search"
        :placeholder="t('dumps.search')"
        :aria-label="t('dumps.search')"
        density="compact"
        variant="solo-filled"
        flat
        hide-details
        clearable
        prepend-inner-icon="mdi-magnify"
        class="dump-search"
      >
        <!-- Regex as a switch inside the field, where the query it changes the
             meaning of is. `.*` reads as an icon a developer knows. -->
        <template #append-inner>
          <v-btn
            icon
            size="x-small"
            variant="text"
            :color="useRegex ? 'primary' : undefined"
            :aria-label="t('dumps.regex')"
            :aria-pressed="useRegex"
            @click="useRegex = !useRegex"
          >
            <v-icon size="18">mdi-regex</v-icon>
            <v-tooltip activator="parent">{{ t('dumps.regex') }}</v-tooltip>
          </v-btn>
        </template>
      </v-text-field>

      <!-- Where it came from. Counts in the menu rather than chips in the bar,
           the same call the level filter made: three chips is wider than the
           rows they filter. -->
      <v-menu :close-on-content-click="false">
        <template #activator="{ props: menuProps }">
          <v-btn
            v-bind="menuProps"
            icon
            variant="text"
            size="small"
            :color="kinds.length ? 'primary' : undefined"
            :aria-label="t('dumps.filterSource')"
          >
            <v-icon>mdi-filter-variant</v-icon>
            <v-tooltip activator="parent">{{ t('dumps.filterSource') }}</v-tooltip>
          </v-btn>
        </template>
        <v-list density="compact">
          <v-list-item
            v-for="k in ['web', 'cli', 'queue']"
            :key="k"
            @click="kinds = kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k]"
          >
            <template #prepend>
              <v-checkbox-btn :model-value="kinds.includes(k)" density="compact" />
            </template>
            <v-list-item-title class="text-caption">
              {{ t(`dumps.source.${k}`) }}
              <span class="text-medium-emphasis"> · {{ counts[k] }}</span>
            </v-list-item-title>
          </v-list-item>
        </v-list>
      </v-menu>

      <v-btn
        icon
        variant="text"
        size="small"
        :color="copied ? 'success' : undefined"
        :aria-label="t('dumps.copy')"
        :disabled="!visible.length"
        @click="copyVisible"
      >
        <v-icon>{{ copied ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
        <v-tooltip activator="parent">{{ t('dumps.copy') }}</v-tooltip>
      </v-btn>

      <v-btn
        icon
        variant="text"
        size="small"
        :color="paused ? 'warning' : undefined"
        :aria-label="paused ? t('dumps.resume') : t('dumps.pause')"
        :aria-pressed="paused"
        @click="paused ? resume() : (paused = true)"
      >
        <v-badge
          :model-value="paused && pending.length > 0"
          :content="pending.length"
          color="warning"
          offset-x="-2"
          offset-y="-2"
        >
          <v-icon>{{ paused ? 'mdi-play' : 'mdi-pause' }}</v-icon>
        </v-badge>
        <v-tooltip activator="parent">
          {{ paused ? t('dumps.resumeHint', { n: pending.length }) : t('dumps.pause') }}
        </v-tooltip>
      </v-btn>

      <v-btn
        icon
        variant="text"
        size="small"
        :disabled="!rows.length"
        :aria-label="t('dumps.clear')"
        @click="clearAll"
      >
        <v-icon>mdi-notification-clear-all</v-icon>
        <v-tooltip activator="parent">{{ t('dumps.clearHint') }}</v-tooltip>
      </v-btn>

      <!-- Both of these used to sit above the list, on every visit, for the
           life of the pane. They are each learned once — that the switch costs
           no container, and that a `dd()` shows a 500 in the browser while the
           dump lands here — and a sentence that is read once should not hold a
           band of the pane forever. Behind a control that says it is help, the
           second time somebody wants it is the only time they pay for it. -->
      <v-menu location="bottom end" :close-on-content-click="false">
        <template #activator="{ props: helpProps }">
          <v-btn v-bind="helpProps" icon variant="text" size="small" :aria-label="t('dumps.help')">
            <v-icon>mdi-help-circle-outline</v-icon>
            <v-tooltip activator="parent">{{ t('dumps.help') }}</v-tooltip>
          </v-btn>
        </template>
        <v-card max-width="380">
          <v-card-text class="text-caption d-flex flex-column ga-3">
            <div>{{ t('dumps.captureHint') }}</div>
            <div>{{ t('dumps.ddEndsTheRequest') }}</div>
          </v-card-text>
        </v-card>
      </v-menu>

      <slot name="actions" />
    </v-toolbar>

    <v-divider />

    <div class="dump-body">
      <ErrorAlert :error="error" type="error" closable class="mb-3" @close="error = null" />

      <div v-if="scope === 'all' && !overview.length" class="text-caption text-medium-emphasis">
        {{ t('dumps.noProjects') }}
      </div>

      <!-- The recreate button belongs to whoever framed this view: on a project
           page it is that project's lifecycle control. -->
      <v-alert
        v-else-if="scope === 'project' && mine && !mine.mounted"
        type="warning"
        variant="tonal"
        class="mb-4"
      >
        <div class="text-caption">{{ t('dumps.needsRecreate') }}</div>
        <slot name="recreate" />
      </v-alert>

      <!-- Nothing yet: the only moment the pane has room to explain itself, and
           the only moment anybody wants it to. -->
      <div v-else-if="!visible.length" class="dump-empty text-caption text-medium-emphasis">
        <div>{{ waitingForSomething ? t('dumps.waiting') : t('dumps.captureOff') }}</div>
        <div v-if="scope === 'all' && !only" class="mt-1">
          {{ t('dumps.capturingCount', { on: followed.length, total: overview.length }) }}
        </div>
      </div>

      <div v-else class="dump-list">
        <div
          v-for="row in visible"
          :key="row.seq"
          class="dump-row"
          :class="{ 'dump-row--open': open.has(row.seq) }"
        >
          <!-- One line, always. What it says about itself before it is opened
               is the summary, the line it came from and the request — the three
               things somebody scanning for a dump is scanning *for*. -->
          <div
            class="dump-line"
            :class="{ 'dump-line--clickable': !flat(row) }"
            @click="!flat(row) && toggle(row)"
          >
            <span v-if="flat(row)" class="dump-chev dump-chev--none" />
            <span v-else class="dump-chev" :class="{ 'dump-chev--open': open.has(row.seq) }"
              >▸</span
            >

            <span class="text-caption text-medium-emphasis dump-time">{{ clock(row.at) }}</span>

            <v-chip v-if="scope === 'all'" size="x-small" variant="tonal">
              {{ row.project }}
            </v-chip>
            <v-chip v-if="row.label" size="x-small" variant="tonal" color="primary">
              {{ row.label }}
            </v-chip>

            <!-- A value with nothing behind it is rendered in place, typed and
                 coloured like it would be in the tree. Anything else shows what
                 it is and how big — never the first line of a formatting, which
                 for every array in existence was `[`. -->
            <DumpValue v-if="flat(row)" :node="row.value" class="dump-flat" />
            <span v-else class="dump-peek">{{ summary(row.value) }}</span>

            <v-spacer />

            <!-- Where it came from, on the collapsed row: it was behind a click
                 before, and it is the first thing anybody wants from a dump
                 they did not expect to see. -->
            <button
              v-if="row.file"
              type="button"
              class="dump-where"
              :title="`${row.file}${row.line ? `:${row.line}` : ''}`"
              @click.stop="openSource(row)"
            >
              {{ shortFile(row.file) }}{{ row.line ? `:${row.line}` : '' }}
            </button>

            <v-chip v-if="sapiGroup(row) !== 'web'" size="x-small" variant="tonal">
              {{ t(`dumps.source.${sapiGroup(row)}`) }}
            </v-chip>
            <span v-if="row.request" class="text-caption text-medium-emphasis dump-req">
              {{ row.request }}
            </span>

            <v-btn
              icon
              size="x-small"
              variant="text"
              class="dump-copy"
              :aria-label="t('dumps.copyValue')"
              @click.stop="copyRow(row)"
            >
              <v-icon size="14">mdi-content-copy</v-icon>
              <v-tooltip activator="parent" location="left">{{ t('dumps.copyValue') }}</v-tooltip>
            </v-btn>
          </div>

          <div v-if="!flat(row) && open.has(row.seq)" class="dump-detail">
            <DumpValue :node="row.value" />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* The log viewer's shape, because it is the same kind of view: a dense bar, a
   rule, and a body that scrolls under it. */
.dump-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.dump-head {
  /* A real `v-toolbar`, and that is the fix rather than a detail.
     
     This was a hand-rolled row with a height written into the stylesheet,
     twice: 64px first, from Vuetify's documented default, then 48px after
     measuring a screenshot. Both were wrong, because the height is not a
     constant at all — `appearance.js` applies the user's density setting to
     Vuetify's `global` defaults, so every toolbar in the app grows and shrinks
     with a knob in Settings. A number here matches at exactly one setting and
     is wrong at the others, and the screenshot that produced 48px was taken in
     a browser, where the preference could not load and the fallback applied.
     
     A toolbar takes its height from the same defaults every other bar does, so
     there is nothing left to keep in step. */
  flex: 0 0 auto;
}

.dump-head :deep(.v-toolbar__content) {
  gap: 8px;
  padding-inline: 16px 8px;
}

.dump-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 16px;
  /* Solid to the bottom of the card, the way a table is: a body that stops at
     its content leaves the card's own surface showing under it and the panel
     stops looking like one. */
  background: rgb(var(--v-theme-surface));
}

/* Bounded so a long project name cannot push the controls off the bar —
   `apibackend.bloomberghtradio` is one of the names this is used with. */
.dump-picker {
  max-width: 260px;
  min-width: 160px;
  flex: 0 1 auto;
}

.dump-switch {
  flex: 0 0 auto;
}

.dump-search {
  max-width: 420px;
  min-width: 180px;
  flex: 1 1 280px;
}

.dump-empty {
  padding-top: 12px;
}

/* Rules between rows rather than cards around them. A stream of forty dumps
   drawn as forty cards is forty borders and thirty-nine gaps, and the eye reads
   the chrome before it reads a single value. */
.dump-list {
  display: flex;
  flex-direction: column;
}

.dump-row + .dump-row {
  border-top: thin solid rgba(var(--v-border-color), var(--v-border-opacity));
}

.dump-row--open {
  background: rgba(var(--v-theme-on-surface), 0.03);
}

/* A dump row has to stay one line whatever is in it: the value can be a
   thousand characters and the request a long URL, and a row that wraps to four
   lines turns a list into a wall. Everything shrinks, the value first. */
.dump-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  width: 100%;
  padding: 6px 4px;
}

.dump-line--clickable {
  cursor: pointer;
}

.dump-line:hover {
  background: rgba(var(--v-theme-on-surface), 0.04);
}

/* Held even when there is nothing to disclose, so the times, the chips and the
   values all start on the same column whatever the row turned out to be. */
.dump-chev {
  flex: 0 0 auto;
  width: 10px;
  font-size: 10px;
  opacity: 0.5;
  transition: transform 120ms ease;
}

.dump-chev--open {
  transform: rotate(90deg);
}

.dump-chev--none {
  visibility: hidden;
}

.dump-time {
  flex: 0 0 auto;
  font-variant-numeric: tabular-nums;
}

/* Two classes, and that is the point rather than an accident: a flat value is
   rendered by `DumpValue`, which styles a value from the older bridge as a
   wrapping block. In a row that must stay one line tall it has to lose, and it
   only loses on specificity. */
.dump-peek,
.dump-line .dump-flat {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

/* The source is a link to a line of code, and reads as one. It keeps its whole
   tail — `Controller.php:41` truncated from the right loses the line number,
   which is the half somebody came for. */
.dump-where {
  flex: 0 0 auto;
  max-width: 30%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  direction: rtl;
  text-align: right;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11px;
  color: rgb(var(--v-theme-primary));
  background: none;
  border: 0;
  padding: 0;
  cursor: pointer;
}

.dump-where:hover {
  text-decoration: underline;
}

/* The request is context, not the subject — it gives up its space last, but it
   does give it up. */
.dump-req {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 30%;
  flex-shrink: 0;
}

/* Present on every row so the column does not reflow on hover, and quiet until
   the row is under the pointer. */
.dump-copy {
  flex: 0 0 auto;
  opacity: 0;
}

.dump-line:hover .dump-copy,
.dump-copy:focus-visible {
  opacity: 1;
}

.dump-detail {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  line-height: 1.6;
  margin: 0 0 8px 22px;
  padding: 10px 12px;
  border-radius: 6px;
  max-height: 60vh;
  overflow: auto;
  background: rgb(var(--v-theme-surface-bright));
  user-select: text;
}
</style>
