<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { VCombobox, VTextField } from 'vuetify/components';
import { api, asList } from '@/lib/ipc';
import { settingLabel } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * What a new instance will be, asked before it is one.
 *
 * This exists for one failure. An image reads `MYSQL_ROOT_PASSWORD` while its
 * data directory is empty and never again, so the only moment a password can be
 * set is *before* the first boot — and the only route the app offered was
 * create-with-defaults and then edit, which writes the value, regenerates the
 * compose file, genuinely recreates the container, reports success, and leaves
 * the database on `root`. Every step works and the outcome is wrong.
 *
 * Ports are here for a smaller version of the same thing: the allocator moves
 * on when the preferred number is taken, so somebody could find out that their
 * MySQL is on 3406 by reading a table afterwards.
 *
 * Nothing is written by opening this. `instance_plan` allocates nothing and
 * reserves nothing — it answers what would happen — and `instance_create`
 * allocates again for real, because between the two something else may have
 * taken a port.
 */

const props = defineProps({
  /** `{ service, version }`, or null when the dialog is closed. */
  target: { type: Object, default: null },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue', 'create']);

const { t, te, locale } = useI18n();

const plan = ref(null);
const loading = ref(false);
const error = ref(null);

const values = ref({});
const ports = ref({});

const open = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

const fieldLabel = (row) => settingLabel(row, { t, te, locale: locale.value });

const rows = computed(() => plan.value?.settings ?? []);
const portRows = computed(() => plan.value?.ports ?? []);

const valueOf = (row) => values.value[row.key] ?? '';
const setValue = (row, v) => (values.value[row.key] = String(v ?? ''));

const asBool = (row) => String(valueOf(row)).toLowerCase() === 'true';
const setBool = (row, on) => setValue(row, on ? 'true' : 'false');

const portOf = (row) => ports.value[row.name] ?? '';
const setPort = (row, v) => (ports.value[row.name] = String(v ?? '').trim());

/**
 * A required field left empty, and a port that is not a port.
 *
 * Unlike the settings sheet, a secret counts here: there is no mask on this
 * form, because there is no stored value to hide — every field holds the
 * manifest's published default, which is sitting in a JSON file on disk.
 */
const missing = computed(() => rows.value.filter((row) => row.required && !valueOf(row).trim()));

const badPort = (row) => {
  const n = Number(portOf(row));
  return !Number.isInteger(n) || n < 1 || n > 65535;
};

/**
 * A port the plan could not find a number for.
 *
 * `instance_plan` answers null per port rather than failing to open, so the
 * dialog can say which one and let the user type a number themselves.
 */
const unallocated = computed(() => portRows.value.filter((row) => !portOf(row).length));

const blocked = computed(
  () =>
    !!plan.value?.refused || missing.value.length > 0 || portRows.value.some((row) => badPort(row))
);

async function load() {
  if (!props.target) return;
  loading.value = true;
  error.value = null;
  plan.value = null;
  values.value = {};
  ports.value = {};
  try {
    const answer = await api.instancePlan(props.target.service, props.target.version);
    plan.value = answer;
    // Seeded from the plan rather than left empty: this form is a chance to
    // change what a package ships with, not a demand to retype it.
    for (const row of asList(answer?.settings)) values.value[row.key] = row.value ?? '';
    for (const row of asList(answer?.ports))
      ports.value[row.name] = row.host ? String(row.host) : '';
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

/**
 * Only what differs from the package's own defaults.
 *
 * A patch that repeats every default would write them all into
 * `instances.json`, which then stops tracking the package: a later version that
 * changes a default would be overridden by a value nobody chose.
 */
function create() {
  const settings = Object.fromEntries(
    rows.value
      .filter((row) => valueOf(row) !== (row.defaultValue ?? ''))
      .map((row) => [row.key, valueOf(row)])
  );
  const chosenPorts = Object.fromEntries(
    portRows.value
      .filter((row) => Number(portOf(row)) !== row.host)
      .map((row) => [row.name, Number(portOf(row))])
  );

  emit('create', {
    service: props.target.service,
    version: props.target.version,
    settings: Object.keys(settings).length ? settings : null,
    ports: Object.keys(chosenPorts).length ? chosenPorts : null,
  });
  open.value = false;
}

watch(
  () => [props.modelValue, props.target?.service, props.target?.version],
  ([isOpen]) => {
    if (isOpen) load();
  },
  { immediate: true }
);
</script>

<template>
  <v-dialog v-model="open" max-width="620" scrollable>
    <v-card>
      <v-card-title class="text-h6">
        {{ t('marketView.createTitle', { id: plan?.id ?? target?.service ?? '' }) }}
      </v-card-title>

      <v-card-text>
        <ErrorAlert v-if="error" :error="error" class="mb-4" />

        <div v-if="loading" class="d-flex justify-center py-8">
          <v-progress-circular indeterminate />
        </div>

        <template v-else-if="plan">
          <!-- Reported by the plan rather than thrown on open, so the dialog
               can say why the button is off instead of failing to appear. -->
          <v-alert
            v-if="plan.refused"
            type="warning"
            variant="tonal"
            density="compact"
            class="mb-4"
            :text="plan.refused"
          />

          <!-- The sentence this dialog exists for. It is shown before the
               fields, not after: it is the reason to look at them. -->
          <div class="text-body-2 text-medium-emphasis mb-4">
            {{ t('marketView.createBody') }}
          </div>

          <div v-if="rows.length" class="d-flex flex-column ga-6">
            <template v-for="row in rows" :key="row.key">
              <v-switch
                v-if="row.kind === 'bool'"
                :model-value="asBool(row)"
                :label="row.required ? `${fieldLabel(row)} *` : fieldLabel(row)"
                color="primary"
                density="comfortable"
                hide-details
                @update:model-value="(v) => setBool(row, v)"
              />
              <component
                :is="row.options?.length ? VCombobox : VTextField"
                v-else
                :model-value="valueOf(row)"
                :items="row.options?.length ? row.options : undefined"
                :label="row.required ? `${fieldLabel(row)} *` : fieldLabel(row)"
                :error="missing.includes(row)"
                :type="row.kind === 'int' ? 'number' : 'text'"
                density="comfortable"
                variant="outlined"
                hide-details
                @update:model-value="(v) => setValue(row, v)"
              >
                <!-- No mask and no eye, unlike the settings sheet, and the
                     difference is what the value is: there is no instance yet
                     and no keystore entry, so this is the package's published
                     first-boot default. Hiding a string that is in a file on
                     disk would be theatre, and the whole point of the field is
                     that `root` gets changed before it means anything. -->
                <template #append-inner>
                  <v-icon v-if="row.secret" size="small" color="warning">mdi-key-outline</v-icon>
                </template>
              </component>
            </template>
          </div>

          <div v-else class="text-caption text-medium-emphasis">
            {{ t('instanceSettings.none') }}
          </div>

          <template v-if="portRows.length">
            <div class="text-subtitle-2 mt-8 mb-1">{{ t('instanceSettings.ports') }}</div>
            <!-- Named when the allocator could not find one, because that is
                 the case where the empty field is the message. -->
            <div v-if="unallocated.length" class="text-caption text-warning mb-3">
              {{
                t('marketView.createNoPort', { handles: unallocated.map((r) => r.name).join(', ') })
              }}
            </div>
            <div class="d-flex flex-column ga-6">
              <v-text-field
                v-for="row in portRows"
                :key="row.name"
                :model-value="portOf(row)"
                :label="row.name"
                :error="badPort(row)"
                type="number"
                density="comfortable"
                variant="outlined"
                hide-details
                @update:model-value="(v) => setPort(row, v)"
              />
            </div>
          </template>
        </template>
      </v-card-text>

      <v-card-actions>
        <span v-if="missing.length" class="text-caption text-error ml-2">
          {{
            t('instanceSettings.requiredMissing', { keys: missing.map((r) => r.key).join(', ') })
          }}
        </span>
        <v-spacer />
        <v-btn variant="text" @click="open = false">{{ t('app.cancel') }}</v-btn>
        <v-btn color="primary" variant="flat" :disabled="blocked || loading" @click="create">
          {{ t('marketView.addInstance') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
