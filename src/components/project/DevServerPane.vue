<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useDevServer } from '@/composables/useDevServer';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * A Node project's dev server.
 *
 * Half of the Runtime section — the other half is `PhpIniPane`, and the two
 * never appear together: this one is empty unless the runtime is `node`.
 *
 * The runtime comes in as a prop rather than being read here. The view has
 * already loaded the project, and a pane that fetched it again would show the
 * wrong runtime for as long as its own request took.
 */
const props = defineProps({
  name: { type: String, required: true },
  runtime: { type: String, default: '' },
});

const { t } = useI18n();

const { status, command, busy, copied, error, blocked, load, toggle, copySnippet } = useDevServer(
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
      <v-icon size="18" class="mr-2">mdi-lightning-bolt-outline</v-icon>
      {{ t('devServer.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('devServer.explain') }}</p>

    <template v-if="status">
      <v-switch
        :model-value="status.enabled"
        :loading="busy"
        :disabled="busy"
        color="primary"
        hide-details
        density="comfortable"
        :label="status.enabled ? t('devServer.on') : t('devServer.off')"
        @update:model-value="toggle($event)"
      />

      <v-text-field
        v-model="command"
        :label="t('devServer.command')"
        :hint="
          status.productionCommand
            ? t('devServer.commandHint', { production: status.productionCommand })
            : ''
        "
        persistent-hint
        density="comfortable"
        variant="outlined"
        class="mt-4"
        :disabled="busy"
        @keyup.enter="status.enabled && toggle(true)"
      />

      <!-- On, but the container predates it — the source is not mounted
           in the thing that is actually running. -->
      <v-alert v-if="status.needsRecreate" type="warning" variant="tonal" class="mt-4">
        <div class="text-caption">{{ t('devServer.needsRecreate') }}</div>
      </v-alert>
      <v-alert
        v-else-if="status.enabled && status.mounted"
        type="success"
        variant="tonal"
        class="mt-4"
      >
        <div class="text-caption">{{ t('devServer.live') }}</div>
      </v-alert>

      <!-- The half that is not ours. A .loc domain gets a flat 403 from
           Vite unless its own config names it, which reads as "the site
           is up and broken" with nothing pointing at the dev server. -->
      <template v-if="status.snippet">
        <div class="section-head mt-5 mb-1">
          <v-icon size="18" class="mr-2">mdi-file-code-outline</v-icon>
          {{ t('devServer.projectConfig') }}
        </div>
        <p class="text-caption text-medium-emphasis mb-2">
          {{ t('devServer.projectConfigWhy') }}
        </p>

        <v-alert v-if="blocked" type="warning" variant="tonal" density="compact" class="mb-2">
          <div class="text-caption">
            {{ t('devServer.notAllowed', { file: status.configFile }) }}
          </div>
        </v-alert>
        <v-alert
          v-else-if="status.hostAllowed"
          type="success"
          variant="tonal"
          density="compact"
          class="mb-2"
        >
          <div class="text-caption">{{ t('devServer.configured') }}</div>
        </v-alert>

        <div class="d-flex align-start ga-2">
          <pre class="snippet flex-grow-1">{{ status.snippet }}</pre>
          <v-btn icon size="small" variant="text" :aria-label="t('a11y.copy')" @click="copySnippet">
            <v-icon>{{ copied ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
            <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
          </v-btn>
        </div>
      </template>
      <div v-else class="text-caption text-medium-emphasis mt-4">
        {{ t('devServer.noAdvice') }}
      </div>

      <div class="text-caption text-medium-emphasis mt-4">
        {{ t('devServer.modulesNote') }}
      </div>
      <div class="text-caption text-medium-emphasis mt-2">
        {{ t('devServer.cliCaveat') }}
      </div>
    </template>
  </v-card>
</template>
