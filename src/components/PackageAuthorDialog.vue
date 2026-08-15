<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Writing a service package, and re-sealing it after an edit (C-1).
 *
 * ## Why this is three buttons and not an editor
 *
 * Everything a package needs already existed: the format, the validator, the
 * compose policy, and a `local` source that installs from any directory. What
 * stopped anybody writing one is that the manifest states the sha256 of every
 * file it ships and the app checks them on every read — so editing the compose
 * fragment leaves a package that refuses to load, complaining about bytes
 * rather than about the line just typed. A person can compute those by hand
 * once; nobody does it twice.
 *
 * So the surface is create, check, seal. The files themselves are edited in
 * whatever the user already uses, which is the same reasoning `quickcmd` gives
 * for opening their terminal rather than shipping a worse one.
 *
 * ## `problems` and `resealed` are two different answers
 *
 * `resealed` is bookkeeping the tool fixed. `problems` is everything it
 * refused, and a report with any means **nothing was written** — the manifest
 * still describes the old bytes, so nothing downstream believes a package is
 * intact when it is not.
 */
const model = defineModel({ type: Boolean, default: false });

const { t } = useI18n();

const category = ref('databases');
const service = ref('');
const version = ref('');
const image = ref('');

const report = ref(null);
const error = ref(null);
const busy = ref(false);

/** The categories the package tree is laid out by. */
const CATEGORIES = [
  'databases',
  'cache',
  'queue',
  'search',
  'storage',
  'monitoring',
  'devtools',
  'admin-uis',
];

const named = computed(() => !!service.value && !!version.value);
const problems = computed(() => asList(report.value?.problems));
const resealed = computed(() => asList(report.value?.resealed));

watch(model, (open) => {
  if (!open) return;
  report.value = null;
  error.value = null;
});

async function run(fn) {
  busy.value = true;
  error.value = null;
  try {
    report.value = await fn();
  } catch (e) {
    error.value = e;
    report.value = null;
  } finally {
    busy.value = false;
  }
}

const create = () =>
  run(() => api.packageScaffold(category.value, service.value, version.value, image.value));
const check = () => run(() => api.packageLint(category.value, service.value, version.value));
const seal = () => run(() => api.packageSeal(category.value, service.value, version.value));
</script>

<template>
  <v-dialog v-model="model" max-width="640" scrollable>
    <v-card class="pa-4">
      <div class="section-head mb-1">
        <v-icon size="18" class="mr-2">mdi-package-variant-plus</v-icon>{{ t('authoring.title') }}
      </div>
      <p class="text-caption text-medium-emphasis mb-4">{{ t('authoring.explain') }}</p>

      <ErrorAlert v-if="error" :error="error" class="mb-3" />

      <div class="d-flex ga-2 mb-2">
        <v-select
          v-model="category"
          :items="CATEGORIES"
          :label="t('authoring.category')"
          density="compact"
          variant="outlined"
          hide-details
        />
        <v-text-field
          v-model="service"
          :label="t('authoring.service')"
          placeholder="widget"
          density="compact"
          variant="outlined"
          hide-details
        />
        <v-text-field
          v-model="version"
          :label="t('authoring.version')"
          placeholder="1.0"
          density="compact"
          variant="outlined"
          hide-details
        />
      </div>

      <!-- Only creating needs it: sealing and checking read the manifest, which
           already says which image the package runs. -->
      <v-text-field
        v-model="image"
        :label="t('authoring.image')"
        :hint="t('authoring.imageHint')"
        persistent-hint
        placeholder="widget:1.0"
        density="compact"
        variant="outlined"
        class="mb-4"
      />

      <div class="d-flex ga-2 mb-4">
        <v-btn
          color="primary"
          variant="flat"
          size="small"
          :loading="busy"
          :disabled="!named || !image"
          @click="create"
        >
          {{ t('authoring.create') }}
        </v-btn>
        <v-spacer />
        <v-btn variant="text" size="small" :loading="busy" :disabled="!named" @click="check">
          {{ t('authoring.check') }}
        </v-btn>
        <v-btn variant="tonal" size="small" :loading="busy" :disabled="!named" @click="seal">
          {{ t('authoring.seal') }}
        </v-btn>
      </div>

      <!-- Refusals first and loudest: a report with any means nothing was
           written, which is the fact that decides what to do next. -->
      <v-alert v-if="problems.length" type="error" variant="tonal" density="compact" class="mb-3">
        <div class="text-caption mb-1">{{ t('authoring.refused') }}</div>
        <ul class="pl-4">
          <li v-for="(problem, i) in problems" :key="i" class="text-caption">{{ problem }}</li>
        </ul>
      </v-alert>

      <template v-else-if="report">
        <v-alert type="success" variant="tonal" density="compact" class="mb-2">
          <div class="text-caption">
            {{ t('authoring.valid', { service: report.service, version: report.version }) }}
          </div>
        </v-alert>
        <div v-if="resealed.length" class="text-caption text-medium-emphasis mb-2">
          {{ t('authoring.resealed', { files: resealed.join(', ') }) }}
        </div>
        <!-- The path, because the next step is editing the files in whatever
             they already use. -->
        <code class="text-caption">{{ report.dir }}</code>
      </template>

      <div class="d-flex mt-4">
        <v-spacer />
        <v-btn variant="text" @click="model = false">{{ t('app.close') }}</v-btn>
      </div>
    </v-card>
  </v-dialog>
</template>
