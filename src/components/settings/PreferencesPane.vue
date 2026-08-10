<script setup>
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { usePreferences, appChoice } from '@/composables/usePreferences';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * The app's own preferences: which editor, terminal and browser to launch, what
 * closing the window does, and whether StackVo starts with the machine.
 *
 * Eleventh pane out of `Settings.vue` under §14.16, and the last that owns
 * state. These are facts about this installation rather than about the stack,
 * so they live in `preferences.json` and never touch the `.env` editor.
 */
const { t } = useI18n();

const { prefs, autostart, load, set: setPref, toggleAutostart } = usePreferences();

/** The installed applications the pickers offer. */
const apps = ref({ terminals: [], editors: [], browsers: [] });

const appItemProps = (a) => ({
  prependIcon: a.icon,
  disabled: !a.available,
  subtitle: a.default ? t('settings.appDefault') : undefined,
});

onMounted(async () => {
  await load();
  // Every key the pickers read has to be present in the fallback too — a
  // missing `browsers` leaves that select bound to undefined instead of empty.
  apps.value = await api
    .appsAvailable()
    .catch(() => ({ terminals: [], editors: [], browsers: [] }));
});
</script>

<template>
  <SettingsGroup
    icon="mdi-application-cog-outline"
    :title="t('settings.externalApps')"
    :description="t('settings.externalAppsDesc')"
  >
    <div class="d-flex flex-column ga-3">
      <!-- Detected rather than typed. The old free-text box asked the
             user to know the launcher name; what is actually installed
             is something the app can find out. Missing apps stay in the
             list but disabled — omitting them would read as lack of
             support. -->
      <v-select
        :model-value="appChoice(prefs?.terminalApp, apps.terminals)"
        :items="apps.terminals"
        item-title="name"
        item-value="id"
        :item-props="appItemProps"
        :label="t('settings.terminalApp')"
        :hint="t('settings.appsHint')"
        persistent-hint
        clearable
        @update:model-value="(v) => setPref({ terminalApp: v || null })"
      />
      <v-select
        :model-value="appChoice(prefs?.editorCommand, apps.editors)"
        :items="apps.editors"
        item-title="name"
        item-value="id"
        :item-props="appItemProps"
        :label="t('settings.editorApp')"
        clearable
        @update:model-value="(v) => setPref({ editorCommand: v || null })"
      />
      <!-- Every "visit" button in the app goes through this. Cleared
             means the system default, which is why the list carries an
             explicit entry for it rather than only an empty state. -->
      <v-select
        :model-value="appChoice(prefs?.browserCommand, apps.browsers)"
        :items="apps.browsers"
        item-title="name"
        item-value="id"
        :item-props="appItemProps"
        :label="t('settings.browserApp')"
        :hint="t('settings.browserAppHint')"
        persistent-hint
        clearable
        @update:model-value="(v) => setPref({ browserCommand: v || null })"
      />
    </div>
  </SettingsGroup>

  <!-- Scheduled database snapshots. A preference rather than a stack setting:
       it is about what this installation does unattended, and it applies to
       whichever workspace is open. -->
  <SettingsGroup
    icon="mdi-database-clock-outline"
    :title="t('settings.backups')"
    :description="t('settings.backupsDesc')"
  >
    <v-select
      :model-value="prefs?.backupSchedule ?? 'off'"
      :items="[
        { value: 'off', title: t('settings.backupOff') },
        { value: 'hourly', title: t('settings.backupHourly') },
        { value: 'daily', title: t('settings.backupDaily') },
        { value: 'weekly', title: t('settings.backupWeekly') },
      ]"
      :label="t('settings.backupSchedule')"
      :hint="t('settings.backupScheduleHint')"
      persistent-hint
      @update:model-value="(v) => setPref({ backupSchedule: v })"
    />
    <v-text-field
      class="mt-3"
      type="number"
      min="1"
      max="100"
      :model-value="prefs?.backupKeep ?? 7"
      :label="t('settings.backupKeep')"
      :hint="t('settings.backupKeepHint')"
      persistent-hint
      @update:model-value="
        (v) => setPref({ backupKeep: Math.min(100, Math.max(1, Number(v) || 1)) })
      "
    />
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-power"
    :title="t('settings.startup')"
    :description="t('settings.startupDesc')"
  >
    <v-switch
      :model-value="autostart"
      :label="t('settings.autostart')"
      color="primary"
      hide-details
      @update:model-value="toggleAutostart"
    />
    <v-switch
      :model-value="prefs?.startMinimized ?? false"
      :label="t('settings.startMinimized')"
      color="primary"
      hide-details
      @update:model-value="(v) => setPref({ startMinimized: v })"
    />

    <v-divider class="my-3" />

    <div class="text-body-2">{{ t('close.behaviour') }}</div>
    <div class="text-caption text-medium-emphasis">{{ t('close.behaviourHint') }}</div>
    <v-radio-group
      :model-value="prefs?.closeBehaviour ?? 'ask'"
      hide-details
      @update:model-value="(v) => setPref({ closeBehaviour: v })"
    >
      <v-radio value="ask" :label="t('close.ask')" />
      <v-radio value="tray" :label="t('close.tray')" />
      <v-radio value="quit" :label="t('close.quit')" />
      <v-radio value="stopAndQuit" :label="t('close.stopAndQuit')" />
    </v-radio-group>
  </SettingsGroup>

  <!-- ---- stack ----------------------------------------------------- -->
  <!-- These shape every generated file. They live in the binary as
       defaults, so a fresh .env has none of them; changing one here
       writes the key, which is what makes a line in that file mean
       something. -->
</template>
