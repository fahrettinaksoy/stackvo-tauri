<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import { formFromManifest, formToSpec, overExtensionLimit, specsDiffer } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';
import ProjectFormFields from '@/components/ProjectFormFields.vue';
import SideSheet from '@/components/SideSheet.vue';

/**
 * Editing a project's manifest as a form.
 *
 * Every field here could already be set when the project was created and never
 * again — the only way to change a PHP version or add an extension afterwards
 * was to hand-edit `stackvo.json` in the raw JSON pane, against a contract with
 * write rules a text area cannot enforce.
 *
 * The one thing this sheet must not do is imply more than it did. Writing the
 * manifest changes a file; it does not change the image the project runs from,
 * because the Dockerfile compose builds is generated *from* that file and is
 * still the old one. So the sheet says so, and offers the two commands that
 * close the gap rather than leaving the user to find them.
 */

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  name: { type: String, required: true },
});
const emit = defineEmits(['update:modelValue', 'saved', 'apply']);

const { t } = useI18n();
const app = useAppStore();

const catalog = ref(null);
const form = ref(null);
/** The spec as it was on disk, so Save can tell an edit from a re-render. */
const original = ref(null);
const report = ref(null);
const error = ref(null);
const loading = ref(false);
const busy = ref(false);
/** Set once the manifest is written, and cleared by applying or reopening. */
const applyPending = ref(false);

const spec = computed(() => (form.value ? formToSpec(form.value) : null));
const dirty = computed(() => !!original.value && specsDiffer(original.value, spec.value));

/**
 * The domain is not only a manifest field: it is a hosts entry and a name on
 * the wildcard certificate. Changing it here leaves both pointing at the old
 * one, and the project answers on neither until they catch up.
 */
const domainChanged = computed(
  () => !!original.value && !!spec.value && original.value.domain !== spec.value.domain
);

const canSave = computed(
  () =>
    dirty.value &&
    !busy.value &&
    !overExtensionLimit(form.value, catalog.value) &&
    report.value?.valid !== false
);

async function load() {
  loading.value = true;
  error.value = null;
  report.value = null;
  applyPending.value = false;
  try {
    const [cat, manifest] = await Promise.all([
      api.catalogGet(),
      api.projectManifestRead(props.name),
    ]);
    catalog.value = cat;
    form.value = formFromManifest(manifest);
    original.value = formToSpec(form.value);
  } catch (e) {
    error.value = e;
    form.value = null;
  } finally {
    loading.value = false;
  }
}

/**
 * Validate through the same command that guards a write, so the sheet cannot
 * disagree with the thing that will reject it.
 */
async function validate() {
  if (!spec.value) return;
  try {
    report.value = await api.projectValidate(props.name, spec.value);
  } catch (e) {
    error.value = e;
  }
}

async function save() {
  busy.value = true;
  error.value = null;
  try {
    await api.projectManifestWrite(props.name, spec.value);
    original.value = spec.value;
    applyPending.value = true;
    emit('saved');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

/**
 * Hand the apply to the page and get out of the way.
 *
 * Regenerating and rebuilding is a Docker build that runs for minutes and
 * streams its output to the operations console. Awaiting it here would hold a
 * spinner in a panel that covers the very console reporting the progress, so
 * the page runs it — that is what its operation machinery is for — and this
 * closes.
 */
function requestApply() {
  applyPending.value = false;
  emit('apply');
  emit('update:modelValue', false);
}

async function saveAndApply() {
  await save();
  if (!error.value) requestApply();
}

function close() {
  emit('update:modelValue', false);
}

watch(
  () => [props.modelValue, props.name],
  ([open]) => {
    if (open) load();
  },
  { immediate: true }
);

// Validated on every change rather than on save: an extension that cannot
// build against the chosen PHP version is worth knowing before the button is
// pressed, not after.
watch(spec, (value, before) => {
  if (value && before && specsDiffer(value, before)) validate();
});
</script>

<template>
  <SideSheet
    :model-value="modelValue"
    :title="t('projectSettings.title', { name })"
    icon="mdi-tune-variant"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <ErrorAlert :error="error" type="error" class="mb-4" />

    <div v-if="loading" class="py-8 text-center">
      <v-progress-circular indeterminate size="24" />
    </div>

    <template v-else-if="form">
      <ProjectFormFields v-model="form" :catalog="catalog" lock-name />

      <v-alert v-if="report && !report.valid" type="warning" variant="tonal" class="mt-5">
        <div v-for="(issue, i) in report.errors" :key="i" class="text-caption">
          <strong>{{ issue.code }}</strong> {{ issue.path }} — {{ issue.message }}
        </div>
      </v-alert>

      <v-alert v-if="domainChanged" type="info" variant="tonal" class="mt-5">
        <div class="text-caption">{{ t('projectSettings.domainChanged') }}</div>
      </v-alert>

      <!-- The manifest is written; the image is not. Said here because the
           alternative is a Save that appears to have worked and a project that
           still runs the old configuration. -->
      <v-alert v-if="applyPending" type="warning" variant="tonal" class="mt-5">
        <div class="d-flex align-center ga-2">
          <span class="text-caption">{{ t('projectSettings.applyPending') }}</span>
          <v-spacer />
          <v-btn
            size="x-small"
            variant="tonal"
            :disabled="!app.engineUp || busy"
            @click="requestApply"
            >{{ t('projectSettings.applyNow') }}</v-btn
          >
        </div>
      </v-alert>
    </template>

    <template #footer>
      <v-btn variant="text" @click="close">{{ t('hosts.cancel') }}</v-btn>
      <v-btn variant="text" :disabled="!canSave" :loading="busy" @click="save">
        {{ t('detail.save') }}
      </v-btn>
      <!-- Disabled with the engine down rather than hidden: the reason is worth
           reading, and the plain Save beside it still works. -->
      <v-btn
        color="primary"
        variant="flat"
        :disabled="!canSave || !app.engineUp"
        :loading="busy"
        @click="saveAndApply"
      >
        {{ t('projectSettings.saveAndApply') }}
        <v-tooltip v-if="!app.engineUp" activator="parent">{{
          t('projectSettings.engineDown')
        }}</v-tooltip>
      </v-btn>
    </template>
  </SideSheet>
</template>
