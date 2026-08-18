<script setup>
import { ref, computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import { useCopyTick } from '@/composables/useCopyTick';

/**
 * The loopback API — §3 #34, ADR 0026.
 *
 * ## Why this pane exists at all, rather than a line in preferences.json
 *
 * The surface is off until somebody asks, and the asking has to be somewhere a
 * person can see, because the thing they are turning on answers questions about
 * their workspace to anything on this machine that holds the token. A setting
 * buried in a file is a setting people forget is on.
 *
 * ## The token is shown once, and this pane is the once
 *
 * `websurface_start` returns it; `websurface_status` does not, and cannot — a
 * status call that carried the token would hand it to every later caller, and
 * the first of those is the surface itself. So the value lives in this
 * component's memory for as long as the pane is open, and after a reload the
 * only way to get a token is to stop and start again.
 *
 * That is deliberate and it is stated on screen rather than left to be
 * discovered. The alternative — writing it somewhere — is a token that outlives
 * the process that meant it.
 */
const { t } = useI18n();
const { copied, copy } = useCopyTick();

const status = ref({ running: false, address: null, tools: [] });
const token = ref(null);
const error = ref(null);
const busy = ref(false);

const running = computed(() => status.value.running);

/** The whole request, ready to paste. Built from what is actually running. */
const example = computed(() => {
  const address = status.value.address ?? '127.0.0.1:0';
  const tool = status.value.tools[0] ?? 'stackvo_overview';
  const bearer = token.value ?? t('settings.localApi.tokenPlaceholder');
  return [
    `curl -s http://${address}/call \\`,
    `  -H 'Authorization: Bearer ${bearer}' \\`,
    `  -d '{"tool":"${tool}"}'`,
  ].join('\n');
});

async function refresh() {
  try {
    status.value = await api.websurfaceStatus();
    error.value = null;
  } catch (e) {
    error.value = e;
  }
}

async function start() {
  busy.value = true;
  try {
    const bound = await api.websurfaceStart();
    token.value = bound.token;
    error.value = null;
    await refresh();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function stop() {
  busy.value = true;
  try {
    await api.websurfaceStop();
    // Forgotten here as well as there. A token left on screen after the
    // surface it opened is gone is a value somebody copies and then cannot
    // work out why it is refused.
    token.value = null;
    error.value = null;
    await refresh();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <SettingsGroup
    icon="mdi-lan-connect"
    :title="t('settings.localApi.title')"
    :description="t('settings.localApi.description')"
  >
    <v-alert type="info" variant="tonal" density="comfortable" class="mb-4">
      <div class="text-body-2">{{ t('settings.localApi.whatItDoes') }}</div>
      <div class="text-caption mt-2 text-medium-emphasis">
        {{ t('settings.localApi.readsOnly') }}
      </div>
    </v-alert>

    <div class="d-flex align-center ga-3 mb-4">
      <v-btn
        v-if="!running"
        color="primary"
        variant="flat"
        :loading="busy"
        prepend-icon="mdi-play"
        @click="start"
      >
        {{ t('settings.localApi.start') }}
      </v-btn>
      <v-btn v-else variant="tonal" :loading="busy" prepend-icon="mdi-stop" @click="stop">
        {{ t('settings.localApi.stop') }}
      </v-btn>

      <v-chip v-if="running" size="small" color="success" variant="tonal">
        {{ status.address }}
      </v-chip>
      <span v-else class="text-caption text-medium-emphasis">
        {{ t('settings.localApi.notRunning') }}
      </span>
    </div>

    <!-- Shown once, and said so. The value is in this component's memory and
         nowhere else; a reload loses it, and getting another one means
         stopping and starting. -->
    <template v-if="running && token">
      <v-alert type="warning" variant="tonal" density="comfortable" class="mb-3">
        <div class="text-body-2">{{ t('settings.localApi.tokenShownOnce') }}</div>
      </v-alert>
      <div class="d-flex align-center ga-2 mb-4">
        <code class="text-caption flex-grow-1 text-truncate">{{ token }}</code>
        <v-btn
          size="small"
          variant="text"
          :prepend-icon="copied === 'token' ? 'mdi-check' : 'mdi-content-copy'"
          @click="copy(token, 'token')"
        >
          {{ t('app.copy') }}
        </v-btn>
      </div>
    </template>

    <v-alert
      v-else-if="running && !token"
      type="info"
      variant="tonal"
      density="comfortable"
      class="mb-4"
    >
      <div class="text-body-2">{{ t('settings.localApi.tokenGone') }}</div>
    </v-alert>

    <template v-if="running">
      <div class="text-subtitle-2 mb-1">{{ t('settings.localApi.example') }}</div>
      <pre class="text-caption pa-3 bg-surface-variant overflow-x-auto">{{ example }}</pre>
      <v-btn
        size="small"
        variant="text"
        class="mt-1 mb-4"
        :prepend-icon="copied === 'example' ? 'mdi-check' : 'mdi-content-copy'"
        @click="copy(example, 'example')"
      >
        {{ t('app.copy') }}
      </v-btn>

      <div class="text-subtitle-2 mb-1">
        {{ t('settings.localApi.served', { count: status.tools.length }) }}
      </div>
      <div class="d-flex flex-wrap ga-1">
        <v-chip v-for="tool in status.tools" :key="tool" size="x-small" variant="outlined">
          {{ tool }}
        </v-chip>
      </div>
    </template>
  </SettingsGroup>
</template>
