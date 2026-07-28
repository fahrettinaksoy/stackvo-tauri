<script setup>
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { useAppearanceStore } from '@/stores/appearance';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * A live container log, with no opinion about where it is shown.
 *
 * Logs used to be a dialog over whatever page you were on. They are content,
 * not an interruption — you read them while looking at the container's detail —
 * so the dialog was retired and this renders inside a page section or a side
 * sheet tab instead. It carries the stream, the follow behaviour and the
 * console theming; the frame around it belongs to whoever mounts it.
 */
const props = defineProps({
  /** Container name or bare id; the Rust side adds the `stackvo-` prefix. */
  container: { type: String, required: true },
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

let unlistenLine = null;
let unlistenClosed = null;

const MAX_LINES = 2000;

async function openStream() {
  close();
  lines.value = [];
  error.value = null;

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

    streamId.value = await api.containerLogsOpen(props.container, 300, true);
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

// Also on a container change: the detail page keeps this mounted and swaps the
// target when the project is rebuilt under a new name.
watch(
  () => [props.active, props.container],
  ([active]) => (active ? openStream() : close()),
  { immediate: true }
);

onUnmounted(close);
</script>

<template>
  <v-theme-provider :theme="consoleTheme">
    <v-locale-provider :locale="consoleLocale">
      <div class="log-root">
        <div class="log-head">
          <v-icon size="20">mdi-text-box-outline</v-icon>
          <span class="text-body-2 log-name">{{ container }}</span>
          <v-chip v-if="streamId" size="x-small" color="success">{{ tc('logs.live') }}</v-chip>
          <v-spacer />

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

          <pre
            v-for="(line, i) in lines"
            :key="i"
            class="log-line"
            :class="{ 'log-stderr': line.stream === 'stderr' }"
            >{{ line.text }}</pre>
        </div>
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

.log-stderr {
  color: rgb(var(--v-theme-error));
}
</style>
