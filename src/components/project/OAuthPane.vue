<script setup>
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useCopyTick } from '@/composables/useCopyTick';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The redirect URI to paste into an identity provider's console (M-12).
 *
 * Beside the tunnel rather than inside it, because most of the time the tunnel
 * is not needed: a redirect URI is a **browser redirect, not a fetch**, so the
 * provider never resolves `shop.loc` — the browser does, on this machine, where
 * the name and the certificate both work. The tunnel is only for the providers
 * that refuse to accept the string at registration.
 *
 * The path is the one thing this app cannot know: it is a route in somebody
 * else's application. So it is typed, normalised on the Rust side, and echoed
 * back — what is on screen is what is in the URL.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();
const { copied, copy } = useCopyTick();

const path = ref('/auth/callback');
const result = ref(null);
const error = ref(null);

async function load() {
  error.value = null;
  try {
    result.value = await api.oauthCallbacks(props.name, path.value);
  } catch (e) {
    // A refused path is the common case while typing one, so the previous
    // answer stays on screen rather than blanking under the field.
    error.value = e;
  }
}

watch(() => [props.name, path.value], load, { immediate: true });
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-login-variant</v-icon>{{ t('oauth.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-3">{{ t('oauth.explain') }}</p>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <v-text-field
      v-model="path"
      :label="t('oauth.path')"
      density="compact"
      variant="outlined"
      hide-details
      class="mb-3"
      style="max-width: 320px"
    />

    <template v-if="result?.local">
      <div class="text-caption text-medium-emphasis">{{ t('oauth.local') }}</div>
      <div class="d-flex align-center ga-2 mb-3">
        <code class="mono">{{ result.local }}</code>
        <v-btn
          icon
          size="x-small"
          variant="text"
          :aria-label="t('a11y.copy')"
          @click="copy(result.local, 'local')"
        >
          <v-icon>{{ copied === 'local' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
        </v-btn>
      </div>
    </template>

    <div class="text-caption text-medium-emphasis">{{ t('oauth.public') }}</div>
    <div v-if="result?.public" class="d-flex align-center ga-2 mb-3">
      <code class="mono">{{ result.public }}</code>
      <v-btn
        icon
        size="x-small"
        variant="text"
        :aria-label="t('a11y.copy')"
        @click="copy(result.public, 'public')"
      >
        <v-icon>{{ copied === 'public' ? 'mdi-check' : 'mdi-content-copy' }}</v-icon>
      </v-btn>
    </div>
    <!-- Said rather than left blank: "no tunnel is running" and "this project
         cannot have a public callback" look identical when the row is empty,
         and only one of them has a button one pane away. -->
    <p v-else class="text-caption text-warning mb-3">{{ t('oauth.noTunnel') }}</p>

    <v-divider class="mb-3" />

    <!-- The table is the feature. Which of the two addresses a provider takes
         is invisible at its console, and the failure is a rejected form with
         no explanation on it. -->
    <div v-for="provider in result?.providers ?? []" :key="provider.id" class="provider">
      <v-chip
        size="x-small"
        variant="tonal"
        :color="provider.accepts === 'any' ? 'success' : 'warning'"
        class="mr-2"
      >
        {{ provider.accepts === 'any' ? t('oauth.takesLocal') : t('oauth.takesPublic') }}
      </v-chip>
      <span class="font-weight-medium">{{ provider.label }}</span>
      <span class="text-caption text-medium-emphasis ml-2">{{ provider.note }}</span>
    </div>
  </v-card>
</template>

<style scoped>
.provider {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  padding: 4px 0;
}
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.82rem;
  word-break: break-all;
}
</style>
