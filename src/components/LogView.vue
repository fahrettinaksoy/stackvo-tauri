<script setup>
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { useAppearanceStore } from '@/stores/appearance';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { LEVELS, countByLevel, filterLines, highlight, withLevels } from '@/lib/logs';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * A live log, with no opinion about where it is shown.
 *
 * Logs used to be a dialog over whatever page you were on. They are content,
 * not an interruption — you read them while looking at the container's detail —
 * so the dialog was retired and this renders inside a page section or a side
 * sheet tab instead. It carries the stream, the follow behaviour and the
 * console theming; the frame around it belongs to whoever mounts it.
 *
 * Three sources, one renderer. The container stream is stdout and stderr, which
 * is what the entrypoint and the web server say; the files under `app_logs` are
 * what the application recorded, and nothing an application logs reaches the
 * container's stdout; and `scope="all"` is the same files across every project
 * at once. All three arrive as `logs:line`, so changing source changes which
 * stream is open and nothing else.
 */
const props = defineProps({
  /** Container name or bare id; the Rust side adds the `stackvo-` prefix.
   *  Empty in `scope="all"`, which is not about one container. */
  container: { type: String, default: '' },
  /**
   * Project name, for the file sources. Omitted for a service, which has no
   * project directory and therefore only its container stream.
   */
  project: { type: String, default: '' },
  /**
   * `container` follows one container or one of its files; `all` follows every
   * project at once. The second is live-only — see `applog::Fanout`: nothing
   * parses a timestamp, so interleaved *history* from sixty files would be an
   * ordering the backend cannot justify.
   */
  scope: { type: String, default: 'container' },
  /**
   * Whether to hold the stream open. False tears it down: a background tail is
   * wasted work here and keeps a reader task alive on the Rust side.
   */
  active: { type: Boolean, default: true },
});

const fanout = computed(() => props.scope === 'all');

const appearance = useAppearanceStore();
const consoleTheme = computed(() => (appearance.value.darkConsoles ? 'dark' : undefined));

const { t, locale } = useI18n();
/**
 * Strings in the console's own language.
 *
 * `v-locale-provider` handles Vuetify's built-in text; vue-i18n needs the
 * locale passed per call, which is what this wrapper does.
 */
const consoleLocale = computed(() =>
  appearance.value.consoleLocale === 'app' ? locale.value : appearance.value.consoleLocale
);
const tc = (key, named) => t(key, named ?? {}, { locale: consoleLocale.value });

const lines = ref([]);
const streamId = ref(null);
const error = ref(null);
const follow = ref(true);
const viewport = ref(null);

/** '' is the container stream; anything else is a LogFile id. */
const source = ref('');
const files = ref([]);

/** Fanout only: which projects to follow, and how much of them is covered. */
const chosen = ref([]);
const coverage = ref(null);

const query = ref('');
/** Treat the query as a pattern. Off by default: most searches are a word. */
const useRegex = ref(false);
/**
 * Frozen buffer.
 *
 * Distinct from turning follow off, which only stops the scroll — lines keep
 * arriving and the oldest fall out of the bounded buffer, so the thing you
 * were reading can still scroll out from under you. Pausing holds the whole
 * view still and counts what came in while you read, which is the state you
 * actually want when something scrolled past too fast.
 */
const paused = ref(false);
const pending = ref([]);
const levels = ref([]);

let unlistenLine = null;
let unlistenClosed = null;
let unlistenSources = null;

const MAX_LINES = 2000;

const app = useAppStore();

/**
 * Container paths under the project's bind mount, with an extension — the
 * shape a stack frame prints. Restricting to `/var/www/html/` keeps this a
 * substitution the compose file states, never a guess.
 */
const CONTAINER_PATH = /\/var\/www\/html\/([A-Za-z0-9_@./-]+\.[A-Za-z0-9]+)/g;

/** The project a line belongs to. One stream has one; the fanout carries it
 *  per line, because that is the only place it differs. */
