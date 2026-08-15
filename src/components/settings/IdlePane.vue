<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Stopping projects nothing has asked for (I-2).
 *
 * ## The idle times are shown whether or not the feature is on
 *
 * "How long is my stack actually idle for" is the question somebody has to
 * answer before they can pick a threshold, and a screen that only showed the
 * numbers *after* the feature was switched on would be asking them to guess
 * first and find out afterwards. So the table is always there and the setting
 * sits above it.
 *
 * ## Suspending is a button as well as a setting
 *
 * The setting is what a sweep uses; the button is what somebody presses when
 * they want the memory back now. Both go through the same command, so there is
 * one answer to "what counts as idle" rather than two that drift.
 */
const { t } = useI18n();

// No `save` emit: `edit` registers the change with the shared editor and the
// view's own save button writes the whole diff — a pane that also emitted would
// be a second path to one write.
const { effective, edit } = useSharedEnvEditor();
const rows = ref([]);
const error = ref(null);
const busy = ref(false);

const KEY = 'IDLE_SUSPEND_MINUTES';

const suspendable = computed(() => rows.value.filter((row) => row.suspendable));

/** Minutes, rounded down — "1847 seconds" is not a unit anybody thinks in. */
function since(row) {
  if (row.seconds == null) return t('idle.never');
  const mins = Math.floor(row.seconds / 60);
  return mins < 1 ? t('idle.justNow') : t('idle.minutes', { minutes: mins });
}

async function load() {
  error.value = null;
  try {
    rows.value = asList(await api.projectsIdle());
  } catch (e) {
    error.value = e;
  }
}

async function suspend() {
  busy.value = true;
  error.value = null;
  try {
    const stopped = asList(await api.projectsSuspendIdle());
    // Named, not counted: this is a background-shaped action and its whole
    // risk is being surprising.
    rows.value = rows.value.filter((row) => !stopped.includes(row.project));
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <SettingsGroup icon="mdi-sleep" :title="t('idle.title')" :subtitle="t('idle.subtitle')">
    <ErrorAlert v-if="error" :error="error" class="mb-3" />
    <p class="text-caption text-medium-emphasis mb-4">{{ t('idle.explain') }}</p>

    <div class="d-flex align-center ga-3 mb-4">
      <v-text-field
        :model-value="effective(KEY)"
        type="number"
        min="0"
        :label="t('idle.threshold')"
        :hint="t('idle.thresholdHint')"
        persistent-hint
        density="compact"
        variant="outlined"
        style="max-width: 220px"
        @update:model-value="(v) => edit(KEY, String(v ?? '0'))"
      />
      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-sleep"
        :disabled="!suspendable.length || busy"
        :loading="busy"
        @click="suspend"
      >
        {{ t('idle.suspendNow', { count: suspendable.length }) }}
      </v-btn>
      <v-btn size="small" variant="text" prepend-icon="mdi-refresh" @click="load">
        {{ t('app.refresh') }}
      </v-btn>
    </div>

    <div v-if="!rows.length" class="text-caption text-medium-emphasis">{{ t('idle.none') }}</div>

    <div v-for="row in rows" :key="row.project" class="idle-row">
      <span class="idle-name">{{ row.project }}</span>
      <span class="idle-since text-medium-emphasis">{{ since(row) }}</span>
      <v-chip v-if="row.suspendable" size="x-small" color="warning" label>
        {{ t('idle.wouldStop') }}
      </v-chip>
    </div>
  </SettingsGroup>
</template>

<style scoped>
.idle-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 0;
  font-size: 0.8rem;
}

/* The name takes the room and the time is pushed to the far edge, so a column
   of projects reads down the left rather than wandering with the name length. */
.idle-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.idle-since {
  margin-left: auto;
  font-size: 0.75rem;
}
</style>
