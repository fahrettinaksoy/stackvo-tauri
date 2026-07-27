<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

const props = defineProps({ modelValue: { type: Boolean, default: false } });
const emit = defineEmits(['update:modelValue', 'created']);

const { t } = useI18n();

const catalog = ref(null);
const busy = ref(false);
const error = ref(null);
const report = ref(null);

const form = ref({
  name: '',
  domain: '',
  runtime: 'php',
  server: 'nginx',
  documentRoot: 'public',
  phpVersion: '',
  extensions: [],
  nodeVersion: '',
  install: 'npm install',
  build: '',
  start: 'npm run dev -- --host 0.0.0.0 --port 3000',
  port: 3000,
});

/** Only runtimes with a generator behind them are selectable (C-02). */
const runtimes = computed(() => catalog.value?.runtimes.filter((r) => r.available) ?? []);
const unavailable = computed(() => catalog.value?.runtimes.filter((r) => !r.available) ?? []);

const phpVersions = computed(
  () => catalog.value?.runtimes.find((r) => r.id === 'php')?.versions ?? []
);
const nodeVersions = computed(
  () => catalog.value?.runtimes.find((r) => r.id === 'node')?.versions ?? []
);

const extensionOptions = computed(() =>
  (catalog.value?.phpExtensions ?? []).map((e) => ({
    value: e.name,
    title: e.name,
    // Flag what cannot build on the chosen version, rather than letting the
    // Docker build discover it minutes later.
    incompatible:
      (e.removedIn && compare(form.value.phpVersion, e.removedIn) >= 0) ||
      (e.minPhp && compare(form.value.phpVersion, e.minPhp) < 0),
  }))
);

const maxExtensions = computed(() => catalog.value?.maxExtensions ?? 50);
const overLimit = computed(() => form.value.extensions.length > maxExtensions.value);

function compare(a, b) {
  const pa = String(a || '0')
    .split('.')
    .map(Number);
  const pb = String(b || '0')
    .split('.')
    .map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d > 0 ? 1 : -1;
  }
  return 0;
}

/** Build the manifest. The payload IS the manifest — nothing is reassembled
 *  server-side, which is precisely what made the web UI's Node path broken. */
function toSpec() {
  const f = form.value;
  const spec = { name: f.name, domain: f.domain || `${f.name}.loc`, runtime: f.runtime };

  if (f.runtime === 'node') {
    spec.node = {
      version: f.nodeVersion,
      install: f.install,
      start: f.start,
      port: Number(f.port),
    };
    if (f.build) spec.node.build = f.build;
  } else {
    spec.server = f.server;
    spec.document_root = f.documentRoot;
    spec.php = { version: f.phpVersion, extensions: [...f.extensions] };
  }
  return spec;
}

async function load() {
  error.value = null;
  report.value = null;
  try {
    catalog.value = await api.catalogGet();
    form.value.phpVersion = catalog.value.runtimes.find((r) => r.id === 'php')?.default ?? '8.4';
    form.value.nodeVersion = catalog.value.runtimes.find((r) => r.id === 'node')?.default ?? '22';
    form.value.server = catalog.value.defaultServer;
    form.value.extensions = catalog.value.phpExtensions
      .filter(
        (e) => e.inDefaultSet && !(e.removedIn && compare(form.value.phpVersion, e.removedIn) >= 0)
      )
      .map((e) => e.name);
  } catch (e) {
    error.value = e;
  }
}

/** Validate before creating, so a bad extension is caught here. */
async function validate() {
  if (!form.value.name) return;
  try {
    report.value = await api.projectValidate(form.value.name, toSpec());
  } catch (e) {
    error.value = e;
  }
}

