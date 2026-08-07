<script setup>
import { useI18n } from 'vue-i18n';
import { useAppearanceStore } from '@/stores/appearance';
import { setLocale } from '@/i18n';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * Language, and the two settings that are about language without being it.
 *
 * Eighth pane out of `Settings.vue` under §14.16 and the smallest. Three
 * controls, three different owners, which is the whole reason it is worth
 * mounting: the app locale goes through `setLocale` because it also persists
 * and relabels the tray; the console locale and the RTL flag are appearance
 * state and go straight to the store.
 */
const { t, locale } = useI18n();
const appearance = useAppearanceStore();
</script>

<template>
  <SettingsGroup
    icon="mdi-web"
    :title="t('settings.language')"
    :description="t('settings.languageDesc')"
  >
    <v-select
      :model-value="locale"
      :items="[
        { value: 'tr', title: 'Türkçe' },
        { value: 'en', title: 'English' },
      ]"
      :label="t('settings.language')"
      @update:model-value="setLocale"
    />
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-console"
    :title="t('settings.consoleLanguage')"
    :description="t('settings.consoleLanguageDesc')"
  >
    <v-select
      :model-value="appearance.value.consoleLocale"
      :items="[
        { value: 'app', title: t('settings.consoleFollowsApp') },
        { value: 'tr', title: 'Türkçe' },
        { value: 'en', title: 'English' },
      ]"
      :label="t('settings.consoleLanguage')"
      :hint="t('settings.consoleLanguageHint')"
      persistent-hint
      @update:model-value="(v) => appearance.set({ consoleLocale: v })"
    />
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-format-textdirection-r-to-l"
    :title="t('settings.direction')"
    :description="t('settings.directionDesc')"
  >
    <v-switch
      :model-value="appearance.value.rtl"
      :label="t('settings.rtl')"
      color="primary"
      hide-details
      @update:model-value="(v) => appearance.set({ rtl: v })"
    />
    <div class="text-caption text-medium-emphasis">{{ t('settings.rtlHint') }}</div>
  </SettingsGroup>

  <!-- ---- workspace ------------------------------------------------ -->

  <!-- ---- preferences ---------------------------------------------- -->
</template>
