<script setup>
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * What this project says it needs, and whether this machine gives it.
 *
 * The pane keeps two lists apart that it would be easy to merge, and the
 * separation is the feature. **Declared** comes from `stackvo.json`, which is
 * committed — somebody wrote it down and a colleague cloned it. **Suggested**
 * comes from the project's own `.env`, which is a guess this app made from
 * `DB_CONNECTION=pgsql` and the like. Presenting a guess as a commitment is how
 * a repository ends up declaring a service nobody chose, and the declaration is
 * exactly the thing the next person will trust without checking.
 *
 * So a suggestion is never enabled and never written on its own: it is offered,
 * and writing it is a separate click that changes a file in their repository.
 */
const props = defineProps({
  name: { type: String, required: true },
});
const emit = defineEmits(['apply']);

const { t } = useI18n();

const state = ref({ declared: [], suggested: [], plan: null });
const picked = ref([]);
const error = ref(null);
const loading = ref(false);
const busy = ref(false);
const written = ref(false);

const missing = computed(() => state.value.declared.filter((s) => s.known && !s.enabled));
const unknown = computed(() => state.value.declared.filter((s) => !s.known));
const empty = computed(() => !state.value.declared.length && !state.value.suggested.length);

/** The profiles to start after applying: what was declared and exists. */
const startable = computed(() => state.value.declared.filter((s) => s.known).map((s) => s.id));

async function load() {
  loading.value = true;
  error.value = null;
  try {
    state.value = await api.projectRequirements(props.name);
    picked.value = state.value.suggested.map((s) => s.service);
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

/** Write the picked suggestions into the manifest, alongside what is declared. */
async function declare() {
  busy.value = true;
  error.value = null;
  try {
    const ids = [...state.value.declared.map((s) => s.id), ...picked.value];
    await api.projectRequirementsDeclare(props.name, ids);
    written.value = true;
    await load();
  } catch (e) {
    await load();
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function apply() {
  busy.value = true;
  error.value = null;
  try {
    const services = startable.value;
    await api.projectRequirementsApply(props.name);
    // The .env is written; regenerating and starting is the caller's, because
    // it is a long operation with its own progress console.
    emit('apply', services);
    await load();
  } catch (e) {
    await load();
    error.value = e;
  } finally {
    busy.value = false;
  }
}

watch(() => props.name, load, { immediate: true });

defineExpose({ load });
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <SettingsGroup
    icon="mdi-cube-scan"
    :title="t('requirements.title')"
    :description="t('requirements.description')"
  >
    <v-progress-linear v-if="loading" indeterminate class="mb-2" />

    <div v-if="empty && !loading" class="text-body-2 text-medium-emphasis">
      {{ t('requirements.none') }}
    </div>

    <!-- ---- declared ---------------------------------------------------- -->
    <template v-if="state.declared.length">
      <div class="text-caption text-medium-emphasis mb-2">
        {{ t('requirements.declaredBy') }}
      </div>
      <v-list density="compact" class="bg-transparent">
        <v-list-item v-for="s in state.declared" :key="s.id" class="px-0">
          <template #prepend>
            <v-icon
              :icon="
                !s.known
                  ? 'mdi-help-circle-outline'
                  : s.enabled
                    ? 'mdi-check-circle-outline'
                    : 'mdi-circle-outline'
              "
              :color="!s.known ? 'warning' : s.enabled ? 'success' : undefined"
              class="mr-3"
            />
          </template>
          <v-list-item-title class="text-body-2">{{ s.id }}</v-list-item-title>
          <v-list-item-subtitle class="text-caption">
            {{
              !s.known
                ? t('requirements.state.unknown')
                : s.enabled
                  ? t('requirements.state.enabled')
                  : t('requirements.state.missing')
            }}
          </v-list-item-subtitle>
        </v-list-item>
      </v-list>

      <v-alert
        v-if="unknown.length"
        type="warning"
        variant="tonal"
        density="comfortable"
        class="mb-4"
        :text="t('requirements.unknownExplained')"
      />

      <div v-if="missing.length" class="d-flex align-center ga-3 flex-wrap mb-2">
        <v-btn
          color="primary"
          variant="tonal"
          :loading="busy"
          prepend-icon="mdi-play-circle-outline"
          @click="apply"
        >
          {{ t('requirements.enable', { count: missing.length }) }}
        </v-btn>
        <span class="text-caption text-medium-emphasis">
          {{ t('requirements.enableDetail') }}
        </span>
      </div>
    </template>

    <!-- ---- suggested --------------------------------------------------- -->
    <template v-if="state.suggested.length">
      <v-divider v-if="state.declared.length" class="my-4" />

      <div class="text-caption text-medium-emphasis mb-1">
        {{ t('requirements.suggestedBy') }}
      </div>
      <!-- Said before the checkboxes: this is a guess, and the file it would be
           written into is the one their colleague will trust. -->
      <div class="text-caption text-medium-emphasis mb-2">
        {{ t('requirements.suggestedCaveat') }}
      </div>

      <v-checkbox
        v-for="s in state.suggested"
        :key="s.service"
        v-model="picked"
        :value="s.service"
        density="compact"
        hide-details
        class="mt-0"
      >
        <template #label>
          <span class="text-body-2">{{ s.service }}</span>
          <span class="text-caption text-medium-emphasis ml-2">
            {{ t('requirements.becauseOf', { key: s.key }) }}
          </span>
        </template>
      </v-checkbox>

      <v-btn
        class="mt-3"
        variant="tonal"
        :loading="busy"
        :disabled="!picked.length"
        prepend-icon="mdi-content-save-outline"
        @click="declare"
      >
        {{ t('requirements.declare', { count: picked.length }) }}
      </v-btn>
    </template>

    <v-alert
      v-if="written"
      type="success"
      variant="tonal"
      density="comfortable"
      class="mt-4"
      :text="t('requirements.written')"
    />
  </SettingsGroup>
</template>
