<script setup>
import { onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppStore } from '@/stores/app';
import { useTemplates } from '@/composables/useTemplates';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * The template-override pane, as a component rather than 102 lines of
 * `Settings.vue`.
 *
 * The second slice of the §14.16 split, and the second shape mirror retired.
 * `tests/template-overrides.spec.js` rebuilt the button and its two refs in the
 * test file and then read `Settings.vue` as text to check the copy still
 * matched — because the pane could not be mounted. `tests/settings-templates.spec.js`
 * mounts this instead.
 *
 * The bug that test existed for is worth keeping in view: the button shipped
 * *spinning*, because `templateBusy === templateToOverride` is true when both
 * are null, which is the state the pane opens in. `useTemplates.busyWith` is
 * where that now lives, with the check that fixes it.
 */
const { t } = useI18n();
const app = useAppStore();

const {
  templates,
  busy,
  error,
  chosen,
  revertTarget,
  overridden,
  shipped,
  busyWith,
  load,
  override,
  open,
  revert,
} = useTemplates();

onMounted(load);
</script>

<template>
  <!-- The templates are in the binary; a copy under `core/` exists
       only because somebody made one here. That is what makes this
       list possible at all — every workspace used to hold all thirty
       files, so "has a copy" said nothing. -->
  <SettingsGroup
    icon="mdi-file-replace-outline"
    :title="t('settings.templates.title')"
    :description="t('settings.templates.description')"
  >
    <template #append>
      <v-btn size="small" variant="text" prepend-icon="mdi-refresh" :loading="busy" @click="load">
        {{ t('settings.templates.reload') }}
      </v-btn>
    </template>

    <v-alert
      v-if="error"
      type="error"
      variant="tonal"
      density="comfortable"
      class="mb-3"
      :text="error.message || String(error)"
    />

    <div class="text-body-2 mb-3">
      {{
        overridden.length
          ? t('settings.templates.count', {
              count: overridden.length,
              total: templates.length,
            })
          : t('settings.templates.none', { total: templates.length })
      }}
    </div>

    <!-- Overridden ones first and always visible: they are the
         answer to "why does my stack not match the docs", and a
         forgotten edit is the reason that question gets asked. -->
    <v-list v-if="overridden.length" density="compact" class="pa-0 mb-2">
      <v-list-item v-for="file in overridden" :key="file.path" class="px-0" :title="file.path">
        <template #prepend>
          <v-icon size="18" color="warning" class="mr-2">mdi-pencil</v-icon>
        </template>
        <template #append>
          <v-btn
            size="x-small"
            variant="text"
            prepend-icon="mdi-open-in-new"
            @click="open(file.path, app.workspace?.root)"
          >
            {{ t('settings.templates.open') }}
          </v-btn>
          <v-btn
            size="x-small"
            variant="text"
            color="error"
            prepend-icon="mdi-undo-variant"
            :loading="busyWith(file.path)"
            @click="revertTarget = file.path"
          >
            {{ t('settings.templates.revert') }}
          </v-btn>
        </template>
      </v-list-item>
    </v-list>

    <v-select
      v-model="chosen"
      :items="shipped"
      item-title="path"
      item-value="path"
      :label="t('settings.templates.pick')"
      :hint="t('settings.templates.pickHint')"
      persistent-hint
      variant="outlined"
      density="comfortable"
      hide-no-data
    />
    <v-btn
      size="small"
      variant="tonal"
      color="primary"
      prepend-icon="mdi-file-edit-outline"
      class="mt-3"
      :disabled="!chosen"
      :loading="busyWith(chosen)"
      @click="override"
    >
      {{ t('settings.templates.override') }}
    </v-btn>
  </SettingsGroup>

  <!-- Reverting deletes the file the user edited. There is no copy of it
       anywhere — the binary holds the shipped version, not theirs. -->
  <v-dialog :model-value="!!revertTarget" max-width="520" @update:model-value="revertTarget = null">
    <v-card v-if="revertTarget">
      <v-card-item>
        <template #prepend><v-icon color="error">mdi-undo-variant</v-icon></template>
        <v-card-title class="text-body-1">{{ t('settings.templates.revertTitle') }}</v-card-title>
      </v-card-item>
      <v-card-text>
        <p class="text-body-2 mb-2">{{ t('settings.templates.revertBody') }}</p>
        <code class="text-caption break">{{ revertTarget }}</code>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="revertTarget = null">{{ t('hosts.cancel') }}</v-btn>
        <v-btn color="error" variant="flat" @click="revert">
          {{ t('settings.templates.revert') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
