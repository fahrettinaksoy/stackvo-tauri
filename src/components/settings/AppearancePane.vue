<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useTheme } from 'vuetify';
import { useAppearanceStore } from '@/stores/appearance';
import {
  DEFAULT_APPEARANCE,
  PRIMARY_SWATCHES,
  NEUTRALS,
  FONT_FAMILIES,
  STATUS_PALETTES,
} from '@/lib/appearance';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * How the app looks: theme, accent, density, font and the saved presets.
 *
 * Sixth pane out of `Settings.vue` under §14.16 and the cleanest seam so far —
 * it is the only one that touches neither the `.env` editor nor the operation
 * console. Everything it changes lives in `useAppearanceStore`, which persists
 * and applies on its own, so the pane is markup over a store and nothing else.
 */
const { t } = useI18n();
const appearance = useAppearanceStore();
const theme = useTheme();

const isDark = computed(() => theme.global.current.value.dark);

/** Name being typed for a new preset. Empty disables the save button. */
const presetName = ref('');

async function savePreset() {
  await appearance.savePreset(presetName.value);
  presetName.value = '';
}

const statusItems = computed(() =>
  STATUS_PALETTES.map((p) => ({ value: p.id, title: t(`settings.statusPalettes.${p.id}`) }))
);

const fontItems = computed(() =>
  FONT_FAMILIES.map((f) => ({ value: f.id, title: t(`settings.fonts.${f.id}`) }))
);

/** Shown next to the reset button so "back to defaults" is not a leap of faith. */
const isDefaultAppearance = computed(() =>
  Object.keys(DEFAULT_APPEARANCE).every((k) => appearance.value[k] === DEFAULT_APPEARANCE[k])
);
</script>