function lineProject(line) {
  return line.project || props.project;
}

/** Split one line around clickable file paths. Services have no source
 *  directory, so without a project the line comes back whole. */
function segments(line) {
  const text = line.text;
  if (!lineProject(line)) return [{ text }];
  const out = [];
  let last = 0;
  for (const m of text.matchAll(CONTAINER_PATH)) {
    if (m.index > last) out.push({ text: text.slice(last, m.index) });
    out.push({ text: m[0], file: m[1] });
    last = m.index + m[0].length;
  }
  if (last < text.length || !out.length) out.push({ text: text.slice(last) });
  return out;
}

/** The substitution the bind mount states: /var/www/html ↔ projects/<name>.
 *  `open_in_editor` still confines the result to the workspace on the Rust
 *  side — this is convenience, not the boundary. */
async function jump(line, file) {
  const root = app.workspace?.root;
  const project = lineProject(line);
  if (!root || !project) return;
  try {
    await api.openInEditor(`${root}/projects/${project}/${file}`);
  } catch (e) {
    error.value = e;
  }
}

/**
 * Levels are resolved once per buffer change rather than per filter change: a
 * continuation line takes the level of the entry above it, which is a scan of
 * the whole buffer and not something to redo on every keystroke.
 */
const tagged = computed(() => withLevels(lines.value));
const visible = computed(() =>
  filterLines(tagged.value, {
    query: query.value,
    levels: levels.value,
    regex: useRegex.value,
  })
);
const counts = computed(() => countByLevel(tagged.value));
const filtering = computed(() => !!query.value.trim() || levels.value.length > 0);

/**
 * A toggle's label names what the click will do, not the feature it belongs
 * to. "Follow output" on a button that is already following says nothing
 * about what pressing it changes — and the icon alone is the only other
 * signal, which a screen reader does not get at all.
 */
/** Release what arrived while paused, in the order it arrived. */
function resume() {
  paused.value = false;
  if (pending.value.length) {
    lines.value.push(...pending.value);
    pending.value = [];
    if (lines.value.length > MAX_LINES) lines.value.splice(0, lines.value.length - MAX_LINES);
  }
  if (follow.value) nextTick(scrollToEnd);
}

/**
 * Empty the view.
 *
 * The reproduce loop this pane exists for: clear, trigger the bug, read only
 * what the bug wrote. Nothing is deleted from disk — the buffer is a window,
 * and the next poll refills it from wherever the tail is.
 */
function clearView() {
  lines.value = [];
  pending.value = [];
}

const followLabel = computed(() =>
  follow.value ? tc('a11y.stopFollowing') : tc('a11y.followOutput')
);

/**
 * Where the seed ends and live output begins, as an index into `visible`.
 *
 * -1 when there is no seed or nothing live yet. Computed over the *visible*
 * lines rather than the buffer, so a filter that hides the last historic line
 * moves the boundary with it instead of leaving it stranded.
 */
const liveFrom = computed(() => {
  const rows = visible.value;
  const first = rows.findIndex((l) => !l.historic);
  return first > 0 && rows[first - 1]?.historic ? first : -1;
});

/** The picker's entries: the container stream first, then files by group. */
const sources = computed(() => {
  const items = [
    { value: '', title: tc('logs.containerStream'), props: { subtitle: props.container } },
  ];
  for (const group of ['application', 'server']) {
    const inGroup = files.value.filter((f) => f.group === group);
    if (!inGroup.length) continue;
    items.push({ type: 'subheader', title: tc(`logs.group.${group}`) });
    for (const file of inGroup) {
      items.push({ value: file.id, title: file.label, props: { subtitle: fileSize(file.bytes) } });
    }
  }
  return items;
});

