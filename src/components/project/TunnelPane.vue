<script setup>
import { toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useCopyTick } from '@/composables/useCopyTick';
import { useTunnel } from '@/composables/useTunnel';
import ErrorAlert from '@/components/ErrorAlert.vue';
import QrCode from '@/components/QrCode.vue';

/**
 * The public URL this project can be reached at while it is running.
 *
 * One of three panes out of the Container section under §14.16. `running` is a
 * prop rather than a fetch: a tunnel to a stopped container resolves to
 * nothing, and the view already knows the state.
 */
const props = defineProps({
  name: { type: String, required: true },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();

const { tunnel, busy, error, load, start, stop } = useTunnel(toRef(props, 'name'));
const { copied, copy } = useCopyTick();

watch(() => props.name, load, { immediate: true });
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-earth</v-icon>{{ t('tunnel.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('tunnel.explain') }}</p>

    <!-- The tunnel forwards to the container; a stopped container would
         serve 502s from a URL that looks like it worked. -->
    <v-alert v-if="!running" type="info" variant="tonal">
      <div class="text-caption">{{ t('tunnel.needsRunning') }}</div>
    </v-alert>

    <template v-else-if="tunnel?.running">
      <v-alert v-if="tunnel.url" type="success" variant="tonal" class="mb-3">
        <div class="d-flex align-center ga-2 flex-wrap">
          <button type="button" class="field-link" @click="api.openInBrowser(tunnel.url)">
            {{ tunnel.url }}
          </button>
          <v-btn
            icon
            :aria-label="t('a11y.copy')"
            size="x-small"
            variant="text"
            @click="copy(tunnel.url, 'tunnel')"
          >
            <v-icon>{{ copied === 'tunnel' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
            <v-tooltip activator="parent">{{ t('a11y.copy') }}</v-tooltip>
          </v-btn>
          <v-spacer />
          <v-btn size="small" color="error" variant="tonal" :loading="busy" @click="stop">
            {{ t('tunnel.stop') }}
          </v-btn>
        </div>
      </v-alert>

      <!-- The URL is four random words from Cloudflare and it is opened on a
           phone as often as on this machine — a webhook sender is not the only
           thing anybody points at a tunnel. -->
      <div v-if="tunnel.url" class="d-flex align-start ga-3 mb-3">
        <QrCode :text="tunnel.url" :size="152" />
        <div class="text-caption text-medium-emphasis pt-1">{{ t('tunnel.scan') }}</div>
      </div>

      <div v-if="!tunnel.url" class="d-flex align-center ga-3">
        <v-progress-circular indeterminate size="18" width="2" color="primary" />
        <span class="text-caption text-medium-emphasis">{{ t('tunnel.connecting') }}</span>
        <v-spacer />
        <v-btn size="small" color="error" variant="tonal" :loading="busy" @click="stop">
          {{ t('tunnel.stop') }}
        </v-btn>
      </div>

      <!-- Said before anyone pastes the URL into a public issue: the
           link is live, unauthenticated, and reaches this machine. -->
      <v-alert type="warning" variant="tonal" class="mt-3">
        <div class="text-caption">{{ t('tunnel.publicWarning') }}</div>
      </v-alert>
    </template>

    <template v-else>
      <v-btn color="primary" variant="flat" prepend-icon="mdi-earth" :loading="busy" @click="start">
        {{ t('tunnel.start') }}
      </v-btn>
      <div class="text-caption text-medium-emphasis mt-3">
        {{ t('tunnel.startHint') }}
      </div>
    </template>
  </v-card>
</template>
