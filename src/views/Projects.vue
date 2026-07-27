<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import { api } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import TerminalPanel from '@/components/TerminalPanel.vue';
import HostsDialog from '@/components/HostsDialog.vue';
import NewProjectDialog from '@/components/NewProjectDialog.vue';

const { t } = useI18n();
const router = useRouter();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const app = useAppStore();

const search = ref('');
const actionError = ref(null);

const terminalTarget = ref(null);
const hostsFixFor = ref(null);
const showNewProject = ref(false);
const deleteTarget = ref(null);
const deleteFiles = ref(false);

/** Projects whose manifest changed on disk since the list was loaded. */
const staleManifests = ref(new Set());

const headers = computed(() => [
  { title: t('projectsView.colDomain'), key: 'domain', sortable: true, align: 'start' },
  { title: t('projectsView.colRuntime'), key: 'runtime', sortable: true, align: 'start' },
  { title: t('projectsView.colServer'), key: 'server', sortable: true, align: 'start' },
  {
    title: t('projectsView.colConfiguration'),
    key: 'configuration',
    sortable: false,
    align: 'center',
    width: 120,
  },
  {
    title: t('projectsView.colStopStart'),
    key: 'control',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('projectsView.colRestart'),
    key: 'restart',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('projectsView.colTerminal'),
    key: 'terminal',
    sortable: false,
    align: 'center',
    width: 100,
  },
  { title: t('projectsView.colOpen'), key: 'open', sortable: false, align: 'center', width: 100 },
  {
    title: t('projectsView.colDetail'),
    key: 'detail',
    sortable: false,
    align: 'center',
    width: 100,
  },
  {
    title: t('projectsView.colDelete'),
    key: 'delete',
    sortable: false,
    align: 'center',
    width: 100,
  },
]);

async function act(project, fn) {
  actionError.value = null;
  ops.markBusy(project.name, true);
  try {
    await fn(project.name);
  } catch (e) {
    actionError.value = e;
    ops.markBusy(project.name, false);
  }
}

async function confirmDelete() {
  const project = deleteTarget.value;
  deleteTarget.value = null;
  await act(project, (n) => api.projectDelete(n, deleteFiles.value));
  deleteFiles.value = false;
}

async function regenerate(project) {
  await act(project, () => api.generateRun('projects'));
  staleManifests.value = new Set([...staleManifests.value].filter((n) => n !== project.name));
}

let teardown = null;

onMounted(async () => {
  inventory.loadProjects();

  const offRefresh = await listenAll(REFRESH_TRIGGERS, () => inventory.loadProjects());

  // The watcher reports a manifest change; it does not regenerate. Rebuilding a
  // container under someone who is mid-edit is worse than the staleness.
  const offManifest = await listenAll(['manifest:changed'], (_name, payload) => {
    staleManifests.value = new Set([...staleManifests.value, payload.project]);
    inventory.loadProjects();
  });

  const offHosts = await listenAll(['hosts:changed'], () => inventory.loadProjects());

  teardown = () => {
    offRefresh();
    offManifest();
    offHosts();
  };
});

onUnmounted(() => teardown?.());
</script>

