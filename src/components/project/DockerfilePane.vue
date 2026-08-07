<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useCopyTick } from '@/composables/useCopyTick';
import { useDockerfilePreview } from '@/composables/useDockerfilePreview';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The Dockerfile this project would be built from.
 *
 * Rendered as soon as the pane is mounted. It used to start empty with a note
 * asking the user to pick one of two modes named "strict" and "compat" — a
 * question about the generator port, put before anyone had seen the file.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();

const { preview, mode, loading, error, lines, load } = useDockerfilePreview(toRef(props, 'name'));
const { copied, copy } = useCopyTick();

watch(
  () => props.name,
  () => load(),
  { immediate: true }
);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-file-document-outline</v-icon>
      {{ t('detail.dockerfile') }}
    </div>
    <div class="text-caption text-medium-emphasis mb-3">
      {{ t('detail.dockerfileDesc') }}
    </div>

    <div class="d-flex align-center ga-3 flex-wrap mb-2">
      <v-btn-toggle
        :model-value="mode"
        mandatory
        divided
        color="primary"
        variant="flat"
        class="bg-surface-light"
        @update:model-value="load"
      >
        <v-btn value="compat" size="small">{{ t('detail.compat') }}</v-btn>
        <v-btn value="strict" size="small">{{ t('detail.strict') }}</v-btn>
      </v-btn-toggle>

      <!-- What the chip means depends on the mode above it, so they sit
           together rather than at opposite ends of a bar. -->
      <v-chip
        v-if="preview"
        size="small"
        :color="preview.matchesBashOutput ? 'success' : 'warning'"
        :prepend-icon="preview.matchesBashOutput ? 'mdi-check-circle' : 'mdi-alert'"
      >
        {{ preview.matchesBashOutput ? t('detail.matchesBash') : t('detail.differsFromBash') }}
      </v-chip>

      <v-spacer />

      <v-btn
        v-if="preview"
        icon
        size="small"
        variant="text"
        :aria-label="t('a11y.copy')"
        @click="copy(preview.dockerfile, 'dockerfile')"
      >
        <v-icon>{{ copied === 'dockerfile' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
        <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
      </v-btn>
      <v-btn
        icon
        size="small"
        variant="text"
        :loading="loading"
        :aria-label="t('app.refresh')"
        @click="load()"
      >
        <v-icon>mdi-refresh</v-icon>
        <v-tooltip activator="parent">{{ t('app.refresh') }}</v-tooltip>
      </v-btn>
    </div>

    <div class="text-caption text-medium-emphasis mb-3">
      {{ mode === 'strict' ? t('detail.strictHint') : t('detail.compatHint') }}
    </div>

    <!-- Bash drops an unbuildable extension without a word; strict mode
         exists so the reason is visible instead. -->
    <v-alert v-if="preview?.skipped?.length" type="warning" variant="tonal" class="mb-3">
      <div class="text-caption font-weight-medium mb-1">
        {{ t('detail.silentlySkipped') }}
      </div>
      <div v-for="s in preview.skipped" :key="s.extension" class="text-caption">
        <strong>{{ s.extension }}</strong> — {{ s.reason }}
      </div>
    </v-alert>

    <div v-if="preview" class="dockerfile">
      <div v-for="(line, i) in lines" :key="i" class="df-line">
        <span class="df-no">{{ i + 1 }}</span>
        <code class="df-code">{{ line }}</code>
      </div>
    </div>
    <div v-else-if="loading" class="d-flex justify-center py-8">
      <v-progress-circular indeterminate color="primary" />
    </div>
  </v-card>
</template>
