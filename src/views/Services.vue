<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import { api } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import ServiceDetailSheet from '@/components/ServiceDetailSheet.vue';

const { t } = useI18n();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const app = useAppStore();

const search = ref('');
const actionError = ref(null);
/** The row the detail sheet is showing, or null. */
const detailTarget = ref(null);

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
  {
    title: t('servicesView.colDetail'),
    key: 'detail',
    sortable: false,
    align: 'center',
    width: 80,
  },
]);

/** Enabled first, then alphabetical — the running stack reads before the rest. */
const items = computed(() =>
  [...inventory.services].sort((a, b) => {
    if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
    return a.id.localeCompare(b.id);
  })
);

// From the store, which reads DEFAULT_TLD_SUFFIX once — the same key the
// Traefik generator uses, so a changed suffix moves the links and the routes
// together. It used to be fetched here with `stackvo.loc` as the starting
// value, which meant a workspace configured for anything else showed and
// linked to the wrong host until the fetch landed, and kept showing it if the
// fetch failed.
const tld = computed(() => app.tld);

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

/**
 * The service waiting on the "this deletes its data" confirmation.
 *
 * Disabling used to only stop the container, so it needed no dialog. It now
 * removes the container, its named volumes, its image and its log directory —
 * turning a service off leaves nothing behind, which is what was asked for and
 * is also irreversible. MySQL's volume is somebody's databases.
 */
const disableTarget = ref(null);

async function confirmDisable() {
  const service = disableTarget.value;
  disableTarget.value = null;
  if (service) await act(service, api.serviceDisable);
}

let teardown = null;

onMounted(async () => {
  inventory.loadServices();

  teardown = await listenAll(REFRESH_TRIGGERS, () => inventory.loadServices());
});

onUnmounted(() => teardown?.());
</script>

<template>
  <PageLayout
    top-icon="mdi-server"
    :top-title="t('servicesView.title')"
    :top-subtitle="t('servicesView.subtitle')"
    :bar-title="t('servicesView.list')"
  >
    <template #bar-append>
      <div class="d-flex ga-2 align-center">
        <v-chip size="large" variant="tonal" color="success">
          {{ inventory.runningServices.length }} / {{ inventory.services.length }}
          {{ t('projectsView.running') }}
        </v-chip>
        <v-btn
          icon
          variant="tonal"
          size="small"
          elevation="0"
          class="mr-2"
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
        :headers="headers"
        :items="items"
        :search="search"
        :loading="inventory.loadingServices"
        items-per-page="-1"
        class="elevation-0"
        fixed-header
        hover
        item-value="id"
        striped="even"
        hide-default-footer
        height="100%"
        density="compact"
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
            @click="api.openInBrowser(`https://${domainOf(item)}`)"
          >
            <v-icon>mdi-open-in-new</v-icon>
          </v-btn>
        </template>

        <template #item.detail="{ item }">
          <v-btn
            icon
            size="small"
            variant="text"
            :aria-label="t('servicesView.colDetail')"
            @click="detailTarget = item"
          >
            <v-icon>mdi-information-outline</v-icon>
            <v-tooltip activator="parent">{{ t('servicesView.colDetail') }}</v-tooltip>
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
            @click="disableTarget = item"
          >
            <v-icon start>mdi-check-circle</v-icon>{{ t('servicesView.enabled') }}
          </v-btn>
        </template>
      </v-data-table>
    </div>

    <!-- Disabling deletes data now, so it asks — the same shape the template
         revert dialog uses, and for the same reason: there is no undo. -->
    <v-dialog
      :model-value="!!disableTarget"
      max-width="520"
      @update:model-value="disableTarget = null"
    >
      <v-card>
        <v-card-title>{{
          t('servicesView.disableTitle', { name: disableTarget?.id })
        }}</v-card-title>
        <v-card-text class="text-body-2">
          <p>{{ t('servicesView.disableBody') }}</p>
          <ul class="mt-2 ms-4">
            <li>{{ t('servicesView.disableContainer') }}</li>
            <li>{{ t('servicesView.disableVolumes') }}</li>
            <li>{{ t('servicesView.disableImage') }}</li>
            <li>{{ t('servicesView.disableLogs') }}</li>
            <li v-if="domainOf(disableTarget ?? {})">
              {{ t('servicesView.disableHosts', { domain: domainOf(disableTarget) }) }}
            </li>
          </ul>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="disableTarget = null">{{ t('app.cancel') }}</v-btn>
          <v-btn color="error" variant="flat" @click="confirmDisable">
            {{ t('servicesView.disableConfirm') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- One sheet for whichever row is open; `service` is what it reads. -->
    <ServiceDetailSheet
      :service="detailTarget"
      :tld="tld"
      :model-value="!!detailTarget"
      @update:model-value="detailTarget = $event ? detailTarget : null"
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

/* Column labels are short phrases, and wrapping them onto a second line makes
   the header band twice the height of a row for no gain. They stay on one line
   and take the width they need. */
.table-wrap :deep(thead th) {
  white-space: nowrap;
}

.table-wrap :deep(.v-data-table-header__content) {
  flex-wrap: nowrap;
}

/* A bind-mount source is an absolute host path; without this it runs out of
   the column and under the next one. */
.path-chip {
  max-width: 100%;
  height: auto;
  min-height: 24px;
}

.path-chip :deep(.v-chip__content) {
  white-space: normal;
  word-break: break-all;
  padding: 2px 0;
}

.panel-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 500;
  margin-bottom: 8px;
}

/* Key, value, and a reveal button that only some rows have. */
.credential {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 28px;
}

.credential-value {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  font-weight: 600;
  word-break: break-all;
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
