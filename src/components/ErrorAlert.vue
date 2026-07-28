<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

/**
 * One place that renders a StackvoError.
 *
 * Two things this fixes. The markup was repeated in six views, so a change to
 * how errors look meant six edits. And the message shown was whatever Rust
 * produced — always English, in an app that otherwise speaks two languages.
 *
 * The Rust message is specific ("`imap` was removed in PHP 8.2"); the error
 * code is a category. Showing the localised category as the headline and the
 * specific message underneath keeps both, rather than trading one for the
 * other.
 */
const props = defineProps({
  error: { type: [Object, null], default: null },
  type: { type: String, default: 'error' },
  closable: { type: Boolean, default: false },
});
defineEmits(['close']);

const { t, te } = useI18n();

const headline = computed(() => {
  const code = props.error?.code;
  // Fall back to the raw message when the code has no translation — better a
  // useful English sentence than a placeholder.
  return code && te(`errors.${code}`) ? t(`errors.${code}`) : null;
});

const detail = computed(() => props.error?.message ?? '');
</script>

<template>
  <v-alert
    v-if="error"
    :type="type"
    variant="tonal"
    :closable="closable"
    @click:close="$emit('close')"
  >
    <div v-if="headline" class="text-body-2 font-weight-medium">{{ headline }}</div>
    <div class="text-caption" :class="{ 'mt-1': headline }">{{ detail }}</div>
    <div v-if="error.hint" class="text-caption mt-1 text-medium-emphasis">{{ error.hint }}</div>
  </v-alert>
</template>
