<script setup>
import { ref, useId } from 'vue';

/**
 * A pane whose body folds away behind its own heading.
 *
 * For the two panes on the configuration tab whose content has no natural
 * size: the manifest editor is a 24-row textarea whatever the manifest says,
 * and the Dockerfile preview is however many lines the generator emitted —
 * around 120 for a PHP project with extensions. Between them they made a tab
 * you scroll for several screens to reach the panes underneath.
 *
 * Closed on arrival, which is the whole point: both answer questions that are
 * asked occasionally — "what exactly is in the manifest", "what would this
 * build from" — and neither is what the tab is for.
 *
 * The heading is the control. A separate chevron button beside a title that
 * does nothing gives the same action two sizes, and the small one is the only
 * one that works; here the whole title is the button, and anything that acts on
 * the pane rather than opening it goes in `actions`, outside it.
 */
const expanded = ref(false);

/**
 * `aria-controls` needs the body to have an id, and two of these render on one
 * page — so it cannot be a constant. `useId` is per-instance and stable across
 * the server/client boundary, unlike a module counter.
 */
const bodyId = useId();
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="d-flex align-center ga-2">
      <button
        type="button"
        class="pane-toggle"
        :aria-expanded="expanded"
        :aria-controls="bodyId"
        @click="expanded = !expanded"
      >
        <v-icon size="18">{{ expanded ? 'mdi-chevron-down' : 'mdi-chevron-right' }}</v-icon>
        <slot name="title" />
      </button>

      <!-- Beside the title rather than inside the fold: a pane says what it is
           worth opening for while it is shut, or it is a row of headings. -->
      <slot name="meta" />

      <v-spacer />

      <slot name="actions" />
    </div>

    <!-- `v-show`, not `v-if`. The body holds a draft the user may have typed
         and a preview that costs a backend render to produce; unmounting it
         would make the fold a way to lose both. Hidden content takes no
         height, which is what was being asked for. -->
    <v-expand-transition>
      <div v-show="expanded" :id="bodyId" class="pane-body">
        <slot />
      </div>
    </v-expand-transition>
  </v-card>
</template>

<style scoped>
/* The reset a button needs to read as the heading it replaced, plus the cursor
   and the focus ring it needs to read as a button. */
.pane-toggle {
  appearance: none;
  background: none;
  border: 0;
  padding: 0;
  font: inherit;
  text-align: inherit;
  color: inherit;

  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  border-radius: 4px;
}

.pane-toggle:focus-visible {
  outline: 2px solid rgb(var(--v-theme-primary));
  outline-offset: 2px;
}

/* Only when open. A margin above a hidden body is a gap under the heading of
   every closed pane. */
.pane-body {
  padding-top: 12px;
}
</style>
