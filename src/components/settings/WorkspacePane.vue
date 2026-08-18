<script setup>
import { onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useAppStore } from '@/stores/app';
import { api } from '@/lib/ipc';
import { useStackPreset, useGeneratorCheck } from '@/composables/useStackPreset';
import SettingsGroup from '@/components/SettingsGroup.vue';
import TemplateOverridesPane from '@/components/settings/TemplateOverridesPane.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The workspace: where the stack lives, how it is run, and how it is handed to
 * somebody else.
 *
 * Fifth pane out of `Settings.vue` under §14.16 and the largest — the folder,
 * the compose verbs and the preset were three panes for one subject, and were
 * also three places to look before finding the button you wanted.
 *
 * The compose verbs stay with the view: `up` and `down` report through the
 * shared operation console and their busy state is the view's, so the pane
 * emits and the view runs them.
 */
const { t } = useI18n();
const app = useAppStore();

defineProps({
  /** True while the view is running a compose verb. */
  busy: { type: Boolean, default: false },
});

/**
 * The compose verbs and the folder picker are emitted, not called.
 *
 * `up`, `restart` and `down` report through the shared operation console and
 * their busy state is the view's; picking a workspace changes state the whole
 * app reads. A pane that ran them itself would be a pane that owns the stack.
 */
const emit = defineEmits(['pick', 'up', 'restart', 'down']);

const {
  name: presetName,
  preset,
  plan: presetPlan,
  busy: presetBusy,
  applied: presetApplied,
  error: presetError,
  json: presetJson,
  enabledCount: presetEnabledCount,
  load: loadPreset,
  exportTo,
  planFrom,
  apply: applyPreset,
  clearPlan: clearPresetPlan,
} = useStackPreset();

const {
  report: generator,
  verifying,
  error: generatorError,
  verify,
  regenerateAndVerify: runGenerate,
} = useGeneratorCheck();

/**
 * The dialogs are passed in rather than reached for inside the composable: a
 * composable that imports a Tauri plugin cannot be exercised without one, and
 * choosing a path is the user's act, not the preset logic's.
 */
async function exportPreset() {
  const { save } = await import('@tauri-apps/plugin-dialog');
  await exportTo((defaultPath) => save({ defaultPath }));
}

async function choosePreset() {
  const { open } = await import('@tauri-apps/plugin-dialog');
  await planFrom(() => open({ multiple: false, directory: false }));
}

// Each section is behind a `v-if`, so mounting this *is* opening it. That is
// what the old `watch(tab, …)` was trying to express, against a section key
// that had stopped existing — see `useStackPreset`.
onMounted(loadPreset);
</script>

