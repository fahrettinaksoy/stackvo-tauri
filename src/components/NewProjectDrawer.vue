<script setup>
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';
import SideSheet from '@/components/SideSheet.vue';

const props = defineProps({ modelValue: { type: Boolean, default: false } });
const emit = defineEmits(['update:modelValue', 'created']);

const { t } = useI18n();

const nameField = ref(null);

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
const runtimes = computed(() => catalog.value?.runtimes?.filter((r) => r.available) ?? []);
const unavailable = computed(() => catalog.value?.runtimes?.filter((r) => !r.available) ?? []);

const phpVersions = computed(
  () => catalog.value?.runtimes?.find((r) => r.id === 'php')?.versions ?? []
);
const nodeVersions = computed(
  () => catalog.value?.runtimes?.find((r) => r.id === 'node')?.versions ?? []
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

function close() {
  emit('update:modelValue', false);
}

watch(
  () => props.modelValue,
  async (open) => {
    if (!open) return;
    load();

    // Focused here rather than with `autofocus`: a drawer keeps its content in
    // the DOM whether it is open or shut, so the attribute would fire once at
    // app start and steal the caret from whatever the user was doing.
    await nextTick();
    nameField.value?.focus();
  }
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
  <!-- A side sheet rather than a dialog: this form is long enough to scroll on
       a laptop, and a panel beside the project list keeps the list it is about
       visible instead of covering it with a floating card. -->
  <SideSheet
    :model-value="modelValue"
    :title="t('newProject.title')"
    icon="mdi-folder-plus-outline"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <ErrorAlert :error="error" type="error" class="mb-4" />

    <!-- One field per row.
           The two-column grid came from the dialog, which was 720px wide and
           had room for it. In a 560px panel each column was a ~250px box, and
           a form whose fields are half as wide as the label they carry reads as
           cramped rather than compact. A single column also makes the tab order
           and the reading order the same thing. -->
    <div class="fields">
      <div class="sheet-group">{{ t('newProject.sectionProject') }}</div>

      <v-text-field
        ref="nameField"
        v-model="form.name"
        :label="t('newProject.name')"
        prepend-inner-icon="mdi-folder-outline"
        :hint="t('newProject.nameHint')"
        persistent-hint
      />
      <v-text-field
        v-model="form.domain"
        :label="t('newProject.domain')"
        :placeholder="form.name ? `${form.name}.loc` : ''"
        persistent-placeholder
        prepend-inner-icon="mdi-web"
        :hint="t('newProject.domainHint')"
        persistent-hint
      />
      <v-select
        v-model="form.runtime"
        :items="runtimes.map((r) => ({ value: r.id, title: r.id }))"
        :label="t('newProject.runtime')"
        prepend-inner-icon="mdi-code-braces"
      />

      <template v-if="form.runtime === 'php'">
        <div class="sheet-group">{{ t('newProject.sectionPhp') }}</div>

        <v-select
          v-model="form.phpVersion"
          :items="phpVersions"
          :label="t('newProject.phpVersion')"
          prepend-inner-icon="mdi-tag-outline"
        />
        <v-select
          v-model="form.server"
          :items="catalog?.servers ?? []"
          :label="t('newProject.server')"
          prepend-inner-icon="mdi-server"
        />
        <v-text-field
          v-model="form.documentRoot"
          :label="t('newProject.documentRoot')"
          prepend-inner-icon="mdi-folder-outline"
          :hint="t('newProject.documentRootHint')"
          persistent-hint
        />

        <div>
          <v-autocomplete
            v-model="form.extensions"
            :items="extensionOptions"
            item-title="title"
            item-value="value"
            :label="t('newProject.extensions')"
            prepend-inner-icon="mdi-puzzle-outline"
            multiple
            chips
            closable-chips
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
          <div class="text-caption mt-1" :class="overLimit ? 'text-error' : 'text-medium-emphasis'">
            {{ form.extensions.length }} / {{ maxExtensions }}
            <span v-if="overLimit">— {{ t('newProject.tooManyExtensions') }}</span>
          </div>
        </div>
      </template>

      <template v-else>
        <div class="sheet-group">{{ t('newProject.sectionNode') }}</div>

        <v-select
          v-model="form.nodeVersion"
          :items="nodeVersions"
          :label="t('newProject.nodeVersion')"
          prepend-inner-icon="mdi-tag-outline"
        />
        <v-text-field
          v-model="form.port"
          type="number"
          :label="t('newProject.port')"
          prepend-inner-icon="mdi-lan-connect"
          :hint="t('newProject.portHint')"
          persistent-hint
        />
        <v-text-field
          v-model="form.install"
          :label="t('newProject.install')"
          prepend-inner-icon="mdi-download-outline"
        />
        <v-text-field
          v-model="form.build"
          :label="t('newProject.build')"
          prepend-inner-icon="mdi-hammer-wrench"
        />

        <div>
          <v-text-field
            v-model="form.start"
            :label="t('newProject.start')"
            prepend-inner-icon="mdi-play-outline"
            :hint="t('newProject.bindHint')"
            persistent-hint
          />
        </div>
      </template>
    </div>

    <v-alert v-if="report && !report.valid" type="warning" variant="tonal" class="mt-5">
      <div v-for="(issue, i) in report.errors" :key="i" class="text-caption">
        <strong>{{ issue.code }}</strong> {{ issue.path }} — {{ issue.message }}
      </div>
    </v-alert>

    <v-alert v-if="unavailable.length" type="info" variant="tonal" class="mt-5">
      <div class="text-caption">
        {{ t('newProject.unavailableRuntimes', { list: unavailable.map((r) => r.id).join(', ') }) }}
      </div>
    </v-alert>

    <template #footer>
      <v-btn variant="text" @click="close">{{ t('hosts.cancel') }}</v-btn>
      <v-btn color="primary" variant="flat" :loading="busy" :disabled="!canCreate" @click="create">
        {{ t('newProject.create') }}
      </v-btn>
    </template>
  </SideSheet>
</template>

<style scoped>
/* One column, one rhythm. */
.fields {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
