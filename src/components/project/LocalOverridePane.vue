<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * `stackvo.local.json` — what this machine does differently (B-2).
 *
 * The pane above holds the committed manifest, which is the thing that makes a
 * checkout reproducible and is exactly why there was nowhere to say "on *this*
 * machine, PHP 8.3, because I am chasing a bug in it". This is that place.
 *
 * ## Two editors, not one with a switch
 *
 * They are different files with different rules — one is committed and one must
 * not be, one is validated whole and one is a fragment — and a single editor
 * with a toggle would put the reader one misread control away from typing a
 * machine setting into the file the team shares. Two panes cannot be confused
 * for one another.
 *
 * ## The git answer is reported, not enforced
 *
 * Whether the file stays out of a commit is a fact about the user's repository
 * and the user's ignore rules, and this app does not write either. So it asks
 * git and says what git said — including "git had no answer", which is a third
 * state and not a warning: a project directory that is not under version
 * control has nothing to leak into anybody's clone.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();

const state = ref(null);
const text = ref('');
const dirty = ref(false);
const saving = ref(false);
const loading = ref(false);
const error = ref(null);

const applied = computed(() => asList(state.value?.applied));
const refused = computed(() => asList(state.value?.refused));

/** A starting point, so the empty state is not a blank box with no clue in it. */
const EXAMPLE = `{
  "php": { "version": "8.3" }
}
`;

async function load() {
  loading.value = true;
  error.value = null;
  try {
    state.value = await api.projectLocalRead(props.name);
    text.value = state.value.text;
    dirty.value = false;
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  error.value = null;
  try {
    state.value = await api.projectLocalWrite(props.name, text.value);
    text.value = state.value.text;
    dirty.value = false;
    emit('changed');
  } catch (e) {
    error.value = e;
  } finally {
    saving.value = false;
  }
}

/**
 * Removing the file is saving nothing, which is the same command.
 *
 * A separate `delete` would be a second way to reach one state, and the two
 * would eventually disagree about what happens to the editor afterwards.
 */
async function clear() {
  text.value = '';
  await save();
}

const emit = defineEmits(['changed']);

onMounted(load);
watch(() => props.name, load);
</script>

<template>
  <v-card variant="flat" class="pane">
    <div class="d-flex align-center ga-2 mb-1">
      <div class="section-head">
        <v-icon size="18" class="mr-2">mdi-laptop</v-icon>{{ t('local.title') }}
      </div>
      <v-spacer />
      <v-btn
        v-if="state?.exists"
        size="small"
        variant="text"
        prepend-icon="mdi-delete-outline"
        :loading="saving"
        @click="clear"
      >
        {{ t('local.remove') }}
      </v-btn>
      <v-btn
        size="small"
        color="primary"
        variant="flat"
        :disabled="!dirty"
        :loading="saving"
        @click="save"
      >
        {{ t('detail.save') }}
      </v-btn>
    </div>

    <p class="text-caption text-medium-emphasis mb-3">{{ t('local.explain') }}</p>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <!-- What is actually in force. Named field by field rather than "overrides
         active": the whole hazard of this feature is a value in effect that
         nobody remembers setting. -->
    <v-alert v-if="applied.length" type="info" variant="tonal" density="compact" class="mb-3">
      <div class="text-caption mb-1">{{ t('local.applied') }}</div>
      <v-chip v-for="field in applied" :key="field" size="x-small" class="mr-1 mb-1" label>
        {{ field }}
      </v-chip>
    </v-alert>

    <!-- Named, not dropped: a file that quietly ignores the one key somebody
         set is how the feature gets written off as broken. -->
    <v-alert v-if="refused.length" type="warning" variant="tonal" density="compact" class="mb-3">
      <div class="text-caption">{{ t('local.refused', { keys: refused.join(', ') }) }}</div>
    </v-alert>

    <!-- Three states, and only one of them is a warning. -->
    <v-alert
      v-if="state?.exists && state.ignored === false"
      type="warning"
      variant="tonal"
      density="compact"
      class="mb-3"
    >
      <div class="text-caption">{{ t('local.notIgnored') }}</div>
    </v-alert>
    <div
      v-else-if="state?.exists && state.ignored === true"
      class="text-caption text-medium-emphasis mb-3"
    >
      <v-icon size="14" class="mr-1">mdi-check</v-icon>{{ t('local.ignored') }}
    </div>

    <v-textarea
      v-model="text"
      :aria-label="t('local.title')"
      :placeholder="EXAMPLE"
      variant="outlined"
      rows="8"
      class="mono-input"
      hide-details
      :loading="loading"
      @update:model-value="dirty = true"
    />
  </v-card>
</template>