<template>
  <ErrorAlert
    v-if="presetError || generatorError"
    :error="presetError || generatorError"
    class="mb-4"
  />

  <SettingsGroup
    icon="mdi-folder-open-outline"
    :title="t('settings.workspaceGroup')"
    :description="t('settings.workspaceGroupDesc')"
  >
    <!-- The one the user chose. -->
    <div class="text-body-2 break">
      {{ app.workspace?.projectsDir || t('workspace.none') }}
    </div>
    <div v-if="app.workspace" class="text-caption text-medium-emphasis mt-1">
      {{ t(`workspace.source.${app.workspace.source}`) }}
      <template v-if="app.workspace.stackvoVersion">
        · {{ t('workspace.version') }} {{ app.workspace.stackvoVersion }}
      </template>
    </div>

    <div class="d-flex ga-2 flex-wrap mt-3">
      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-folder-open-outline"
        @click="emit('pick')"
      >
        {{ t('workspace.change') }}
      </v-btn>
      <v-btn
        v-if="app.workspace?.projectsDir"
        size="small"
        variant="text"
        prepend-icon="mdi-open-in-new"
        @click="api.openFolder(app.workspace.projectsDir)"
      >
        {{ t('projects.openFolder') }}
      </v-btn>
    </div>

    <!-- And the one it never asks about. Shown because "where did my
         compose file go" is a fair question, and because the answer
         is a hidden directory nobody would find by looking. -->
    <v-divider class="my-4" />

    <div class="text-caption text-medium-emphasis">{{ t('workspace.appDir') }}</div>
    <div class="text-body-2 break mt-1">{{ app.workspace?.root }}</div>
    <div class="text-caption text-medium-emphasis mt-1">
      {{ t('workspace.appDirDesc') }}
    </div>
    <v-btn
      v-if="app.workspace?.root"
      size="small"
      variant="text"
      class="mt-2 ml-n2"
      prepend-icon="mdi-open-in-new"
      @click="api.openFolder(app.workspace.root)"
    >
      {{ t('projects.openFolder') }}
    </v-btn>
  </SettingsGroup>

  <TemplateOverridesPane />

  <SettingsGroup
    icon="mdi-play-box-multiple-outline"
    :title="t('settings.compose')"
    :description="t('settings.stackSub')"
  >
    <div class="d-flex ga-2 flex-wrap">
      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-play-box-multiple-outline"
        :loading="busy"
        :disabled="!app.engineUp"
        @click="emit('up')"
      >
        {{ t('actions.up') }}
      </v-btn>
      <v-btn
        size="small"
        variant="tonal"
        prepend-icon="mdi-refresh"
        :loading="busy"
        :disabled="!app.engineUp"
        @click="emit('restart')"
      >
        {{ t('actions.composeRestart') }}
      </v-btn>
      <v-btn
        size="small"
        variant="tonal"
        color="error"
        prepend-icon="mdi-stop-circle-outline"
        :loading="busy"
        :disabled="!app.engineUp"
        @click="emit('down')"
      >
        {{ t('actions.down') }}
      </v-btn>
    </div>
  </SettingsGroup>

  <!-- Does what is on disk still match what the generator would write? A
       workspace drifts when a generated file is edited by hand or a manifest
       changes with nothing regenerating after it, and neither leaves a mark
       anywhere else. -->
  <SettingsGroup
    icon="mdi-cog-sync-outline"
    :title="t('settings.generator')"
    :description="t('settings.generatorDesc')"
  >
    <template #append>
      <v-btn
        size="x-small"
        variant="text"
        icon="mdi-refresh"
        :aria-label="t('settings.verifyNow')"
        :loading="verifying"
        @click="verify"
      />
    </template>

    <template v-if="generator">
      <div class="d-flex align-center ga-2 mb-2">
        <v-chip size="small" :color="generator.inSync ? 'success' : 'warning'">
          {{ generator.matched }} /
          {{ generator.matched + generator.differed }}
        </v-chip>
        <span class="text-caption text-medium-emphasis">
          {{ generator.inSync ? t('settings.generatorReady') : t('settings.generatorDiffers') }}
        </span>
      </div>

      <div
        v-for="f in generator.files.filter((x) => x.status !== 'match')"
        :key="f.file"
        class="text-caption text-warning"
      >
        {{ f.file }} — {{ f.status }}
        <span v-if="f.firstDifferenceLine">(line {{ f.firstDifferenceLine }})</span>
      </div>

      <v-alert
        v-for="(w, i) in generator.warnings"
        :key="i"
        type="warning"
        variant="tonal"
        class="mt-2"
      >
        <div class="text-caption">{{ w }}</div>
      </v-alert>

      <v-divider class="my-3" />

      <!-- One generator, no selector: the Rust engine took over
           after reaching byte parity on every file, and the report
           above is now a drift check — does the disk still hold
           what this generator would write? -->
      <v-btn size="small" variant="tonal" block :loading="verifying" @click="runGenerate">
        {{ t('actions.generate') }}
      </v-btn>
    </template>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-export-variant"
    :title="t('stackPreset.export')"
    :description="t('stackPreset.exportDesc')"
  >
    <div class="d-flex ga-2 align-start">
      <v-text-field
        v-model="presetName"
        :label="t('stackPreset.name')"
        :placeholder="t('stackPreset.namePlaceholder')"
        persistent-placeholder
        hide-details
      />
      <v-btn color="primary" variant="flat" :loading="presetBusy" @click="exportPreset">
        {{ t('stackPreset.saveFile') }}
      </v-btn>
    </div>

    <div v-if="preset" class="text-caption text-medium-emphasis mt-3">
      {{
        t('stackPreset.summary', {
          enabled: presetEnabledCount,
          total: Object.keys(preset.services).length,
        })
      }}
    </div>

    <!-- Shown, not just written. The reason to read it is the reason
         to trust it: there is no password in there because there is
         nowhere in the format to put one. -->
    <v-expansion-panels v-if="presetJson" variant="accordion" class="mt-3">
      <v-expansion-panel :title="t('stackPreset.preview')">
        <v-expansion-panel-text>
          <pre class="preset-json">{{ presetJson }}</pre>
        </v-expansion-panel-text>
      </v-expansion-panel>
    </v-expansion-panels>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-import"
    :title="t('stackPreset.import')"
    :description="t('stackPreset.importDesc')"
  >
    <v-btn variant="tonal" :loading="presetBusy" @click="choosePreset">
      {{ t('stackPreset.chooseFile') }}
    </v-btn>

    <template v-if="presetPlan">
      <div class="text-body-2 mt-4">
        <strong>{{ presetPlan.name || t('stackPreset.untitled') }}</strong>
        <span v-if="presetPlan.description" class="text-medium-emphasis">
          — {{ presetPlan.description }}
        </span>
      </div>

      <!-- "Nothing to do" and "everything was rejected" both produce
           an empty change list and need opposite responses, so the
           unchanged count is what tells them apart. -->
      <v-alert
        v-if="presetApplied"
        type="success"
        variant="tonal"
        class="mt-3"
        :text="t('stackPreset.applied')"
      />
      <v-alert
        v-else-if="!presetPlan.changes.length"
        type="info"
        variant="tonal"
        class="mt-3"
        :text="
          presetPlan.unchanged
            ? t('stackPreset.alreadyMatches', { n: presetPlan.unchanged })
            : t('stackPreset.nothingUsable')
        "
      />

      <v-table v-if="presetPlan.changes.length" density="compact" class="mt-3">
        <thead>
          <tr>
            <th>{{ t('stackPreset.colSubject') }}</th>
            <th>{{ t('stackPreset.colFrom') }}</th>
            <th>{{ t('stackPreset.colTo') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="change in presetPlan.changes" :key="change.key">
            <td>
              <div>{{ change.subject }}</div>
              <div class="text-caption text-medium-emphasis mono">{{ change.key }}</div>
            </td>
            <td class="mono text-medium-emphasis">
              {{ change.from ?? t('stackPreset.absent') }}
            </td>
            <td class="mono">{{ change.to }}</td>
          </tr>
        </tbody>
      </v-table>

      <!-- Named, never silently dropped: a preset that quietly skips
           half of what it was given is how somebody concludes it
           worked and then loses an afternoon to the service it
           ignored. -->
      <v-alert v-if="presetPlan.rejected.length" type="warning" variant="tonal" class="mt-3">
        <div class="text-caption font-weight-medium mb-1">
          {{ t('stackPreset.rejected') }}
        </div>
        <div v-for="line in presetPlan.rejected" :key="line" class="text-caption">
          {{ line }}
        </div>
      </v-alert>

      <div class="d-flex ga-2 align-center mt-4">
        <v-btn
          v-if="!presetApplied && presetPlan.changes.length"
          color="primary"
          variant="flat"
          :loading="presetBusy"
          @click="applyPreset"
        >
          {{ t('stackPreset.apply', { n: presetPlan.changes.length }) }}
        </v-btn>
        <v-btn variant="text" :disabled="presetBusy" @click="clearPresetPlan">
          {{ presetApplied ? t('app.close') : t('app.cancel') }}
        </v-btn>
      </div>

      <!-- Enabling a service changes what the generator emits, so the
           import is not live until regenerate-then-up. Saying so here
           is the difference between a feature that worked and one the
           user believes did nothing. -->
      <div
        v-if="presetApplied && presetPlan.needsRegenerate"
        class="text-caption text-medium-emphasis mt-2"
      >
        {{ t('stackPreset.thenRegenerate') }}
      </div>
    </template>
  </SettingsGroup>
</template>
