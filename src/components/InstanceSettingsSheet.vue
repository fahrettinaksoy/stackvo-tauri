<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
// Named rather than left to the template's auto-import: `<component :is>` takes
// a value, and `VCombobox` as a bare identifier in a template resolves to
// nothing the plugin has been asked to provide.
import { VCombobox, VTextField } from 'vuetify/components';
import { api, asList } from '@/lib/ipc';
import { settingLabel } from '@/lib/manifest';
import ErrorAlert from '@/components/ErrorAlert.vue';
import SideSheet from '@/components/SideSheet.vue';

/**
 * One instance's settings, edited and applied.
 *
 * The settings are the manifest's — a package says what it can be configured
 * with, and this renders that list without knowing what any of it means. There
 * is no `.env` here and that is the point: two versions of MySQL can be running
 * and `SERVICE_MYSQL_DATABASE` names neither of them.
 *
 * Applying is not saving. The container already running was created with the
 * old environment and keeps it through a restart, so a sheet that wrote the
 * value and stopped would be telling the truth about the file and a lie about
 * the service. Everything here is built around making that visible: the button
 * says the container will be rebuilt, and the confirmation says which one.
 */

const props = defineProps({
  instance: { type: Object, default: null },
  modelValue: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue', 'applied']);

const { t, te, locale } = useI18n();

const settings = ref([]);
const edits = ref({});
const loading = ref(false);
const applying = ref(false);
const confirming = ref(false);
const error = ref(null);

/**
 * Host ports, edited beside the settings.
 *
 * Not settings — the manifest declares them separately and `instances.json`
 * holds them in their own map — but they belong on this sheet because until now
 * there was no way to change one at all. `instance_create` allocates, moves on
 * when the preferred number is taken, and nothing afterwards could move it: a
 * user whose 3306 had gone elsewhere had to edit `instances.json` by hand,
 * which is a regression on the `.env` model this replaced.
 *
 * Read off the instance row rather than fetched: `instance_list` already
 * carries the handle → port map, and a second command for a map the caller was
 * handed on the way in is a round trip for nothing.
 */
const portEdits = ref({});

const ports = computed(() =>
  Object.entries(props.instance?.ports ?? {}).map(([handle, port]) => ({ handle, port }))
);

const portValue = (row) => portEdits.value[row.handle] ?? String(row.port);

function editPort(row, value) {
  const next = String(value ?? '').trim();
  if (next === String(row.port)) delete portEdits.value[row.handle];
  else portEdits.value[row.handle] = next;
}

/**
 * A port that cannot be sent, told apart from one that is merely taken.
 *
 * Range only. Whether a number is free is a question about this machine and
 * about the instance table, and both answers live in Rust — a form that guessed
 * would be a second opinion that goes stale between the guess and the write.
 */
const badPort = (row) => {
  const raw = portEdits.value[row.handle];
  if (raw === undefined) return false;
  const n = Number(raw);
  return !Number.isInteger(n) || n < 1 || n > 65535;
};

const portsInvalid = computed(() => ports.value.some(badPort));

const dirty = computed(
  () => Object.keys(edits.value).length > 0 || Object.keys(portEdits.value).length > 0
);
const open = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

/**
 * A field's label. Shared with the create dialog, which renders the same rows
 * from the same manifest — see `settingLabel`.
 */
const fieldLabel = (row) => settingLabel(row, { t, te, locale: locale.value });

const MASK = '••••••••';

/**
 * Revealing is a view state, not an edit.
 *
 * Putting the revealed value straight into `edits` would make simply looking at
 * a password count as a change: the Apply button lights up, the confirmation
 * lists the key, and the value is rewritten to what it already was. Held
 * separately, showing and hiding cost nothing, and a real edit is still an edit
 * — hiding a field you have typed into keeps what you typed.
 */
const secrets = ref({});
const shown = ref(new Set());

const isHidden = (row) => row.secret && !shown.value.has(row.key);
/** What the field started as, once revealed — the baseline an edit is against. */
const baseline = (row) => secrets.value[row.key] ?? row.value;
const valueOf = (row) => (isHidden(row) ? MASK : (edits.value[row.key] ?? baseline(row)));

function edit(row, value) {
  if (value === baseline(row)) delete edits.value[row.key];
  else edits.value[row.key] = value;
}

/**
 * A boolean setting, as a boolean.
 *
 * The manifest's type was arriving and being ignored: `kind` crossed the
 * boundary and the form asked one question — does this row carry options —
 * so a `bool` rendered as a text box somebody was expected to type the word
 * `true` into, and `int` as a box that accepted `eight`.
 *
 * Stored as a string either way, because that is what a compose file
 * interpolates and what `instances.json` holds. Only the control changes.
 */
const asBool = (row) => String(valueOf(row) ?? '').toLowerCase() === 'true';
const editBool = (row, on) => edit(row, on ? 'true' : 'false');

/**
 * Put a field back to what the package ships.
 *
 * The sheet has always said which values are the manifest's own — that is the
 * "default" chip — and offered no way back to one once it had been changed, so
 * undoing meant remembering. Offered only where there is something to put back
 * and it is not already there.
 *
 * Never for a secret: its default does not cross the boundary, on purpose. A
 * password is restored by revealing it and typing, which is a request the user
 * makes rather than a value the form is handed.
 */
const canReset = (row) =>
  row.defaultValue !== null &&
  row.defaultValue !== undefined &&
  String(valueOf(row) ?? '') !== row.defaultValue;

const reset = (row) => edit(row, row.defaultValue);

/**
 * What is about to be written, as `was → is`.
 *
 * The confirmation used to list the keys and nothing else, on a dialog whose
 * whole job is to be the last look before a container is stopped and rebuilt.
 * A secret shows the key alone: the point of the mask is not to put the old
 * password on screen, least of all beside the new one.
 */
const changes = computed(() => [
  ...Object.entries(edits.value).map(([key, next]) => {
    const row = settings.value.find((r) => r.key === key);
    return { key, secret: !!row?.secret, from: row ? baseline(row) : '', to: next };
  }),
  // Ports are in the same list rather than a second one: they are rebuilt by
  // the same press, and two lists under one confirmation would read as two
  // things about to happen.
  ...Object.entries(portEdits.value).map(([handle, next]) => ({
    key: t('instanceSettings.portOf', { handle }),
    secret: false,
    from: String(props.instance?.ports?.[handle] ?? ''),
    to: next,
  })),
]);

/**
 * Required fields left empty.
 *
 * A manifest marking a setting required is saying the service will not start
 * without it, and the sheet used to render that flag as nothing at all — the
 * field looked like every other one, emptying it applied, the container was
 * recreated and failed to boot. `instance_apply_settings` refuses this too; the
 * point of having it here as well is that a refusal arriving *after* a
 * container has been stopped is a worse way to learn it.
 *
 * A masked secret never counts. Its value is eight bullets whether the keystore
 * holds a password or has never been written, so "is it empty" is a question
 * this side cannot answer without revealing it — and a form that revealed
 * secrets in order to validate them would be doing the one thing the mask is
 * for. Revealed or typed into, it is an ordinary string again and is checked.
 */
const missing = computed(() =>
  settings.value.filter((row) => row.required && !String(valueOf(row) ?? '').trim())
);

const isMissing = (row) => missing.value.includes(row);

/**
 * Settings an image reads once, when it initialises its data directory.
 *
 * This is the sharpest thing wrong with the form and nothing on screen said
 * it: `MYSQL_ROOT_PASSWORD` is consulted by the entrypoint only while
 * `/var/lib/mysql` is empty. Change it on an instance that has data, and the
 * value is written to `instances.json`, the compose file is regenerated, the
 * container is genuinely recreated with the new environment — and the password
 * in the database does not move. Every step reports success and the service is
 * still on the old credential.
 *
 * Matched on the key, which is a heuristic and is marked as one. The durable
 * answer is a manifest field — a package knows which of its settings are
 * first-boot only, and this side is guessing from a name. It guesses towards
 * warning: a caveat shown over a setting that would in fact have applied costs
 * a sentence, and the silence it replaces costs an afternoon.
 */
const FIRST_BOOT = /PASSWORD|PASS$|_USER|USERNAME|DATABASE|^DB$|INITDB|ROOT_/;

const firstBootEdits = computed(() =>
  Object.keys(edits.value).filter(
    (key) => FIRST_BOOT.test(key) || settings.value.find((row) => row.key === key)?.secret
  )
);

/**
 * Close, and ask first if there is anything to lose.
 *
 * Cancel dropped a form of typed-in values without a word. The dialog is the
 * platform's, as the destructive confirmations elsewhere in the app are — it is
 * the one that cannot be dismissed by clicking past it.
 */
async function close() {
  if (dirty.value) {
    const { confirm } = await import('@tauri-apps/plugin-dialog');
    const ok = await confirm(t('instanceSettings.discardBody'), {
      title: t('instanceSettings.discardTitle'),
      kind: 'warning',
    });
    if (!ok) return;
  }
  open.value = false;
}

async function toggleReveal(row) {
  error.value = null;
  if (shown.value.has(row.key)) {
    // New Set rather than a mutation: a Set changed in place is not a change
    // Vue tracks, and the eye would stop matching what the field shows.
    const next = new Set(shown.value);
    next.delete(row.key);
    shown.value = next;
    return;
  }
  try {
    if (secrets.value[row.key] === undefined) {
      secrets.value[row.key] = await api.instanceReveal(props.instance.id, row.key);
    }
    shown.value = new Set(shown.value).add(row.key);
  } catch (e) {
    error.value = e;
  }
}

async function load() {
  if (!props.instance) return;
  loading.value = true;
  error.value = null;
  edits.value = {};
  portEdits.value = {};
  secrets.value = {};
  shown.value = new Set();
  try {
    settings.value = asList(await api.instanceSettings(props.instance.id));
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
    // Numbers, not the strings the field holds: the command takes a u16 map,
    // and `"3307"` is a type error at the boundary rather than a port.
    const movedPorts = Object.fromEntries(
      Object.entries(portEdits.value).map(([handle, port]) => [handle, Number(port)])
    );
    await api.instanceApplySettings(
      props.instance.id,
      { ...edits.value },
      Object.keys(movedPorts).length ? movedPorts : null
    );
    confirming.value = false;
    emit('applied', props.instance.id);
    open.value = false;
  } catch (e) {
    error.value = e;
    confirming.value = false;
  } finally {
    applying.value = false;
  }
}

watch(
  () => [props.modelValue, props.instance?.id],
  ([isOpen]) => {
    if (isOpen) load();
  },
  { immediate: true }
);
</script>

<template>
  <SideSheet v-model="open" :title="instance?.id ?? ''" icon="mdi-cog-outline" :width="640">
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
        :text="t('instanceSettings.none')"
      />

      <!-- Spaced by the column rather than by a margin on each field.
           `hide-details` takes away the reserved line under every input, which
           is the only thing separating them, and a dozen outlined boxes with
           four pixels between them read as one control. -->
      <div class="d-flex flex-column ga-6">
        <!-- A switch for a boolean, and it is its own branch rather than a
             third entry in the `:is` below because it is the one control here
             that does not take a string: the value on the wire stays `"true"`
             or `"false"`, because that is what a compose file interpolates,
             and only the control changes. Without this a `bool` was a text box
             somebody was expected to type the word `true` into. -->
        <template v-for="row in settings" :key="row.key">
          <div v-if="row.kind === 'bool'" class="d-flex align-center">
            <v-switch
              :model-value="asBool(row)"
              :label="row.required ? `${fieldLabel(row)} *` : fieldLabel(row)"
              color="primary"
              density="comfortable"
              hide-details
              @update:model-value="(v) => editBool(row, v)"
            />
            <v-chip v-if="row.isDefault" size="x-small" variant="tonal" class="ml-2">
              {{ t('instanceSettings.default') }}
            </v-chip>
          </div>

          <!-- A combobox where the row carries options, a text field otherwise
               — one component, because the row says which it is. A combobox
               rather than a select on purpose: a manifest listing the values it
               knows about should not make the one it did not think of
               unreachable from the app that is supposed to be the way to set
               it. The same reasoning covers an `instanceRef`, whose options are
               the instances on this machine that answer the capability it
               names. -->
          <component
            :is="row.options?.length ? VCombobox : VTextField"
            v-else
            :model-value="valueOf(row)"
            :items="row.options?.length ? row.options : undefined"
            :label="row.required ? `${fieldLabel(row)} *` : fieldLabel(row)"
            :readonly="isHidden(row)"
            :error="isMissing(row)"
            :type="row.kind === 'int' ? 'number' : 'text'"
            density="comfortable"
            variant="outlined"
            hide-details
            @update:model-value="(v) => edit(row, v ?? '')"
          >
            <!-- The key the package uses, on demand. It still matters — it is
               what you search a manifest or a compose fragment for — but not
               enough to sit under every field as a permanent second line. -->
            <template #prepend-inner>
              <v-tooltip :text="row.key" location="top" open-on-click :open-on-hover="false">
                <template #activator="{ props: tip }">
                  <v-btn
                    v-bind="tip"
                    size="x-small"
                    variant="text"
                    icon="mdi-tag-outline"
                    class="mr-1"
                    :aria-label="t('instanceSettings.showKey', { key: row.key })"
                  />
                </template>
              </v-tooltip>
            </template>

            <template #append-inner>
              <v-chip v-if="row.isDefault" size="x-small" variant="tonal" class="mr-1">
                {{ t('instanceSettings.default') }}
              </v-chip>
              <!-- The way back. The chip beside it has always said which values
                   are the package's own; until now, undoing a change to one
                   meant remembering what it had been. -->
              <v-btn
                v-if="canReset(row)"
                size="x-small"
                variant="text"
                icon="mdi-restore"
                :aria-label="t('instanceSettings.reset', { value: row.defaultValue })"
                @click="reset(row)"
              />
              <!-- One control, both directions. A reveal with no way back leaves
                 a password on screen until the sheet is closed. -->
              <v-btn
                v-if="row.secret"
                size="x-small"
                variant="text"
                :icon="isHidden(row) ? 'mdi-eye-outline' : 'mdi-eye-off-outline'"
                :aria-label="
                  isHidden(row) ? t('instanceSettings.reveal') : t('instanceSettings.hide')
                "
                @click="toggleReveal(row)"
              />
            </template>
          </component>
        </template>
      </div>

      <!-- Host ports.
           Their own block below the settings, because they are not settings —
           the manifest declares them separately and instances.json holds them
           in their own map — but they are on this sheet because they need the
           same rebuild, and a separate dialog would stop and recreate the
           container a second time for one press of one button.

           Whether a number is free is answered in Rust, against this machine
           and the instance table. The form checks the range only: a second
           opinion here would go stale between the guess and the write. -->
      <template v-if="ports.length">
        <div class="text-subtitle-2 mt-8 mb-1">{{ t('instanceSettings.ports') }}</div>
        <div class="text-caption text-medium-emphasis mb-4">
          {{ t('instanceSettings.portsSubtitle') }}
        </div>

        <div class="d-flex flex-column ga-6">
          <v-text-field
            v-for="row in ports"
            :key="row.handle"
            :model-value="portValue(row)"
            :label="row.handle"
            :error="badPort(row)"
            type="number"
            density="comfortable"
            variant="outlined"
            hide-details
            @update:model-value="(v) => editPort(row, v)"
          />
        </div>
      </template>
    </template>

    <template #footer>
      <!-- Why the button is off, beside the button. `hide-details` takes the
           reserved message line away from every field — which is what keeps a
           dozen outlined boxes from reading as one control — so the red outline
           is all a field can say on its own, and "which one, and what about it"
           has to be said here. -->
      <span v-if="missing.length" class="text-caption text-error mr-2">
        {{ t('instanceSettings.requiredMissing', { keys: missing.map((r) => r.key).join(', ') }) }}
      </span>
      <v-spacer />
      <v-btn variant="text" @click="close">{{ t('app.cancel') }}</v-btn>
      <v-btn
        color="primary"
        variant="flat"
        prepend-icon="mdi-autorenew"
        :disabled="!dirty || missing.length > 0 || portsInvalid"
        @click="confirming = true"
      >
        {{ t('instanceSettings.apply') }}
      </v-btn>
    </template>
  </SideSheet>

  <!-- Asked for by name: applying stops and recreates a container, which is a
       different thing from writing a file, and the user is entitled to know
       that before it happens rather than from the logs afterwards. -->
  <v-dialog v-model="confirming" max-width="460">
    <v-card>
      <v-card-title class="text-h6">{{ t('instanceSettings.confirmTitle') }}</v-card-title>
      <v-card-text>
        <p class="mb-3">{{ t('instanceSettings.confirmBody', { instance: instance?.id }) }}</p>
        <!-- `was → is`, not a bare list of keys. This dialog is the last look
             before a container is stopped and rebuilt, and a key on its own
             does not say what is about to happen to it. A secret shows the key
             alone: the point of the mask is not to put the old password on
             screen, least of all beside the new one. -->
        <div v-for="change in changes" :key="change.key" class="change">
          <span class="change-key">{{ change.key }}</span>
          <template v-if="change.secret">
            <span class="text-medium-emphasis">{{ t('instanceSettings.secretChanged') }}</span>
          </template>
          <template v-else>
            <span class="change-from">{{ change.from || '—' }}</span>
            <v-icon size="x-small">mdi-arrow-right</v-icon>
            <span class="change-to">{{ change.to || '—' }}</span>
          </template>
        </div>

        <!-- The one failure this whole dialog is otherwise silent about.
             Recreating the container is not the same as re-initialising the
             data, and for a credential the image reads once, at first boot,
             the difference is the difference between the setting applying and
             not. Warning is named rather than general — a caveat about "some
             settings" is one nobody can act on. -->
        <v-alert
          v-if="firstBootEdits.length"
          type="warning"
          variant="tonal"
          density="compact"
          class="mt-4"
        >
          <div class="text-caption">
            {{ t('instanceSettings.firstBootWarning', { keys: firstBootEdits.join(', ') }) }}
          </div>
        </v-alert>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" :disabled="applying" @click="confirming = false">
          {{ t('app.cancel') }}
        </v-btn>
        <v-btn color="primary" variant="flat" :loading="applying" @click="apply">
          {{ t('instanceSettings.confirmApply') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
/* One line per change, so `was → is` reads as one statement rather than as a
   row of chips the eye has to pair up. */
.change {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.8rem;
  padding: 2px 0;
  flex-wrap: wrap;
}

.change-key {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  opacity: 0.65;
  margin-inline-end: 4px;
}

/* Struck through rather than merely dimmed: on a dialog that is the last look
   before a container is rebuilt, which of the two values is leaving should not
   depend on noticing an opacity. */
.change-from {
  text-decoration: line-through;
  opacity: 0.6;
  word-break: break-all;
}

.change-to {
  font-weight: 600;
  word-break: break-all;
}
</style>
