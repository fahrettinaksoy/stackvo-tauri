<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Where service packages come from, in the place people look for a setting.
 *
 * It was reachable from exactly two screens and neither was this one: the
 * first-run gate, which a user sees once and may skip, and the Market page,
 * whose only control opened a **folder picker** — so a person with a URL had
 * no way in at all after the first launch. "Where does this fetch from" is a
 * setting; it belongs where the settings are.
 *
 * ## Test is a separate act from use, and stays separate
 *
 * `market_probe` fetches into a scratch directory and throws it away. Nothing
 * is cached, nothing is remembered, and a refresh still has to be pressed. A
 * test that quietly became the change would be the same button twice with
 * different words on it — and the one thing somebody testing an address wants
 * is to find out *without* committing to it.
 *
 * ## What it reports is what would happen, not whether the server answered
 *
 * An index older than the cached one is a successful fetch and a refusal:
 * `market::refresh` will not go backwards, because that is how a withdrawn
 * version comes back (T-6). So the probe reports `goesBackwards` as a fact and
 * this pane says so before the button is pressed rather than after.
 *
 * ## The translated address is shown, always
 *
 * A GitHub repository URL is not where files are served from, and the app
 * rewrites it. Showing only the typed address would make a working setup look
 * like it was fetching from a page; showing only the resolved one would answer
 * a question nobody asked. Both, whenever they differ.
 */
const { t } = useI18n();

const status = ref(null);
const policy = ref(null);
const probe = ref(null);
const busy = ref(null);
const error = ref(null);
const address = ref('');

const managed = computed(() => policy.value?.market ?? null);
const bundle = computed(() => managed.value?.offlineBundle ?? null);
const mirror = computed(() => managed.value?.registryUrl ?? null);
/** A policy names the source, so the field below cannot decide it. */
const decided = computed(() => !!bundle.value || !!mirror.value);

const current = computed(() => status.value?.sourceLocation ?? null);
const fetched = computed(() => status.value?.fetched === true);

/** Typed and resolved differ only when a repository URL was translated. */
const translated = computed(
  () => !!probe.value && probe.value.resolved !== probe.value.location.trim().replace(/\/+$/, '')
);

async function load() {
  error.value = null;
  try {
    status.value = await api.marketStatus();
    policy.value = await api.policyStatus();
    // Seeded with what is in force, so the common edit is a correction rather
    // than retyping an address from memory.
    address.value = bundle.value ?? mirror.value ?? current.value ?? '';
  } catch (e) {
    error.value = e;
  }
}

onMounted(load);

async function test() {
  if (!address.value) return;
  busy.value = 'probe';
  error.value = null;
  probe.value = null;
  try {
    probe.value = await api.marketProbe(address.value);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

async function use() {
  busy.value = 'refresh';
  error.value = null;
  try {
    status.value = await api.marketRefresh(address.value);
    probe.value = null;
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

async function pickFolder() {
  const chosen = await open({ directory: true, multiple: false });
  if (typeof chosen === 'string') {
    address.value = chosen;
    // Straight into a test. Picking a folder is already the deliberate act;
    // making somebody press Test afterwards is a second one for no answer.
    await test();
  }
}
</script>

<template>
  <div>
    <ErrorAlert :error="error" class="mb-4" />

    <!-- What is in force now. Absent is a state of its own (ADR 0011): nothing
         is embedded, so a machine that has never fetched has no catalogue at
         all rather than an empty one. -->
    <v-alert :type="fetched ? 'success' : 'info'" variant="tonal" density="compact" class="mb-4">
      <template v-if="fetched">
        {{
          t('catalogueSettings.current', {
            location: current ?? '—',
            packages: status.packages,
            installed: status.installed,
          })
        }}
      </template>
      <template v-else>{{ t('catalogueSettings.none') }}</template>
    </v-alert>

    <v-alert v-if="decided" type="info" variant="tonal" density="compact" class="mb-4">
      {{
        bundle
          ? t('catalogueSettings.policyBundle', { path: bundle })
          : t('catalogueSettings.policyMirror', { url: mirror })
      }}
    </v-alert>

    <v-alert
      v-if="status?.signatureRequired"
      type="warning"
      variant="tonal"
      density="compact"
      class="mb-4"
    >
      {{ t('catalogueSettings.signatureRequired') }}
    </v-alert>

    <v-text-field
      v-model="address"
      :label="t('catalogueSettings.address')"
      :hint="t('catalogueSettings.addressHint')"
      :disabled="decided"
      persistent-hint
      density="compact"
      variant="outlined"
      class="mb-3"
    />

    <div class="d-flex ga-2 mb-4">
      <v-btn
        variant="tonal"
        prepend-icon="mdi-check-network-outline"
        :loading="busy === 'probe'"
        :disabled="!address || !!busy"
        @click="test"
      >
        {{ t('catalogueSettings.test') }}
      </v-btn>
      <v-btn
        variant="text"
        prepend-icon="mdi-folder-search-outline"
        :disabled="!!busy"
        @click="pickFolder"
      >
        {{ t('catalogueSettings.pickFolder') }}
      </v-btn>
      <v-spacer />
      <v-btn
        color="primary"
        variant="flat"
        prepend-icon="mdi-cloud-download-outline"
        :loading="busy === 'refresh'"
        :disabled="!address || !!busy || status?.signatureRequired"
        @click="use"
      >
        {{ t('catalogueSettings.use') }}
      </v-btn>
    </div>

    <!-- The answer. Reachable-but-refused is its own row rather than a failure,
         because the server did answer and the refusal is this app's. -->
    <v-alert
      v-if="probe"
      :type="!probe.reachable ? 'error' : probe.goesBackwards ? 'warning' : 'success'"
      variant="tonal"
      density="compact"
    >
      <div v-if="!probe.reachable">
        <div class="text-subtitle-2 mb-1">{{ t('catalogueSettings.failed') }}</div>
        <div class="text-caption">{{ probe.error }}</div>
        <div v-if="probe.hintKey" class="text-caption mt-1">
          {{ t(`errorHints.${probe.hintKey}`) }}
        </div>
      </div>
      <div v-else>
        <div class="text-subtitle-2 mb-1">
          {{
            t('catalogueSettings.ok', {
              packages: probe.packages,
              versions: probe.versions,
              sequence: probe.sequence,
            })
          }}
        </div>
        <div v-if="probe.goesBackwards" class="text-caption">
          {{
            t('catalogueSettings.backwards', {
              sequence: probe.sequence,
              current: probe.currentSequence,
            })
          }}
        </div>
      </div>

      <!-- Shown whenever the app fetched from somewhere other than what was
           typed, which is every GitHub repository URL. -->
      <div v-if="translated" class="text-caption mt-2 font-mono">
        {{ t('catalogueSettings.resolved', { url: probe.resolved }) }}
      </div>
    </v-alert>
  </div>
</template>
