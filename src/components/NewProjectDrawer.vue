<script setup>
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import { blankForm, formToSpec, isIncompatible, overExtensionLimit } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';
import ProjectFormFields from '@/components/ProjectFormFields.vue';
import SideSheet from '@/components/SideSheet.vue';

const props = defineProps({ modelValue: { type: Boolean, default: false } });
const emit = defineEmits(['update:modelValue', 'created']);

const { t } = useI18n();
const app = useAppStore();

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
/**
 * Which group is expanded. Twenty-eight templates in one column was a scroll
 * with no landmarks; as an accordion the panel opens to six headings, and the
 * chosen one is named on its heading so a closed panel still answers "what did
 * I pick".
 */
const openGroup = ref(null);
/**
 * Grouped, because the list stopped being short. The heading is the runtime
 * the choice implies — picking Nuxt is picking node, and a flat list of nine
 * makes that something you work out from the names.
 */
const TEMPLATE_GROUPS = [
  { key: 'blank', items: ['empty'] },
  {
    key: 'php',
    items: ['laravel', 'symfony', 'cakephp', 'yii', 'codeigniter', 'laminas', 'slim'],
  },
  { key: 'cms', items: ['wordpress', 'drupal', 'prestashop', 'typo3', 'tina'] },
  {
    key: 'node',
    items: ['nextjs', 'nuxt', 'vue', 'react', 'svelte', 'astro', 'nest', 'angular'],
  },
  { key: 'python', items: ['django', 'fastapi', 'flask'] },
  { key: 'go', items: ['gin', 'echo'] },
  { key: 'other', items: ['rails', 'sinatra', 'rocket'] },
];
/**
 * Which runtime each template installs.
 *
 * Named per template rather than per group because the groups are about where
 * a user looks for a thing, not what runs it: `cms` holds four PHP frameworks
 * and TinaCMS, which is Node. What this decides is which fields the form can
 * usefully ask for — a PHP template gets the version/server/extension choice,
 * everything else is read back off what its installer wrote.
 */
const TEMPLATE_RUNTIME = {
  laravel: 'php',
  symfony: 'php',
  cakephp: 'php',
  yii: 'php',
  codeigniter: 'php',
  laminas: 'php',
  slim: 'php',
  wordpress: 'php',
  drupal: 'php',
  prestashop: 'php',
  typo3: 'php',
  tina: 'node',
  nextjs: 'node',
  nuxt: 'node',
  vue: 'node',
  react: 'node',
  svelte: 'node',
  astro: 'node',
  nest: 'node',
  angular: 'node',
  django: 'python',
  fastapi: 'python',
  flask: 'python',
  gin: 'go',
  echo: 'go',
  rails: 'ruby',
  sinatra: 'ruby',
  rocket: 'rust',
};
const TEMPLATE_ICONS = {
  empty: 'mdi-folder-outline',
  laravel: 'mdi-laravel',
  wordpress: 'mdi-wordpress',
  symfony: 'mdi-alpha-s-box-outline',
  nextjs: 'mdi-triangle-outline',
  nuxt: 'mdi-alpha-n-circle-outline',
  vue: 'mdi-vuejs',
  react: 'mdi-react',
  svelte: 'mdi-alpha-s-circle-outline',
  astro: 'mdi-rocket-launch-outline',
  cakephp: 'mdi-cake-variant-outline',
  yii: 'mdi-alpha-y-box-outline',
  codeigniter: 'mdi-fire',
  laminas: 'mdi-alpha-l-box-outline',
  drupal: 'mdi-water',
  prestashop: 'mdi-cart-outline',
  django: 'mdi-language-python',
  rails: 'mdi-language-ruby',
  slim: 'mdi-alpha-s-circle-outline',
  nest: 'mdi-hexagon-outline',
  tina: 'mdi-note-edit-outline',
  angular: 'mdi-angular',
  typo3: 'mdi-alpha-t-box-outline',
  gin: 'mdi-glass-cocktail',
  echo: 'mdi-radio-tower',
  flask: 'mdi-flask-outline',
  fastapi: 'mdi-lightning-bolt-outline',
  sinatra: 'mdi-microphone-variant',
  rocket: 'mdi-rocket-outline',
};
const scaffolding = computed(() => template.value !== 'empty');

