<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { extensionLimit, isIncompatible, overExtensionLimit } from '@/lib/manifest';

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
});

const form = defineModel({ type: Object, required: true });

const { t } = useI18n();

/** Only runtimes with a generator behind them are selectable (C-02). */
const runtimes = computed(() => props.catalog?.runtimes?.filter((r) => r.available) ?? []);

const phpVersions = computed(
  () => props.catalog?.runtimes?.find((r) => r.id === 'php')?.versions ?? []
);
const nodeVersions = computed(
  () => props.catalog?.runtimes?.find((r) => r.id === 'node')?.versions ?? []
);

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
      v-model="form.name"
      :label="t('newProject.name')"
      prepend-inner-icon="mdi-folder-outline"
      :readonly="lockName"
      :persistent-hint="true"
      :hint="lockName ? t('projectSettings.nameLocked') : t('newProject.nameHint')"
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

        <!-- The manifest cap is a Bash parser limit, not a preference: entry 51
             onward is dropped without a word (C-04). -->
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
</template>

<style scoped>
/* One column, one rhythm. */
.fields {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
