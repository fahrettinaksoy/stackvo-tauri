<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { usePhpIni, PHP_INI_FIELDS } from '@/composables/usePhpIni';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The four php.ini directives a project can override.
 *
 * The other half of the Runtime section, and the mirror of `DevServerPane`:
 * this one is empty unless the runtime is `php`.
 */
const props = defineProps({
  name: { type: String, required: true },
  runtime: { type: String, default: '' },
});

const { t } = useI18n();

const { status, draft, busy, error, dirty, wouldRemoveFile, load, save, resetDraft } = usePhpIni(
  toRef(props, 'name')
);

watch(
  () => [props.name, props.runtime],
  () => load(props.runtime),
  { immediate: true }
);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-language-php</v-icon>{{ t('phpIni.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('phpIni.explain') }}</p>

    <template v-if="status">
      <v-row dense>
        <v-col v-for="key in PHP_INI_FIELDS" :key="key" cols="12" sm="6">
          <!-- The placeholder is what PHP in the container reports right
               now, not a documented default. Measured, because assuming
               was wrong: these images ship no php.ini at all, and
               max_execution_time is 0 under FPM rather than the 30 the
               manual lists. Falls back to the field name when nothing is
               running — inventing a number would be worse. -->
          <v-text-field
            v-model="draft[key]"
            :label="t(`phpIni.field.${key}`)"
            :placeholder="status.effective?.[key] || t('phpIni.notMeasured')"
            :hint="t(`phpIni.hint.${key}`)"
            persistent-placeholder
            persistent-hint
            density="comfortable"
            variant="outlined"
            :disabled="busy"
          />
        </v-col>
      </v-row>

      <!-- Legal, and almost always a mistake: the upload fails at the
           smaller of the two, which is a number the user can see they
           have already raised. -->
      <v-alert v-if="status.warning" type="warning" variant="tonal" class="mt-3">
        <div class="text-caption">{{ status.warning }}</div>
      </v-alert>

      <div class="d-flex align-center ga-2 mt-4">
        <v-btn
          color="primary"
          variant="flat"
          size="small"
          :loading="busy"
          :disabled="!dirty"
          @click="save"
        >
          {{ wouldRemoveFile && dirty ? t('phpIni.removeFile') : t('phpIni.save') }}
        </v-btn>
        <v-btn variant="text" size="small" :disabled="!dirty || busy" @click="resetDraft">
          {{ t('app.cancel') }}
        </v-btn>
        <span class="text-caption text-medium-emphasis">{{ t('phpIni.emptyRemoves') }}</span>
      </div>

      <div v-if="status.effective" class="text-caption text-medium-emphasis mt-2">
        {{ t('phpIni.measured') }}
      </div>

      <!-- What is true after a save, which is not the same as saved. PHP
           reads its ini at process start, so a bind-mounted edit is on
           disk and not yet in the process. -->
      <v-alert v-if="status.needsRecreate" type="warning" variant="tonal" class="mt-4">
        <div class="text-caption">{{ t('phpIni.needsRecreate') }}</div>
      </v-alert>
      <v-alert v-else-if="status.exists && status.running" type="info" variant="tonal" class="mt-4">
        <div class="text-caption">{{ t('phpIni.needsRestart') }}</div>
      </v-alert>

      <!-- Directives the form does not manage, shown because they are
           preserved on every write and a form that hid them would look
           like it had lost them. -->
      <template v-if="Object.keys(status.unmanaged).length">
        <div class="section-head mt-5 mb-2">
          <v-icon size="18" class="mr-2">mdi-file-document-edit-outline</v-icon>
          {{ t('phpIni.unmanaged') }}
        </div>
        <v-table density="compact">
          <tbody>
            <tr v-for="(value, key) in status.unmanaged" :key="key">
              <td class="text-medium-emphasis mono">{{ key }}</td>
              <td class="mono">{{ value }}</td>
            </tr>
          </tbody>
        </v-table>
      </template>

      <v-table density="compact" class="mt-5">
        <tbody>
          <tr>
            <td class="text-medium-emphasis">{{ t('phpIni.file') }}</td>
            <td class="mono">{{ status.path }}</td>
          </tr>
          <tr>
            <td class="text-medium-emphasis">{{ t('phpIni.mountedAt') }}</td>
            <td class="mono">{{ status.containerPath }}</td>
          </tr>
        </tbody>
      </v-table>

      <div class="text-caption text-medium-emphasis mt-3">{{ t('phpIni.cliCaveat') }}</div>
    </template>
  </v-card>
</template>
