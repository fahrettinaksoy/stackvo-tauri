<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import { humaniseField } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';
import SideSheet from '@/components/SideSheet.vue';

/**
 * One service's `.env` settings, edited and applied.
 *
 * Applying is not saving. The container already running was created with the
 * old environment and keeps it through a restart, so a sheet that wrote the
 * key and stopped would be telling the truth about the file and a lie about
 * the service. Everything here is built around making that visible: the button
 * says the container will be rebuilt, and the confirmation says which one.
 */

const props = defineProps({
  service: { type: Object, default: null },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue', 'applied']);

const { t, te } = useI18n();

const settings = ref([]);
const edits = ref({});
const loading = ref(false);
const applying = ref(false);
const confirming = ref(false);
const error = ref(null);

const dirty = computed(() => Object.keys(edits.value).length > 0);
const open = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

/**
 * A field's label in the reader's language, or a readable form of its own name.
 *
 * Only the vocabulary that repeats across services is translated — version,
 * port, password, username and friends. The rest is left in the terms its own
 * documentation uses: `BOOTSTRAP_SERVERS` is what Kafka calls that setting, and
 * a Turkish rendering of it is a phrase nobody can search for. Those fall back
 * to sentence case, which is still an improvement on SHOUTING_SNAKE_CASE.
 */
function fieldLabel(field) {
  // Written out at each call rather than held in a variable: the i18n check
  // reads keys out of the source, and a template literal it cannot see inside
  // a t-family call makes the whole namespace look dead.
  if (te(`serviceSettings.fields.${field}`)) return t(`serviceSettings.fields.${field}`);
  return humaniseField(field);
}

const MASK = '••••••••';

/**
 * Revealing is a view state, not an edit.
 *
 * The first version put the revealed value straight into `edits`, which made
 * simply looking at a password count as a change: the Apply button lit up, the
 * confirmation listed the key, and the value was rewritten to what it already
 * was. Held separately, showing and hiding cost nothing, and a real edit is
 * still an edit — hiding a field you have typed into keeps what you typed.
 */
const secrets = ref({});
const shown = ref(new Set());

const isHidden = (row) => row.secret && !shown.value.has(row.envKey);
/** What the field started as, once revealed — the baseline an edit is against. */
const baseline = (row) => secrets.value[row.envKey] ?? row.value;
const valueOf = (row) => (isHidden(row) ? MASK : (edits.value[row.envKey] ?? baseline(row)));

function edit(row, value) {
  if (value === baseline(row)) delete edits.value[row.envKey];
  else edits.value[row.envKey] = value;
}

async function toggleReveal(row) {
  error.value = null;
  if (shown.value.has(row.envKey)) {
    // New Set rather than a mutation: a Set changed in place is not a change
    // Vue tracks, and the eye would stop matching what the field shows.
    const next = new Set(shown.value);
    next.delete(row.envKey);
    shown.value = next;
    return;
  }
  try {
    if (secrets.value[row.envKey] === undefined) {
      secrets.value[row.envKey] = await api.envReveal(row.envKey);
    }
    shown.value = new Set(shown.value).add(row.envKey);
  } catch (e) {
    error.value = e;
  }
}

async function load() {
  if (!props.service) return;
  loading.value = true;
  error.value = null;
  edits.value = {};
  secrets.value = {};
  shown.value = new Set();
  try {
    settings.value = asList(await api.serviceSettings(props.service.id));
  } catch (e) {
    error.value = e;
    settings.value = [];
  } finally {
    loading.value = false;
  }
}

async function apply() {
  applying.value = true;
  error.value = null;
  try {
    await api.serviceApplySettings(props.service.id, { ...edits.value });
    confirming.value = false;
    emit('applied', props.service.id);
    open.value = false;
  } catch (e) {
    error.value = e;
    confirming.value = false;
  } finally {
    applying.value = false;
  }
}

watch(
  () => [props.modelValue, props.service?.id],
  ([isOpen]) => {
    if (isOpen) load();
  },
  { immediate: true }
);
</script>

<template>
  <SideSheet v-model="open" :title="service?.id ?? ''" icon="mdi-cog-outline" :width="640">
    <ErrorAlert v-if="error" :error="error" class="mb-4" />

    <div v-if="loading" class="d-flex justify-center py-8">
      <v-progress-circular indeterminate />
    </div>

    <template v-else>
      <v-alert
        v-if="!settings.length"
        type="info"
        variant="tonal"
        density="comfortable"
        :text="t('serviceSettings.none')"
      />

      <!-- Spaced by the column rather than by a margin on each field.
           `hide-details` took away the reserved line under every input, which
           was the only thing separating them, and twelve outlined boxes with
           four pixels between them read as one control. -->
      <div class="d-flex flex-column ga-6">
        <v-text-field
          v-for="row in settings"
          :key="row.envKey"
          :model-value="valueOf(row)"
          :label="fieldLabel(row.key)"
          :readonly="isHidden(row)"
          density="comfortable"
          variant="outlined"
          hide-details
          @update:model-value="(v) => edit(row, v)"
        >
          <!-- The `.env` key used to sit under every field as a permanent
               second line, which doubled the height of the form to repeat
               something already in the label. It is on demand now: the
               identifier still matters — it is what you paste into a file or
               search for — but not enough to be read twenty times at once. -->
          <template #prepend-inner>
            <v-tooltip :text="row.envKey" location="top" open-on-click :open-on-hover="false">
              <template #activator="{ props: tip }">
                <v-btn
                  v-bind="tip"
                  size="x-small"
                  variant="text"
                  icon="mdi-tag-outline"
                  class="mr-1"
                  :aria-label="t('serviceSettings.showKey', { key: row.envKey })"
                />
              </template>
            </v-tooltip>
          </template>

          <template #append-inner>
            <v-chip v-if="row.isDefault" size="x-small" variant="tonal" class="mr-1">
              {{ t('serviceSettings.default') }}
            </v-chip>
            <!-- One control, both directions. A reveal with no way back leaves
                 a password on screen until the sheet is closed. -->
            <v-btn
              v-if="row.secret"
              size="x-small"
              variant="text"
              :icon="isHidden(row) ? 'mdi-eye-outline' : 'mdi-eye-off-outline'"
              :aria-label="isHidden(row) ? t('serviceSettings.reveal') : t('serviceSettings.hide')"
              @click="toggleReveal(row)"
            />
          </template>
        </v-text-field>
      </div>
    </template>

    <template #footer>
      <v-spacer />
      <v-btn variant="text" @click="open = false">{{ t('app.cancel') }}</v-btn>
      <v-btn
        color="primary"
        variant="flat"
        prepend-icon="mdi-autorenew"
        :disabled="!dirty"
        @click="confirming = true"
      >
        {{ t('serviceSettings.apply') }}
      </v-btn>
    </template>
  </SideSheet>

  <!-- Asked for by name: applying stops and recreates a container, which is a
       different thing from writing a file, and the user is entitled to know
       that before it happens rather than from the logs afterwards. -->
  <v-dialog v-model="confirming" max-width="460">
    <v-card>
      <v-card-title class="text-h6">{{ t('serviceSettings.confirmTitle') }}</v-card-title>
      <v-card-text>
        <p class="mb-3">{{ t('serviceSettings.confirmBody', { service: service?.id }) }}</p>
        <v-chip
          v-for="key in Object.keys(edits)"
          :key="key"
          size="small"
          variant="tonal"
          class="mr-1 mb-1"
        >
          {{ key }}
        </v-chip>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" :disabled="applying" @click="confirming = false">
          {{ t('app.cancel') }}
        </v-btn>
        <v-btn color="primary" variant="flat" :loading="applying" @click="apply">
          {{ t('serviceSettings.confirmApply') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
