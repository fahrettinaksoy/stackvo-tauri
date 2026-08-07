<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useXdebug } from '@/composables/useXdebug';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Xdebug's three layers, and the switch that moves all of them.
 *
 * One of three panes out of the Debug section under §14.16. Toggling rewrites
 * the manifest on disk, which is also what the Configuration section is
 * showing — hence `changed`: the view re-reads the manifest rather than this
 * pane reaching across to an editor it does not own.
 */
const props = defineProps({
  name: { type: String, required: true },
  runtime: { type: String, default: '' },
  running: { type: Boolean, default: false },
});

const emit = defineEmits(['changed']);

const { t } = useI18n();

const { status, busy, error, load, toggle } = useXdebug(toRef(props, 'name'));

watch(
  () => [props.name, props.runtime],
  () => load(props.runtime),
  { immediate: true }
);

async function set(enabled) {
  if (await toggle(enabled)) emit('changed');
}
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="d-flex align-center ga-2 mb-3">
      <div class="section-head">
        <v-icon size="18" class="mr-2">mdi-bug-outline</v-icon>{{ t('xdebug.title') }}
      </div>
      <span class="text-caption text-medium-emphasis">{{ t('xdebug.subtitle') }}</span>
    </div>

    <template v-if="status">
      <v-switch
        :model-value="status.enabled"
        :loading="busy"
        :disabled="busy"
        color="primary"
        hide-details
        density="comfortable"
        :label="status.enabled ? t('xdebug.on') : t('xdebug.off')"
        @update:model-value="set($event)"
      />

      <!-- The extension is compiled in, so the manifest can be ahead of
           the image. Saying nothing here is how a toggle becomes a lie. -->
      <v-alert v-if="status.needsRebuild" type="warning" variant="tonal" class="mt-3">
        <div class="text-caption">{{ t('xdebug.needsRebuild') }}</div>
      </v-alert>
      <v-alert
        v-else-if="status.enabled && status.running && status.active === false"
        type="warning"
        variant="tonal"
        class="mt-3"
      >
        <div class="text-caption">{{ t('xdebug.notActive') }}</div>
      </v-alert>
      <v-alert
        v-else-if="status.enabled && status.active === true"
        type="success"
        variant="tonal"
        class="mt-3"
      >
        <div class="text-caption">{{ t('xdebug.active') }}</div>
      </v-alert>

      <!-- The path mapping is the step people get wrong, and both halves
           are already known here. -->
      <template v-if="status.enabled">
        <div class="section-head mt-5 mb-2">
          <v-icon size="18" class="mr-2">mdi-tune</v-icon>{{ t('xdebug.ideSettings') }}
        </div>
        <v-table density="compact">
          <tbody>
            <tr>
              <td class="text-medium-emphasis">{{ t('xdebug.port') }}</td>
              <td class="mono">{{ status.port }}</td>
            </tr>
            <tr>
              <td class="text-medium-emphasis">{{ t('xdebug.ideKey') }}</td>
              <td class="mono">{{ status.ideKey }}</td>
            </tr>
            <tr v-if="status.serverName">
              <td class="text-medium-emphasis">{{ t('xdebug.serverName') }}</td>
              <td class="mono">{{ status.serverName }}</td>
            </tr>
            <tr>
              <td class="text-medium-emphasis">{{ t('xdebug.pathMapping') }}</td>
              <td class="mono">{{ status.hostPath }} → {{ status.containerPath }}</td>
            </tr>
            <tr v-if="status.peclVersion">
              <td class="text-medium-emphasis">{{ t('xdebug.version') }}</td>
              <td class="mono">{{ status.peclVersion }} (PHP {{ status.phpVersion }})</td>
            </tr>
          </tbody>
        </v-table>

        <!-- The one thing this design cannot fix, said where it will be
             read rather than left for someone to discover. -->
        <div class="text-caption text-medium-emphasis mt-3">
          {{ t('xdebug.cliCaveat') }}
        </div>
      </template>
    </template>
  </v-card>
</template>
