<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Answering for this machine's development names, instead of editing
 * `/etc/hosts` once per project (E-1).
 *
 * ## Two switches, not one, because they are two different acts
 *
 * The responder is a socket this app owns and can turn on and off freely. The
 * resolver file changes how the **whole machine** resolves a suffix and needs a
 * password. One button doing both would make an administrator prompt appear
 * from something that reads like turning a feature on, which is exactly how
 * people learn to approve prompts without reading them.
 *
 * So they are shown as two rows in order, and the second says what it writes
 * before it writes it.
 *
 * ## Three platforms, and one of them gets a sentence rather than a switch
 *
 * Windows has no per-suffix resolver — the only mechanism redirects every name
 * on the machine, which is precisely what this refuses to be. Saying that is
 * better than a toggle that quietly does nothing, so the pane reads the
 * backend's `support` and draws what that platform can actually do.
 */
const { t } = useI18n();

const status = ref(null);
const error = ref(null);
const busy = ref(false);

const supported = computed(() => status.value?.support !== 'unsupported');
/** macOS has a file this app can write; Linux has a line only the user can place. */
const writable = computed(() => status.value?.support === 'resolver');

async function run(fn) {
  busy.value = true;
  error.value = null;
  try {
    status.value = await fn();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

const load = () => run(() => api.dnsStatus());
const toggle = () => run(() => (status.value?.listening ? api.dnsStop() : api.dnsStart()));
const toggleResolver = () =>
  run(() =>
    status.value?.resolverConfigured ? api.dnsResolverRemove() : api.dnsResolverInstall()
  );

onMounted(load);
</script>

<template>
  <SettingsGroup icon="mdi-dns-outline" :title="t('dns.title')" :subtitle="t('dns.subtitle')">
    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <p class="text-caption text-medium-emphasis mb-4">{{ t('dns.explain') }}</p>

    <template v-if="!supported">
      <v-alert type="info" variant="tonal" density="compact">
        <div class="text-caption">{{ t('dns.unsupported') }}</div>
      </v-alert>
    </template>

    <template v-else>
      <!-- Row one: the socket. -->
      <div class="d-flex align-center ga-3 mb-2">
        <v-switch
          :model-value="!!status?.listening"
          color="primary"
          density="compact"
          hide-details
          :loading="busy"
          :label="t('dns.responder', { port: status?.port ?? '' })"
          @update:model-value="toggle"
        />
      </div>
      <p class="text-caption text-medium-emphasis mb-4">
        {{ t('dns.responderHint', { suffix: status?.suffix ?? '' }) }}
      </p>

      <v-divider class="mb-4" />

      <!-- Row two: the machine's resolver. Only macOS has a file to write. -->
      <template v-if="writable">
        <div class="d-flex align-center ga-3 mb-2">
          <v-switch
            :model-value="!!status?.resolverConfigured"
            color="primary"
            density="compact"
            hide-details
            :loading="busy"
            :label="t('dns.resolver')"
            @update:model-value="toggleResolver"
          />
        </div>
        <p class="text-caption text-medium-emphasis mb-2">
          {{ t('dns.resolverHint', { file: status?.resolverFile ?? '' }) }}
        </p>
        <!-- What it writes, before it writes it. -->
        <pre class="instruction">{{ status?.instruction }}</pre>
      </template>

      <template v-else>
        <p class="text-caption text-medium-emphasis mb-2">{{ t('dns.manual') }}</p>
        <pre class="instruction">{{ status?.instruction }}</pre>
      </template>
    </template>
  </SettingsGroup>
</template>

<style scoped>
.instruction {
  padding: 8px 10px;
  border-radius: var(--app-radius);
  background: rgba(var(--v-border-color), var(--v-border-opacity));
  font-size: 0.75rem;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
