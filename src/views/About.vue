<script setup>
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { getVersion } from '@tauri-apps/api/app';
import { api } from '@/lib/ipc';

/**
 * The About window.
 *
 * Deliberately not the Settings pane. That one is a place you arrive at while
 * configuring something and it can afford to be long; this one is opened from
 * the menu bar to answer one question in one glance, so it holds the identity,
 * the version, and the two links somebody wants next — and nothing that would
 * make it worth scrolling.
 */

const { t } = useI18n();

const version = ref('');
onMounted(async () => {
  version.value = await getVersion().catch(() => '');
});

const LINKS = [
  { key: 'docs', icon: 'mdi-book-open-variant', url: 'https://stackvo.github.io/stackvo' },
  { key: 'source', icon: 'mdi-github', url: 'https://github.com/stackvo/stackvo' },
  { key: 'issues', icon: 'mdi-bug-outline', url: 'https://github.com/stackvo/stackvo/issues' },
];
</script>

<template>
  <div class="about-window d-flex flex-column align-center text-center pa-8">
    <v-avatar rounded="lg" size="88" color="primary" class="mb-5">
      <v-icon size="52" icon="mdi-cube-outline" />
    </v-avatar>

    <h1 class="text-h5 mb-1">StackVo</h1>
    <p class="text-body-2 text-medium-emphasis mb-4">{{ t('about.tagline') }}</p>

    <div class="d-flex ga-2 mb-6">
      <v-chip v-if="version" size="small" variant="tonal" prepend-icon="mdi-tag">
        {{ version }}
      </v-chip>
      <v-chip size="small" variant="tonal" prepend-icon="mdi-scale-balance">MIT</v-chip>
    </div>

    <v-divider class="w-100 mb-4" />

    <div class="d-flex flex-column w-100 ga-1">
      <v-btn
        v-for="l in LINKS"
        :key="l.key"
        variant="text"
        class="justify-start"
        :prepend-icon="l.icon"
        @click="api.openInBrowser(l.url)"
      >
        {{ t(`about.links.${l.key}`) }}
        <v-spacer />
        <v-icon size="x-small" icon="mdi-open-in-new" />
      </v-btn>
    </div>

    <v-spacer />

    <p class="text-caption text-medium-emphasis mt-6">{{ t('about.copyright') }}</p>
  </div>
</template>

<style scoped>
/* Fills the window it was opened as, so the copyright line sits on the floor
   rather than under the last button. */
.about-window {
  min-height: 100vh;
}
</style>