<template>
  <SettingsGroup
    icon="mdi-palette"
    :title="t('settings.themeColors')"
    :description="t('settings.themeColorsDesc')"
  >
    <template #append>
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-backup-restore"
        :disabled="isDefaultAppearance"
        @click="appearance.reset()"
      >
        {{ t('settings.resetAppearance') }}
      </v-btn>
    </template>

    <div class="field-label">{{ t('settings.theme') }}</div>
    <!-- Three buttons rather than a dropdown: the choice is small,
           fixed and worth showing all of at once. `system` is Vuetify's
           own theme name, so it tracks prefers-color-scheme live rather
           than being read once at launch. -->
    <v-btn-toggle
      :model-value="appearance.value.theme"
      mandatory
      divided
      color="primary"
      variant="flat"
      class="mb-5 bg-surface-light"
      @update:model-value="(v) => appearance.set({ theme: v })"
    >
      <v-btn value="system" size="small" prepend-icon="mdi-theme-light-dark">
        {{ t('settings.themeSystem') }}
      </v-btn>
      <v-btn value="light" size="small" prepend-icon="mdi-white-balance-sunny">
        {{ t('settings.themeLight') }}
      </v-btn>
      <v-btn value="dark" size="small" prepend-icon="mdi-weather-night">
        {{ t('settings.themeDark') }}
      </v-btn>
    </v-btn-toggle>

    <div class="d-flex align-center ga-2 mb-1">
      <div class="field-label mb-0">{{ t('settings.primaryColor') }}</div>
      <v-spacer />
      <!-- Offered only where it can be answered: on Linux there is no
             one accent colour to read. -->
      <v-switch
        v-if="appearance.systemAccent"
        :model-value="appearance.value.useSystemAccent"
        :label="t('settings.systemAccent')"
        color="primary"
        hide-details
        class="flex-grow-0"
        @update:model-value="(v) => appearance.set({ useSystemAccent: v })"
      />
    </div>
    <div class="swatches mb-5" :class="{ 'is-disabled': appearance.value.useSystemAccent }">
      <button
        v-for="c in PRIMARY_SWATCHES"
        :key="c"
        type="button"
        class="swatch"
        :class="{ 'is-active': appearance.value.primary === c }"
        :style="{ background: c }"
        :title="c"
        :aria-label="c"
        :disabled="appearance.value.useSystemAccent"
        @click="appearance.set({ primary: c })"
      >
        <v-icon v-if="appearance.value.primary === c" size="15" color="white"> mdi-check </v-icon>
      </button>
    </div>

    <div class="field-label">{{ t('settings.neutralPalette') }}</div>
    <div class="swatches mb-5">
      <button
        v-for="n in NEUTRALS"
        :key="n.id"
        type="button"
        class="swatch swatch--neutral"
        :class="{ 'is-active': appearance.value.neutral === n.id }"
        :style="{ background: isDark ? n.dark.surface : n.light['surface-variant'] }"
        :title="t(`settings.neutrals.${n.id}`)"
        :aria-label="t(`settings.neutrals.${n.id}`)"
        @click="appearance.set({ neutral: n.id })"
      >
        <v-icon v-if="appearance.value.neutral === n.id" size="15">mdi-check</v-icon>
      </button>
    </div>

    <div class="field-label">
      {{ t('settings.radius', { px: appearance.value.radius }) }}
    </div>
    <!-- Previewed while dragging, written when the handle is let go:
           a slider emits on every pixel, and preferences.json is a file
           on disk. -->
    <!-- The label above is a plain `div`, so nothing connects it to the
           control: a screen reader announced "slider, 12" with no indication of
           what was at 12. Found by the axe pass once this pane could be
           mounted. -->
    <v-slider
      :aria-label="t('settings.radius', { px: appearance.value.radius })"
      :model-value="appearance.value.radius"
      :min="0"
      :max="24"
      :step="1"
      hide-details
      @update:model-value="(v) => appearance.preview({ radius: v })"
      @end="appearance.commit()"
    />
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-format-font"
    :title="t('settings.typography')"
    :description="t('settings.typographyDesc')"
  >
    <v-select
      :model-value="appearance.value.fontFamily"
      :items="fontItems"
      :label="t('settings.fontFamily')"
      :hint="t('settings.fontFamilyHint')"
      persistent-hint
      class="mb-5"
      @update:model-value="(v) => appearance.set({ fontFamily: v })"
    />

    <div class="field-label">{{ t('settings.density') }}</div>
    <!-- One knob for the whole app: every `density` prop written on a
           component was removed, because a prop outranks a default and
           would have made this setting a no-op wherever one existed. -->
    <v-btn-toggle
      :model-value="appearance.value.density"
      mandatory
      divided
      color="primary"
      variant="flat"
      class="mb-5 bg-surface-light"
      @update:model-value="(v) => appearance.set({ density: v })"
    >
      <v-btn value="compact" size="small">{{ t('settings.densityCompact') }}</v-btn>
      <v-btn value="comfortable" size="small">
        {{ t('settings.densityComfortable') }}
      </v-btn>
      <v-btn value="default" size="small">{{ t('settings.densitySpacious') }}</v-btn>
    </v-btn-toggle>

    <div class="field-label">
      {{ t('settings.uiScale', { px: appearance.value.fontSize }) }}
    </div>
    <!-- Vuetify's type scale is in rem throughout, so the root size
           scales every label, table row and dialog with it — this is a
           UI scale control, not just a font size. -->
    <v-slider
      :aria-label="t('settings.uiScale', { px: appearance.value.fontSize })"
      :model-value="appearance.value.fontSize"
      :min="12"
      :max="20"
      :step="1"
      hide-details
      class="mb-4"
      @update:model-value="(v) => appearance.preview({ fontSize: v })"
      @end="appearance.commit()"
    />

    <v-switch
      :model-value="appearance.value.highContrast"
      :label="t('settings.highContrast')"
      color="primary"
      hide-details
      @update:model-value="(v) => appearance.set({ highContrast: v })"
    />
    <div class="text-caption text-medium-emphasis">
      {{ t('settings.highContrastHint') }}
    </div>

    <v-switch
      :model-value="appearance.value.reduceMotion"
      :label="t('settings.reduceMotion')"
      color="primary"
      hide-details
      class="mt-2"
      @update:model-value="(v) => appearance.set({ reduceMotion: v })"
    />
    <div class="text-caption text-medium-emphasis">
      {{ t('settings.reduceMotionHint') }}
    </div>
  </SettingsGroup>

  <!-- The one palette in the app that is not decoration: these four
         colours are how a container reports what it is doing. -->
  <SettingsGroup
    icon="mdi-traffic-light-outline"
    :title="t('settings.statusColors')"
    :description="t('settings.statusColorsDesc')"
  >
    <v-select
      :model-value="appearance.value.statusPalette"
      :items="statusItems"
      :label="t('settings.statusPalette')"
      class="mb-3"
      @update:model-value="(v) => appearance.set({ statusPalette: v })"
    />

    <!-- Shown, not described: whether two states are tellable apart is
           a question about your eyes, not about the palette's name. -->
    <div class="d-flex ga-2 flex-wrap">
      <v-chip size="small" color="success" prepend-icon="mdi-check-circle">
        {{ t('system.running') }}
      </v-chip>
      <v-chip size="small" color="error" prepend-icon="mdi-alert-circle">
        {{ t('system.stopped') }}
      </v-chip>
      <v-chip size="small" color="warning" prepend-icon="mdi-alert">
        {{ t('settings.generatorDiffers') }}
      </v-chip>
      <v-chip size="small" color="info" prepend-icon="mdi-information">
        {{ t('settings.about') }}
      </v-chip>
    </div>

    <v-switch
      :model-value="appearance.value.darkConsoles"
      :label="t('settings.darkConsoles')"
      color="primary"
      hide-details
      class="mt-3"
      @update:model-value="(v) => appearance.set({ darkConsoles: v })"
    />
    <div class="text-caption text-medium-emphasis">
      {{ t('settings.darkConsolesHint') }}
    </div>
  </SettingsGroup>

  <SettingsGroup
    icon="mdi-bookmark-multiple-outline"
    :title="t('settings.presets')"
    :description="t('settings.presetsDesc')"
  >
    <div class="d-flex ga-2 align-start">
      <v-text-field
        v-model="presetName"
        :label="t('settings.presetName')"
        hide-details
        @keyup.enter="savePreset"
      />
      <v-btn color="primary" variant="flat" :disabled="!presetName.trim()" @click="savePreset">
        {{ t('settings.savePreset') }}
      </v-btn>
    </div>

    <div v-if="appearance.presets.length" class="d-flex ga-2 flex-wrap mt-3">
      <v-chip
        v-for="p in appearance.presets"
        :key="p.name"
        closable
        prepend-icon="mdi-palette-swatch"
        @click="appearance.applyPreset(p.name)"
        @click:close="appearance.deletePreset(p.name)"
      >
        {{ p.name }}
      </v-chip>
    </div>
    <div v-else class="text-caption text-medium-emphasis mt-3">
      {{ t('settings.noPresets') }}
    </div>
  </SettingsGroup>

  <!-- ---- localisation ---------------------------------------------- -->
</template>
