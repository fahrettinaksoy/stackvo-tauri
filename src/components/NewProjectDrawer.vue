<script setup>
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { blankForm, formToSpec, isIncompatible, overExtensionLimit } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';
import ProjectFormFields from '@/components/ProjectFormFields.vue';
import SideSheet from '@/components/SideSheet.vue';

const props = defineProps({ modelValue: { type: Boolean, default: false } });
const emit = defineEmits(['update:modelValue', 'created']);

const { t } = useI18n();

const fields = ref(null);

const catalog = ref(null);
const busy = ref(false);
const error = ref(null);
const report = ref(null);

const form = ref(blankForm());

/**
 * `empty` creates a bare project from the form below; a framework template
 * instead runs the framework's own installer in a throwaway container and
 * then adopts the result — detection reads runtime, server and document root
 * from what the installer actually wrote, so the form's runtime fields have
 * nothing to say and are hidden.
 */
const template = ref('empty');
const TEMPLATES = ['empty', 'laravel', 'wordpress', 'symfony', 'nextjs'];
const scaffolding = computed(() => template.value !== 'empty');

const unavailable = computed(() => catalog.value?.runtimes?.filter((r) => !r.available) ?? []);

async function load() {
  error.value = null;
  report.value = null;
  template.value = 'empty';
  form.value = blankForm();
  try {
    catalog.value = await api.catalogGet();
    form.value.phpVersion = catalog.value.runtimes.find((r) => r.id === 'php')?.default ?? '8.4';
    form.value.nodeVersion = catalog.value.runtimes.find((r) => r.id === 'node')?.default ?? '22';
    form.value.server = catalog.value.defaultServer;
    // A new project starts from the default set, minus anything the default
    // PHP version cannot build.
    form.value.extensions = catalog.value.phpExtensions
      .filter((e) => e.inDefaultSet && !isIncompatible(e, form.value.phpVersion))
      .map((e) => e.name);
  } catch (e) {
    error.value = e;
  }
}

/** Validate before creating, so a bad extension is caught here. */
async function validate() {
  if (!form.value.name) return;
  try {
    report.value = await api.projectValidate(form.value.name, formToSpec(form.value));
  } catch (e) {
    error.value = e;
  }
}

async function create() {
  busy.value = true;
  error.value = null;
  try {
    if (scaffolding.value) {
      // Install first, adopt second: adoption is the same detection a git
      // clone gets, so the manifest reflects what the installer wrote.
      await api.projectScaffold(form.value.name, template.value);
      await api.projectAdopt(form.value.name);
    } else {
      await api.projectCreate(formToSpec(form.value));
    }
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
    fields.value?.focusName();
  }
);
watch(() => [form.value.name, form.value.runtime, form.value.phpVersion], validate);

const canCreate = computed(
  () =>
    !!form.value.name &&
    !overExtensionLimit(form.value, catalog.value) &&
    report.value?.valid !== false &&
    !busy.value
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

    <!-- What fills the directory: nothing, or a framework's own installer. -->
    <v-select
      v-model="template"
      :items="TEMPLATES.map((k) => ({ value: k, title: t(`newProject.templates.${k}`) }))"
      :label="t('newProject.template')"
      :hint="scaffolding ? t('newProject.templateHint') : undefined"
      persistent-hint
      density="comfortable"
      variant="outlined"
      class="mb-4"
    />

    <!-- A scaffolded project's runtime, server and document root come from
         detection over what the installer wrote — the form would only let the
         two disagree. Only the name is the user's to choose. -->
    <v-text-field
      v-if="scaffolding"
      v-model="form.name"
      :label="t('newProject.name')"
      :hint="t('newProject.nameHint')"
      persistent-hint
      density="comfortable"
      variant="outlined"
    />
    <ProjectFormFields v-else ref="fields" v-model="form" :catalog="catalog" />

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
