<script setup>
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppearanceStore } from '@/stores/appearance';
import { useTerminal } from '@/composables/useTerminal';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * A shell inside this project's container, in the window.
 *
 * The app has offered "open a terminal" since the port, and it has always meant
 * the *system* terminal — `terminal_open_external`, which launches Terminal.app
 * or its equivalent. That still exists and is still the right answer when
 * somebody wants their own shell with their own profile. This is the other
 * half: a session that lives beside the project it belongs to, which is what
 * `pty.rs` was written for and what nothing had ever called.
 *
 * ## Loaded on demand
 *
 * xterm and its fit addon are imported inside `start()` rather than at the top
 * of the file. They are ~250 KB that only a user who asks for a shell needs,
 * and a static import would put them in the bundle every project page pays
 * for — including the ones with no container to attach to.
 *
 * ## Not opened automatically
 *
 * Mounting a pane must not spawn a process. Every other pane on this page reads
 * something; this one would start a shell in a container merely because the
 * page scrolled far enough to render it, and would keep doing so on every
 * visit.
 */
const props = defineProps({
  containerName: { type: String, default: null },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();
const appearance = useAppearanceStore();

const host = ref(null);
const term = shallowRef(null);
const fit = shallowRef(null);
let observer = null;

const { status, error, exitCode, open, write, resize, close } = useTerminal(
  (data) => term.value?.write(data),
  (code) => term.value?.writeln(`\r\n\x1b[90m${t('terminal.exited', { code })}\x1b[0m`)
);

async function start() {
  if (!props.containerName || term.value) return;

  const [{ Terminal }, { FitAddon }] = await Promise.all([
    import('@xterm/xterm'),
    import('@xterm/addon-fit'),
  ]);
  await import('@xterm/xterm/css/xterm.css');

  const instance = new Terminal({
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: 13,
    cursorBlink: true,
    // Consoles keep their own dark theme when the setting says so, the same
    // rule `OperationConsole` follows — a terminal on a white sheet is a
    // different product.
    theme: appearance.value.darkConsoles ? { background: '#12121a' } : undefined,
    // A shell that scrolls away its own output is worse than one that keeps
    // too much; this is bounded but generous.
    scrollback: 5000,
  });
  const fitAddon = new FitAddon();
  instance.loadAddon(fitAddon);
  instance.open(host.value);
  fitAddon.fit();

  term.value = instance;
  fit.value = fitAddon;

  // Keystrokes go straight through. The shell, not this pane, decides what a
  // character means — echo, line editing and control sequences are all its.
  instance.onData((data) => write(data));

  // The PTY has to be told the size or every full-screen program in it draws
  // to the wrong width: `top`, `vim` and a wrapped prompt all read it.
  observer = new ResizeObserver(() => {
    fitAddon.fit();
    resize(instance.cols, instance.rows);
  });
  observer.observe(host.value);

  await open({ kind: 'container', name: props.containerName }, instance.cols, instance.rows);
}

async function stop() {
  await close();
  observer?.disconnect();
  observer = null;
  term.value?.dispose();
  term.value = null;
  fit.value = null;
}

// A stopped container has no shell to attach to, and the session it had is
// already gone with it — leaving the pane looking live would be a lie.
watch(
  () => props.running,
  (isRunning) => {
    if (!isRunning && term.value) stop();
  }
);

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
  term.value?.dispose();
  term.value = null;
});
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-console</v-icon>{{ t('terminal.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('terminal.explain') }}</p>

    <v-alert v-if="!running" type="info" variant="tonal" class="mb-0">
      {{ t('terminal.needsRunning') }}
    </v-alert>

    <template v-else>
      <div class="d-flex align-center ga-2 mb-3">
        <v-btn
          v-if="!term"
          size="small"
          color="primary"
          variant="tonal"
          :loading="status === 'opening'"
          @click="start"
        >
          {{ t('terminal.start') }}
        </v-btn>
        <v-btn v-else size="small" color="error" variant="tonal" @click="stop">
          {{ t('terminal.stop') }}
        </v-btn>

        <span v-if="exitCode !== null" class="text-caption text-medium-emphasis">
          {{ t('terminal.exited', { code: exitCode }) }}
        </span>
      </div>

      <!-- `tabindex` and the label are what make this reachable without a
           mouse: xterm renders into a canvas, so a screen reader has nothing
           to announce and the keyboard has nothing to land on unless the host
           element provides both. -->
      <div
        v-show="term"
        ref="host"
        class="terminal-host"
        role="application"
        tabindex="0"
        :aria-label="t('terminal.title')"
      ></div>
    </template>
  </v-card>
</template>

<style scoped>
.terminal-host {
  height: 340px;
  border-radius: var(--app-radius, 8px);
  overflow: hidden;
  background: #12121a;
  padding: 8px;
}
</style>