function fileSize(bytes) {
  if (!Number.isFinite(bytes)) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Fanout only: the projects that have any log file at all, with how many. */
const projectItems = computed(() => {
  const counts = new Map();
  for (const file of files.value) {
    counts.set(file.project, (counts.get(file.project) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, n]) => ({
      value: name,
      title: name,
      props: { subtitle: tc('logs.files', { n }) },
    }));
});

async function loadFiles() {
  if (fanout.value) {
    try {
      files.value = await api.appLogsAll();
    } catch {
      files.value = [];
    }
    return;
  }
  if (!props.project) {
    files.value = [];
    return;
  }
  try {
    files.value = await api.appLogs(props.project);
  } catch {
    // A project with no log directories is the common case, not a failure.
    files.value = [];
  }
}

async function openStream() {
  close();
  lines.value = [];
  error.value = null;

  const target = source.value;
  coverage.value = null;

  try {
    // Listen before opening, or the first lines race the subscription.
    unlistenLine = await listen('logs:line', (event) => {
      if (event.payload.streamId !== streamId.value) return;
      const { line, stream, container, source: from, historic } = event.payload;
      const row = {
        text: line,
        stream,
        // The fanout's seed: what each file already held when the tail
        // started. Drawn behind a boundary rather than passed off as output
        // that just arrived — the ordering across files is by file, not time.
        historic: !!historic,
        // Present on the fanout only, where one buffer carries many files.
        project: from ? container : '',
        // What level inheritance is keyed on: a stack frame must take the
        // level of the entry above it *in its own file*, not of whichever
        // project happened to write in between. Doubles as the badge's
        // tooltip, so the separator is one a human reads — safe as a key
        // either way, since `is_safe_name` keeps it out of a project name.
        origin: from ? `${container} · ${from}` : '',
      };

      // Paused: hold the line rather than drop it. Dropping would make the
      // pause a gap in the record; holding makes it a delay.
      if (paused.value) {
        pending.value.push(row);
        if (pending.value.length > MAX_LINES) pending.value.shift();
        return;
      }

      lines.value.push(row);
      if (lines.value.length > MAX_LINES) lines.value.splice(0, lines.value.length - MAX_LINES);
      if (follow.value) scrollToEnd();
    });

    unlistenClosed = await listen('logs:closed', (event) => {
      if (event.payload.streamId === streamId.value) streamId.value = null;
    });

    // Coverage *updates*. The first one comes back from the open call itself,
    // because an event would race this assignment and leave the line blank
    // until the next rediscovery thirty seconds later.
    unlistenSources = await listen('logs:sources', (event) => {
      if (event.payload.streamId !== streamId.value) return;
      coverage.value = event.payload;
    });

    if (fanout.value) {
      const opened = await api.appLogsAllOpen(chosen.value.length ? [...chosen.value] : null);
      streamId.value = opened.streamId;
      coverage.value = opened;
    } else if (target) {
      streamId.value = await api.appLogOpen(props.project, target);
    } else {
      streamId.value = await api.containerLogsOpen(props.container, 300, true);
    }
  } catch (e) {
    error.value = e;
  }
}

function close() {
  if (streamId.value) {
    api.containerLogsClose(streamId.value).catch(() => {});
    streamId.value = null;
  }
  if (unlistenLine) unlistenLine();
  if (unlistenClosed) unlistenClosed();
  if (unlistenSources) unlistenSources();
  unlistenLine = null;
  unlistenClosed = null;
  unlistenSources = null;
}

async function scrollToEnd() {
  await nextTick();
  if (viewport.value) viewport.value.scrollTop = viewport.value.scrollHeight;
}

/** Copy what is on screen — the filtered lines, not the whole buffer.
 *  Interleaved lines are prefixed with their project: pasted without it, a
 *  fanout excerpt is a set of lines from nowhere in particular. */
async function copyVisible() {
  const text = visible.value
    .map((l) => (l.project ? `[${l.project}] ${l.text}` : l.text))
    .join('\n');
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    /* clipboard unavailable */
  }
}

