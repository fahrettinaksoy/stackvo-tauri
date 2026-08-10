<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  LANG_DEFAULTS,
  LANG_RUNTIMES,
  canonical,
  extensionLimit,
  isIncompatible,
  overExtensionLimit,
  domainAdvice,
  domainSuggestions,
} from '@/lib/manifest';
import { useAppStore } from '@/stores/app';

/**
 * The manifest fields, shared by the create drawer and the settings sheet.
 *
 * Creating a project and editing one write the same file through the same
 * validator, so they get the same fields in the same order — someone who has
 * created a project already knows where `document_root` is. The two callers
 * differ in what they do with the result, not in what they ask for.
 *
 * The i18n keys are the `newProject.*` ones. They name fields, not the create
 * action, and duplicating fifteen of them under a second prefix would be two
 * places to keep one label.
 */

const props = defineProps({
  /** From `catalog_get`: the versions, servers and extensions .env allows. */
  catalog: { type: Object, default: null },
  /**
   * Editing an existing project. The directory name is the project's identity
   * (W-04) — `listProjects` keys containers off it, so renaming is a directory
   * move, not a field.
   */
  lockName: { type: Boolean, default: false },
  /**
   * The project is being scaffolded from a template, so some of the form is
   * the framework's answer rather than the user's.
   *
   * Hidden: the runtime (the template IS the runtime) and the document root
   * (`public` for Laravel, `web` for TYPO3 — detection reads it off what the
   * installer wrote, and a field would only let the two disagree).
   *
   * Still asked: PHP version, web server and extensions. None of those are
   * discoverable from the installed code — a framework declares a *floor*
   * (`"php": "^8.3"`), not the version you want to run, and nothing in a
   * checkout says nginx or apache. Left to detection they defaulted silently,
   * which is what put a Laravel 13 project on PHP 8.3 with seven extensions.
   */
  scaffold: { type: Boolean, default: false },
});

const form = defineModel({ type: Object, required: true });

const { t } = useI18n();
const app = useAppStore();
/**
 * The suffix is a choice, not a fixture. The configured one leads because it
 * is what the certificate already covers; the rest are offered because a
 * developer may well want `.test` or `.dev`, and typing a whole hostname to
 * get one is not offering it.
 */
/**
 * Both identifiers are written back lower-case as they are typed.
 *
 * The name becomes a directory, a container name and an image reference, and
 * Docker refuses the last of those with a capital in it. The domain is compared
 * byte-for-byte in three places — the Traefik rule, the hosts line, the
 * certificate — so one stray capital is a project that resolves and 404s.
 */
const name = computed({
  get: () => form.value.name,
  set: (value) => {
    form.value.name = canonical(value);
  },
});
const domain = computed({
  get: () => form.value.domain,
  set: (value) => {
    form.value.domain = canonical(value);
  },
});

/**
 * The same lower-casing the domain gets, for the same reason: these become a
 * Traefik rule, a hosts line and a certificate SAN, compared byte for byte.
 *
 * A `*.` prefix survives — it is the one hostname form that is deliberately not
 * a hostname, and `canonical()` would not know that.
 */
const aliases = computed({
  get: () => form.value.aliases ?? [],
  set: (value) =>
    (form.value.aliases = (value ?? [])
      .map((host) => String(host).trim().toLowerCase())
      .filter(Boolean)),
});

/**
 * Said where the wildcard is typed, not after saving.
 *
 * A wildcard reaches the certificate and the router and cannot reach
 * `/etc/hosts` — no hosts file can express one. Somebody who types it and is
 * not told will conclude the feature is broken when the name does not resolve.
 */
const wildcardWarning = computed(() =>
  aliases.value.some((host) => host.startsWith('*.')) ? t('newProject.aliasesWildcard') : ''
);

const domainItems = computed(() => domainSuggestions(form.value.name, app.tld));
const domainHint = computed(() => t('newProject.domainHint'));
const domainWarning = computed(() => {
  const effective = form.value.domain || domainItems.value[0] || '';
  const advice = domainAdvice(effective, app.tld, app.sslEnabled);
  // Spelled out rather than built from the advice, so the keys stay findable
  // by a search — and by the test that proves every string is reachable.
  if (advice === 'https') return t('newProject.domain_https');
  if (advice === 'certificate') return t('newProject.domain_certificate');
  return '';
});

