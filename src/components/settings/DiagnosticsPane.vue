<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { bytes } from '@/lib/format';
import SettingsGroup from '@/components/SettingsGroup.vue';
import DoctorPanel from '@/components/DoctorPanel.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * What to send when something is wrong: the engine as this app sees it, the
 * doctor report, the log folder, and the archive that packages all of it.
 *
 * Twelfth and last pane out of `Settings.vue` under §14.16. With it gone the
 * view holds no pane markup at all — only the rail, the shared `.env` editor
 * and the About card.
 */
const { t } = useI18n();
const app = useAppStore();

const logs = ref(null);

// The diagnostic archive. `bundle` holds the last result so the pane can name
// what went in — a success toast that says "saved" leaves the user to open the
// zip to find out whether the thing they were asked for is in it.
const bundling = ref(false);
const bundle = ref(null);
const bundleError = ref(null);

/**
 * Collect the bundle to a path the user picks.
 *
 * The save dialog rather than a fixed location, for the reason
 * `mail_attachment_save` uses one: this writes a file outside everything the
 * app owns, and the only acceptable authority for that is the person at the
 * keyboard. A cancelled dialog is an answer, not a failure — it returns null
 * and nothing is reported.
 */
async function saveDiagnosticBundle() {
  bundleError.value = null;
  bundle.value = null;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({
      defaultPath: `stackvo-diagnostics.zip`,
      filters: [{ name: 'Zip archive', extensions: ['zip'] }],
    });
    if (!path) return;

    bundling.value = true;
    bundle.value = await api.diagnosticsBundle(path);
  } catch (e) {
    bundleError.value = e;
  } finally {
    bundling.value = false;
  }
}
/**
 * The open pane, persisted for the session only.
 *
 * Deliberately not in preferences.json: which pane you last had open is not a
 * setting, and writing the config file on every click would be noise in a file
 * the user may be reading.
 */

const engineRows = computed(() => {
  const e = app.engine;
  if (!e) return [];
  return [
    { label: t('engine.title'), value: e.reachable ? t('engine.running') : t('engine.down') },
    { label: 'Platform', value: t(`engine.platform.${e.platform}`) },
    { label: t('engine.version'), value: e.version || t('app.never') },
    { label: t('engine.apiVersion'), value: e.apiVersion || t('app.never') },
    { label: t('engine.context'), value: e.context || t('app.never') },
    { label: t('engine.socket'), value: e.socketPath || t('app.never') },
  ];
});

onMounted(async () => {
  logs.value = await api.logsInfo().catch(() => null);
});
</script>

<template>
  <SettingsGroup
    icon="mdi-docker"
    :title="t('engine.title')"
    :description="t('settings.engineGroupDesc')"
  >
    <template #append>
      <v-chip size="small" :color="app.engineUp ? 'success' : 'error'">
        {{ app.engineUp ? t('engine.running') : t('engine.down') }}
      </v-chip>
    </template>

    <div v-for="row in engineRows" :key="row.label" class="d-flex justify-space-between py-1 ga-4">
      <span class="text-caption text-medium-emphasis">{{ row.label }}</span>
      <span class="text-caption text-right break">{{ row.value }}</span>
    </div>
    <div v-if="app.engine?.error" class="text-caption text-error mt-2">
      {{ app.engine.error }}
    </div>
  </SettingsGroup>

  <DoctorPanel />

  <SettingsGroup
    icon="mdi-bug-outline"
    :title="t('settings.diagnostics')"
    :description="t('settings.diagnosticsHint')"
  >
    <div v-if="!logs?.directory" class="text-caption text-medium-emphasis">
      {{ t('settings.logsUnavailable') }}
    </div>
    <template v-else>
      <div class="d-flex align-center ga-2 flex-wrap">
        <code class="text-caption log-path">{{ logs.directory }}</code>
        <v-spacer />
        <v-chip size="x-small" variant="tonal">{{ bytes(logs.totalBytes) }}</v-chip>
        <v-btn
          size="small"
          variant="tonal"
          prepend-icon="mdi-folder-open"
          @click="api.openFolder(logs.directory)"
        >
          {{ t('settings.openLogs') }}
        </v-btn>
        <!-- The folder button leaves the reporter to find the right
               file among seven and to know the doctor output is a
               separate thing. This is the one that answers the whole
               question. -->
        <v-btn
          size="small"
          variant="flat"
          color="primary"
          prepend-icon="mdi-package-variant-closed"
          :loading="bundling"
          @click="saveDiagnosticBundle"
        >
          {{ t('settings.saveBundle') }}
        </v-btn>
      </div>
      <!-- Said out loud because the alternative is a user who assumes
             the opposite and attaches nothing, or one who assumes it is
             safe when it is not. -->
      <div class="text-caption text-medium-emphasis mt-2">
        {{ t('settings.logsRedacted') }}
      </div>
      <div class="text-caption text-medium-emphasis mt-1">
        {{ t('settings.saveBundleHint') }}
      </div>
      <!-- Named, not counted. "Saved 6 files" tells nobody whether
             the thing they were asked for is in there. -->
      <ErrorAlert v-if="bundleError" :error="bundleError" class="mt-3" />
      <v-alert
        v-if="bundle"
        type="success"
        variant="tonal"
        density="compact"
        class="mt-3 text-caption"
      >
        <div>{{ t('settings.saveBundleDone', { bytes: bytes(bundle.bytes) }) }}</div>
        <code class="text-caption log-path d-block mt-1">{{ bundle.path }}</code>
        <div class="mt-1">{{ bundle.entries.map((e) => e.name).join(', ') }}</div>
      </v-alert>
    </template>
  </SettingsGroup>

  <!-- ---- certificates ---------------------------------------------- -->
  <!-- HTTPS worked before this pane existed and was invisible: the one
       question a browser warning raises — "is my domain in the
       certificate?" — had no answer anywhere in the app. -->
</template>
