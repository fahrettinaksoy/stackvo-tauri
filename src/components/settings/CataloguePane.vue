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

/**
 * Write an offline bundle (§3 #31).
 *
 * Here rather than on the Market page, and for the same reason the address
 * field is: "where do packages come from" is a setting, and producing a bundle
 * is the other end of that one question. The Market page is about which
 * services exist.
 *
 * It is a folder picker and no text field, deliberately. The destination has
 * to be empty and `market_bundle` refuses one that is not — a typed path is
 * the way to discover that by having it refused, and a picker is the way to
 * make a new folder while choosing.
 */
const bundled = ref(null);

async function writeBundle() {
  const chosen = await open({ directory: true, multiple: false });
  if (typeof chosen !== 'string') return;

  busy.value = 'bundle';
  error.value = null;
  bundled.value = null;
  try {
    bundled.value = await api.marketBundle(chosen);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

/** Whole mebibytes, one decimal — the number answers "will this fit". */
const bundleSize = computed(() =>
  bundled.value ? `${(bundled.value.bytes / (1024 * 1024)).toFixed(1)} MiB` : ''
);
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

    <!-- The other end of the same question: getting this catalogue to a
         machine that cannot fetch one (§3 #31). ADR 0011 makes this the only
         way such a machine ever has services at all, which is why it is a
         section here and not an advanced menu. -->
    <v-divider class="my-6" />

    <div class="text-subtitle-2 mb-1">{{ t('catalogueSettings.bundleTitle') }}</div>
    <div class="text-caption text-medium-emphasis mb-3">
      {{ t('catalogueSettings.bundleWhat') }}
    </div>

    <v-btn
      variant="tonal"
      prepend-icon="mdi-package-variant-closed"
      :loading="busy === 'bundle'"
      :disabled="!fetched || !!busy"
      @click="writeBundle"
    >
      {{ t('catalogueSettings.bundleAction') }}
    </v-btn>

    <!-- Why the button is off, rather than a button that does nothing. A
         bundle is a copy of the catalogue this machine fetched, so there has
         to be one. -->
    <div v-if="!fetched" class="text-caption text-medium-emphasis mt-2">
      {{ t('catalogueSettings.bundleNeedsCatalogue') }}
    </div>

    <v-alert
      v-if="bundled"
      :type="bundled.signed ? 'success' : 'warning'"
      variant="tonal"
      density="compact"
      class="mt-3"
    >
      <div class="text-subtitle-2 mb-1">
        {{
          t('catalogueSettings.bundleDone', {
            packages: bundled.packages,
            versions: bundled.versions,
            files: bundled.files,
            size: bundleSize,
          })
        }}
      </div>

      <!-- Both of these are facts the person carrying the bundle needs before
           they walk away from the network, so they are on the screen rather
           than in a field somebody may not open. -->
      <div v-if="!bundled.signed" class="text-caption">
        {{ t('catalogueSettings.bundleUnsigned') }}
      </div>
      <div v-if="bundled.skipped?.length" class="text-caption mt-1">
        {{ t('catalogueSettings.bundleSkipped') }}
        <ul class="pl-4">
          <li v-for="line in bundled.skipped" :key="line">{{ line }}</li>
        </ul>
      </div>

      <div class="text-caption mt-2">{{ t('catalogueSettings.bundleNext') }}</div>
    </v-alert>
  </div>
</template>
