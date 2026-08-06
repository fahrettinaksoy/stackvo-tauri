<script setup>
import { onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useServerConfig, CONFIGURABLE_SERVERS } from '@/composables/useServerConfig';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The extra-directives editor, one file per server.
 *
 * The third slice of the §14.16 split. Only this half of the Servers tab came
 * out: the limits form above it drives the shared `.env` editor that six panes
 * use, and pulling that apart is its own change (`useEnvEditor` in the review's
 * §2.3). This half owns its own file, its own dirty check and its own tab.
 */
const { t } = useI18n();

const { server, text, busy, error, dirty, load, save } = useServerConfig();

/**
 * Directives reach a container only through a regenerate, so the pane says so
 * rather than letting the user believe a save was enough. The parent owns that
 * notice because it is shared with the `.env` editor.
 */
const emit = defineEmits(['saved']);

async function onSave() {
  const keys = await save();
  if (keys.length) emit('saved', keys);
}

onMounted(load);
</script>

<template>
  <SettingsGroup
    icon="mdi-file-code-outline"
    :title="t('settings.servers.extra')"
    :description="t('settings.servers.extraDesc')"
  >
    <template #append>
      <v-btn
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-content-save-outline"
        :disabled="!dirty"
        :loading="busy"
        @click="onSave"
      >
        {{ t('settings.save', { count: 1 }) }}
      </v-btn>
    </template>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <v-tabs v-model="server" density="compact" bg-color="transparent" class="mb-3">
      <v-tab v-for="srv in CONFIGURABLE_SERVERS" :key="srv" :value="srv">{{ srv }}</v-tab>
    </v-tabs>

    <v-textarea
      v-model="text"
      :placeholder="t('settings.servers.extraPlaceholder')"
      :hint="t('settings.servers.extraHint')"
      persistent-hint
      rows="12"
      variant="outlined"
      density="comfortable"
      class="server-config"
      spellcheck="false"
    />
  </SettingsGroup>
</template>

<style scoped>
/* Config is read in columns — an alignment that a proportional face destroys. */
.server-config :deep(textarea) {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.8125rem;
  line-height: 1.6;
}
</style>