function toggleLevel(level) {
  levels.value = levels.value.includes(level)
    ? levels.value.filter((l) => l !== level)
    : [...levels.value, level];
}

// Also on a container change: the detail page keeps this mounted and swaps the
// target when the project is rebuilt under a new name.
watch(
  () => [props.active, props.container],
  ([active]) => {
    if (!active) {
      close();
      return;
    }
    // The file the user was reading belongs to the previous project.
    source.value = '';
    loadFiles();
    openStream();
  },
  { immediate: true }
);

// Switching source is a new stream over the same renderer.
watch(source, () => {
  if (props.active) openStream();
});

// So is narrowing the fanout: the selection is applied on the Rust side, which
// is what stops sixty pollers running for eight lines the user wants to see.
watch(chosen, () => {
  if (props.active && fanout.value) openStream();
});

onUnmounted(close);
</script>

<template>
  <v-theme-provider :theme="consoleTheme">
    <v-locale-provider :locale="consoleLocale">
      <div class="log-root">
        <v-toolbar flat class="log-head">
          <v-icon size="20">mdi-text-box-outline</v-icon>

          <!-- The fanout picks projects, not files: choosing a file across a
               whole workspace is the question this view exists to avoid having
               to answer. Empty means every project. -->
          <v-select
            v-if="fanout"
            v-model="chosen"
            :items="projectItems"
            :placeholder="tc('logs.allProjects')"
            multiple
            chips
            closable-chips
            density="compact"
            variant="plain"
            hide-details
            class="log-source"
          />

          <!-- Only offered when there is something to choose between. A project
               with no log files gets the plain container name it always had. -->
          <v-select
            v-else-if="files.length"
            v-model="source"
            :items="sources"
            density="compact"
            variant="plain"
            hide-details
            class="log-source"
          />
          <span v-else class="text-body-2 log-name">{{ container }}</span>

          <v-chip v-if="streamId" size="x-small" color="success">{{ tc('logs.live') }}</v-chip>

          <!-- Coverage, because the fanout follows at most 60 files. A view
               that caps itself and says nothing reads as "nothing else is
               happening". -->
          <span v-if="fanout && coverage" class="text-caption text-medium-emphasis">
            {{
              tc('logs.following', {
                followed: coverage.followed,
                total: coverage.total,
                projects: coverage.projects,
              })
            }}
          </span>
          <v-spacer />

          <v-text-field
            v-model="query"
            :placeholder="tc('logs.search')"
            density="compact"
            variant="solo-filled"
            flat
            hide-details
            clearable
            prepend-inner-icon="mdi-magnify"
            class="log-search"
          >
            <!-- Regex as a switch inside the field, where the query it changes
                 the meaning of is. `.*` reads as an icon a developer knows. -->
            <template #append-inner>
              <v-btn
                icon
                size="x-small"
                variant="text"
                :color="useRegex ? 'primary' : undefined"
                :aria-label="tc('logs.regex')"
                :aria-pressed="useRegex"
                @click="useRegex = !useRegex"
              >
                <v-icon size="18">mdi-regex</v-icon>
                <v-tooltip activator="parent">{{ tc('logs.regex') }}</v-tooltip>
              </v-btn>
            </template>
          </v-text-field>

          <!-- Counts in the menu rather than chips in the bar: six levels of
               chips is wider than most of the lines they filter. -->
          <v-menu :close-on-content-click="false">
            <template #activator="{ props: menuProps }">
              <v-btn
                v-bind="menuProps"
                icon
                variant="text"
                size="small"
                :color="levels.length ? 'primary' : undefined"
                :aria-label="tc('logs.filterLevel')"
              >
                <v-icon>mdi-filter-variant</v-icon>
                <v-tooltip activator="parent">{{ tc('logs.filterLevel') }}</v-tooltip>
              </v-btn>
            </template>
            <v-list density="compact">
              <v-list-item
                v-for="level in LEVELS"
                :key="level"
                :active="levels.includes(level)"
                @click="toggleLevel(level)"
              >
                <template #prepend>
                  <v-icon size="16" :class="`level-${level}`">mdi-circle-medium</v-icon>
                </template>
                <v-list-item-title class="text-caption">
                  {{ tc(`logs.level.${level}`) }}
                </v-list-item-title>
                <template #append>
                  <span class="text-caption text-medium-emphasis ml-4">{{ counts[level] }}</span>
                </template>
              </v-list-item>
              <v-divider class="my-1" />
              <v-list-item :disabled="!levels.length" @click="levels = []">
                <v-list-item-title class="text-caption">{{
                  tc('logs.clearFilter')
                }}</v-list-item-title>
              </v-list-item>
            </v-list>
          </v-menu>

          <v-btn
            icon
            variant="text"
            size="small"
            :aria-label="tc('logs.copy')"
            :disabled="!visible.length"
            @click="copyVisible"
          >
            <v-icon>mdi-content-copy</v-icon>
            <v-tooltip activator="parent">{{ tc('logs.copy') }}</v-tooltip>
          </v-btn>

          <v-btn
            icon
            variant="text"
            size="small"
            :color="follow ? 'primary' : undefined"
            :aria-label="followLabel"
            :aria-pressed="follow"
            @click="follow = !follow"
          >
            <v-icon>{{
              follow ? 'mdi-arrow-down-bold-box' : 'mdi-arrow-down-bold-box-outline'
            }}</v-icon>
            <v-tooltip activator="parent">{{ followLabel }}</v-tooltip>
          </v-btn>

          <v-btn
            icon
            variant="text"
            size="small"
            :color="paused ? 'warning' : undefined"
            :aria-label="paused ? tc('logs.resume') : tc('logs.pause')"
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
              {{ paused ? tc('logs.resumeHint', { n: pending.length }) : tc('logs.pause') }}
            </v-tooltip>
          </v-btn>

          <v-btn
            icon
            variant="text"
            size="small"
            :disabled="!lines.length"
            :aria-label="tc('logs.clear')"
            @click="clearView"
          >
            <v-icon>mdi-notification-clear-all</v-icon>
            <v-tooltip activator="parent">{{ tc('logs.clearHint') }}</v-tooltip>
          </v-btn>

          <!-- Whatever the frame needs to add — a dialog puts its dismiss here. -->
          <slot name="actions" />
        </v-toolbar>

        <v-divider />

        <div ref="viewport" class="log-view">
          <ErrorAlert :error="error" type="error" />

          <!-- The fanout is live-only, so an empty pane is the normal opening
               state and not a fault: nothing has been written since it opened.
               Saying so is the difference between "waiting" and "broken". -->
          <div
            v-if="!error && !lines.length"
            class="text-medium-emphasis text-caption pa-4 text-center"
          >
            {{ fanout ? tc('logs.waitingAll') : tc('logs.waiting') }}
          </div>

          <!-- Distinguished from an empty log: one means nothing has been
               written, the other means a filter is hiding what was. -->
          <div
            v-else-if="!visible.length"
            class="text-medium-emphasis text-caption pa-4 text-center"
          >
            {{ tc('logs.noMatch', { n: lines.length }) }}
          </div>

          <!-- Container paths in a stack frame become clickable: the bind
               mount states the substitution (/var/www/html ↔ projects/<name>),
               so a frame is one click from the editor, not a search. -->
          <template v-for="(line, i) in visible" :key="i">
            <!-- The live boundary. Everything above it was already in the file
               when the tail started, grouped by file rather than by time —
               which is the only claim this code can honestly make about it. -->
            <div v-if="i === liveFrom" class="log-boundary">
              <span>{{ tc('logs.liveFrom') }}</span>
            </div>
            <pre
              class="log-line"
              :class="[
                { 'log-stderr': line.stream === 'stderr', 'log-historic': line.historic },
                line.level ? `level-${line.level}` : null,
              ]"
            ><span
              v-if="line.project"
              class="log-origin"
              :title="line.origin"
              >{{ line.project }}</span
            ><template v-for="(seg, j) in segments(line)"><span
                v-if="seg.file"
                :key="j"
                class="log-jump"
                role="link"
                :title="tc('logs.openInEditor')"
                @click="jump(line, seg.file)"
                >{{ seg.text }}</span
              ><!-- Marking the hit is the difference between "this line
                    matched" and "this is why it matched" — on a 200-character
                    request line, the second one is the answer. -->
              <template v-else><template
                  v-for="(part, k) in highlight(seg.text, query, useRegex)"
                  :key="k"
                ><mark v-if="part.hit" class="log-hit">{{ part.text }}</mark><template
                    v-else
                    >{{ part.text }}</template
                  ></template></template></template></pre>
          </template>
        </div>

        <template v-if="filtering && lines.length">
          <v-divider />
          <div class="log-foot text-caption text-medium-emphasis">
            {{ tc('logs.showing', { shown: visible.length, total: lines.length }) }}
          </div>
        </template>
      </div>
    </v-locale-provider>
  </v-theme-provider>