/** Only runtimes with a generator behind them are selectable (C-02). */
const runtimes = computed(() => props.catalog?.runtimes?.filter((r) => r.available) ?? []);

const phpVersions = computed(
  () => props.catalog?.runtimes?.find((r) => r.id === 'php')?.versions ?? []
);
const nodeVersions = computed(
  () => props.catalog?.runtimes?.find((r) => r.id === 'node')?.versions ?? []
);

const isLang = computed(() => LANG_RUNTIMES.includes(form.value.runtime));
const langVersions = computed(
  () => props.catalog?.runtimes?.find((r) => r.id === form.value.runtime)?.versions ?? []
);

/**
 * Seed the lang fields with the runtime's ecosystem defaults on switch — an
 * empty required Start field teaches nothing, the convention does. Explicit
 * handler rather than a watcher for the same reason `onPhpVersion` is: loading
 * an existing manifest into the form must not overwrite its values.
 */
function onRuntime(runtime) {
  form.value.runtime = runtime;
  const defaults = LANG_DEFAULTS[runtime];
  if (defaults) {
    // The checkout's own default version wins over the ecosystem constant —
    // .env's SUPPORTED_LANGUAGES_*_DEFAULT is what the catalog serves.
    form.value.langVersion =
      props.catalog?.runtimes?.find((r) => r.id === runtime)?.default ?? defaults.version;
    form.value.langInstall = defaults.install;
    form.value.langBuild = defaults.build;
    form.value.langStart = defaults.start;
    form.value.langPort = defaults.port;
  }
}

/**
 * The catalog's extensions, plus any the project already asks for that the
 * catalog does not list.
 *
 * The second half only matters when editing. A manifest can name an extension
 * that has since been dropped from `SUPPORTED_LANGUAGES_PHP_EXTENSIONS`, and
 * offering only the catalog would leave that value selected but absent from
 * the list — visible as a chip, impossible to put back once removed.
 */
const extensionOptions = computed(() => {
  const known = props.catalog?.phpExtensions ?? [];
  const options = known.map((e) => ({
    value: e.name,
    title: e.name,
    incompatible: isIncompatible(e, form.value.phpVersion),
    unknown: false,
  }));

  const listed = new Set(options.map((o) => o.value));
  for (const name of form.value.extensions) {
    if (!listed.has(name)) {
      options.push({ value: name, title: name, incompatible: false, unknown: true });
    }
  }
  return options;
});

const maxExtensions = computed(() => extensionLimit(props.catalog));
const overLimit = computed(() => overExtensionLimit(form.value, props.catalog));

/**
 * Drop extensions the newly chosen PHP version cannot build.
 *
 * Called from the version field rather than a watcher: a watcher would also
 * fire when a manifest is loaded into the form, and quietly deleting
 * extensions from a project the moment its settings are opened is not an edit
 * the user made.
 */
function onPhpVersion(version) {
  form.value.phpVersion = version;
  const bad = new Set(
    (props.catalog?.phpExtensions ?? [])
      .filter((e) => isIncompatible(e, version))
      .map((e) => e.name)
  );
  form.value.extensions = form.value.extensions.filter((e) => !bad.has(e));
}

// The create drawer focuses this field when it opens. The ref lives with the
// field rather than in the caller, which cannot reach into the child's DOM.
const nameField = ref(null);

defineExpose({ focusName: () => nameField.value?.focus() });
</script>

