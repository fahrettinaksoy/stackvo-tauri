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
  barTitle: { type: String, default: '' },
  hideBar: { type: Boolean, default: false },
});
</script>

<template>
  <div class="page-root">
    <v-card rounded="0" flat class="page-card d-flex flex-column">
      <v-toolbar color="primary" height="100" extended flat>
        <v-toolbar-title class="d-flex align-center text-h5 font-weight-medium">
          <v-icon size="40" class="mr-2">{{ topIcon }}</v-icon
          >{{ topTitle }}
          <slot name="top-title-extra" />
        </v-toolbar-title>

        <template v-if="$slots['top-append']" #append>
          <slot name="top-append" />
        </template>
      </v-toolbar>

      <v-card elevation="2" class="inner-card d-flex flex-column mx-5 mt-n16 mb-4">
        <template v-if="!hideBar">
          <v-toolbar>
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
          <v-divider />
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
   Pinning the root to the viewport minus the app bar bounds the chain. */
.page-root {
  height: calc(100dvh - 64px);
  width: 100%;
  overflow: hidden;
}

.page-card {
  height: 100%;
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
