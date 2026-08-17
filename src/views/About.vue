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

/**
 * The third-party notice, read from the binary rather than from a file.
 *
 * MIT, BSD, ISC and Apache-2.0 all ask that the notice travel with the
 * software. A `NOTICE.md` in the repository does not travel with a `.dmg`, so
 * the text is compiled in and this window is where somebody who has only the
 * app can read it. Fetched on open rather than on mount: it is ~85 KB that most
 * visits to this window do not need.
 */
const showLicences = ref(false);
const licences = ref('');
const licencesError = ref(false);

async function openLicences() {
  showLicences.value = true;
  if (licences.value) return;
  try {
    licences.value = (await api.licencesNotice()) ?? '';
    licencesError.value = !licences.value;
  } catch {
    // A build with no notice compiled in cannot happen — the file is
    // `include_str!`'d — but a failure here must say so rather than showing an
    // empty panel that reads as "no dependencies".
    licencesError.value = true;
  }
}

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

    <v-btn
      variant="text"
      size="small"
      class="mt-2"
      prepend-icon="mdi-license"
      @click="openLicences"
    >
      {{ t('about.licences') }}
    </v-btn>

    <v-spacer />

    <p class="text-caption text-medium-emphasis mt-6">{{ t('about.copyright') }}</p>

    <v-dialog v-model="showLicences" scrollable max-width="880">
      <v-card>
        <v-card-title>{{ t('about.licences') }}</v-card-title>
        <v-card-subtitle>{{ t('about.licencesDesc') }}</v-card-subtitle>
        <v-card-text>
          <v-alert v-if="licencesError" type="error" variant="tonal" class="mb-2">
            {{ t('about.licencesFailed') }}
          </v-alert>
          <!-- Rendered as the plain text it is. Turning 85 KB of markdown into
               HTML would need a parser in the bundle to make a legal notice
               prettier, and the shape that matters — one package per line — is
               already readable. -->
          <pre v-else class="notice-text" :aria-label="t('about.licences')" tabindex="0">{{
            licences
          }}</pre>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showLicences = false">{{ t('about.close') }}</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<style scoped>
/* Fills the window it was opened as, so the copyright line sits on the floor
   rather than under the last button. */
.about-window {
  min-height: 100vh;
}

/* The notice is a table of packages: it has to wrap rather than scroll
   sideways, and it has to stay monospace so the columns line up. */
.notice-text {
  white-space: pre-wrap;
  word-break: break-word;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.75rem;
  line-height: 1.5;
  text-align: start;
}
</style>
