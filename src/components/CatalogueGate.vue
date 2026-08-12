<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The third of the pattern, after requirements and bootstrap.
 *
 * ADR 0011 is why it exists. StackVo ships **no** service definitions inside
 * itself — not a template, not a config, not even a snapshot of the index — so
 * a machine that has never fetched a catalogue does not have an empty one. It
 * has none, and those are different states with different answers.
 *
 * ## Two sentences, not one
 *
 * "No internet" and "this machine has no catalogue yet" get separate wording,
 * because the second one has an answer the first does not: an offline bundle.
 * Collapsing them into "could not reach the catalogue" would hide the whole of
 * what an air-gapped install is supposed to do — and that install is not an
 * enterprise extra here, it is the only path such a machine has.
 *
 * ## It is skippable, and that is not a compromise
 *
 * StackVo without services is still a reverse proxy, a certificate authority
 * and a project runner. Somebody who came to run a Laravel app against a
 * database they already have does not need this screen to be a wall — and a
 * first-run screen that cannot be got past is one people close the app on.
 * Skipping is per session; the Market page carries the same two buttons.
 *
 * ## Nothing here decides where packages come from
 *
 * If a policy names a bundle or a mirror, the buttons still call
 * `market_refresh` and the Rust side ignores the location it was handed. The
 * screen says which one is in force rather than pretending the choice was the
 * user's — a managed machine that looked unmanaged would produce a support call
 * for every refusal.
 */
const { t } = useI18n();
const emit = defineEmits(['done', 'skip']);

/**
 * Where the official catalogue is published.
 *
 * The repository URL rather than a bare host, and that is a correction rather
 * than a preference: the first version of this offered `packages.stackvo.dev`,
 * which does not resolve. A first-run screen whose suggested address cannot
 * work is worse than one with an empty field — it spends the user's first
 * minute on a failure the app authored.
 *
 * Rust translates this to the raw base before fetching, so what is shown here
 * is the address a person recognises and can open in a browser to check.
 */
const OFFICIAL = 'https://github.com/stackvo/stackvo-service-packages';

const status = ref(null);
const policy = ref(null);
const busy = ref(null);
const error = ref(null);
const address = ref(OFFICIAL);

const managed = computed(() => policy.value?.market ?? null);
const bundle = computed(() => managed.value?.offlineBundle ?? null);
const mirror = computed(() => managed.value?.registryUrl ?? null);

/**
 * A machine whose policy already answers the question.
 *
 * Both buttons still work — they go to the same command — but the address field
 * is pointless when the location is overridden, and an editable field that is
 * ignored is worse than none.
 */
const decided = computed(() => !!bundle.value || !!mirror.value);

/**
 * Signatures required and not available is its own state and gets its own
 * sentence. It is not a failure to fetch: it is a refusal to pretend, and until
 * ADR 0015's key exists it is the state a managed machine that asked for
 * verification is permanently in. Saying "could not reach the catalogue" here
 * would send an administrator to look at their network.
 */
const blockedBySignature = computed(() => status.value?.signatureRequired === true);

onMounted(async () => {
  try {
    status.value = await api.marketStatus();
    policy.value = await api.policyStatus();
    if (status.value?.fetched) emit('done');
  } catch (e) {
    error.value = e;
  }
});

async function fetchFrom(location) {
  busy.value = location;
  error.value = null;
  try {
    status.value = await api.marketRefresh(location);
    if (status.value?.fetched) emit('done');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

const fromNetwork = () => fetchFrom(mirror.value ?? address.value);

async function fromDirectory() {
  // Even when a policy names the bundle, the picker is offered: the command
  // ignores what it is handed in that case, and a disabled button on a screen
  // that says "use a bundle" reads as broken rather than as managed.
  const chosen = bundle.value ?? (await open({ directory: true, multiple: false }));
  if (typeof chosen === 'string') await fetchFrom(chosen);
}
</script>

<template>
  <v-container class="py-12" style="max-width: 720px">
    <div class="text-center mb-8">
      <v-icon size="64" color="primary" icon="mdi-package-variant-closed" class="mb-4" />
      <h1 class="text-h5 mb-2">{{ t('catalogueGate.title') }}</h1>
      <p class="text-body-2 text-medium-emphasis">
        {{ t('catalogueGate.body') }}
      </p>
    </div>

    <ErrorAlert :error="error" class="mb-4" />

    <!-- Required and impossible. Its own sentence, because it is not a network
         problem and looking for one is the wrong afternoon. -->
    <v-alert v-if="blockedBySignature" type="warning" variant="tonal" class="mb-4">
      {{ t('catalogueGate.signatureRequired') }}
    </v-alert>

    <!-- What an administrator already decided, said out loud. A managed machine
         that looked unmanaged would produce a support call per refusal. -->
    <v-alert v-if="decided" type="info" variant="tonal" density="compact" class="mb-4">
      {{
        bundle
          ? t('catalogueGate.policyBundle', { path: bundle })
          : t('catalogueGate.policyMirror', { url: mirror })
      }}
    </v-alert>

    <v-card variant="outlined" class="mb-4">
      <v-card-item>
        <v-card-title class="text-subtitle-1">{{ t('catalogueGate.online') }}</v-card-title>
        <v-card-subtitle>{{ t('catalogueGate.onlineBody') }}</v-card-subtitle>
      </v-card-item>
      <v-card-text>
        <v-text-field
          v-if="!decided"
          v-model="address"
          :label="t('catalogueGate.address')"
          density="compact"
          variant="outlined"
          hide-details
          class="mb-3"
        />
        <v-btn
          color="primary"
          variant="flat"
          prepend-icon="mdi-cloud-download-outline"
          :loading="!!busy"
          :disabled="blockedBySignature"
          @click="fromNetwork"
        >
          {{ t('catalogueGate.fetch') }}
        </v-btn>
      </v-card-text>
    </v-card>

    <v-card variant="outlined" class="mb-6">
      <v-card-item>
        <v-card-title class="text-subtitle-1">{{ t('catalogueGate.offline') }}</v-card-title>
        <v-card-subtitle>{{ t('catalogueGate.offlineBody') }}</v-card-subtitle>
      </v-card-item>
      <v-card-text>
        <v-btn
          variant="tonal"
          prepend-icon="mdi-folder-search-outline"
          :loading="!!busy"
          @click="fromDirectory"
        >
          {{ t('catalogueGate.choose') }}
        </v-btn>
      </v-card-text>
    </v-card>

    <div class="text-center">
      <v-btn variant="text" size="small" @click="emit('skip')">
        {{ t('catalogueGate.skip') }}
      </v-btn>
      <div class="text-caption text-medium-emphasis mt-1">
        {{ t('catalogueGate.skipHint') }}
      </div>
    </div>
  </v-container>
</template>
