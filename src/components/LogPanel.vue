<script setup>
import { nextTick, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

const props = defineProps({
  /** Container name or bare id; the Rust side adds the `stackvo-` prefix. */
  container: { type: String, required: true },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);

const { t } = useI18n();

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

// Open on show, tear down on hide — a background tail is wasted work and keeps
// a reader task alive on the Rust side.
watch(
  () => props.modelValue,
  (open) => (open ? openStream() : close())
);

onUnmounted(close);
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    max-width="1000"
    scrollable
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-item>
        <v-card-title class="text-body-1 d-flex align-center ga-2">
          <v-icon size="20">mdi-text-box-outline</v-icon>
          {{ container }}
          <v-chip v-if="streamId" size="x-small" color="success">{{ t('logs.live') }}</v-chip>
        </v-card-title>
        <template #append>
          <v-btn
            :icon="follow ? 'mdi-arrow-down-bold-box' : 'mdi-arrow-down-bold-box-outline'"
            variant="text"
            size="small"
            :aria-label="t('a11y.followOutput')"
            :aria-pressed="follow"
            @click="follow = !follow"
          />
          <v-btn
            icon="mdi-close"
            variant="text"
            size="small"
            :aria-label="t('a11y.close')"
            @click="emit('update:modelValue', false)"
          />
        </template>
      </v-card-item>

      <v-divider />

      <v-card-text ref="viewport" class="log-view">
        <ErrorAlert :error="error" type="error" />

        <div
          v-if="!error && !lines.length"
          class="text-medium-emphasis text-caption pa-4 text-center"
        >
          {{ t('logs.waiting') }}
        </div>

        <pre
          v-for="(line, i) in lines"
          :key="i"
          class="log-line"
          :class="{ 'log-stderr': line.stream === 'stderr' }"
          >{{ line.text }}</pre>
      </v-card-text>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.log-view {
  height: 60vh;
  overflow-y: auto;
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
