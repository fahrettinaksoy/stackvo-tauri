<script setup>
import { onBeforeUnmount, ref, shallowRef, watch, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

const props = defineProps({
  /** { kind: 'container', name } or { kind: 'host', cwd } */
  target: { type: Object, required: true },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);

const { t } = useI18n();

const host = ref(null);
const error = ref(null);
const sessionId = ref(null);
// shallowRef: xterm instances are large and must not be made reactive.
const term = shallowRef(null);
const fit = shallowRef(null);

let unlistenOutput = null;
let unlistenClosed = null;
let resizeObserver = null;

async function start() {
  await stop();
  error.value = null;
  await nextTick();
  if (!host.value) return;

  const terminal = new Terminal({
    fontSize: 12,
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    cursorBlink: true,
    convertEol: true,
    theme: { background: '#0E1116', foreground: '#E6EDF3' },
  });
  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  terminal.open(host.value);
  fitAddon.fit();

  term.value = terminal;
  fit.value = fitAddon;

  try {
    // Subscribe before opening, or the shell's banner races the listener.
    unlistenOutput = await listen('terminal:output', (event) => {
      if (event.payload.sessionId === sessionId.value) terminal.write(event.payload.data);
    });
    unlistenClosed = await listen('terminal:closed', (event) => {
      if (event.payload.sessionId !== sessionId.value) return;
      terminal.writeln('\r\n\x1b[90m— session ended —\x1b[0m');
      sessionId.value = null;
    });

    sessionId.value = await api.ptyOpen(props.target, terminal.cols, terminal.rows);

    terminal.onData((data) => {
      if (sessionId.value) api.ptyWrite(sessionId.value, data).catch(() => {});
    });

    // Keep the PTY's idea of the window in sync, or full-screen programs
    // (top, vim) render into the wrong geometry.
    resizeObserver = new ResizeObserver(() => {
      try {
        fitAddon.fit();
        if (sessionId.value) api.ptyResize(sessionId.value, terminal.cols, terminal.rows);
      } catch {
        /* element detached mid-resize */
      }
    });
    resizeObserver.observe(host.value);

    terminal.focus();
  } catch (e) {
    error.value = e;
  }
}

async function stop() {
  if (sessionId.value) {
    await api.ptyClose(sessionId.value).catch(() => {});
    sessionId.value = null;
  }
  unlistenOutput?.();
  unlistenClosed?.();
  unlistenOutput = null;
  unlistenClosed = null;
  resizeObserver?.disconnect();
  resizeObserver = null;
  term.value?.dispose();
  term.value = null;
}

watch(
  () => props.modelValue,
  (open) => (open ? start() : stop())
);
onBeforeUnmount(stop);

const title = () =>
  props.target.kind === 'container' ? props.target.name : props.target.cwd || 'shell';
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    max-width="1000"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-item>
        <v-card-title class="text-body-1 d-flex align-center ga-2">
          <v-icon size="20">mdi-console</v-icon>
          {{ title() }}
          <v-chip v-if="sessionId" size="x-small" color="success">{{ t('logs.live') }}</v-chip>
        </v-card-title>
        <template #append>
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

      <v-card-text class="pa-0">
        <ErrorAlert :error="error" type="error" />
        <div v-show="!error" ref="host" class="terminal-host" />
      </v-card-text>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.terminal-host {
  height: 60vh;
  padding: 8px;
  background: #0e1116;
}
</style>
