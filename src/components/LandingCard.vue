<script setup>
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The one page that lists every site (M-4).
 *
 * On the dashboard rather than in settings, because it is not a setting: it is
 * an address somebody opens, and the question it answers — "what is in this
 * workspace and which of it is up" — is the dashboard's own question asked
 * from a browser instead of from here.
 *
 * ## Why refresh is a separate button
 *
 * The container serves a file. Starting a project after the page was written
 * leaves it stale with nothing having stopped, so "write it again" is a
 * different action from "serve it", and one button doing both would silently
 * restart a container to update a list.
 */
const { t } = useI18n();

const status = ref(null);
const busy = ref(false);
const error = ref(null);

async function load() {
  try {
    status.value = await api.landingStatus();
  } catch (e) {
    // No workspace is the common case on first run, not a fault worth an
    // alert on the dashboard.
    status.value = null;
    error.value = null;
  }
}

async function start() {
  busy.value = true;
  error.value = null;
  try {
    await api.landingStart();
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function stop() {
  busy.value = true;
  error.value = null;
  try {
    await api.landingStop();
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function refresh() {
  busy.value = true;
  error.value = null;
  try {
    status.value = await api.landingRefresh();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <v-card v-if="status" elevation="1" class="pa-4">
    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <div class="d-flex align-center ga-3 flex-wrap">
      <v-icon :color="status.running ? 'success' : 'grey'" size="24">mdi-home-city-outline</v-icon>
      <div>
        <div class="text-subtitle-2">{{ t('landing.title') }}</div>
        <button
          v-if="status.running"
          type="button"
          class="field-link text-caption"
          @click="api.openInBrowser(status.url)"
        >
          {{ status.url }}
        </button>
        <div v-else class="text-caption text-medium-emphasis">{{ t('landing.explain') }}</div>
      </div>

      <v-spacer />

      <span class="text-caption text-medium-emphasis">
        {{ t('landing.counts', { projects: status.projects, services: status.services }) }}
      </span>

      <v-btn
        v-if="status.running"
        size="small"
        variant="text"
        prepend-icon="mdi-refresh"
        :loading="busy"
        @click="refresh"
      >
        {{ t('landing.refresh') }}
      </v-btn>
      <v-btn
        :color="status.running ? 'error' : 'primary'"
        size="small"
        variant="tonal"
        :loading="busy"
        @click="status.running ? stop() : start()"
      >
        {{ status.running ? t('landing.stop') : t('landing.start') }}
      </v-btn>
    </div>

    <!-- The page is a snapshot. Said here as well as on the page itself,
         because this is where somebody is standing when they change what it
         should say. -->
    <p v-if="status.running && status.rendered" class="text-caption text-medium-emphasis mb-0 mt-2">
      {{ t('landing.rendered', { when: status.rendered }) }}
    </p>
  </v-card>
</template>
