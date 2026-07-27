<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import { api } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import LogPanel from '@/components/LogPanel.vue';
import TerminalPanel from '@/components/TerminalPanel.vue';

const { t } = useI18n();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const app = useAppStore();

const search = ref('');
const actionError = ref(null);
const expanded = ref([]);
const logTarget = ref(null);
const terminalTarget = ref(null);

const headers = computed(() => [
  { title: t('servicesView.colService'), key: 'id', sortable: true, align: 'start' },
  {
    title: t('servicesView.colContainerName'),
    key: 'containerName',
    sortable: true,
    align: 'start',
  },
  { title: t('servicesView.colDomain'), key: 'url', sortable: true, align: 'start' },
  { title: t('servicesView.colVersion'), key: 'version', sortable: true, align: 'start' },
  {
    title: t('servicesView.colStopStart'),
    key: 'control',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('servicesView.colRestart'),
    key: 'restart',
    sortable: false,
    align: 'center',
    width: 100,
  },
  { title: t('servicesView.colOpen'), key: 'open', sortable: false, align: 'center', width: 100 },
  {
    title: t('servicesView.colStatus'),
    key: 'status',
    sortable: true,
    align: 'center',
    width: 140,
  },
]);

/** Enabled first, then alphabetical — the running stack reads before the rest. */
const items = computed(() =>
  [...inventory.services].sort((a, b) => {
    if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
    return a.id.localeCompare(b.id);
  })
);

const tld = ref('stackvo.loc');

function domainOf(service) {
  return service.url ? `${service.url}.${tld.value}` : null;
}

async function act(service, fn) {
  actionError.value = null;
  ops.markBusy(service.id, true);
  try {
    await fn(service.id);
  } catch (e) {
    actionError.value = e;
    ops.markBusy(service.id, false);
  }
}

let teardown = null;

onMounted(async () => {
  inventory.loadServices();

  // The service domains are built from DEFAULT_TLD_SUFFIX, the same key the
  // Traefik generator uses — so a changed TLD moves both together.
  api
    .envGet()
    .then((env) => {
      if (env.DEFAULT_TLD_SUFFIX) tld.value = env.DEFAULT_TLD_SUFFIX;
    })
    .catch(() => {});

  teardown = await listenAll(REFRESH_TRIGGERS, () => inventory.loadServices());
});

onUnmounted(() => teardown?.());
</script>

