<script setup>
/**
 * One pane of the settings page: an icon, a name, and a line saying what the
 * pane is for.
 *
 * The description is not decoration. A settings pane named "Stack control"
 * tells you nothing about whether clicking something in it will stop your
 * containers; the sentence under the name is where that is said, once, at the
 * point of entry rather than on every button inside.
 */
defineProps({
  icon: { type: String, default: '' },
  title: { type: String, default: '' },
  description: { type: String, default: '' },
});
</script>

<template>
  <section>
    <header class="d-flex align-start ga-3">
      <v-avatar rounded="lg" size="40" class="section-icon">
        <v-icon size="20" color="primary">{{ icon }}</v-icon>
      </v-avatar>
      <div class="min-w-0">
        <div class="text-body-1 font-weight-medium">{{ title }}</div>
        <div class="text-caption text-medium-emphasis">{{ description }}</div>
      </div>
    </header>

    <!-- No rule under the header: the gap already separates it from the groups,
         and a line plus five outlined cards is more chrome than content. -->
    <div class="d-flex flex-column ga-4 mt-4">
      <slot />
    </div>
  </section>
</template>

<style scoped>
/* A tint of the accent rather than a filled swatch: the section header is a
   label, and a solid block here would compete with the group icons below it,
   which mark the things you can actually act on. */
.section-icon {
  background: rgba(var(--v-theme-primary), 0.14);
}

.min-w-0 {
  min-width: 0;
}
</style>
