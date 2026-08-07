<script setup>
import { useI18n } from 'vue-i18n';
import LogView from '@/components/LogView.vue';

/**
 * One project's logs.
 *
 * A thin wrapper over `LogView`, and the point of it is the `built` guard: the
 * container stream carries stdout and nothing an application logs goes there,
 * so the file sources are what make this pane useful — and they only exist once
 * the project has been built.
 */
defineProps({
  project: { type: Object, required: true },
  name: { type: String, required: true },
  active: { type: Boolean, default: false },
});

const { t } = useI18n();
</script>

<template>
  <LogView
    v-if="project.built"
    :container="project.containerName"
    :project="name"
    :active="active"
  />
  <div v-else class="text-caption text-medium-emphasis py-8 text-center">
    {{ t('detail.notBuilt') }}
  </div>
</template>
