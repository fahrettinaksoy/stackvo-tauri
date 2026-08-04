<script setup>
import { useI18n } from 'vue-i18n';
import PageLayout from '@/components/PageLayout.vue';
import DumpView from '@/components/DumpView.vue';

/**
 * Every project's dumps, in one place.
 *
 * The same argument that made the log viewer a page, and the same shape. The
 * per-project pane answers "what did this project dump"; it cannot answer
 * "which of my eight projects just dumped something", which is the question
 * you have before you know which project to open.
 *
 * It also closes a hole rather than only adding a view. Capture stays on
 * across navigation deliberately — a dump from a queue worker or an artisan
 * command should be caught while you are looking at something else — but until
 * now the reader only ran inside one project's page, so those events piled up
 * with nobody watching. Herd opens its dump window by itself for exactly this;
 * this is somewhere you leave open instead.
 *
 * Everything on screen is `DumpView` in `scope="all"`. Nothing about rows,
 * search, the source link or the capture switch is reimplemented here.
 */
const { t } = useI18n();
</script>

<template>
  <PageLayout
    top-icon="mdi-bug-outline"
    :top-title="t('dumps.title')"
    :top-subtitle="t('dumps.allDescription')"
    hide-bar
  >
    <DumpView scope="all" class="flex-grow-1" />
  </PageLayout>
</template>
