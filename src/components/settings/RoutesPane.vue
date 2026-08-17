<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Names pointed at something StackVo did not start (E-4).
 *
 * ## The notes are the feature
 *
 * The backend rewrites `localhost` to `host.docker.internal`, drops a path a
 * proxy target cannot carry, and says when a name is outside the certificate's
 * suffix. Every one of those is a thing that would otherwise fail silently —
 * a 502, an ignored path, a certificate warning — so the rows show them rather
 * than quietly applying them. A row that says "sending it to
 * host.docker.internal instead" is the difference between this feature and an
 * afternoon.
 *
 * ## Saved whole, and the draft is local until then
 *
 * `routes_save` replaces the list, so this holds a draft and sends it. Editing
 * in place against the server would mean a half-typed hostname reaching the
 * validator on every keystroke.
 */
const { t } = useI18n();

const rows = ref([]);
const saving = ref(false);
const error = ref(null);
const dirty = ref(false);

/** What came back from the last save or load, keyed by domain. */
const checked = ref([]);
const noteFor = (domain) =>
  checked.value.find((route) => route.domain === domain?.trim().toLowerCase());

const canSave = computed(() => rows.value.every((row) => row.domain && row.target));

async function load() {
  error.value = null;
  try {
    checked.value = asList(await api.routesList());
    rows.value = checked.value.map((route) => ({
      // What the user typed, not what the proxy got: an editor that showed the
      // rewritten target would make the next save rewrite the rewrite.
      domain: route.domain,
      target: route.rewrittenFrom ?? route.target,
      enabled: route.enabled !== false,
    }));
    dirty.value = false;
  } catch (e) {
    error.value = e;
  }
}

function add() {
  rows.value.push({ domain: '', target: 'http://localhost:3000', enabled: true });
  dirty.value = true;
}

function remove(index) {
  rows.value.splice(index, 1);
  dirty.value = true;
}

async function save() {
  saving.value = true;
  error.value = null;
  try {
    checked.value = asList(await api.routesSave(rows.value));
    dirty.value = false;
  } catch (e) {
    error.value = e;
  } finally {
    saving.value = false;
  }
}

onMounted(load);
</script>

<template>
  <SettingsGroup
    icon="mdi-arrow-decision-outline"
    :title="t('routes.title')"
    :subtitle="t('routes.subtitle')"
  >
    <ErrorAlert v-if="error" :error="error" class="mb-3" />
    <p class="text-caption text-medium-emphasis mb-4">{{ t('routes.explain') }}</p>

    <div v-if="!rows.length" class="text-caption text-medium-emphasis mb-3">
      {{ t('routes.empty') }}
    </div>

    <div v-for="(row, index) in rows" :key="index" class="mb-3">
      <div class="d-flex align-center ga-2">
        <v-switch
          v-model="row.enabled"
          color="primary"
          density="compact"
          hide-details
          :aria-label="t('routes.enabled')"
          @update:model-value="dirty = true"
        />
        <v-text-field
          v-model="row.domain"
          :label="t('routes.domain')"
          placeholder="api.stackvo.loc"
          density="compact"
          variant="outlined"
          hide-details
          @update:model-value="dirty = true"
        />
        <v-icon size="16" class="text-medium-emphasis">mdi-arrow-right</v-icon>
        <v-text-field
          v-model="row.target"
          :label="t('routes.target')"
          placeholder="http://localhost:3000"
          density="compact"
          variant="outlined"
          hide-details
          @update:model-value="dirty = true"
        />
        <v-btn
          icon="mdi-close"
          variant="text"
          size="small"
          :aria-label="t('routes.remove')"
          @click="remove(index)"
        />
      </div>

      <!-- What the backend changed and why. Applying these quietly is what
           makes each of them a silent failure instead of a sentence. -->
      <div v-if="noteFor(row.domain)?.error" class="note text-error">
        {{ noteFor(row.domain).error }}
      </div>
      <div
        v-for="(note, i) in asList(noteFor(row.domain)?.notes)"
        :key="i"
        class="note text-medium-emphasis"
      >
        {{ note }}
      </div>
    </div>

    <div class="d-flex ga-2">
      <v-btn size="small" variant="text" prepend-icon="mdi-plus" @click="add">
        {{ t('routes.add') }}
      </v-btn>
      <v-spacer />
      <v-btn
        size="small"
        color="primary"
        variant="flat"
        :disabled="!dirty || !canSave"
        :loading="saving"
        @click="save"
      >
        {{ t('routes.save') }}
      </v-btn>
    </div>
  </SettingsGroup>
</template>

<style scoped>
.note {
  padding-inline-start: 56px;
  padding-top: 2px;
  font-size: 0.7rem;
}
</style>
