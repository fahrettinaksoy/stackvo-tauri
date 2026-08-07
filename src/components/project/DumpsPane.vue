<script setup>
import { useI18n } from 'vue-i18n';
import { useOperationsStore } from '@/stores/operations';
import DumpView from '@/components/DumpView.vue';

/**
 * What `dd()` and `dump()` wrote, caught before they reached the response.
 *
 * The recreate button belongs to the project page — it is the same operation
 * the profiler warning offers, and this is where the project's lifecycle
 * controls live — so it is passed up as `apply` rather than run here.
 */
defineProps({
  name: { type: String, required: true },
});

const emit = defineEmits(['apply']);

const { t } = useI18n();
const ops = useOperationsStore();
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-bug-check-outline</v-icon>{{ t('dumps.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('dumps.explain') }}</p>

    <DumpView :project="name" scope="project">
      <!-- The recreate button belongs to the project page: it is the
           same operation the profiler warning offers, and this is
           where the project's lifecycle controls live. -->
      <template #recreate>
        <v-btn
          size="small"
          color="warning"
          variant="tonal"
          class="mt-2"
          prepend-icon="mdi-autorenew"
          :loading="ops.isBusy(name)"
          @click="emit('apply')"
        >
          {{ t('projectDetail.applyToContainer') }}
        </v-btn>
      </template>
    </DumpView>
  </v-card>
</template>
