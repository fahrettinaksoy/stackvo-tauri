<script setup>
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useCopyTick } from '@/composables/useCopyTick';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The address a phone on the same Wi-Fi can open this project at.
 *
 * Beside the tunnel and not inside it, because they answer different questions
 * with different costs. The tunnel publishes to the internet and needs a
 * sidecar running; this needs neither, reaches only the local network, and its
 * one real cost is a certificate warning on the visiting device.
 *
 * `running` is not a prop here and its absence is deliberate: the tunnel
 * forwards to the container, so a stopped one serves 502s from a URL that looks
 * like it worked. This is a name in the router and the certificate — it is
 * correct while the project is stopped, and what the visitor gets is Traefik's
 * own answer rather than a broken promise from this pane.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const emit = defineEmits(['changed']);

const { t } = useI18n();
const { copied, copy } = useCopyTick();

const status = ref(null);
const busy = ref(false);
const error = ref(null);

/**
 * The switch reads the stored intent out of the status rather than out of a
 * manifest fetched beside it. One source: a manifest says the project asked and
 * the status says what it currently answers on, and two fetches that can
 * disagree is a switch that shows off beside a live address.
 */
const row = computed(() => status.value?.projects?.find((p) => p.name === props.name) ?? null);
const shared = computed(() => row.value !== null);
/** Null when the project asked and this machine has no address to build one. */
const host = computed(() => row.value?.host ?? null);

async function load() {
  try {
    status.value = await api.lanStatus();
  } catch (e) {
    error.value = e;
  }
}

async function toggle(enabled) {
  busy.value = true;
  error.value = null;
  try {
    await api.projectLanShare(props.name, enabled);
    await load();
    // The name only reaches the router and the certificate on a regenerate,
    // and this pane deliberately does not do that itself — see the note the
    // command carries. The page above owns that decision.
    emit('changed');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

watch(() => props.name, load, { immediate: true });
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-wifi</v-icon>{{ t('lan.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('lan.explain') }}</p>

    <v-switch
      :model-value="shared"
      :loading="busy"
      :disabled="busy"
      color="primary"
      density="compact"
      hide-details
      :label="t('lan.share')"
      @update:model-value="toggle($event)"
    />

    <!-- Two different absences with one answer, said apart because only one of
         them has anything the user can do about it. -->
    <v-alert v-if="shared && !status?.address" type="warning" variant="tonal" class="mt-3">
      <div class="text-caption">{{ t('lan.noAddress') }}</div>
    </v-alert>

    <template v-else-if="shared && host">
      <div class="d-flex align-center ga-2 flex-wrap mt-3">
        <button type="button" class="field-link" @click="api.openInBrowser(`https://${host}`)">
          {{ host }}
        </button>
        <v-btn
          icon
          :aria-label="t('a11y.copy')"
          size="x-small"
          variant="text"
          @click="copy(`https://${host}`, 'lan')"
        >
          <v-icon>{{ copied === 'lan' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
          <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
        </v-btn>
      </div>

      <!-- Said before the phone says it, because on a phone the two failures
           this could be look identical and only one of them is expected. -->
      <v-alert type="info" variant="tonal" class="mt-3">
        <div class="text-caption">{{ t('lan.certWarning') }}</div>
      </v-alert>

      <div class="text-caption text-medium-emphasis mt-3">
        {{ t('lan.regenerateHint') }}
      </div>
    </template>

    <!-- A laptop that changed networks. The name in the compose file and the
         certificate is a copy, and a copy of an expired lease points at
         whichever machine took it next. -->
    <v-alert v-if="status?.stale" type="warning" variant="tonal" class="mt-3">
      <div class="text-caption">{{ t('lan.stale', { host: status.stale }) }}</div>
    </v-alert>
  </v-card>
</template>
