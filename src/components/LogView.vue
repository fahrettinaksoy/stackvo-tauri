<script setup>
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { useAppearanceStore } from '@/stores/appearance';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { LEVELS, countByLevel, filterLines, withLevels } from '@/lib/logs';
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
 * Two sources, one renderer. The container stream is stdout and stderr, which
 * is what the entrypoint and the web server say; the files under `app_logs` are
 * what the application recorded, and nothing an application logs reaches the
 * container's stdout. Both arrive as `logs:line`, so switching source changes
 * which stream is open and nothing else.
 */
const props = defineProps({
  /** Container name or bare id; the Rust side adds the `stackvo-` prefix. */
  container: { type: String, required: true },
  /**
   * Project name, for the file sources. Omitted for a service, which has no
   * project directory and therefore only its container stream.
   */
  project: { type: String, default: '' },
  /**
   * Whether to hold the stream open. False tears it down: a background tail is
   * wasted work here and keeps a reader task alive on the Rust side.
   */
  active: { type: Boolean, default: true },
});

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

const query = ref('');
const levels = ref([]);

let unlistenLine = null;
let unlistenClosed = null;

const MAX_LINES = 2000;

const app = useAppStore();

/**
 * Container paths under the project's bind mount, with an extension — the
 * shape a stack frame prints. Restricting to `/var/www/html/` keeps this a
 * substitution the compose file states, never a guess.
 */
const CONTAINER_PATH = /\/var\/www\/html\/([A-Za-z0-9_@./-]+\.[A-Za-z0-9]+)/g;

/** Split one line around clickable file paths. Services have no source
 *  directory, so without a project the line comes back whole. */
function segments(text) {
  if (!props.project) return [{ text }];
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
async function jump(file) {
  const root = app.workspace?.root;
  if (!root) return;
  try {
    await api.openInEditor(`${root}/projects/${props.project}/${file}`);
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
  filterLines(tagged.value, { query: query.value, levels: levels.value })
);
const counts = computed(() => countByLevel(tagged.value));
const filtering = computed(() => !!query.value.trim() || levels.value.length > 0);

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

async function loadFiles() {
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

  try {
    // Listen before opening, or the first lines race the subscription.
    unlistenLine = await listen('logs:line', (event) => {
      if (event.payload.streamId !== streamId.value) return;
      lines.value.push({ text: event.payload.line, stream: event.payload.stream });
      if (lines.value.length > MAX_LINES) lines.value.splice(0, lines.value.length - MAX_LINES);
      if (follow.value) scrollToEnd();
    });

    unlistenClosed = await listen('logs:closed', (event) => {
      if (event.payload.streamId === streamId.value) streamId.value = null;
    });

    streamId.value = target
      ? await api.appLogOpen(props.project, target)
      : await api.containerLogsOpen(props.container, 300, true);
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
  unlistenLine = null;
  unlistenClosed = null;
}

async function scrollToEnd() {
  await nextTick();
  if (viewport.value) viewport.value.scrollTop = viewport.value.scrollHeight;
}

/** Copy what is on screen — the filtered lines, not the whole buffer. */
async function copyVisible() {
  try {
    await navigator.clipboard.writeText(visible.value.map((l) => l.text).join('\n'));
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

onUnmounted(close);
</script>

<template>
  <v-theme-provider :theme="consoleTheme">
    <v-locale-provider :locale="consoleLocale">
      <div class="log-root">
        <div class="log-head">
          <v-icon size="20">mdi-text-box-outline</v-icon>

          <!-- Only offered when there is something to choose between. A project
               with no log files gets the plain container name it always had. -->
          <v-select
            v-if="files.length"
            v-model="source"
            :items="sources"
            density="compact"
            variant="plain"
            hide-details
            class="log-source"
          />
          <span v-else class="text-body-2 log-name">{{ container }}</span>

          <v-chip v-if="streamId" size="x-small" color="success">{{ tc('logs.live') }}</v-chip>
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
          />

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
            :aria-label="tc('a11y.followOutput')"
            :aria-pressed="follow"
            @click="follow = !follow"
          >
            <v-icon>{{
              follow ? 'mdi-arrow-down-bold-box' : 'mdi-arrow-down-bold-box-outline'
            }}</v-icon>
            <v-tooltip activator="parent">{{ tc('a11y.followOutput') }}</v-tooltip>
          </v-btn>

          <!-- Whatever the frame needs to add — a dialog puts its dismiss here. -->
          <slot name="actions" />
        </div>

        <v-divider />

        <div ref="viewport" class="log-view">
          <ErrorAlert :error="error" type="error" />

          <div
            v-if="!error && !lines.length"
            class="text-medium-emphasis text-caption pa-4 text-center"
          >
            {{ tc('logs.waiting') }}
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
          <pre
            v-for="(line, i) in visible"
            :key="i"
            class="log-line"
            :class="[
              { 'log-stderr': line.stream === 'stderr' },
              line.level ? `level-${line.level}` : null,
            ]"
            ><template v-for="(seg, j) in segments(line.text)"><span
                v-if="seg.file"
                :key="j"
                class="log-jump"
                role="link"
                :title="tc('logs.openInEditor')"
                @click="jump(seg.file)"
                >{{ seg.text }}</span
              ><template v-else>{{ seg.text }}</template></template></pre>
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
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 8px 8px 16px;
  background: rgb(var(--v-theme-surface));
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

.log-search {
  max-width: 240px;
  flex: 0 1 auto;
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
