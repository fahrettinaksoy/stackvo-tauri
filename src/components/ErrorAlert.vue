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
  error: { type: [Object, String, null], default: null },
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

/**
 * Whatever was thrown, said out loud.
 *
 * It read `error.message` and nothing else, which is right for this app's own
 * errors and wrong for everything else that can reach here: a Tauri plugin
 * rejects with a plain string, and a string has no `.message`. The result was
 * a red box with nothing in it — worse than no box, because it says something
 * failed and refuses to say what.
 */
const detail = computed(() => {
  const e = props.error;
  if (!e) return '';
  if (typeof e === 'string') return e;
  if (typeof e.message === 'string' && e.message) return e.message;
  // Last resort. `String(e)` on a bare object gives "[object Object]", which is
  // no more useful than the empty box; JSON at least carries the fields.
  try {
    const text = JSON.stringify(e);
    return text && text !== '{}' ? text : String(e);
  } catch {
    return String(e);
  }
});
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

<style scoped>
/* Vuetify ships `.v-alert { flex: 1 1 }`, which is right for an alert laid out
   in a row beside something else and wrong for every use here: each of these
   sits in a `flex-direction: column` page body, so `flex-grow: 1` handed the
   alert all the space the content below did not use. A two-line message about
   a failed delete came out as a 300px red panel with the text floating in the
   middle of it. The height of an alert should be the height of what it says. */
.v-alert {
  flex: 0 0 auto;
}
</style>