<template>
  <PageLayout
    top-icon="mdi-server"
    :top-title="t('servicesView.title')"
    :bar-title="t('servicesView.list')"
  >
    <template #bar-append>
      <div class="d-flex ga-2 align-center">
        <v-chip size="small" variant="tonal" color="success">
          {{ inventory.runningServices.length }} / {{ inventory.services.length }}
          {{ t('projectsView.running') }}
        </v-chip>
        <v-btn
          icon
          :aria-label="t('app.refresh')"
          :loading="inventory.loadingServices"
          @click="inventory.loadServices()"
        >
          <v-icon>mdi-refresh</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
        </v-btn>
      </div>
    </template>

    <ErrorAlert
      :error="actionError || inventory.servicesError"
      type="error"
      closable
      class="ma-2"
      @close="actionError = null"
    />

    <v-text-field
      v-model="search"
      prepend-inner-icon="mdi-magnify"
      :label="t('servicesView.searchPlaceholder')"
      class="rounded-0 search-field"
      variant="filled"
      rounded="0"
      single-line
      hide-details
      clearable
    />

    <div class="table-wrap">
      <v-data-table
        v-model:expanded="expanded"
        :headers="headers"
        :items="items"
        :search="search"
        :loading="inventory.loadingServices"
        items-per-page="-1"
        class="elevation-0"
        show-expand
        fixed-header
        hover
        density="compact"
        item-value="id"
        striped="even"
        hide-default-footer
        height="100%"
      >
        <template #item.id="{ item }">
          <span class="font-weight-bold">{{ item.id }}</span>
        </template>

        <template #item.containerName="{ item }">
          <v-chip size="small" variant="tonal" color="grey">
            <v-icon start size="small">mdi-docker</v-icon>{{ item.containerName }}
          </v-chip>
        </template>

        <template #item.url="{ item }">
          <span v-if="domainOf(item)">{{ domainOf(item) }}</span>
          <span v-else class="text-grey">–</span>
        </template>

        <template #item.version="{ item }">
          {{ item.version || '–' }}
        </template>

        <template #item.control="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="error"
            variant="tonal"
            :loading="ops.isBusy(item.id)"
            @click="act(item, api.serviceStop)"
          >
            <v-icon>mdi-stop</v-icon>
          </v-btn>
          <v-btn
            v-else-if="item.enabled"
            block
            size="small"
            color="success"
            variant="tonal"
            :loading="ops.isBusy(item.id)"
            :disabled="!app.engineUp"
            @click="act(item, item.built ? api.serviceStart : api.composeUpService)"
          >
            <v-icon>mdi-play</v-icon>
          </v-btn>
        </template>

        <template #item.restart="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="warning"
            variant="tonal"
            :loading="ops.isBusy(item.id)"
            @click="act(item, api.serviceRestart)"
          >
            <v-icon>mdi-restart</v-icon>
          </v-btn>
        </template>

        <template #item.open="{ item }">
          <v-btn
            v-if="domainOf(item) && item.running"
            block
            size="small"
            color="primary"
            variant="tonal"
            @click="openUrl(`https://${domainOf(item)}`)"
          >
            <v-icon>mdi-open-in-new</v-icon>
          </v-btn>
        </template>

        <template #item.status="{ item }">
          <!-- Enabling writes .env, regenerates and brings the profile up;
               disabling stops the container first, then unconfigures. -->
          <v-btn
            v-if="!item.enabled"
            size="small"
            color="grey"
            variant="tonal"
            :loading="ops.isBusy(item.id)"
            :disabled="!app.engineUp"
            @click="act(item, api.serviceEnable)"
          >
            <v-icon start>mdi-power</v-icon>{{ t('servicesView.disabled') }}
          </v-btn>
          <v-btn
            v-else
            size="small"
            color="success"
            variant="tonal"
            :loading="ops.isBusy(item.id)"
            @click="act(item, api.serviceDisable)"
          >
            <v-icon start>mdi-check-circle</v-icon>{{ t('servicesView.enabled') }}
          </v-btn>
        </template>

        <template #expanded-row="{ columns, item }">
          <tr>
            <td :colspan="columns.length" class="pa-4">
              <v-row>
                <v-col cols="12" md="4">
                  <div class="text-subtitle-2 mb-2 d-flex align-center">
                    <v-icon size="small" color="info" class="mr-2">mdi-network</v-icon>
                    {{ t('servicesView.networkInfo') }}
                  </div>
                  <div class="detail-row">
                    <span class="detail-key">{{ t('servicesView.colContainerName') }}</span>
                    <v-chip size="small" variant="tonal" color="primary">
                      <v-icon start size="small">mdi-docker</v-icon>{{ item.containerName }}
                    </v-chip>
                  </div>
                  <div v-if="item.hostPort" class="detail-row">
                    <span class="detail-key">{{ t('services.hostPort') }}</span>
                    <span class="detail-val">{{ item.hostPort }}</span>
                  </div>
                  <div v-for="port in item.ports" :key="port.container" class="detail-row">
                    <span class="detail-key">{{ t('detail.ports') }}</span>
                    <span class="detail-val">
                      {{ port.host ? `${port.host}→` : '' }}{{ port.container }}/{{ port.protocol }}
                    </span>
                  </div>
                </v-col>

                <v-col cols="12" md="4">
                  <div class="text-subtitle-2 mb-2 d-flex align-center">
                    <v-icon size="small" color="info" class="mr-2">mdi-link-variant</v-icon>
                    {{ t('servicesView.dependencies') }}
                  </div>

                  <!-- The web UI modelled dependencies for three of twenty
                       services and referenced one that does not exist, so admin
                       UIs could be started against nothing. -->
                  <div
                    v-if="!item.required.length && !item.optional.length"
                    class="text-caption text-medium-emphasis"
                  >
                    {{ t('servicesView.noDependencies') }}
                  </div>
                  <div v-for="dep in item.required" :key="dep" class="detail-row">
                    <span class="detail-key">{{ t('servicesView.required') }}</span>
                    <v-chip
                      size="x-small"
                      label
                      :color="item.unmetDependencies.includes(dep) ? 'warning' : 'success'"
                      >{{ dep }}</v-chip
                    >
                  </div>
                  <div v-for="dep in item.optional" :key="dep" class="detail-row">
                    <span class="detail-key">{{ t('servicesView.optional') }}</span>
                    <v-chip size="x-small" label>{{ dep }}</v-chip>
                  </div>
                </v-col>

                <v-col cols="12" md="4">
                  <div class="text-subtitle-2 mb-2 d-flex align-center">
                    <v-icon size="small" color="info" class="mr-2">mdi-tools</v-icon>
                    {{ t('servicesView.actions') }}
                  </div>
                  <div class="d-flex ga-2 flex-wrap">
                    <v-btn
                      size="small"
                      variant="tonal"
                      prepend-icon="mdi-text-box-outline"
                      :disabled="!item.built"
                      @click="logTarget = item.containerName"
                      >{{ t('actions.logs') }}</v-btn
                    >
                    <v-btn
                      size="small"
                      variant="tonal"
                      prepend-icon="mdi-console"
                      :disabled="!item.running"
                      @click="terminalTarget = { kind: 'container', name: item.containerName }"
                      >{{ t('projects.terminal') }}</v-btn
                    >
                  </div>
                </v-col>
              </v-row>
            </td>
          </tr>
        </template>

        <template #bottom />
      </v-data-table>
    </div>

    <LogPanel
      v-if="logTarget"
      :container="logTarget"
      :model-value="!!logTarget"
      @update:model-value="logTarget = $event ? logTarget : null"
    />

    <TerminalPanel
      v-if="terminalTarget"
      :target="terminalTarget"
      :model-value="!!terminalTarget"
      @update:model-value="terminalTarget = $event ? terminalTarget : null"
    />
  </PageLayout>
</template>

<style scoped>
.search-field {
  flex: 0 0 auto;
}

.table-wrap {
  flex: 1 1 auto;
  min-height: 0;
}

.table-wrap :deep(.v-table) {
  height: 100%;
}

.detail-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
  font-size: 0.8rem;
}

.detail-key {
  opacity: 0.65;
  min-width: 110px;
}

.detail-val {
  font-weight: 600;
}
</style>
