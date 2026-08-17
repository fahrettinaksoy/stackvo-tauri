<script setup>
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useCommands, matchCommands } from '@/composables/useCommands';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * One box, every action, reachable from the keyboard.
 *
 * A-2, the visible half. `useCommands` decides what exists; this decides how it
 * is chosen. The shortcut that opens it lives in `App.vue` rather than here,
 * because a listener owned by a component that only mounts while open cannot be
 * what opens it.
 *
 * ## The list is a listbox, not a menu
 *
 * `role="listbox"` with a single `aria-activedescendant` on the input, rather
 * than moving focus row to row. Focus has to stay in the text field — the user
 * is still typing — so the rows can never be focused, and a screen reader
 * needs some other way to be told which one is current. That is exactly the
 * pattern `aria-activedescendant` exists for; a `role="menu"` here would
 * promise arrow-key focus movement that this deliberately does not do.
 *
 * ## Rows are buttons anyway
 *
 * They are driven by the keyboard, but they are also clickable, and a div with
 * a click handler is not reachable, not announced and not a control. The
 * repository has a source guard for that shape.
 */
const model = defineModel({ type: Boolean, default: false });

const { t } = useI18n();
const { commands } = useCommands();

const query = ref('');
const cursor = ref(0);
const error = ref(null);
const input = ref(null);

const results = computed(() => matchCommands(commands.value, query.value));

/**
 * Section headers, computed from the result order rather than by grouping.
 *
 * Grouping would have to re-sort the results into their sections, which throws
 * away the ranking the matcher just produced — the best match would stop being
 * the first row the moment it belonged to the second section. So the list stays
 * in rank order and a header is drawn wherever the section changes; with a
 * query typed, that is usually no headers at all, which is correct.
 */
const rows = computed(() => {
  let last = null;
  return results.value.map((command, index) => {
    const heading = command.section !== last ? command.section : null;
    last = command.section;
    return { command, index, heading };
  });
});

// Typing moves the ground under the cursor: what was row 4 may not exist.
watch(query, () => {
  cursor.value = 0;
});

watch(model, async (open) => {
  if (!open) return;
  // Opened fresh every time. A palette that remembered the last query would
  // make the second use start by clearing the first, which is a keystroke
  // spent undoing something the user did not ask for.
  query.value = '';
  cursor.value = 0;
  error.value = null;
  await nextTick();
  input.value?.focus();
});

function move(delta) {
  const count = results.value.length;
  if (!count) return;
  // Wraps, because a list this short is faster to reach from the other end and
  // an arrow key that stops dead reads as a stuck control.
  cursor.value = (cursor.value + delta + count) % count;
}

async function run(command) {
  if (!command || command.disabled) return;
  // Closed before the command runs, not after: several of these open a drawer
  // or navigate, and a palette still on screen over the thing it just opened is
  // a second overlay the user has to dismiss.
  model.value = false;
  try {
    await command.run();
  } catch (e) {
    error.value = e;
  }
}

function onKeydown(event) {
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    move(1);
  } else if (event.key === 'ArrowUp') {
    event.preventDefault();
    move(-1);
  } else if (event.key === 'Enter') {
    event.preventDefault();
    run(results.value[cursor.value]);
  }
  // Escape is left to the dialog, which already closes on it.
}

/** Keep the highlighted row in view while the arrows walk past the fold. */
watch(cursor, async () => {
  await nextTick();
  document.getElementById(`palette-row-${cursor.value}`)?.scrollIntoView({ block: 'nearest' });
});
</script>

<template>
  <!-- Top-anchored rather than centred: the box grows downward as results
       arrive, and a centred dialog would move under the cursor while the user
       is still typing. -->
  <v-dialog v-model="model" max-width="620" scrollable location-strategy="static">
    <v-card class="palette">
      <div class="palette-field">
        <v-icon size="20" class="mr-3 text-medium-emphasis">mdi-console-line</v-icon>
        <input
          ref="input"
          v-model="query"
          type="text"
          class="palette-input"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          :aria-activedescendant="results.length ? `palette-row-${cursor}` : undefined"
          :aria-label="t('palette.title')"
          :placeholder="t('palette.placeholder')"
          @keydown="onKeydown"
        />
      </div>

      <v-divider />

      <div v-if="!results.length" class="pa-6 text-center text-caption text-medium-emphasis">
        {{ t('palette.empty', { query: query.trim() }) }}
      </div>

      <div
        v-else
        id="palette-list"
        role="listbox"
        :aria-label="t('palette.title')"
        class="palette-list"
      >
        <template v-for="row in rows" :key="row.command.id">
          <div v-if="row.heading" class="palette-heading">{{ row.heading }}</div>
          <button
            :id="`palette-row-${row.index}`"
            type="button"
            role="option"
            class="palette-row"
            :class="{ 'is-current': row.index === cursor, 'is-disabled': row.command.disabled }"
            :aria-selected="row.index === cursor"
            :aria-disabled="row.command.disabled ? 'true' : undefined"
            @click="run(row.command)"
            @mousemove="cursor = row.index"
          >
            <v-icon size="18" class="palette-row-icon">{{ row.command.icon }}</v-icon>
            <span class="palette-row-label">{{ row.command.label }}</span>
            <span v-if="row.command.hint" class="palette-row-hint">{{ row.command.hint }}</span>
          </button>
        </template>
      </div>

      <v-divider />

      <div class="palette-foot text-caption text-medium-emphasis">{{ t('palette.keys') }}</div>
    </v-card>

    <v-snackbar :model-value="!!error" color="transparent" location="bottom" timeout="8000">
      <ErrorAlert :error="error" type="error" closable @close="error = null" />
    </v-snackbar>
  </v-dialog>
</template>

<style scoped>
.palette {
  overflow: hidden;
}

.palette-field {
  display: flex;
  align-items: center;
  padding: 14px 18px;
}

/* A bare input rather than `v-text-field`: this needs `role="combobox"` and
   `aria-activedescendant` on the element that has focus, and Vuetify owns those
   attributes on its own field. */
.palette-input {
  flex: 1 1 auto;
  min-width: 0;
  border: 0;
  outline: none;
  background: none;
  font: inherit;
  font-size: 1rem;
  color: rgb(var(--v-theme-on-surface));
}

.palette-list {
  max-height: 360px;
  overflow-y: auto;
  padding: 6px;
}

.palette-heading {
  padding: 10px 12px 4px;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  opacity: 0.55;
}

.palette-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 12px;
  border: 0;
  border-radius: var(--app-radius);
  background: none;
  font: inherit;
  font-size: 0.875rem;
  text-align: start;
  cursor: pointer;
}

.palette-row.is-current {
  background: rgba(var(--v-theme-primary), 0.12);
}

.palette-row.is-disabled {
  opacity: 0.45;
  cursor: default;
}

.palette-row-icon {
  flex: 0 0 auto;
  opacity: 0.75;
}

.palette-row-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.palette-row-hint {
  margin-inline-start: auto;
  padding-inline-start: 12px;
  opacity: 0.55;
  font-size: 0.75rem;
}

.palette-foot {
  padding: 8px 18px;
}
</style>
