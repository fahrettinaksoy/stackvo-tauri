<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';

/**
 * A QR code for one of the addresses this app hands out (M-3).
 *
 * ## Why the encoding is not done here
 *
 * The front end holds the URL and could encode it in JavaScript, which would
 * save a round trip. It would also be a second implementation to keep correct,
 * and the one in `qr.rs` is the one checked against macOS's own decoder. A
 * picture that is subtly wrong is not an error anybody sees until somebody
 * points a phone at it and nothing happens.
 *
 * ## Always black on white, whatever the theme is
 *
 * The rest of this app follows the system theme. A QR code must not: scanners
 * expect dark modules on a light ground, and while some read an inverted code,
 * plenty do not — and the failure is a phone that simply sits there. The quiet
 * zone is drawn for the same reason. It is not padding, it is part of the
 * symbol: four light modules on every side are what makes the finder patterns
 * recognisable, and a code drawn hard against a coloured card is the single
 * most common reason a valid symbol will not scan.
 */
const props = defineProps({
  /** The text to encode. Changing it re-encodes; empty draws nothing. */
  text: { type: String, default: '' },
  /** Drawn size in pixels, quiet zone included. */
  size: { type: Number, default: 168 },
});

const { t } = useI18n();

const symbol = ref(null);
const failed = ref(false);

watch(
  () => props.text,
  async (text) => {
    symbol.value = null;
    failed.value = false;
    if (!text) return;
    try {
      symbol.value = await api.qrEncode(text);
    } catch {
      // A URL too long for the encoder is the only way this fails, and the
      // address is on the screen beside it either way — so this says so
      // quietly rather than raising an alert over a picture.
      failed.value = true;
    }
  },
  { immediate: true }
);

const QUIET = 4;

/** The whole symbol as one SVG path: one `M`/`h`/`v` run per dark module. */
const path = computed(() => {
  const rows = symbol.value?.rows ?? [];
  const parts = [];
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length; x += 1) {
      if (row[x] === '1') parts.push(`M${x + QUIET} ${y + QUIET}h1v1h-1z`);
    }
  });
  return parts.join('');
});

const span = computed(() => (symbol.value?.size ?? 0) + QUIET * 2);
</script>

<template>
  <div v-if="symbol" class="qr" :style="{ width: `${size}px`, height: `${size}px` }">
    <svg
      :viewBox="`0 0 ${span} ${span}`"
      width="100%"
      height="100%"
      shape-rendering="crispEdges"
      role="img"
      :aria-label="t('qr.label', { text })"
    >
      <rect :width="span" :height="span" fill="#ffffff" />
      <path :d="path" fill="#000000" />
    </svg>
  </div>
  <div v-else-if="failed" class="text-caption text-medium-emphasis">{{ t('qr.tooLong') }}</div>
</template>

<style scoped>
.qr {
  /* The white ground is the symbol's, so it needs an edge of its own on a dark
     page — without one it reads as a hole rather than a card. */
  border: 1px solid rgba(128, 128, 128, 0.35);
  border-radius: 4px;
  background: #ffffff;
  padding: 0;
  flex: 0 0 auto;
}
</style>
