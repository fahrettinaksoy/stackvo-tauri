<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import { useCopyTick } from '@/composables/useCopyTick';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Stripe's own events, forwarded into this project (M-11).
 *
 * ## Why this is not the tunnel with a different label
 *
 * A quick tunnel's URL changes on every start, so a webhook endpoint
 * registered against one has to be re-registered every time and its signing
 * secret changes with it. `stripe listen` opens an outbound connection
 * instead: nothing has to be reachable, and the signing secret it prints is
 * the one to put in the application for the session.
 *
 * ## The key field never shows a key
 *
 * It is written to the OS keystore and read back as a boolean. There is no
 * path by which this pane can display what was stored — the only thing it can
 * do is replace it or clear it, which is what a credential field should be.
 */
const props = defineProps({
  name: { type: String, required: true },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();
const { copied, copy } = useCopyTick();

const status = ref(null);
const key = ref('');
const path = ref('/stripe/webhook');
const busy = ref(false);
const error = ref(null);

const listener = computed(() => status.value);

async function load() {
  try {
    const all = asList(await api.stripeStatus());
    status.value = all.find((s) => s.project === props.name) ?? null;
  } catch (e) {
    status.value = null;
  }
}

async function saveKey(value) {
  busy.value = true;
  error.value = null;
  try {
    await api.stripeKeySet(props.name, value);
    key.value = '';
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function start() {
  busy.value = true;
  error.value = null;
  try {
    await api.stripeStart(props.name, path.value, []);
    // The signing secret appears in the log a moment after the container is
    // up, so the first read is usually empty — the same shape the tunnel's
    // URL has, and polled for the same reason.
    for (let i = 0; i < 10 && !status.value?.signingSecret; i += 1) {
      await new Promise((r) => setTimeout(r, 1000));
      await load();
    }
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
    await api.stripeStop(props.name);
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

watch(() => props.name, load, { immediate: true });
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-credit-card-outline</v-icon>{{ t('stripe.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-3">{{ t('stripe.explain') }}</p>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <!-- The key first, because nothing else on this pane can do anything
         without it and a start button that always fails is worse than one
         that is not there yet. -->
    <div v-if="!listener?.hasKey" class="d-flex ga-2 align-center mb-3">
      <v-text-field
        v-model="key"
        :label="t('stripe.key')"
        :hint="t('stripe.keyHint')"
        persistent-hint
        type="password"
        density="compact"
        variant="outlined"
        autocomplete="off"
      />
      <v-btn size="small" color="primary" variant="tonal" :loading="busy" @click="saveKey(key)">
        {{ t('stripe.saveKey') }}
      </v-btn>
    </div>
    <div v-else class="d-flex align-center ga-2 mb-3">
      <v-icon size="16" color="success">mdi-key-variant</v-icon>
      <span class="text-caption">{{ t('stripe.keyStored') }}</span>
      <v-btn size="x-small" variant="text" :loading="busy" @click="saveKey(null)">
        {{ t('stripe.clearKey') }}
      </v-btn>
    </div>

    <v-text-field
      v-model="path"
      :label="t('stripe.path')"
      density="compact"
      variant="outlined"
      hide-details
      class="mb-3"
      style="max-width: 320px"
      :disabled="listener?.running"
    />

    <!-- Said rather than discovered by pressing start: the CLI would accept
         the events, fail to deliver each one and report the failures back to
         Stripe, filling the dashboard with delivery errors for a project that
         was simply not started. -->
    <v-alert v-if="!running" type="info" variant="tonal" class="mb-3">
      <div class="text-caption">{{ t('stripe.needsRunning') }}</div>
    </v-alert>

    <template v-if="listener?.running">
      <v-alert v-if="listener.signingSecret" type="success" variant="tonal" class="mb-3">
        <div class="text-caption mb-1">{{ t('stripe.secretIs') }}</div>
        <div class="d-flex align-center ga-2 flex-wrap">
          <code class="mono">{{ listener.signingSecret }}</code>
          <v-btn
            icon
            size="x-small"
            variant="text"
            :aria-label="t('a11y.copy')"
            @click="copy(listener.signingSecret, 'secret')"
          >
            <v-icon>{{ copied === 'secret' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
          </v-btn>
        </div>
      </v-alert>

      <!-- The CLI's own words. "That key was rejected" and "there is no
           network" are different problems with different fixes and they look
           identical from outside the container. -->
      <v-alert v-else-if="listener.failure" type="error" variant="tonal" class="mb-3">
        <div class="text-caption mono">{{ listener.failure }}</div>
      </v-alert>

      <div v-else class="d-flex align-center ga-3 mb-3">
        <v-progress-circular indeterminate size="18" width="2" color="primary" />
        <span class="text-caption text-medium-emphasis">{{ t('stripe.connecting') }}</span>
      </div>
    </template>

    <v-btn
      v-if="listener?.running"
      size="small"
      color="error"
      variant="tonal"
      :loading="busy"
      @click="stop"
    >
      {{ t('stripe.stop') }}
    </v-btn>
    <v-btn
      v-else
      size="small"
      color="primary"
      variant="flat"
      :disabled="!running || !listener?.hasKey"
      :loading="busy"
      @click="start"
    >
      {{ t('stripe.start') }}
    </v-btn>
  </v-card>
</template>

<style scoped>
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.82rem;
  word-break: break-all;
}
</style>
