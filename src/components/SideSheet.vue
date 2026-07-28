<script setup>
import { onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';

/**
 * Material's modal side sheet, as the app's second surface for detail.
 *
 * A dialog covers what it is about; a side sheet sits beside it, which is the
 * difference that matters for a panel you read against a list. Vuetify has no
 * side-sheet component — a `v-navigation-drawer` is the closest thing, and it
 * needs the same four corrections every time, so they live here rather than in
 * each panel that wants one:
 *
 *   - the layout insets a drawer below whatever claimed the top edge and hands
 *     it a z-index under the app bar, so a modal panel opens with one bright,
 *     clickable strip of window above its own scrim;
 *   - the edge border is for a drawer docked beside content, not for one
 *     floating over it, where the same hairline reads as a seam;
 *   - Escape closes a dialog for free and a drawer not at all;
 *   - and the leading corner is the shape that says "this slid in".
 */
const props = defineProps({
  modelValue: { type: Boolean, default: false },
  title: { type: String, default: '' },
  icon: { type: String, default: '' },
  /**
   * Fixed by default, because most panels are a single column of controls and
   * get no better for being wider. A panel whose content does want the room —
   * one with paths and tables in it — computes its own and passes it.
   */
  width: { type: [Number, String], default: 560 },
  /**
   * Hand the content the whole body, unpadded, and stop the body from
   * scrolling — for content that fills and scrolls on its own, like a log.
   */
  flush: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);

const { t } = useI18n();

function close() {
  emit('update:modelValue', false);
}

/**
 * Bound on the window rather than the panel: focus may be anywhere inside the
 * content or, right after opening, nowhere at all.
 */
function onKeydown(event) {
  if (event.key === 'Escape' && props.modelValue) close();
}

window.addEventListener('keydown', onKeydown);
onUnmounted(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <!-- Teleported to the document body.
       A sheet opened from inside a page renders inside `v-main`, whose stacking
       context the app bar's is not inside — so the bar paints over the sheet's
       header however high the sheet's own z-index is, and a page with
       `overflow: hidden` clips the panel outright. Moving the element to the
       body settles both; the layout injection follows the component tree, not
       the DOM, so Vuetify still treats it as a layout item. -->
  <Teleport to="body">
    <v-navigation-drawer
      :model-value="modelValue"
      location="right"
      temporary
      :width="width"
      border="0"
      elevation="12"
      :class="['side-sheet', { 'side-sheet--flush': flush }]"
      @update:model-value="emit('update:modelValue', $event)"
    >
      <!-- `prepend` renders outside `.v-navigation-drawer__content`, which is
           the element that scrolls — so the header and any tabs under it stay
           put while the content moves, the same way the footer does. -->
      <template #prepend>
        <!-- The md3 blueprint gives every bare button `color: primary`, which
             on a primary-filled bar is a button you cannot see. Reset for the
             header and anything slotted into it, rather than per button. -->
        <v-defaults-provider :defaults="{ VBtn: { color: 'on-primary', variant: 'text' } }">
          <header class="side-sheet__header">
            <v-icon v-if="icon" size="24">{{ icon }}</v-icon>
            <span class="text-h6 side-sheet__title">{{ title }}</span>
            <slot name="header-append" />
            <v-spacer />
            <v-btn icon :aria-label="t('a11y.close')" @click="close">
              <v-icon>mdi-close</v-icon>
              <v-tooltip activator="parent">{{ t('a11y.close') }}</v-tooltip>
            </v-btn>
          </header>
        </v-defaults-provider>

        <slot name="tabs" />
      </template>

      <div class="side-sheet__body" :class="{ 'side-sheet__body--flush': flush }">
        <slot />
      </div>

      <!-- Pinned to the floor: the body scrolls past a screenful and whatever
           ends it must not scroll away with it. -->
      <template v-if="$slots.footer" #append>
        <v-divider class="side-sheet__rule" />
        <div class="side-sheet__footer">
          <slot name="footer" />
        </div>
      </template>
    </v-navigation-drawer>
  </Teleport>
</template>

<style scoped>
/* Just above the app bar (1010) and far below Vuetify's overlay stack, which
   starts at 2000 — the selects and menus opened inside a sheet are overlays and
   have to land above it. */
.side-sheet {
  top: 0 !important;
  height: 100% !important;
  max-height: 100% !important;
  z-index: 1020 !important;

  overflow: hidden;
  border-start-start-radius: var(--app-radius);
  border-end-start-radius: var(--app-radius);
}

/* 64dp, 24dp of start padding, dismiss at the end. Filled with the accent: the
   sheet opens over a page that is still lit, and a header the same colour as
   its own body gives it no edge to start from. */
.side-sheet__header {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 64px;
  padding: 0 8px 0 24px;
  background: rgb(var(--v-theme-primary));
  color: rgb(var(--v-theme-on-primary));
}

.side-sheet__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side-sheet__body {
  padding: 8px 24px 24px;
}

.side-sheet__body--flush {
  padding: 0;
  height: 100%;
}

/* The scroll belongs to the content in flush mode, not to the sheet. */
.side-sheet--flush :deep(.v-navigation-drawer__content) {
  overflow: hidden;
}

.side-sheet__rule {
  opacity: 0.5;
}

.side-sheet__footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px;
}

/* Offered to slotted content: a list subheader that labels the block under it
   without drawing a box around it. `:deep` because the content belongs to
   whoever filled the slot. */
.side-sheet__body :deep(.sheet-group) {
  font-size: 12px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  opacity: 0.62;
  margin: 20px 0 8px;
}

.side-sheet__body :deep(.sheet-group:first-child) {
  margin-top: 8px;
}
</style>

<!-- Unscoped: the scrim is teleported out of this component's subtree, so a
     scoped rule cannot reach it. Every scrim in the app belongs to a side
     sheet — the two left drawers are permanent rails and never draw one. -->
<style>
.v-navigation-drawer__scrim {
  /* `!important` because the layout writes the z-index as an inline style. */
  z-index: 1015 !important;
}
</style>
