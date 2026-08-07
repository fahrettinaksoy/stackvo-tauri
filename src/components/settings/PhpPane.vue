<script setup>
import { onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSharedEnvEditor } from '@/composables/useEnvEditor';
import { useVersionChoices, RUNTIME_DEFAULTS } from '@/composables/useCatalog';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * The versions a new project starts on, and the PHP tooling that ships with it.
 *
 * Seventh pane out of `Settings.vue` under §14.16. The choices come from the
 * catalog compiled into the binary rather than a list typed here, so a release
 * added there shows up without a second edit — see `useCatalog`.
 */
const { t } = useI18n();

const env = useSharedEnvEditor();
const { effective, edit, listOf, setList, dirty, changedCount, saving, saved } = env;

const { phpVersions, nodeVersions, serverChoices, runtimeItems, loadCatalog } =
  useVersionChoices(env);

/** The file is the view's to write; six panes share one diff over it. */
const emit = defineEmits(['save']);
const save = () => emit('save');

onMounted(loadCatalog);
</script>

<template>
  <SettingsGroup
    icon="mdi-code-braces"
    :title="t('settings.defaults.runtimes')"
    :description="t('settings.runtimes.desc')"
  >
    <template #append>
      <v-btn
        v-if="dirty"
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-content-save-outline"
        :loading="saving"
        @click="save"
      >
        {{ t('settings.save', { count: changedCount }) }}
      </v-btn>
      <v-chip v-else-if="saved" color="success" size="small">
        {{ t('settings.saved') }}
      </v-chip>
    </template>

    <v-row dense>
      <v-col v-for="r in RUNTIME_DEFAULTS" :key="r.id" cols="12" sm="6">
        <v-select
          :model-value="effective(r.key)"
          :items="runtimeItems(r)"
          :label="r.id"
          :prepend-inner-icon="r.icon"
          density="comfortable"
          variant="outlined"
          hide-details
          @update:model-value="(v) => edit(r.key, v)"
        />
      </v-col>
    </v-row>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-tag-outline"
    :title="t('settings.defaults.php')"
    :description="t('settings.php.versionDesc')"
  >
    <template #append>
      <v-btn
        v-if="dirty"
        size="small"
        variant="tonal"
        color="primary"
        prepend-icon="mdi-content-save-outline"
        :loading="saving"
        @click="save"
      >
        {{ t('settings.save', { count: changedCount }) }}
      </v-btn>
      <v-chip v-else-if="saved" color="success" size="small">
        {{ t('settings.saved') }}
      </v-chip>
    </template>

    <v-row dense>
      <v-col cols="12" md="6">
        <v-select
          :model-value="effective('SUPPORTED_LANGUAGES_PHP_DEFAULT')"
          :items="phpVersions"
          :label="t('settings.php.version')"
          :hint="t('settings.php.versionHint')"
          persistent-hint
          density="comfortable"
          variant="outlined"
          prepend-inner-icon="mdi-language-php"
          @update:model-value="(v) => edit('SUPPORTED_LANGUAGES_PHP_DEFAULT', v)"
        />
      </v-col>
      <v-col cols="12" md="6">
        <v-select
          :model-value="effective('SUPPORTED_SERVERS_DEFAULT')"
          :items="serverChoices"
          :label="t('settings.php.server')"
          :hint="t('settings.php.serverHint')"
          persistent-hint
          density="comfortable"
          variant="outlined"
          prepend-inner-icon="mdi-server"
          @update:model-value="(v) => edit('SUPPORTED_SERVERS_DEFAULT', v)"
        />
      </v-col>
    </v-row>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-hammer-wrench"
    :title="t('settings.defaults.phpTools')"
    :description="t('settings.shape.phpDesc')"
  >
    <v-row dense class="mb-1">
      <v-col cols="12" md="6">
        <v-combobox
          :model-value="effective('PHP_TOOL_COMPOSER_VERSION')"
          :items="['latest']"
          :label="t('settings.php.composer')"
          :hint="t('settings.php.composerHint')"
          persistent-hint
          density="comfortable"
          variant="outlined"
          prepend-inner-icon="mdi-package-variant"
          @update:model-value="(v) => edit('PHP_TOOL_COMPOSER_VERSION', v ?? '')"
        />
      </v-col>
      <v-col cols="12" md="6">
        <v-select
          :model-value="effective('PHP_TOOL_NODEJS_VERSION')"
          :items="nodeVersions"
          :label="t('settings.php.nodejs')"
          :hint="t('settings.php.nodejsHint')"
          persistent-hint
          density="comfortable"
          variant="outlined"
          prepend-inner-icon="mdi-nodejs"
          @update:model-value="(v) => edit('PHP_TOOL_NODEJS_VERSION', v)"
        />
      </v-col>
    </v-row>

    <v-combobox
      :model-value="listOf('PHP_DEFAULT_TOOLS')"
      :label="t('settings.shape.tools')"
      :hint="t('settings.shape.toolsHint')"
      multiple
      chips
      closable-chips
      persistent-hint
      density="comfortable"
      variant="outlined"
      class="mb-3"
      @update:model-value="(v) => setList('PHP_DEFAULT_TOOLS', v)"
    />
    <v-combobox
      :model-value="listOf('PHP_DEFAULT_APT_PACKAGES')"
      :label="t('settings.shape.apt')"
      :hint="t('settings.shape.aptHint')"
      multiple
      chips
      closable-chips
      persistent-hint
      density="comfortable"
      variant="outlined"
      @update:model-value="(v) => setList('PHP_DEFAULT_APT_PACKAGES', v)"
    />
  </SettingsGroup>

  <!-- ---- stack preset ----------------------------------------------- -->
  <!-- The stack, made portable. `stackvo.json` is already in the
       teammate's clone; which services are on and at which versions is
       not, because that lives in .env and .env is where the passwords
       are. A preset carries the first half and, by construction, has
       nowhere to put the second. -->

  <!-- ---- runtimes --------------------------------------------------- -->

  <!-- ---- servers ---------------------------------------------------- -->
</template>
