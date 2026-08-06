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
 * The suggestion, translated.
 *
 * This is the line that tells someone what to *do* — start Docker, choose a
 * folder, adopt the directory instead — and it was printed raw. A Turkish user
 * got a translated heading over an English explanation over an English
 * instruction, which is the one of the three worst left in English.
 *
 * `hintKey` comes from the catalogue in `src-tauri/src/hints.rs`; `hint` is the
 * English the Rust side carries either way. Three hints are still built at
 * runtime from a value only the caller has — a program name, a git failure —
 * and those arrive with no key and fall through to the English, exactly as
 * every hint did before.
 *
 * `te` guards the lookup rather than trusting it: a locale missing the key must
 * show the English sentence, not the key itself. The Rust test
 * `hint_translations.rs` is what stops that from happening quietly, but the
 * fallback is what stops it from being ugly if it ever does.
 */
const hint = computed(() => {
  const e = props.error;
  if (!e || typeof e === 'string') return null;
  if (e.hintKey && te(`errorHints.${e.hintKey}`)) return t(`errorHints.${e.hintKey}`);
  return e.hint || null;
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

/**
 * The findings behind a rejected manifest, which were being thrown away.
 *
 * `parse_spec` attaches every one of them — code, path and a sentence naming
 * the field — and nothing rendered them, so a project refused over a single
 * unbuildable extension said only "the project definition is not valid". That
 * is a message you cannot act on: it does not say which of thirty-two
 * extensions, or that the subject is an extension at all.
 *
 * Shaped like the New Project sheet's own validation list, because it is the
 * same data from the same function.
 */
const findings = computed(() => {
  const list = props.error?.details?.errors;
  return Array.isArray(list) ? list.filter((f) => f && (f.message || f.path)) : [];
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
    <ul v-if="findings.length" class="text-caption mt-2 ms-4">
      <li v-for="(f, i) in findings" :key="i">
        <strong v-if="f.code">{{ f.code }}</strong>
        <span v-if="f.path" class="text-medium-emphasis"> {{ f.path }}</span>
        <template v-if="f.message"> — {{ f.message }}</template>
      </li>
    </ul>
    <div v-if="hint" class="text-caption mt-1 text-medium-emphasis">{{ hint }}</div>
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
