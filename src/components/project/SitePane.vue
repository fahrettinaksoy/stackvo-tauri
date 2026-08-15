<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * Three per-project settings that reach the container through this app's own
 * layers rather than through the manifest (M-5, M-6, M-10).
 *
 * They share a pane because they share a file and a moment: somebody setting up
 * one project sets all three at once, and three panes would be three places to
 * look for the same `.stackvo/site.json`.
 *
 * ## Why each switch can be unavailable, and says so
 *
 * A directory listing is a **server** directive, and Apache and Swoole have no
 * configuration file for one — Apache is configured by `sed` inside its own
 * Dockerfile. Forwarding an SSH agent needs an agent to forward. In both cases
 * the backend answers whether it is possible here, and the pane draws the
 * reason rather than a control that would do nothing.
 */
const props = defineProps({
  name: { type: String, required: true },
  runtime: { type: String, default: '' },
});

const emit = defineEmits(['apply']);

const { t } = useI18n();

const settings = ref(null);
const rows = ref([]);
const error = ref(null);
const busy = ref(false);

/** Node projects have a compose service too, so the variables apply there. */
const applies = computed(() => !!props.runtime);

/** The editor is a list so a half-typed key does not vanish on re-render. */
function toRows(env) {
  return Object.entries(env || {}).map(([key, value]) => ({ key, value }));
}

async function load() {
  if (!applies.value) return;
  error.value = null;
  try {
    settings.value = await api.siteSettings(props.name);
    rows.value = toRows(settings.value.env);
  } catch (e) {
    error.value = e;
  }
}

/**
 * Blank rows are dropped rather than sent: an empty key is what a row that is
 * being typed looks like, and the backend refusing it would turn every save
 * during editing into an error.
 */
async function save(patch = {}) {
  busy.value = true;
  error.value = null;
  try {
    const env = {};
    for (const row of rows.value) {
      const key = row.key.trim();
      if (key) env[key] = row.value;
    }
    settings.value = await api.siteSave(
      props.name,
      env,
      patch.directoryListing ?? settings.value.directoryListing,
      patch.sshAgent ?? settings.value.sshAgent
    );
    rows.value = toRows(settings.value.env);
    // The variables and the agent are a compose overlay: the running container
    // is still the old one until it is recreated.
    emit('apply');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

watch(() => [props.name, props.runtime], load, { immediate: true });
</script>

<template>
  <v-card v-if="applies && settings" variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-tune-variant</v-icon>{{ t('site.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-3">{{ t('site.explain') }}</p>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <!-- M-5 -->
    <div class="section-head mt-2 mb-2">
      <v-icon size="16" class="mr-2">mdi-code-braces</v-icon>{{ t('site.envTitle') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-2">{{ t('site.envExplain') }}</p>

    <div v-for="(row, index) in rows" :key="index" class="d-flex ga-2 mb-2" data-test="env-row">
      <v-text-field
        v-model="row.key"
        :label="t('site.key')"
        density="compact"
        variant="outlined"
        hide-details
        class="flex-grow-0"
        style="max-width: 220px"
      />
      <v-text-field
        v-model="row.value"
        :label="t('site.value')"
        density="compact"
        variant="outlined"
        hide-details
      />
      <v-btn
        icon
        size="small"
        variant="text"
        :aria-label="t('site.removeRow')"
        @click="rows.splice(index, 1)"
      >
        <v-icon>mdi-close</v-icon>
      </v-btn>
    </div>

    <div class="d-flex ga-2 mb-4">
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-plus"
        @click="rows.push({ key: '', value: '' })"
      >
        {{ t('site.addRow') }}
      </v-btn>
      <v-spacer />
      <v-btn size="small" color="primary" variant="tonal" :loading="busy" @click="save()">
        {{ t('site.save') }}
      </v-btn>
    </div>

    <v-divider class="mb-3" />

    <!-- M-6 -->
    <div class="d-flex align-center ga-3">
      <v-switch
        :model-value="settings.directoryListing"
        color="primary"
        density="compact"
        hide-details
        :disabled="!settings.listingSupported || busy"
        :label="t('site.listing')"
        @update:model-value="save({ directoryListing: !settings.directoryListing })"
      />
    </div>
    <p class="text-caption text-medium-emphasis mb-3">
      {{
        settings.listingSupported
          ? t('site.listingHint')
          : t('site.listingUnsupported', { server: settings.server })
      }}
    </p>

    <!-- M-10 -->
    <div class="d-flex align-center ga-3">
      <v-switch
        :model-value="settings.sshAgent"
        color="primary"
        density="compact"
        hide-details
        :disabled="!settings.agentAvailable || busy"
        :label="t('site.sshAgent')"
        @update:model-value="save({ sshAgent: !settings.sshAgent })"
      />
    </div>
    <!-- The cost is on the switch that charges it: anything in that container
         can sign with every key in the agent for as long as it is up. -->
    <p
      class="text-caption mb-0"
      :class="settings.agentAvailable ? 'text-medium-emphasis' : 'text-warning'"
      data-test="ssh-hint"
    >
      {{ settings.agentAvailable ? t('site.sshAgentHint') : t('site.sshAgentNone') }}
    </p>
  </v-card>
</template>