</template>

<style scoped>
/* Fills whatever it is put in — a dialog card or a page section — rather than
   carrying a height of its own. */
.log-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.log-head {
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

.log-head :deep(.v-toolbar__content) {
  gap: 8px;
  padding-inline: 16px 8px;
}

.log-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Bounded so a long channel path cannot push the controls off the bar. */
.log-source {
  max-width: 260px;
  min-width: 140px;
  flex: 0 1 auto;
}

.log-hit {
  background: rgba(var(--v-theme-warning), 0.35);
  color: inherit;
  border-radius: 2px;
  padding: 0 1px;
}

.log-historic {
  opacity: 0.6;
}

/* A labelled rule, not a line of text: it separates, it does not report. */
.log-boundary {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 6px 0 4px;
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  opacity: 0.55;
}

.log-boundary::before,
.log-boundary::after {
  content: '';
  flex: 1;
  height: 1px;
  background: currentColor;
  opacity: 0.35;
}

.log-search {
  max-width: 420px;
  min-width: 200px;
  flex: 1 1 320px;
}

.log-view {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 8px 16px;
  background: rgb(var(--v-theme-surface-bright));
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

.log-line {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

/* Which project a line came from, on the fanout only. A fixed width so the
   text starts on one column — a ragged left edge is what makes an interleaved
   buffer unreadable, and the whole value of this view is that you can scan it.
   Truncated rather than wrapped; the full origin is in the title. */
.log-origin {
  display: inline-block;
  width: 11ch;
  margin-right: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
  opacity: 0.6;
  user-select: none;
}

/* A stack-frame path, one click from the editor. Underlined only on hover so
   a dense trace stays readable. */
.log-jump {
  cursor: pointer;
  text-decoration-line: underline;
  text-decoration-style: dotted;
  text-underline-offset: 2px;
}

.log-jump:hover {
  text-decoration-style: solid;
  color: rgb(var(--v-theme-primary));
}

.log-foot {
  padding: 6px 16px;
  background: rgb(var(--v-theme-surface));
}

/* Severity as colour on the text itself. A left border or a filled row would
   be a block of colour per stack frame, which is most of an error log. */
.level-critical {
  color: rgb(var(--v-theme-error));
  font-weight: 600;
}

.level-error {
  color: rgb(var(--v-theme-error));
}

.level-warning {
  color: rgb(var(--v-theme-warning));
}

.level-notice,
.level-info,
.level-debug {
  /* Left as the body colour: the common levels are the background against
     which the rare ones have to stand out. */
  color: inherit;
}

.level-debug {
  opacity: 0.7;
}

.log-stderr {
  color: rgb(var(--v-theme-error));
}
</style>