/** Everything except the empty project, which is presented on its own. */
const FRAMEWORK_GROUPS = TEMPLATE_GROUPS.filter((g) => g.key !== 'blank');

/** Pick a template and keep its group open, so the choice stays in view. */
function choose(key) {
  template.value = key;
  openGroup.value = FRAMEWORK_GROUPS.find((g) => g.items.includes(key))?.key ?? null;
  // The template IS the runtime, so the form follows it rather than offering a
  // second, contradictable answer. It decides which fields are worth asking
  // for: PHP templates get version/server/extensions, the rest get neither.
  form.value.runtime = TEMPLATE_RUNTIME[key] ?? 'php';
}

/** A scaffolded PHP project still has three choices its installer cannot make. */
const phpScaffold = computed(() => scaffolding.value && form.value.runtime === 'php');

const unavailable = computed(() => catalog.value?.runtimes?.filter((r) => !r.available) ?? []);

async function load() {
  error.value = null;
  report.value = null;
  template.value = 'empty';
  openGroup.value = null;
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
  // A scaffold's manifest is written by adoption from what the installer left
  // on disk, so the only part of this form that reaches it is the PHP
  // override. For every other template the spec below is a fabrication, and
  // findings about a document root nobody will write are noise.
  if (scaffolding.value && !phpScaffold.value) {
    report.value = null;
    return;
  }
  try {
    report.value = await api.projectValidate(form.value.name, formToSpec(form.value, app.tld));
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
      // Detection fills the runtime and the document root from what the
      // installer wrote. The rest it cannot know: a domain is a choice, and a
      // `composer.json` states the PHP floor the framework needs rather than
      // the version to run it on — read as an answer, `"php": "^8.3"` puts a
      // brand-new Laravel on 8.3. Those ride along as overrides.
      await api.projectAdopt(form.value.name, null, {
        domain: form.value.domain || null,
        ...(phpScaffold.value
          ? {
              phpVersion: form.value.phpVersion,
              server: form.value.server,
              extensions: [...form.value.extensions],
            }
          : {}),
      });
    } else {
      await api.projectCreate(formToSpec(form.value, app.tld));
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
    :width="920"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <ErrorAlert :error="error" type="error" class="mb-4" />

    <!-- Two columns, because these are two different kinds of decision. What
         fills the directory is a branch — it changes which fields even apply —
         and four branches hidden inside a select is a menu you have to open to
         learn what the panel can do. The form is the work; the choice sits
         beside it, visible, and the two stack on a narrow window. -->
    <div class="new-project">
      <div class="new-project__form">
        <!-- One form for both paths, in `scaffold` mode for a template. It
             drops the runtime and the document root — the template is the
             first and the framework is the second — and keeps the three a
             framework does not answer: PHP version, web server, extensions.
             The duplicate name and domain fields that used to live here were
             the same two fields written twice. -->
        <ProjectFormFields ref="fields" v-model="form" :catalog="catalog" :scaffold="scaffolding" />

        <!-- Said rather than left as an absence: a Laravel document root typed
             by hand is a 404 nobody can explain. -->
        <v-alert v-if="scaffolding" type="info" variant="tonal" class="mt-4">
          <div class="text-caption">{{ t('newProject.templateHint') }}</div>
          <div class="text-caption mt-2">{{ t('newProject.detectedHint') }}</div>
        </v-alert>

        <v-alert v-if="report && !report.valid" type="warning" variant="tonal" class="mt-5">
          <div v-for="(issue, i) in report.errors" :key="i" class="text-caption">
            <strong>{{ issue.code }}</strong> {{ issue.path }} — {{ issue.message }}
          </div>
        </v-alert>

        <v-alert v-if="unavailable.length" type="info" variant="tonal" class="mt-5">
          <div class="text-caption">
            {{
              t('newProject.unavailableRuntimes', { list: unavailable.map((r) => r.id).join(', ') })
            }}
          </div>
        </v-alert>
      </div>

      <div class="new-project__choice">
        <div class="sheet-group">{{ t('newProject.template') }}</div>

        <!-- The empty project sits outside the accordion: it is the default and
             the one choice that is not a framework, so burying it inside a
             group would make the common case the one you have to go looking
             for. -->
        <v-card
          :variant="template === 'empty' ? 'tonal' : 'text'"
          :color="template === 'empty' ? 'primary' : undefined"
          class="template-card mb-2"
          @click="choose('empty')"
        >
          <div class="d-flex align-center ga-2">
            <v-icon size="20">{{ TEMPLATE_ICONS.empty }}</v-icon>
            <div class="text-body-2">{{ t('newProject.templates.empty') }}</div>
          </div>
        </v-card>

        <v-expansion-panels v-model="openGroup" variant="accordion" rounded="lg" flat>
          <v-expansion-panel
            v-for="group in FRAMEWORK_GROUPS"
            :key="group.key"
            :value="group.key"
            elevation="0"
          >
            <v-expansion-panel-title class="template-panel-title">
              <span class="text-caption font-weight-medium">
                {{ t(`newProject.templateGroups.${group.key}`) }}
              </span>
              <!-- Named on the heading, so a collapsed panel still says what
                   was chosen inside it. -->
              <v-chip
                v-if="group.items.includes(template)"
                size="x-small"
                color="primary"
                variant="tonal"
                class="ml-2"
              >
                {{ t(`newProject.templates.${template}`) }}
              </v-chip>
            </v-expansion-panel-title>

            <v-expansion-panel-text class="template-panel-text">
              <v-card
                v-for="key in group.items"
                :key="key"
                :variant="template === key ? 'tonal' : 'text'"
                :color="template === key ? 'primary' : undefined"
                class="template-card"
                @click="choose(key)"
              >
                <div class="d-flex align-center ga-2">
                  <v-icon size="20">{{ TEMPLATE_ICONS[key] }}</v-icon>
                  <div class="text-body-2 text-truncate">
                    {{ t(`newProject.templates.${key}`) }}
                  </div>
                </div>
              </v-card>
            </v-expansion-panel-text>
          </v-expansion-panel>
        </v-expansion-panels>
      </div>
    </div>

    <template #footer>
      <v-btn variant="text" @click="close">{{ t('hosts.cancel') }}</v-btn>
      <v-btn color="primary" variant="flat" :loading="busy" :disabled="!canCreate" @click="create">
        {{ t('newProject.create') }}
      </v-btn>
    </template>
  </SideSheet>
</template>

<style scoped>
.new-project {
  display: flex;
  gap: 24px;
  align-items: flex-start;
}

.new-project__form {
  flex: 1 1 auto;
  min-width: 0;
}

/* Fixed, and narrow: it is a list of five short labels, and letting it share
   the growth would take width from the form, which is where the typing is. */
.new-project__choice {
  flex: 0 0 260px;
}

.template-panel-title {
  min-height: 40px;
  padding: 6px 12px;
}

.template-panel-text :deep(.v-expansion-panel-text__wrapper) {
  padding: 4px 6px 8px;
}

.template-card {
  padding: 8px 10px;
  cursor: pointer;
}

/* One column on a narrow window: two 260px columns in a 560px sheet would
   leave the form narrower than its own labels. */
@media (max-width: 720px) {
  .new-project {
    flex-direction: column;
  }

  .new-project__choice {
    flex: 1 1 auto;
    width: 100%;
    order: -1;
  }
}

.min-w-0 {
  min-width: 0;
}
</style>
