<script setup>
import { ref, computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import SettingsGroup from '@/components/SettingsGroup.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import { useCopyTick } from '@/composables/useCopyTick';

/**
 * Registering the MCP server with the assistants on this machine.
 *
 * The pane's shape follows from what it edits. Every row writes to a file
 * belonging to another application, so each one names that file on screen —
 * not as decoration, but because the two outcomes this cannot rule out (a
 * refusal to touch a file with comments in it, and a registration somebody
 * wants to undo by hand) both end with the reader opening that exact path.
 *
 * The write switch is above the list rather than inside a row, and off. It is
 * the only decision here with a consequence: `--allow-writes` hands the
 * assistant `stack_down` and `project_stop`, not just the read tools people
 * come for. A per-row toggle would ask the same question six times and get a
 * less considered answer each time.
 */
const { t } = useI18n();
const { copied, copy } = useCopyTick();

const status = ref({ binary: null, source: null, root: null, clients: [] });
const error = ref(null);
const busy = ref(null);
const loading = ref(false);
const allowWrites = ref(false);

const ready = computed(() => Boolean(status.value.binary));

/** Clients this machine has. The rest are listed dimmed rather than hidden —
 *  "Cursor is not here" is an answer, and an absent row reads as a bug. */
const rows = computed(() =>
  [...status.value.clients].sort((a, b) => Number(b.present) - Number(a.present))
);

/**
 * The block to paste when this cannot write the file itself.
 *
 * Built from the same values the installer would use, so what is shown and
 * what would be written cannot drift apart.
 */
function snippet(client) {
  const entry = { command: status.value.binary ?? '/path/to/stackvo-mcp' };
  if (client.id === 'vscode') entry.type = 'stdio';
  if (allowWrites.value) entry.args = ['--allow-writes'];
  if (status.value.root) entry.env = { STACKVO_ROOT: status.value.root };

  const key = client.id === 'vscode' ? 'servers' : 'mcpServers';
  return JSON.stringify({ [key]: { stackvo: entry } }, null, 2);
}

function state(client) {
  if (!client.parseable) return 'unparseable';
  if (!client.present) return 'absent';
  if (client.command && client.current) return 'registered';
  if (client.command) return 'stale';
  return 'available';
}

async function load() {
  loading.value = true;
  error.value = null;
  try {
    status.value = await api.agentsStatus();
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

async function install(client) {
  busy.value = client.id;
  error.value = null;
  try {
    await api.agentsInstall(client.id, allowWrites.value);
    await load();
  } catch (e) {
    // Re-read first, then report — the same order SecretsPane settled on, for
    // the same reason: a failed write may still have changed the file, and a
    // row showing the old state would be a claim nobody checked. `load()`
    // clears the error on the way in, so it is set afterwards.
    await load();
    error.value = e;
  } finally {
    busy.value = null;
  }
}

async function remove(client) {
  busy.value = client.id;
  error.value = null;
  try {
    await api.agentsRemove(client.id);
    await load();
  } catch (e) {
    await load();
    error.value = e;
  } finally {
    busy.value = null;
  }
}

onMounted(load);

defineExpose({ snippet, state });
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <SettingsGroup
    icon="mdi-robot-outline"
    :title="t('settings.agents.title')"
    :description="t('settings.agents.description')"
  >
    <v-alert type="info" variant="tonal" density="comfortable" class="mb-4">
      <div class="text-body-2">{{ t('settings.agents.whatItDoes') }}</div>
      <div class="text-caption mt-2 text-medium-emphasis">
        {{ t('settings.agents.neverClobbers') }}
      </div>
    </v-alert>

    <!-- Not a warning about a failure: the binary is a separate build and
         always has been. What would be a failure is registering a path that is
         not there, which is why every button below is withheld until it is. -->
    <v-alert
      v-if="!ready && !loading"
      type="warning"
      variant="tonal"
      density="comfortable"
      class="mb-4"
    >
      <div class="text-body-2">{{ t('settings.agents.noBinary') }}</div>
      <code class="d-block mt-2 text-caption">{{ t('settings.agents.buildCommand') }}</code>
      <v-btn
        size="small"
        variant="text"
        class="mt-1"
        :prepend-icon="copied === 'build' ? 'mdi-check' : 'mdi-content-copy'"
        @click="copy(t('settings.agents.buildCommand'), 'build')"
      >
        {{ t('app.copy') }}
      </v-btn>
    </v-alert>

    <div v-if="ready" class="mb-4">
      <div class="text-caption text-medium-emphasis">{{ t('settings.agents.serverBinary') }}</div>
      <code class="text-caption">{{ status.binary }}</code>
    </div>

    <v-switch
      v-model="allowWrites"
      color="warning"
      density="compact"
      hide-details
      class="mb-1"
      :label="t('settings.agents.allowWrites')"
    />
    <div class="text-caption text-medium-emphasis mb-4">
      {{ t('settings.agents.allowWritesDetail') }}
    </div>

    <v-progress-linear v-if="loading" indeterminate class="mb-2" />

    <v-list density="compact" class="bg-transparent">
      <v-list-item v-for="client in rows" :key="client.id" class="px-0">
        <template #prepend>
          <v-icon
            :icon="state(client) === 'registered' ? 'mdi-check-circle-outline' : 'mdi-application'"
            :color="
              { registered: 'success', stale: 'warning', unparseable: 'warning' }[state(client)]
            "
            class="mr-3"
          />
        </template>

        <v-list-item-title class="text-body-2">{{ client.label }}</v-list-item-title>
        <v-list-item-subtitle class="text-caption">
          {{ t(`settings.agents.state.${state(client)}`) }} — <code>{{ client.path }}</code>
        </v-list-item-subtitle>

        <template #append>
          <v-btn
            v-if="state(client) === 'registered'"
            size="small"
            variant="tonal"
            :loading="busy === client.id"
            :disabled="busy !== null && busy !== client.id"
            @click="remove(client)"
          >
            {{ t('settings.agents.remove') }}
          </v-btn>
          <v-btn
            v-else-if="state(client) !== 'unparseable'"
            size="small"
            variant="tonal"
            color="primary"
            :loading="busy === client.id"
            :disabled="!ready || (busy !== null && busy !== client.id)"
            @click="install(client)"
          >
            {{ state(client) === 'stale' ? t('settings.agents.update') : t('settings.agents.add') }}
          </v-btn>
          <v-btn
            v-else
            size="small"
            variant="text"
            :prepend-icon="copied === client.id ? 'mdi-check' : 'mdi-content-copy'"
            @click="copy(snippet(client), client.id)"
          >
            {{ t('settings.agents.copyBlock') }}
          </v-btn>
        </template>
      </v-list-item>
    </v-list>

    <div class="text-caption text-medium-emphasis mt-4">
      {{ t('settings.agents.notListed') }}
    </div>
  </SettingsGroup>
</template>
