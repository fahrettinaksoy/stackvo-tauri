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
 * second switch changes how the **whole machine** resolves a suffix and needs a
 * password. One button doing both would make an administrator prompt appear
 * from something that reads like turning a feature on, which is exactly how
 * people learn to approve prompts without reading them.
 *
 * So they are shown as two rows in order, and the second says what it writes,
 * where, and what gets reloaded — before it writes it.
 *
 * ## The mechanism is a detected fact, not a platform
 *
 * This pane used to draw one of three things by asking which OS it was on, and
 * told Windows users the feature did not exist there. The backend now reports
 * *which* mechanism this machine has — a resolver file, one of two dnsmasq
 * drop-ins, systemd-resolved, or the NRPT — so the pane names it and shows the
 * file. `manual` is the one case with no switch: nothing recognisable is in
 * front of the resolver, and the honest offer is the line to place.
 *
 * ## A test button, because "configured" is not "working"
 *
 * Reading a file back proves a write happened. `dnsCheck` asks the responder
 * over both transports and then asks the machine itself, and the four answers
 * are shown separately: which one failed is the whole of what tells somebody
 * what to fix.
 */
const { t } = useI18n();

const status = ref(null);
const check = ref(null);
const error = ref(null);
const busy = ref(false);
const testing = ref(false);

/** Nothing this app can apply — the pane shows the line instead of a switch. */
const manual = computed(() => status.value?.mechanism === 'manual');
/** A mechanism exists but no password prompt does. Linux without polkit. */
const readOnly = computed(() => !manual.value && status.value?.writable === false);

const mechanismLabel = computed(() =>
  status.value?.mechanism ? t(`dns.mechanisms.${status.value.mechanism}`) : ''
);

const probes = computed(() => {
  const result = check.value;
  if (!result) return [];
  return ['udp', 'tcp', 'system', 'public'].map((key) => ({
    key,
    label: t(`dns.probes.${key}`),
    ...result[key],
  }));
});

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
  run(() => (status.value?.configured ? api.dnsResolverRemove() : api.dnsResolverInstall()));

/**
 * The status is reloaded with the check: a machine that was pointed at us and
 * has since had its resolver changed underneath the app would otherwise keep a
 * stale switch position beside a fresh failure.
 */
async function test() {
  testing.value = true;
  error.value = null;
  try {
    check.value = await api.dnsCheck();
    status.value = await api.dnsStatus();
  } catch (e) {
    error.value = e;
  } finally {
    testing.value = false;
  }
}

onMounted(load);
</script>

<template>
  <SettingsGroup icon="mdi-dns-outline" :title="t('dns.title')" :subtitle="t('dns.subtitle')">
    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <p class="text-caption text-medium-emphasis mb-4">{{ t('dns.explain') }}</p>

    <!-- The state where every name under the suffix fails and everything else
         on screen looks fine: the machine asks us, and nothing is answering. -->
    <v-alert
      v-if="status?.broken"
      type="warning"
      variant="tonal"
      density="compact"
      class="mb-4"
      data-test="dns-broken"
    >
      <div class="text-caption">
        {{ t('dns.broken', { suffix: status.suffix, port: status.port }) }}
      </div>
    </v-alert>

    <!-- Left over from a suffix this workspace no longer uses. On macOS the
         file is per suffix, so the old one keeps refusing a TLD that used to
         resolve — which is worse than never having written it. -->
    <v-alert
      v-if="status?.stale?.length"
      type="warning"
      variant="tonal"
      density="compact"
      class="mb-4"
      data-test="dns-stale"
    >
      <div class="text-caption">
        {{ t('dns.stale', { files: status.stale.join(', ') }) }}
      </div>
    </v-alert>

    <!-- Row one: the socket. Always available — it is this app's own port. -->
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
    <p class="text-caption text-medium-emphasis mb-1">
      {{ t('dns.responderHint', { suffix: status?.suffix ?? '' }) }}
    </p>
    <!-- Only when it is the surprising half: UDP up, TCP not. -->
    <p v-if="status?.listening && !status?.tcp" class="text-caption text-warning mb-4">
      {{ t('dns.udpOnly', { port: status?.port ?? '' }) }}
    </p>
    <div v-else class="mb-4"></div>

    <v-divider class="mb-4" />

    <!-- Row two: the machine's resolver, in whatever shape this machine has. -->
    <template v-if="!manual && !readOnly">
      <div class="d-flex align-center ga-3 mb-2">
        <v-switch
          :model-value="!!status?.configured"
          color="primary"
          density="compact"
          hide-details
          :loading="busy"
          :label="t('dns.resolver')"
          @update:model-value="toggleResolver"
        />
      </div>
      <p class="text-caption text-medium-emphasis mb-2">
        {{
          status?.file
            ? t('dns.resolverHint', { file: status.file, mechanism: mechanismLabel })
            : t('dns.resolverHintRule', { mechanism: mechanismLabel })
        }}
      </p>
      <!-- What it writes, before it writes it. -->
      <pre class="instruction">{{ status?.instruction }}</pre>
      <p v-if="status?.reload" class="text-caption text-medium-emphasis mt-2 mb-0">
        {{ t('dns.reload', { command: status.reload }) }}
      </p>
      <!-- Somebody else's file at that path. Said before the switch is
           pressed — a file discovered to be gone afterwards is not a warning,
           it is an apology. -->
      <p v-if="status?.foreign" class="text-caption text-warning mt-2 mb-0" data-test="dns-foreign">
        {{ t('dns.foreign', { detail: status.foreign }) }}
      </p>
    </template>

    <template v-else>
      <p class="text-caption text-medium-emphasis mb-2">
        {{ readOnly ? t('dns.noPrompt', { mechanism: mechanismLabel }) : t('dns.manual') }}
      </p>
      <pre class="instruction">{{ status?.instruction }}</pre>
      <p v-if="status?.file" class="text-caption text-medium-emphasis mt-2 mb-0">
        {{ t('dns.manualFile', { file: status.file }) }}
      </p>
    </template>

    <v-divider class="my-4" />

    <!-- Row three: the measurement. -->
    <div class="d-flex align-center ga-3 flex-wrap">
      <v-btn
        variant="tonal"
        size="small"
        :loading="testing"
        prepend-icon="mdi-check-network-outline"
        @click="test"
      >
        {{ t('dns.test') }}
      </v-btn>
      <span class="text-caption text-medium-emphasis">{{ t('dns.testHint') }}</span>
    </div>

    <v-list v-if="check" density="compact" class="mt-2 bg-transparent" data-test="dns-probes">
      <v-list-item v-for="probe in probes" :key="probe.key" class="px-0">
        <template #prepend>
          <v-icon
            :icon="probe.ok ? 'mdi-check-circle-outline' : 'mdi-alert-circle-outline'"
            :color="probe.ok ? 'success' : 'warning'"
            size="small"
          />
        </template>
        <v-list-item-title class="text-caption">{{ probe.label }}</v-list-item-title>
        <v-list-item-subtitle class="text-caption">{{ probe.detail }}</v-list-item-subtitle>
      </v-list-item>
    </v-list>
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