<template>
  <!-- One field per row. In a 560px panel a two-column grid gives each column
       a ~250px box, and a form whose fields are half as wide as the label they
       carry reads as cramped rather than compact. A single column also makes
       the tab order and the reading order the same thing. -->
  <div class="fields">
    <div class="sheet-group">{{ t('newProject.sectionProject') }}</div>

    <v-text-field
      ref="nameField"
      v-model="name"
      :label="t('newProject.name')"
      prepend-inner-icon="mdi-folder-outline"
      :readonly="lockName"
      :persistent-hint="true"
      :hint="lockName ? t('projectSettings.nameLocked') : t('newProject.nameHint')"
    />
    <v-combobox
      v-model="domain"
      :label="t('newProject.domain')"
      :items="domainItems"
      :placeholder="form.name ? `${form.name}.${app.tld}` : ''"
      persistent-placeholder
      prepend-inner-icon="mdi-web"
      :hint="domainHint"
      persistent-hint
      :messages="domainWarning ? [domainWarning] : []"
    />
    <!-- Extra hostnames. A combobox rather than a text field because the value
         is a list and splitting on commas is how `a.loc , b.loc` becomes a
         hostname with a space in it. -->
    <v-combobox
      v-model="aliases"
      :label="t('newProject.aliases')"
      multiple
      chips
      closable-chips
      clearable
      prepend-inner-icon="mdi-dns-outline"
      :hint="t('newProject.aliasesHint')"
      persistent-hint
      :messages="wildcardWarning ? [wildcardWarning] : []"
    />

    <!-- Scaffolding picked the runtime the moment the template was chosen. -->
    <v-select
      v-if="!scaffold"
      :model-value="form.runtime"
      :items="runtimes.map((r) => ({ value: r.id, title: r.id }))"
      :label="t('newProject.runtime')"
      prepend-inner-icon="mdi-code-braces"
      @update:model-value="onRuntime"
    />

    <template v-if="form.runtime === 'php'">
      <div class="sheet-group">{{ t('newProject.sectionPhp') }}</div>

      <v-select
        :model-value="form.phpVersion"
        :items="phpVersions"
        :label="t('newProject.phpVersion')"
        prepend-inner-icon="mdi-tag-outline"
        @update:model-value="onPhpVersion"
      />
      <v-select
        v-model="form.server"
        :items="catalog?.servers ?? []"
        :label="t('newProject.server')"
        prepend-inner-icon="mdi-server"
      />
      <!-- The framework decides this one — `public` for Laravel, `web` for
           TYPO3 — and detection reads it off what the installer wrote. -->
      <v-text-field
        v-if="!scaffold"
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
              :subtitle="
                item.raw.incompatible
                  ? t('newProject.incompatible')
                  : item.raw.unknown
                    ? t('projectSettings.extensionUnknown')
                    : undefined
              "
            />
          </template>
        </v-autocomplete>

        <!-- A count, not a quota: the ceiling is the catalog itself now that
             the Bash parser's 50-line window is gone (C-04, closed). -->
        <div class="text-caption mt-1" :class="overLimit ? 'text-error' : 'text-medium-emphasis'">
          {{ form.extensions.length }} / {{ maxExtensions }}
          <span v-if="overLimit">— {{ t('newProject.tooManyExtensions') }}</span>
        </div>
      </div>
    </template>

    <!-- The non-PHP runtimes are configured entirely from what the installer
         wrote: `.nvmrc`, `engines.node`, the dev script, the port it binds.
         Asking for them here would be asking the user to repeat the template,
         and to disagree with it. PHP is the exception above, because a
         `composer.json` declares a floor rather than a choice. -->
    <template v-else-if="isLang && !scaffold">
      <div class="sheet-group">{{ t('newProject.sectionLang', { runtime: form.runtime }) }}</div>

      <v-select
        v-model="form.langVersion"
        :items="langVersions"
        :label="t('newProject.langVersion')"
        prepend-inner-icon="mdi-tag-outline"
      />
      <v-text-field
        v-model="form.langPort"
        type="number"
        :label="t('newProject.port')"
        prepend-inner-icon="mdi-lan-connect"
        :hint="t('newProject.portHint')"
        persistent-hint
      />
      <v-text-field
        v-model="form.langInstall"
        :label="t('newProject.install')"
        prepend-inner-icon="mdi-download-outline"
        :hint="t('newProject.optionalStep')"
        persistent-hint
      />
      <v-text-field
        v-model="form.langBuild"
        :label="t('newProject.build')"
        prepend-inner-icon="mdi-hammer-wrench"
        :hint="t('newProject.optionalStep')"
        persistent-hint
      />
      <v-text-field
        v-model="form.langStart"
        :label="t('newProject.start')"
        prepend-inner-icon="mdi-play-outline"
        :hint="t('newProject.langBindHint')"
        persistent-hint
      />
    </template>

    <template v-else-if="!scaffold">
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
</template>

<style scoped>
/* One column, one rhythm. */
.fields {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