<template>
  <PageLayout
    top-icon="mdi-folder-multiple"
    :top-title="t('projectsView.title')"
    :bar-title="t('projectsView.list')"
  >
    <template #bar-append>
      <div class="d-flex ga-2 align-center">
        <v-chip size="small" variant="tonal" color="success">
          {{ inventory.runningProjects.length }} / {{ inventory.projects.length }}
          {{ t('projectsView.running') }}
        </v-chip>
        <v-btn
          icon
          :aria-label="t('newProject.title')"
          :disabled="!app.hasWorkspace"
          @click="showNewProject = true"
        >
          <v-icon>mdi-plus</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('newProject.title') }}</v-tooltip>
        </v-btn>
        <v-btn
          icon
          :aria-label="t('app.refresh')"
          :loading="inventory.loadingProjects"
          @click="inventory.loadProjects()"
        >
          <v-icon>mdi-refresh</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('app.refresh') }}</v-tooltip>
        </v-btn>
      </div>
    </template>

    <ErrorAlert
      :error="actionError || inventory.projectsError"
      type="error"
      closable
      class="ma-2"
      @close="actionError = null"
    />

    <v-text-field
      v-model="search"
      prepend-inner-icon="mdi-magnify"
      :label="t('projectsView.searchPlaceholder')"
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
        :items="inventory.projects"
        :search="search"
        :loading="inventory.loadingProjects"
        items-per-page="-1"
        class="elevation-0"
        fixed-header
        hover
        density="compact"
        item-value="name"
        striped="even"
        hide-default-footer
        height="100%"
      >
        <template #item.domain="{ item }">
          <div v-if="item.domain" class="d-flex align-center ga-2">
            <a
              class="domain-link"
              @click="item.domainConfigured && openUrl(`https://${item.domain}`)"
              >{{ item.domain }}</a
            >

            <!-- A domain with no hosts entry cannot resolve. The web UI could
                 detect this; here the icon is also the fix. -->
            <v-tooltip v-if="!item.domainConfigured" location="top">
              <template #activator="{ props }">
                <v-icon
                  v-bind="props"
                  color="warning"
                  size="small"
                  @click.stop="hostsFixFor = item.domain"
                  >mdi-alert-circle</v-icon
                >
              </template>
              <div class="text-caption">
                <strong>{{ t('projectsView.noDnsRecord') }}</strong
                ><br />
                {{ t('projectsView.addToHosts') }}<br />
                <code>127.0.0.1 {{ item.domain }}</code>
              </div>
            </v-tooltip>

            <!-- Contract violations, shown rather than swallowed: the Bash
                 generator skips such projects without a word. -->
            <v-tooltip v-if="!item.manifestValid" location="top">
              <template #activator="{ props }">
                <v-icon v-bind="props" color="error" size="small">mdi-file-alert</v-icon>
              </template>
              <div class="text-caption">
                <strong>{{ t('projects.invalidManifest') }}</strong
                ><br />
                <span v-for="(issue, i) in item.manifest.errors" :key="i">
                  {{ issue.code }} {{ issue.path }} — {{ issue.message }}<br />
                </span>
              </div>
            </v-tooltip>

            <v-tooltip v-if="staleManifests.has(item.name)" location="top">
              <template #activator="{ props }">
                <v-icon v-bind="props" color="info" size="small" @click.stop="regenerate(item)"
                  >mdi-sync-alert</v-icon
                >
              </template>
              <span class="text-caption">{{ t('projects.manifestChanged') }}</span>
            </v-tooltip>
          </div>
          <span v-else class="text-grey">—</span>
        </template>

        <template #item.runtime="{ item }">
          <template v-if="item.runtime === 'node'">
            <v-icon start>mdi-nodejs</v-icon>Node {{ item.manifest.node?.version || 'N/A' }}
          </template>
          <template v-else>
            <v-icon start>mdi-language-php</v-icon>PHP {{ item.manifest.php?.version || 'N/A' }}
          </template>
        </template>

        <template #item.server="{ item }">
          {{ item.runtime === 'node' ? '—' : item.manifest.server || '—' }}
        </template>

        <template #item.configuration>
          <v-chip size="small" variant="tonal" color="grey" class="w-100">
            <v-icon start size="small">mdi-cog-outline</v-icon>{{ t('projectsView.default') }}
          </v-chip>
        </template>

        <template #item.control="{ item }">
          <v-btn
            v-if="!item.built"
            block
            size="small"
            color="info"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :disabled="!app.engineUp || !item.manifestValid"
            @click="act(item, (n) => api.projectBuild(n))"
          >
            <v-icon>mdi-hammer-wrench</v-icon>
          </v-btn>
          <v-btn
            v-else-if="item.running"
            block
            size="small"
            color="error"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            @click="act(item, api.projectStop)"
          >
            <v-icon>mdi-stop</v-icon>
          </v-btn>
          <v-btn
            v-else
            block
            size="small"
            color="success"
            variant="tonal"
            :loading="ops.isBusy(item.name)"
            :disabled="!app.engineUp"
            @click="act(item, api.projectStart)"
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
            :loading="ops.isBusy(item.name)"
            @click="act(item, api.projectRestart)"
          >
            <v-icon>mdi-restart</v-icon>
          </v-btn>
        </template>

        <template #item.terminal="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="info"
            variant="tonal"
            @click="terminalTarget = { kind: 'container', name: item.containerName }"
          >
            <v-icon>mdi-console</v-icon>
          </v-btn>
        </template>

        <template #item.open="{ item }">
          <!-- Only when the domain resolves; otherwise the browser shows an
               error page and the user has no idea why. -->
          <v-btn
            v-if="item.domain && item.running && item.domainConfigured"
            block
            size="small"
            color="primary"
            variant="tonal"
            @click="openUrl(`https://${item.domain}`)"
          >
            <v-icon>mdi-open-in-new</v-icon>
          </v-btn>
        </template>

        <template #item.detail="{ item }">
          <v-btn
            block
            size="small"
            color="info"
            variant="tonal"
            @click="router.push(`/projects/${item.name}`)"
          >
            <v-icon>mdi-open-in-app</v-icon>
          </v-btn>
        </template>

        <template #item.delete="{ item }">
          <v-btn
            block
            size="small"
            color="error"
            variant="tonal"
            :disabled="ops.isBusy(item.name)"
            @click="deleteTarget = item"
          >
            <v-icon>mdi-delete</v-icon>
          </v-btn>
        </template>

        <template #no-data>
          <div class="pa-8 text-center text-medium-emphasis">
            <v-icon size="32" class="mb-2">mdi-folder-off-outline</v-icon>
            <div>{{ t('projects.empty') }}</div>
          </div>
        </template>

        <template #bottom />
      </v-data-table>
    </div>

    <!-- Deleting source code needs an explicit opt-in, not a default. -->
    <v-dialog
      :model-value="!!deleteTarget"
      max-width="480"
      @update:model-value="deleteTarget = null"
    >
      <v-card v-if="deleteTarget">
        <v-card-item>
          <template #prepend><v-icon color="error">mdi-delete-alert-outline</v-icon></template>
          <v-card-title class="text-body-1">
            {{ t('newProject.deleteTitle', { name: deleteTarget.name }) }}
          </v-card-title>
        </v-card-item>
        <v-card-text>
          <p class="text-body-2 mb-3">{{ t('newProject.deleteBody') }}</p>
          <v-checkbox
            v-model="deleteFiles"
            :label="t('newProject.deleteFiles')"
            density="compact"
            hide-details
            color="error"
          />
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="deleteTarget = null">{{ t('hosts.cancel') }}</v-btn>
          <v-btn color="error" variant="flat" @click="confirmDelete">
            {{ t('newProject.delete') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <NewProjectDialog v-model="showNewProject" @created="inventory.loadProjects()" />

    <TerminalPanel
      v-if="terminalTarget"
      :target="terminalTarget"
      :model-value="!!terminalTarget"
      @update:model-value="terminalTarget = $event ? terminalTarget : null"
    />

    <HostsDialog
      v-if="hostsFixFor"
      :add="[hostsFixFor]"
      :model-value="!!hostsFixFor"
      @update:model-value="hostsFixFor = $event ? hostsFixFor : null"
      @applied="inventory.loadProjects()"
    />
  </PageLayout>
</template>

<style scoped>
.domain-link {
  color: rgb(var(--v-theme-primary));
  cursor: pointer;
  text-decoration: none;
}

.domain-link:hover {
  text-decoration: underline;
}

/* The search field keeps its natural height; the table fills the rest. */
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
</style>
