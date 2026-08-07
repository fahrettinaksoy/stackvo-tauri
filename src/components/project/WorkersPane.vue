<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useWorkers } from '@/composables/useWorkers';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Queue and schedule sidecars for this project.
 *
 * Which kinds exist is a fact about the project's files; which are running is a
 * fact about the engine. Both are read here, `running` comes from the view.
 */
const props = defineProps({
  name: { type: String, required: true },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();

const { kinds, busy, error, load, workerFor, toggle } = useWorkers(toRef(props, 'name'));

watch(() => props.name, load, { immediate: true });
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-cog-sync-outline</v-icon>{{ t('workers.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('workers.explain') }}</p>

    <v-alert v-if="!kinds.length" type="info" variant="tonal">
      <div class="text-caption">{{ t('workers.none') }}</div>
    </v-alert>

    <v-alert v-else-if="!running" type="info" variant="tonal" class="mb-3">
      <div class="text-caption">{{ t('workers.needsRunning') }}</div>
    </v-alert>

    <div v-for="kind in kinds" :key="kind" class="worker-row">
      <v-icon :color="workerFor(kind)?.running ? 'success' : 'grey'" size="18">
        {{ workerFor(kind)?.running ? 'mdi-check-circle' : 'mdi-stop-circle-outline' }}
      </v-icon>
      <div class="min-width-0">
        <span class="text-body-2 font-weight-medium">{{ t(`workers.${kind}`) }}</span>
        <div class="text-caption text-medium-emphasis">
          {{ t(`workers.${kind}Desc`) }}
        </div>
        <!-- The healing made visible: 0 is healthy, a big number is a
             crash loop wearing a green chip. -->
        <div
          v-if="workerFor(kind)?.restarts"
          class="text-caption"
          :class="workerFor(kind).restarts > 3 ? 'text-error' : 'text-warning'"
        >
          {{ t('workers.restarts', { count: workerFor(kind).restarts }) }}
        </div>
      </div>
      <v-spacer />
      <v-btn
        size="small"
        :color="workerFor(kind) ? 'error' : 'primary'"
        variant="tonal"
        :loading="busy === kind"
        :disabled="!workerFor(kind) && !running"
        @click="toggle(kind)"
      >
        {{ workerFor(kind) ? t('workers.stop') : t('workers.start') }}
      </v-btn>
    </div>
  </v-card>
</template>
