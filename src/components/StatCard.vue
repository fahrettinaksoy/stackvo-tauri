<script setup>
import { computed } from 'vue';
import { loadColor } from '@/lib/format';

const props = defineProps({
  title: { type: String, required: true },
  icon: { type: String, default: 'mdi-chart-box-outline' },
  /** 0–100; omit for cards that only show detail lines. */
  value: { type: Number, default: null },
  primary: { type: String, default: '—' },
  secondary: { type: String, default: null },
  /** [{ label, value }] rendered under the meter. */
  details: { type: Array, default: () => [] },
});

const hasMeter = computed(() => props.value !== null && !Number.isNaN(props.value));
const color = computed(() => loadColor(props.value));
</script>

<template>
  <v-card height="100%">
    <v-card-item>
      <template #prepend>
        <v-icon :icon="icon" :color="hasMeter ? color : 'primary'" />
      </template>
      <v-card-title class="text-body-2 text-medium-emphasis">{{ title }}</v-card-title>
    </v-card-item>

    <v-card-text>
      <div class="d-flex align-baseline ga-2 mb-3">
        <span class="text-h5 font-weight-medium">{{ primary }}</span>
        <span v-if="secondary" class="text-caption text-medium-emphasis">{{ secondary }}</span>
      </div>

      <!-- Named after the card it belongs to. Vuetify gives the bar
           `role="progressbar"` and `aria-valuenow`, and nothing else: a screen
           reader announced "42" with no indication of what was at 42, on a
           dashboard showing four of these at once. `aria-label` is what turns
           it back into "CPU, 42". Found by the axe pass in
           `tests/a11y-axe.spec.js` on its first run. -->
      <v-progress-linear
        v-if="hasMeter"
        :model-value="value"
        :color="color"
        :aria-label="title"
        height="6"
        rounded
        class="mb-3"
      />

      <div v-for="detail in details" :key="detail.label" class="d-flex justify-space-between">
        <span class="text-caption text-medium-emphasis">{{ detail.label }}</span>
        <span class="text-caption">{{ detail.value }}</span>
      </div>
    </v-card-text>
  </v-card>
</template>
