<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useCopyTick } from '@/composables/useCopyTick';
import { useDockerfilePreview } from '@/composables/useDockerfilePreview';
import ErrorAlert from '@/components/ErrorAlert.vue';
import CollapsiblePane from '@/components/project/CollapsiblePane.vue';

/**
 * The Dockerfile this project would be built from.
 *
 * Rendered as soon as the pane is mounted, even though the pane arrives folded
 * shut: the chip that says whether the file on disk is still current sits
 * outside the fold, so the answer has to exist before anyone opens anything. It
 * used to start empty with a note asking the user to pick one of two modes
 * named "strict" and "compat" — a question about a generator port, put before
 * anyone had seen the file.
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

  <!-- Folded away by default. The generator emits around 120 lines for a PHP
       project with extensions, and printing all of them made this pane taller
       than the rest of the tab put together. -->
  <CollapsiblePane>
    <template #title>
      <span class="section-head">
        <v-icon size="18" class="mr-2">mdi-file-document-outline</v-icon>
        {{ t('detail.dockerfile') }}
      </span>
    </template>

    <!-- The verdict stays out of the fold. Whether the file the build would
         actually use is still the file this manifest describes is the one thing
         here worth knowing without reading it, and a closed pane that hides its
         own warning is a warning nobody sees.

         Only in compat mode. A strict render differs from what was written by
         design — the modes drop different extensions — so asking whether it
         matches disk in strict mode produces a warning that means nothing and
         cannot be cleared. It said "differs from the Bash output" while doing
         it, which was untrue twice over: there is no Bash generator any more,
         and both sides of the comparison come from this one. -->
    <template #meta>
      <v-chip
        v-if="preview && mode !== 'strict'"
        size="small"
        :color="preview.matchesGenerated ? 'success' : 'warning'"
        :prepend-icon="preview.matchesGenerated ? 'mdi-check-circle' : 'mdi-alert'"
      >
        {{ preview.matchesGenerated ? t('detail.matchesGenerated') : t('detail.generatedStale') }}
      </v-chip>
    </template>

    <template #actions>
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
    </template>

    <div class="text-caption text-medium-emphasis mb-3">
      {{ t('detail.dockerfileDesc') }}
    </div>

    <v-btn-toggle
      :model-value="mode"
      mandatory
      divided
      color="primary"
      variant="flat"
      class="bg-surface-light mb-2"
      @update:model-value="load"
    >
      <v-btn value="compat" size="small">{{ t('detail.compat') }}</v-btn>
      <v-btn value="strict" size="small">{{ t('detail.strict') }}</v-btn>
    </v-btn-toggle>

    <div class="text-caption text-medium-emphasis mb-3">
      {{ mode === 'strict' ? t('detail.strictHint') : t('detail.compatHint') }}
    </div>

    <!-- A compat render drops an unbuildable extension without a word, and
         that is the render the real file is written by; strict mode exists so
         the reason is visible instead. -->
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
  </CollapsiblePane>
</template>
