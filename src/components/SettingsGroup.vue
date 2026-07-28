<script setup>
/**
 * One card of related controls inside a settings section.
 *
 * Grouping rather than a flat column of inputs: the preferences pane alone
 * carries eleven controls, and a flat list makes "which of these affect what
 * happens when I close the window" a reading exercise. Each group answers that
 * in its own header.
 *
 * `append` is for a control that acts on the whole group — a save button, a
 * re-check — which belongs beside the group's name rather than lost at the
 * bottom of its contents.
 */
defineProps({
  icon: { type: String, default: '' },
  title: { type: String, default: '' },
  description: { type: String, default: '' },
});
</script>

<template>
  <v-card variant="flat" class="group">
    <div class="d-flex align-center ga-3 px-4 pt-4">
      <v-avatar rounded="lg" size="36" color="primary">
        <v-icon size="18">{{ icon }}</v-icon>
      </v-avatar>
      <div class="min-w-0">
        <div class="text-body-2 font-weight-medium">{{ title }}</div>
        <div class="text-caption text-medium-emphasis">{{ description }}</div>
      </div>
      <v-spacer />
      <slot name="append" />
    </div>

    <div class="pa-4">
      <slot />
    </div>
  </v-card>
</template>

<style scoped>
.group {
  background: rgba(var(--v-theme-surface-bright), 0.55);
  /* A hairline at half the theme's border opacity. `outlined` draws at full
     opacity, which on a dark surface reads as a white box around every group —
     the fill is doing the separating, the line only has to close the shape.
     Derived from the variable rather than fixed, so the high-contrast setting
     still strengthens it. */
  border: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

.min-w-0 {
  min-width: 0;
}
</style>
