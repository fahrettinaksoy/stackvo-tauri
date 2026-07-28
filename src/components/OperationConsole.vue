<script setup>
import { computed, nextTick, ref, watch } from 'vue';
import { useAppearanceStore } from '@/stores/appearance';
import { useI18n } from 'vue-i18n';
import { useOperationsStore } from '@/stores/operations';

/**
 * Console surfaces stay dark when the setting says so, whatever the app theme
 * is — a terminal on a white sheet is a different product. `v-theme-provider`
 * re-themes the subtree without touching the global choice.
 */
const appearance = useAppearanceStore();
const consoleTheme = computed(() => (appearance.value.darkConsoles ? 'dark' : undefined));

const { t } = useI18n();
const ops = useOperationsStore();

const expanded = ref(false);
const viewport = ref(null);

/**
 * The operation the user dismissed, by id.
 *
 * Kept here rather than in the store: the store's record is history — it backs
 * the per-subject busy flags and outlives the panel — while dismissing is about
 * this card. A new operation carries a new id, so the console comes back on its
 * own without anything having to reset this.
 */
const dismissed = ref(null);

const current = computed(() => {
  const op = ops.active[0] ?? ops.latest;
  return op && op.id !== dismissed.value ? op : null;
});

/**
 * Only a finished operation can be dismissed. While one is running the console
 * is the only place its output exists, and the chevron already collapses it.
 */
const finished = computed(() => !!current.value && current.value.state !== 'running');

function dismiss() {
  dismissed.value = current.value?.id ?? null;
}

const statusColor = computed(() => {
  if (!current.value) return 'surface-variant';
  return { running: 'info', done: 'success', failed: 'error' }[current.value.state];
});

// Open automatically when work starts, so a build's output is visible without
// the user having to know a console exists.
watch(
  () => ops.active.length,
  (n, was) => {
    if (n > 0 && was === 0) expanded.value = true;
  }
);

watch(
  () => current.value?.lines.length,
  async () => {
    if (!expanded.value) return;
    await nextTick();
    if (viewport.value) viewport.value.scrollTop = viewport.value.scrollHeight;
  }
);

function duration(op) {
  if (!op?.durationMs) return '';
  const s = op.durationMs / 1000;
  return s < 60 ? `${s.toFixed(1)}s` : `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}
</script>

<template>
  <v-theme-provider :theme="consoleTheme">
    <v-card v-if="current" class="operation-console" :border="`${statusColor} md`">
      <div class="d-flex align-center ga-2 px-3 py-2" @click="expanded = !expanded">
        <v-progress-circular
          v-if="current.state === 'running'"
          indeterminate
          size="16"
          width="2"
          :color="statusColor"
        />
        <v-icon v-else :color="statusColor" size="18">
          {{ current.state === 'failed' ? 'mdi-alert-circle' : 'mdi-check-circle' }}
        </v-icon>

        <span class="text-body-2 font-weight-medium">{{ current.kind }}</span>
        <span class="text-caption text-medium-emphasis">{{ current.subject }}</span>

        <v-spacer />

        <span v-if="current.durationMs" class="text-caption text-medium-emphasis">
          {{ duration(current) }}
        </span>
        <v-btn
          icon
          variant="text"
          size="x-small"
          :aria-label="t('a11y.toggleConsole')"
          :aria-expanded="expanded"
        >
          <v-icon>{{ expanded ? 'mdi-chevron-down' : 'mdi-chevron-up' }}</v-icon>
          <v-tooltip activator="parent">{{ t('a11y.toggleConsole') }}</v-tooltip>
        </v-btn>

        <!-- `.stop` because the header row itself toggles the panel: without it
             closing would expand on the way out. -->
        <v-btn
          v-if="finished"
          icon
          variant="text"
          size="x-small"
          :aria-label="t('a11y.close')"
          @click.stop="dismiss"
        >
          <v-icon>mdi-close</v-icon>
          <v-tooltip activator="parent">{{ t('a11y.close') }}</v-tooltip>
        </v-btn>
      </div>

      <v-expand-transition>
        <div v-show="expanded">
          <v-divider />
          <div ref="viewport" class="console-output">
            <pre v-for="(line, i) in current.lines" :key="i" class="console-line">{{ line }}</pre>
            <div v-if="!current.lines.length" class="text-caption text-medium-emphasis pa-2">
              {{ t('app.loading') }}
            </div>
          </div>

          <v-alert v-if="current.error" type="error" variant="tonal" class="ma-2">
            <pre class="console-line">{{ current.error }}</pre>
          </v-alert>
        </div>
      </v-expand-transition>
    </v-card>
  </v-theme-provider>
</template>

<style scoped>
.operation-console {
  position: fixed;
  right: 24px;
  bottom: 24px;
  width: min(560px, calc(100vw - 48px));
  z-index: 2000;
  cursor: pointer;
}

.console-output {
  max-height: 260px;
  overflow-y: auto;
  padding: 8px 12px;
  background: rgb(var(--v-theme-surface-bright));
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

.console-line {
  margin: 0;
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
