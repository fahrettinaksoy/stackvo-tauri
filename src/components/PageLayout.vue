<script setup>
/**
 * The page chrome every view sits in — ported from the web UI so the desktop
 * app is recognisably the same product.
 *
 * The shape is a tall primary toolbar with a content card pulled up over it
 * (`mt-n16`), which is what produces the floating-panel look.
 */
defineProps({
  topIcon: { type: String, default: '' },
  topTitle: { type: String, default: '' },
  /** A line under the title saying what the page is for. Optional. */
  topSubtitle: { type: String, default: '' },
  barTitle: { type: String, default: '' },
  hideBar: { type: Boolean, default: false },
  /**
   * Blend the bar into the card: transparent background AND no divider under
   * it. One prop for both because they are one decision — a transparent bar
   * over a hairline reads as a floating line, which is the opposite of the
   * borderless look the flag exists for.
   */
  barTransparent: { type: Boolean, default: false },
});
</script>

<template>
  <div class="page-root">
    <v-card rounded="0" flat class="page-card d-flex flex-column">
      <!-- Pinned to the default density: the card below is pulled up over this
           toolbar by a fixed amount, so letting the interface-density setting
           shrink it slides the title under the card. Density belongs to the
           controls, not to the page's own geometry.
           
           `extension-height` matches the card's `mt-n16` (64px) on purpose.
           The title is centred in the 100px band, so the blue above it is
           `(100 - title) / 2`; below it that same gap is reduced by however
           much the card is pulled up past the extension. At Vuetify's default
           48 the card ate 16px more than the extension gave back, and the
           header sat visibly low in its own band. -->
      <v-toolbar color="primary" height="100" extended extension-height="64" flat density="default">
        <v-toolbar-title class="page-title">
          <v-icon size="40" class="mr-3">{{ topIcon }}</v-icon>
          <div class="d-flex flex-column justify-center">
            <div class="d-flex align-center text-h5 font-weight-medium">
              {{ topTitle }}
              <slot name="top-title-extra" />
            </div>
            <div v-if="topSubtitle" class="text-caption page-subtitle">{{ topSubtitle }}</div>
          </div>
        </v-toolbar-title>

        <template v-if="$slots['top-append']" #append>
          <slot name="top-append" />
        </template>
      </v-toolbar>

      <v-card elevation="2" class="inner-card d-flex flex-column mx-5 mt-n16 mb-4">
        <template v-if="!hideBar">
          <v-toolbar :color="barTransparent ? 'transparent' : undefined">
            <!-- A view with tabs puts them here instead of a title. The page
                 name is already in the toolbar above, so repeating it and then
                 stacking a tab strip underneath costs a row to say nothing. -->
            <slot name="bar">
              <v-toolbar-title class="font-weight-bold">{{ barTitle }}</v-toolbar-title>
            </slot>
            <template v-if="$slots['bar-append']" #append>
              <slot name="bar-append" />
            </template>
          </v-toolbar>
          <v-divider v-if="!barTransparent" />
        </template>

        <div class="layout-body d-flex flex-column">
          <slot />
        </div>
      </v-card>
    </v-card>
  </div>
</template>

<style scoped>
/* v-main only sets a min-height, so percentage heights in the flex chain
   collapse to auto and the inner scroll area never gets a definite height.
   Pinning the root to the viewport minus the app bar bounds the chain.
   Read from Vuetify's own layout variable rather than written as 64px: the app
   bar's height moves with the density setting, and a hard-coded number leaves
   the page overflowing by the difference. */
.page-root {
  height: calc(100dvh - var(--v-layout-top, 64px));
  width: 100%;
  overflow: hidden;
}

.page-card {
  height: 100%;
}

/* Vuetify wraps the title's contents in a `__placeholder` element that is
   `nowrap` and clips overflow — laying the icon and the title block out as flex
   has to happen on that element, not on the title. Without this the icon stacks
   above the text and the subtitle is cut off by the card below. */
.page-title :deep(.v-toolbar-title__placeholder) {
  display: flex;
  align-items: center;
  overflow: visible;
}

/* Quieter than the title without leaving the toolbar's on-primary colour —
   a second full-strength line would read as a second heading. */
.page-subtitle {
  opacity: 0.82;
  line-height: 1.2;
}

.inner-card {
  flex: 1 1 auto;
  min-height: 0;
}

.layout-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}
</style>