async function create() {
  busy.value = true;
  error.value = null;
  try {
    await api.projectCreate(toSpec());
    emit('created');
    emit('update:modelValue', false);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

watch(
  () => props.modelValue,
  (open) => open && load()
);
watch(() => [form.value.name, form.value.runtime, form.value.phpVersion], validate);

// Removing an extension that the new PHP version cannot build keeps the form
// honest when the user changes version after picking extensions.
watch(
  () => form.value.phpVersion,
  () => {
    const bad = new Set(extensionOptions.value.filter((o) => o.incompatible).map((o) => o.value));
    form.value.extensions = form.value.extensions.filter((e) => !bad.has(e));
  }
);

const canCreate = computed(
  () => form.value.name && !overLimit.value && report.value?.valid !== false && !busy.value
);
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    max-width="720"
    scrollable
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-item>
        <v-card-title class="text-body-1">{{ t('newProject.title') }}</v-card-title>
      </v-card-item>

      <v-divider />

      <v-card-text>
        <ErrorAlert :error="error" type="error" />

        <v-row dense>
          <v-col cols="12" sm="6">
            <v-text-field v-model="form.name" :label="t('newProject.name')" autofocus />
          </v-col>
          <v-col cols="12" sm="6">
            <v-text-field
              v-model="form.domain"
              :label="t('newProject.domain')"
              :placeholder="form.name ? `${form.name}.loc` : ''"
              persistent-placeholder
            />
          </v-col>

          <v-col cols="12" sm="6">
            <v-select
              v-model="form.runtime"
              :items="runtimes.map((r) => ({ value: r.id, title: r.id }))"
              :label="t('newProject.runtime')"
            />
          </v-col>

          <template v-if="form.runtime === 'php'">
            <v-col cols="12" sm="6">
              <v-select
                v-model="form.phpVersion"
                :items="phpVersions"
                :label="t('newProject.phpVersion')"
              />
            </v-col>
            <v-col cols="12" sm="6">
              <v-select
                v-model="form.server"
                :items="catalog?.servers ?? []"
                :label="t('newProject.server')"
              />
            </v-col>
            <v-col cols="12" sm="6">
              <v-text-field v-model="form.documentRoot" :label="t('newProject.documentRoot')" />
            </v-col>
            <v-col cols="12">
              <v-autocomplete
                v-model="form.extensions"
                :items="extensionOptions"
                item-title="title"
                item-value="value"
                :label="t('newProject.extensions')"
                multiple
                chips
                closable-chips
                variant="outlined"
                density="comfortable"
              >
                <template #item="{ props: itemProps, item }">
                  <v-list-item
                    v-bind="itemProps"
                    :disabled="item.raw.incompatible"
                    :subtitle="item.raw.incompatible ? t('newProject.incompatible') : undefined"
                  />
                </template>
              </v-autocomplete>

              <!-- The manifest cap is a Bash parser limit, not a preference:
                   entry 51 onward is dropped without a word (C-04). -->
              <div
                class="text-caption mt-1"
                :class="overLimit ? 'text-error' : 'text-medium-emphasis'"
              >
                {{ form.extensions.length }} / {{ maxExtensions }}
                <span v-if="overLimit">— {{ t('newProject.tooManyExtensions') }}</span>
              </div>
            </v-col>
          </template>

          <template v-else>
            <v-col cols="12" sm="6">
              <v-select
                v-model="form.nodeVersion"
                :items="nodeVersions"
                :label="t('newProject.nodeVersion')"
              />
            </v-col>
            <v-col cols="12" sm="6">
              <v-text-field v-model="form.port" type="number" :label="t('newProject.port')" />
            </v-col>
            <v-col cols="12">
              <v-text-field v-model="form.install" :label="t('newProject.install')" />
            </v-col>
            <v-col cols="12">
              <v-text-field v-model="form.build" :label="t('newProject.build')" />
            </v-col>
            <v-col cols="12">
              <v-text-field v-model="form.start" :label="t('newProject.start')" />
              <div class="text-caption text-medium-emphasis mt-1">
                {{ t('newProject.bindHint') }}
              </div>
            </v-col>
          </template>
        </v-row>

        <v-alert
          v-if="report && !report.valid"
          type="warning"
          variant="tonal"
          density="compact"
          class="mt-4"
        >
          <div v-for="(issue, i) in report.errors" :key="i" class="text-caption">
            <strong>{{ issue.code }}</strong> {{ issue.path }} — {{ issue.message }}
          </div>
        </v-alert>

        <v-alert
          v-if="unavailable.length"
          type="info"
          variant="tonal"
          density="compact"
          class="mt-4"
        >
          <div class="text-caption">
            {{
              t('newProject.unavailableRuntimes', { list: unavailable.map((r) => r.id).join(', ') })
            }}
          </div>
        </v-alert>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">{{
          t('hosts.cancel')
        }}</v-btn>
        <v-btn
          color="primary"
          variant="flat"
          :loading="busy"
          :disabled="!canCreate"
          @click="create"
        >
          {{ t('newProject.create') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
