<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';

/**
 * "This machine is managed", once, above every settings pane.
 *
 * The per-field badge says which control is not yours. This says the page as a
 * whole answers to somebody else, and it is where the policy's own **failure**
 * surfaces — the reason `policy_status` returns `error` instead of writing it
 * to a log.
 *
 * The failure case matters more than the ordinary one. A policy that could not
 * be parsed applies nothing, and the app carries on exactly as an unmanaged one
 * would; the administrator who pushed it has no way to find that out and every
 * reason to believe it is in force. The one machine where it is visible is this
 * one, so it is shown here, at `warning` rather than folded into the
 * informational line.
 *
 * Renders nothing on an unmanaged machine, which is nearly every one.
 */
const { t } = useI18n();
const { policy } = useSharedEnvEditor();

const active = computed(() => policy.value.active === true);
const source = computed(() => policy.value.source ?? '');
const failure = computed(() => policy.value.error ?? null);
const prefix = computed(() => policy.value.registryPrefix ?? null);
const count = computed(() => policy.value.managed?.length ?? 0);
</script>

<template>
  <template v-if="active">
    <v-alert
      type="info"
      variant="tonal"
      density="comfortable"
      class="mb-4"
      :title="t('settings.policy.title')"
    >
      <div class="text-body-2">{{ t('settings.policy.body', { count }) }}</div>
      <div class="text-caption mt-1">
        <span class="text-medium-emphasis">{{ t('settings.policy.source') }}</span>
        <code class="ml-1">{{ source }}</code>
      </div>
      <div v-if="prefix" class="text-caption mt-1">
        <span class="text-medium-emphasis">{{ t('settings.policy.registry') }}</span>
        <code class="ml-1">{{ prefix }}</code>
      </div>
      <!-- Said out loud rather than implied. A layer the user can redirect with
           an environment variable is not a boundary, and an app that let anyone
           believe otherwise would be the actual problem. -->
      <div class="text-caption mt-2 text-medium-emphasis">
        {{ t('settings.policy.notASecurityBoundary') }}
      </div>
    </v-alert>

    <v-alert
      v-if="failure"
      type="warning"
      variant="tonal"
      density="comfortable"
      class="mb-4"
      :title="t('settings.policy.brokenTitle')"
    >
      <div class="text-body-2">{{ t('settings.policy.brokenBody') }}</div>
      <div class="text-caption mt-1 font-monospace">{{ failure }}</div>
    </v-alert>
  </template>
</template>
