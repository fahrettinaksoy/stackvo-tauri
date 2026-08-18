<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

const props = defineProps({
  /** Domains to add. */
  add: { type: Array, default: () => [] },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue', 'applied']);

const { t } = useI18n();

const plan = ref(null);
const error = ref(null);
const applying = ref(false);

/**
 * A line-level diff of the previewed file against the current one.
 *
 * Showing the whole file would bury the change; showing only "we'll add
 * x.loc" would hide the fact that a system file is being rewritten. The diff
 * is the honest middle — and it is computed from the same `preview` string
 * that actually gets written, so there is nothing to drift.
 */
const diff = computed(() => {
  if (!plan.value) return [];
  const before = new Set(plan.value.current.split('\n'));
  return plan.value.preview
    .split('\n')
    .map((line) => ({ line, added: line.trim() !== '' && !before.has(line) }));
});

async function load() {
  error.value = null;
  plan.value = null;
  try {
    plan.value = await api.hostsPlan(props.add, []);
  } catch (e) {
    error.value = e;
  }
}

async function apply() {
  applying.value = true;
  error.value = null;
  try {
    await api.hostsApply(props.add, []);
    emit('applied');
    emit('update:modelValue', false);
  } catch (e) {
    error.value = e;
  } finally {
    applying.value = false;
  }
}

/**
 * Load the plan whenever this dialog is open — including the moment it mounts.
 *
 * `immediate` is the whole of one bug. Two of the five callers render this
 * behind a `v-if`, so the component is *created* with `modelValue` already
 * true and the watcher had nothing to fire on: no plan, no path in the
 * subtitle, no diff, and an Apply button disabled by `!plan?.changed` with
 * nothing on screen explaining why. The three callers that keep it mounted and
 * flip the flag were fine, which is why it survived.
 *
 * The domains are watched too, and joined rather than compared by identity:
 * `:add="[hostsFixFor]"` builds a fresh array on every render of the parent, so
 * a reference watch would reload the plan continuously. What matters is which
 * names are in it — a dialog left open while a second build finishes elsewhere
 * would otherwise show the first project's diff under the second one's name.
 */
watch([() => props.modelValue, () => props.add.join('\n')], ([open]) => open && load(), {
  immediate: true,
});
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    max-width="720"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-item>
        <template #prepend><v-icon color="warning">mdi-shield-key-outline</v-icon></template>
        <v-card-title class="text-body-1">{{ t('hosts.title') }}</v-card-title>
        <v-card-subtitle>{{ plan?.path }}</v-card-subtitle>
      </v-card-item>

      <v-card-text>
        <p class="text-body-2 mb-3">{{ t('hosts.explain') }}</p>

        <ErrorAlert :error="error" type="error" />

        <v-alert v-if="!error && plan && !plan.changed" type="info" variant="tonal" class="mb-3">
          {{ t('hosts.noChange') }}
        </v-alert>

        <div v-if="plan" class="diff">
          <div
            v-for="(row, i) in diff"
            :key="i"
            class="diff-line"
            :class="{ 'diff-added': row.added }"
          >
            <span class="diff-marker">{{ row.added ? '+' : ' ' }}</span
            >{{ row.line }}
          </div>
        </div>

        <p class="text-caption text-medium-emphasis mt-3">{{ t('hosts.elevation') }}</p>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">{{
          t('hosts.cancel')
        }}</v-btn>
        <v-btn
          color="primary"
          variant="flat"
          :loading="applying"
          :disabled="!plan?.changed"
          @click="apply"
        >
          {{ t('hosts.apply') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.diff {
  max-height: 40vh;
  overflow: auto;
  padding: 8px;
  border-radius: var(--app-radius);
  background: rgb(var(--v-theme-surface-bright));
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.6;
}

.diff-line {
  white-space: pre-wrap;
  word-break: break-all;
}

.diff-marker {
  display: inline-block;
  width: 1.2em;
  opacity: 0.5;
}

.diff-added {
  color: rgb(var(--v-theme-success));
  font-weight: 600;
}
</style>
