<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import { useInventoryStore } from '@/stores/inventory';
import { useOperationsStore } from '@/stores/operations';
import { useAppStore } from '@/stores/app';
import { parentDomain } from '@/lib/manifest';
import { bytes } from '@/lib/format';
import { listenAll, REFRESH_TRIGGERS } from '@/lib/events';
import { api, asList } from '@/lib/ipc';
import PageLayout from '@/components/PageLayout.vue';
import ErrorAlert from '@/components/ErrorAlert.vue';
import HostsDialog from '@/components/HostsDialog.vue';

const { t } = useI18n();
const router = useRouter();
const inventory = useInventoryStore();
const ops = useOperationsStore();
const app = useAppStore();

const search = ref('');
const actionError = ref(null);

const hostsFixFor = ref(null);
const deleteTarget = ref(null);
const deleteFiles = ref(false);

/*
 * There is no `staleManifests` set any more, and that is the point.
 *
 * The badge used to be an accumulator: every `manifest:changed` the watcher
 * emitted added a name, and only a successful regenerate took one out. The
 * watcher cannot tell whose write it saw, so creating a project — the app
 * writing `stackvo.json`, then regenerating from it — added the name and
 * nothing removed it. The badge appeared on every new project and stayed.
 *
 * It reads `item.generatedStale` now, which the backend measures from the
 * manifest's timestamp against the generated output's. A watcher event only
 * triggers a reload; the answer comes from the files.
 */

/**
 * Rows grouped by parent domain, but only where a parent means something.
 *
 * The rule itself lives in `manifest.js` so it can be tested on its own; what
 * is left here is the counting. A parent with a single project keeps its own
 * domain as the key, so Vuetify makes a group of one and the header slot skips
 * it — the row then reads exactly as it did before grouping existed.
 */
const rows = computed(() => {
  const counts = new Map();
  for (const p of inventory.projects) {
    const parent = parentDomain(p.domain, app.tld);
    if (parent) counts.set(parent, (counts.get(parent) ?? 0) + 1);
  }
  return inventory.projects.map((p) => {
    const parent = parentDomain(p.domain, app.tld);
    return {
      ...p,
      // `null` when it stands alone, which is the table's own escape hatch: a
      // group with a null value has its header skipped and its rows always
      // flattened. Giving each one its own key instead made a group of one —
      // and groups start closed, so the header was suppressed, nothing was
      // left to open it, and five projects vanished from the page.
      parentDomain: parent && counts.get(parent) > 1 ? parent : null,
    };
  });
});

/**
 * Groups start expanded.
 *
 * The table keeps that state internally — `opened` is a ref inside its own
 * composable, with no prop to seed it and nothing exposed on the component to
 * reach it. The only handle is `toggleGroup`, handed to this slot, so opening
 * has to be asked for from here.
 *
 * Deferred and remembered, for two reasons. Toggling during render mutates
 * state the same render reads, which Vue rightly complains about; and a group
 * the user collapsed must stay collapsed, so this fires once per group and
 * never again — `seen` is a plain Set rather than a ref because nothing should
 * re-render when it changes.
 */
const seen = new Set();
function openByDefault(item, isGroupOpen, toggleGroup) {
  const open = isGroupOpen(item);
  if (!open && !seen.has(item.id)) {
    seen.add(item.id);
    nextTick(() => toggleGroup(item));
  }
  // Returned so the binding that calls this reflects real state rather than
  // being an attribute that is always empty — an attribute that says nothing
  // is a worse home for a side effect than one that says something.
  return open;
}

const groupBy = [{ key: 'parentDomain', order: 'asc' }];

/**
 * Ordered by domain, inside a group and out.
 *
 * The table sorts by the group key before anything else, so without this the
 * rows arrived in whatever order the inventory returned them — which is the
 * order Docker happened to answer in, and changes between refreshes.
 */
const sortBy = [{ key: 'domain', order: 'asc' }];

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
    title: t('projectsView.colRebuild'),
    key: 'rebuild',
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

/**
 * The terminal the user chose in Settings, on this container.
 *
 * There used to be a second, in-app terminal in a dialog. Two terminals with
 * different behaviour for the same button is a coin toss for the reader, and
 * the external one is the one with scrollback, tabs and a profile.
 */
async function openTerminal(project) {
  actionError.value = null;
  try {
    await api.terminalOpenExternal({ kind: 'container', name: project.containerName });
  } catch (e) {
    actionError.value = e;
  }
}

/**
 * Run a row action with the row marked busy.
 *
 * On success the flag is left for the event stream to clear, which is what
 * keeps the row honest when something else acts on the project — another
 * window, the watcher, a container that stopped on its own.
 *
 * That only works if `fn` runs a command whose events carry **the project's
 * own name** as their subject. Start, stop, restart, build and delete all do.
 * A command that reports under something else has to clear the flag itself —
 * see `regenerate`.
 */
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

/**
 * Re-render the generated tree after a manifest changed underneath us.
 *
 * Not `act`, and the reason is the subject. `generate_run` reports under the
 * SCOPE it was handed — every one of its events carries `"projects"` — so the
 * flag `act` sets under the project's name has nothing coming to clear it. The
 * regenerate finished, said so, and the row's button spun for ever.
 *
 * Cleared in `finally` here instead, which is sound because `generate_run`
 * awaits the whole render before it resolves: the promise settling means the
 * files are written, not that the work was accepted.
 *
 * The badge needs no clearing. It is `item.generatedStale`, measured from the
 * files, so reloading the list after a successful render is what turns it off
 * — and a render that failed leaves it on, which is correct and used not to
 * be: `act` swallows the error, so the marker came off either way.
 */
async function regenerate(project) {
  actionError.value = null;
  ops.markBusy(project.name, true);
  try {
    await api.generateRun('projects');
    await inventory.loadProjects();
  } catch (e) {
    actionError.value = e;
  } finally {
    ops.markBusy(project.name, false);
  }
}

/**
 * The stale-manifest badge does whichever half is actually outstanding.
 *
 * It used to regenerate, always, and that was the weaker of the two acts: a
 * project with an image already built keeps running the old one, so the files
 * on disk agree with `stackvo.json`, the badge goes out, and the container is
 * still the thing it was. The badge said "changed — regenerate to apply it",
 * and regenerating did not apply it.
 *
 * So: built means rebuild — regenerate, build the image, recreate the container
 * — and unbuilt means regenerate, because there is nothing to rebuild yet and
 * pulling a base image is not what a badge click should start.
 */
function applyChange(project) {
  if (!project.built) return regenerate(project);
  return act(project, (n) => api.projectBuild(n));
}

/**
 * Folders under `projects/` with no `stackvo.json`.
 *
 * Shown above the table rather than behind a menu: they are invisible
 * everywhere else in the app, which is exactly why they accumulate. On the
 * checkout this was written against there were eleven of them, three of which
 * were Laravel applications.
 */
const adoptable = ref([]);
const adopting = ref(null);

async function loadAdoptable() {
  try {
    adoptable.value = asList(await api.projectAdoptable());
  } catch {
    // A missing workspace is already reported by the requirements gate.
    adoptable.value = [];
  }
}

/**
 * Sites belonging to XAMPP or Laragon.
 *
 * Beside the adoptable folders rather than in a wizard of its own: both
 * questions are "there is code on this machine StackVo is not running", and a
 * migration behind a menu is one people do not find in the window where they
 * are still deciding.
 */
const installs = ref([]);
const importing = ref(null);
const importMove = ref(false);

/**
 * An installation somewhere the well-known paths do not look.
 *
 * The defaults are installer defaults and people move things — and without
 * this the answer for somebody with XAMPP on a second drive is "StackVo says I
 * do not have XAMPP", which is worse than no scan at all.
 */
async function pickInstall(source) {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({ directory: true, multiple: false });
  if (!path) return;

  actionError.value = null;
  try {
    const found = await api.importsScanAt(source, path);
    if (!found) {
      actionError.value = { code: 'NOT_FOUND', message: t('imports.notThere', { source }) };
      return;
    }
    // Replaces a previous answer for the same path rather than stacking, so
    // pointing at the same folder twice does not list its sites twice.
    installs.value = [...installs.value.filter((i) => i.path !== found.path), found];
  } catch (e) {
    actionError.value = e;
  }
}

async function loadImports() {
  try {
    installs.value = asList(await api.importsScan());
  } catch {
    // Nothing installed is the ordinary case, not a failure to report.
    installs.value = [];
  }
}

/**
 * Copy the site in, then adopt it exactly as any other folder is adopted.
 *
 * Two calls rather than one command that does both, and deliberately: adoption
 * already validates the manifest, applies the name rules and asks the schema
 * whether the result is legal. An importer with its own manifest writer would
 * be a second set of those rules to keep in step.
 */
async function importSite(install, site) {
  importing.value = site.path;
  actionError.value = null;
  try {
    await api.importsTake(site.path, site.name, importMove.value);
    // The domain only when the other tool actually said one. Laragon writes a
    // vhost per site; XAMPP serves by path, so there is nothing to carry and
    // adoption falls back to the suffix like every other project.
    await api.projectAdopt(site.name, null, site.domain ? { domain: site.domain } : undefined);
    await Promise.all([inventory.loadProjects(), loadAdoptable(), loadImports()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    importing.value = null;
  }
}

async function adopt(folder) {
  adopting.value = folder.name;
  actionError.value = null;
  try {
    // No spec: the Rust side re-detects and validates against the same schema
    // project_create uses. Passing the detection back would let a stale reading
    // from before an edit be the thing that gets written.
    await api.projectAdopt(folder.name);
    await Promise.all([inventory.loadProjects(), loadAdoptable()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    adopting.value = null;
  }
}

/**
 * Reading a folder's own `docker-compose.yml` before adopting it.
 *
 * Detection reads the code and gets runtime, framework and document root. The
 * compose file records what its author decided — the PHP version, the domain,
 * the extensions, and the backing services, which no marker file states at all.
 * Adopting without it produces a project that builds and then cannot reach its
 * database.
 *
 * Reviewed before applied: the diff covers a manifest *and* somebody's `.env`,
 * which is more than an adoption has ever written in one go.
 */
const migration = ref(null);
const migrationFor = ref('');
const migrationBusy = ref(false);

async function scanCompose(folder) {
  migrationBusy.value = true;
  actionError.value = null;
  migrationFor.value = folder.name;
  try {
    migration.value = await api.migrateScan(folder.name);
  } catch (e) {
    migration.value = null;
    migrationFor.value = '';
    actionError.value = e;
  } finally {
    migrationBusy.value = false;
  }
}

async function applyMigration() {
  migrationBusy.value = true;
  actionError.value = null;
  try {
    await api.migrateApply(migrationFor.value);
    migration.value = null;
    migrationFor.value = '';
    await Promise.all([inventory.loadProjects(), loadAdoptable()]);
  } catch (e) {
    actionError.value = e;
  } finally {
    migrationBusy.value = false;
  }
}

function closeMigration() {
  migration.value = null;
  migrationFor.value = '';
}

/** The conclusions worth a row. Anything the file did not state is left out
 *  rather than shown as a blank — an empty cell reads as "it said nothing"
 *  only if you already know the row was optional. */
const migrationFields = computed(() => {
  const m = migration.value?.migration;
  if (!m) return {};
  const rows = {
    runtime: m.runtime,
    server: m.server,
    phpVersion: m.phpVersion,
    nodeVersion: m.nodeVersion,
    documentRoot: m.documentRoot,
    domain: m.domain,
    extensions: m.extensions.length ? m.extensions.join(', ') : null,
  };
  return Object.fromEntries(Object.entries(rows).filter(([, v]) => v));
});

let teardown = null;

onMounted(async () => {
  inventory.loadProjects();
  loadAdoptable();
  loadImports();

  const offRefresh = await listenAll(REFRESH_TRIGGERS, () => inventory.loadProjects());

  // The watcher reports a manifest change; it does not regenerate. Rebuilding a
  // container under someone who is mid-edit is worse than the staleness.
  //
  // The event is a nudge to look again, not the answer: whether the project is
  // actually behind its generated output is `generatedStale`, which the reload
  // brings back with the row.
  const offManifest = await listenAll(['manifest:changed'], () => inventory.loadProjects());

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
    :top-subtitle="t('projectsView.subtitle')"
    :bar-title="t('projectsView.list')"
  >
    <template #bar-append>
      <div class="d-flex ga-2 align-center">
        <v-chip size="large" variant="tonal" color="success">
          {{ inventory.runningProjects.length }} / {{ inventory.projects.length }}
          {{ t('projectsView.running') }}
        </v-chip>
        <v-btn
          icon
          variant="tonal"
          size="small"
          elevation="0"
          :aria-label="t('newProject.title')"
          :disabled="!app.hasWorkspace"
          @click="app.newProjectOpen = true"
        >
          <v-icon>mdi-plus</v-icon>
          <v-tooltip activator="parent" location="bottom">{{ t('newProject.title') }}</v-tooltip>
        </v-btn>
        <v-btn
          icon
          variant="tonal"
          size="small"
          elevation="0"
          class="mr-2"
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

    <!-- Unmanaged folders ------------------------------------------------ -->
    <!-- Real code sitting in projects/ with no stackvo.json. It is invisible
         everywhere else in the app, which is why it accumulates. -->
    <!-- Sites belonging to another tool. Same shape as the adoptable panel
         below it, because it answers the same question from further away. -->
    <div class="d-flex ga-2 px-4 py-2">
      <v-btn
        v-for="source in ['xampp', 'laragon']"
        :key="source"
        size="x-small"
        variant="text"
        prepend-icon="mdi-folder-search-outline"
        @click="pickInstall(source)"
      >
        {{ t('imports.pick', { tool: source }) }}
      </v-btn>
    </div>

    <v-expansion-panels
      v-if="installs.some((i) => i.sites.length)"
      variant="accordion"
      rounded="0"
      flat
      class="adopt-panels"
    >
      <v-expansion-panel v-for="install in installs" :key="install.path" elevation="0">
        <v-expansion-panel-title>
          <v-icon size="small" class="mr-2">mdi-import</v-icon>
          <span class="text-body-2">
            {{ t('imports.found', { tool: install.source, n: install.sites.length }) }}
          </span>
        </v-expansion-panel-title>

        <v-expansion-panel-text class="adopt-body">
          <div class="text-caption text-medium-emphasis mb-2">
            {{ t('imports.explain', { path: install.path }) }}
          </div>

          <!-- The destructive choice is a switch above the list, off, and says
               what it does. A per-row "move" button would be a delete somebody
               reaches for while aiming at the row below. -->
          <v-switch
            v-model="importMove"
            color="warning"
            density="compact"
            hide-details
            class="mb-2"
            :label="t('imports.move')"
          />
          <div class="text-caption text-medium-emphasis mb-3">
            {{ importMove ? t('imports.moveOn') : t('imports.moveOff') }}
          </div>

          <div v-for="site in install.sites" :key="site.path" class="adopt-row">
            <span class="adopt-name">{{ site.name }}</span>

            <v-chip v-if="site.detected.framework" size="x-small" color="success" variant="tonal">
              {{ site.detected.framework }}
            </v-chip>
            <v-chip v-else size="x-small" variant="tonal">{{ site.detected.runtime }}</v-chip>

            <v-chip v-if="site.domain" size="x-small" variant="tonal">{{ site.domain }}</v-chip>

            <span class="adopt-evidence">
              {{
                site.partial
                  ? t('imports.sizeAtLeast', { size: bytes(site.bytes) })
                  : bytes(site.bytes)
              }}
            </span>

            <v-spacer />

            <span v-if="site.taken" class="text-caption text-medium-emphasis mr-2">
              {{ t('imports.taken') }}
            </span>
            <v-btn
              v-else
              size="x-small"
              variant="tonal"
              color="primary"
              prepend-icon="mdi-import"
              :loading="importing === site.path"
              :disabled="!!importing || !!adopting"
              @click="importSite(install, site)"
            >
              {{ t('imports.take') }}
            </v-btn>
          </div>
        </v-expansion-panel-text>
      </v-expansion-panel>
    </v-expansion-panels>

    <!-- Flush and square, like the search field directly under it. Both span
         the card, and a radius on a surface that runs to an edge cuts a notch
         out of the corner rather than rounding it — which is what the inset
         version looked like against the rows below. -->
    <v-expansion-panels
      v-if="adoptable.length"
      variant="accordion"
      rounded="0"
      flat
      class="adopt-panels"
    >
      <v-expansion-panel elevation="0">
        <v-expansion-panel-title>
          <v-icon size="small" class="mr-2">mdi-folder-search-outline</v-icon>
          <span class="text-body-2">{{ t('adopt.found', { n: adoptable.length }) }}</span>
        </v-expansion-panel-title>

        <!-- Bounded and scrolling: a checkout with twenty stray folders pushed
             the table itself off the screen, and this is a thing you deal with
             once rather than the reason the page exists. -->
        <v-expansion-panel-text class="adopt-body">
          <div v-for="folder in adoptable" :key="folder.name" class="adopt-row">
            <span class="adopt-name">{{ folder.name }}</span>

            <v-chip v-if="folder.detected.framework" size="x-small" color="success" variant="tonal">
              {{ folder.detected.framework }}
            </v-chip>
            <v-chip v-else size="x-small" variant="tonal">{{ folder.detected.runtime }}</v-chip>

            <!-- The files the guess came from. A document root inferred wrongly
               builds, starts and serves a 404 with no error anywhere. -->
            <span class="adopt-evidence">
              {{
                folder.detected.evidence.length
                  ? t('adopt.from', { files: folder.detected.evidence.join(', ') })
                  : t('adopt.noEvidence')
              }}
            </span>

            <v-spacer />

            <!-- Offered only when the folder has one. It is the better route when
               it exists: a compose file states the PHP version, the domain and
               the services, none of which any marker file does. -->
            <v-btn
              v-if="folder.composeFile"
              size="x-small"
              variant="tonal"
              color="primary"
              prepend-icon="mdi-file-import-outline"
              :loading="migrationBusy && migrationFor === folder.name"
              :disabled="!!adopting || migrationBusy"
              @click="scanCompose(folder)"
            >
              {{ t('migrate.read') }}
            </v-btn>

            <v-btn
              size="x-small"
              variant="tonal"
              :loading="adopting === folder.name"
              :disabled="!!adopting || !folder.hasFiles || migrationBusy"
              @click="adopt(folder)"
            >
              {{ t('adopt.action') }}
            </v-btn>
          </div>
        </v-expansion-panel-text>
      </v-expansion-panel>
    </v-expansion-panels>

    <!-- The compose review. A dialog rather than an inline expansion: it is a
         decision about two files at once — a manifest and the shared .env —
         and it deserves the whole of the reader's attention. -->
    <v-dialog :model-value="!!migration" max-width="760" @update:model-value="closeMigration">
      <v-card v-if="migration">
        <v-card-title class="d-flex align-center ga-2">
          <v-icon size="20">mdi-file-import-outline</v-icon>
          {{ t('migrate.title', { name: migrationFor }) }}
        </v-card-title>
        <v-card-subtitle class="text-caption pb-2">
          {{ migration.migration.source }}
        </v-card-subtitle>

        <v-divider />

        <v-card-text>
          <div class="section-head mb-2">{{ t('migrate.project') }}</div>
          <v-table density="compact">
            <tbody>
              <tr v-for="(value, key) in migrationFields" :key="key">
                <td class="text-medium-emphasis">{{ t(`migrate.field.${key}`) }}</td>
                <td class="mono">{{ value }}</td>
              </tr>
            </tbody>
          </v-table>

          <!-- The half no marker file can state, and the reason this exists. -->
          <template v-if="migration.env.changes.length">
            <div class="section-head mt-5 mb-2">{{ t('migrate.services') }}</div>
            <v-table density="compact">
              <tbody>
                <tr v-for="change in migration.env.changes" :key="change.key">
                  <td>{{ change.subject }}</td>
                  <td class="mono text-medium-emphasis">{{ change.from ?? '—' }}</td>
                  <td class="mono">{{ change.to }}</td>
                </tr>
              </tbody>
            </v-table>
          </template>
          <div
            v-else-if="migration.migration.services.length"
            class="text-caption text-medium-emphasis mt-4"
          >
            {{ t('migrate.servicesAlready') }}
          </div>

          <!-- Named, not dropped: silently ignoring the one service the project
               actually needs looks finished and is not. -->
          <v-alert
            v-if="migration.migration.unmapped.length"
            type="warning"
            variant="tonal"
            class="mt-4"
          >
            <div class="text-caption font-weight-medium mb-1">{{ t('migrate.unmapped') }}</div>
            <div
              v-for="entry in migration.migration.unmapped"
              :key="entry"
              class="text-caption mono"
            >
              {{ entry }}
            </div>
          </v-alert>

          <v-alert v-if="migration.alreadyManaged" type="info" variant="tonal" class="mt-4">
            <div class="text-caption">{{ t('migrate.alreadyManaged') }}</div>
          </v-alert>

          <v-expansion-panels variant="accordion" class="mt-4">
            <v-expansion-panel :title="t('migrate.evidence')">
              <v-expansion-panel-text>
                <div
                  v-for="line in migration.migration.evidence"
                  :key="line"
                  class="text-caption mono"
                >
                  {{ line }}
                </div>
              </v-expansion-panel-text>
            </v-expansion-panel>
            <v-expansion-panel :title="t('migrate.manifest')">
              <v-expansion-panel-text>
                <pre class="migrate-json">{{ JSON.stringify(migration.spec, null, 2) }}</pre>
              </v-expansion-panel-text>
            </v-expansion-panel>
          </v-expansion-panels>
        </v-card-text>

        <v-divider />

        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" :disabled="migrationBusy" @click="closeMigration">
            {{ t('app.cancel') }}
          </v-btn>
          <v-btn color="primary" variant="flat" :loading="migrationBusy" @click="applyMigration">
            {{ t('migrate.apply') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

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
        :items="rows"
        :group-by="groupBy"
        :sort-by="sortBy"
        :search="search"
        :loading="inventory.loadingProjects"
        items-per-page="-1"
        class="elevation-0"
        fixed-header
        hover
        item-value="name"
        striped="even"
        hide-default-footer
        height="100%"
        density="compact"
      >
        <!-- Standalone projects never reach this slot: their group value is
             null and the table skips the header for those outright. The guard
             is here anyway so a future key change cannot quietly reintroduce a
             one-row group with a heading over it. -->
        <template #group-header="{ item, columns, toggleGroup, isGroupOpen }">
          <tr
            v-if="item.items.length > 1"
            class="group-row"
            :data-open="openByDefault(item, isGroupOpen, toggleGroup)"
          >
            <td :colspan="columns.length">
              <div class="d-flex align-center ga-2">
                <v-btn
                  size="x-small"
                  variant="text"
                  :icon="isGroupOpen(item) ? 'mdi-chevron-down' : 'mdi-chevron-right'"
                  :aria-label="item.value"
                  @click="toggleGroup(item)"
                />
                <v-icon size="small" icon="mdi-sitemap-outline" />
                <span class="font-weight-medium">{{ item.value }}</span>
                <v-chip size="x-small" variant="tonal">{{ item.items.length }}</v-chip>
              </div>
            </td>
          </tr>
        </template>

        <template #item.domain="{ item }">
          <div v-if="item.domain" class="d-flex align-center ga-2">
            <a
              class="domain-link"
              @click="item.domainConfigured && api.openInBrowser(`https://${item.domain}`)"
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

            <v-tooltip v-if="item.generatedStale" location="top">
              <template #activator="{ props }">
                <v-icon v-bind="props" color="info" size="small" @click.stop="applyChange(item)"
                  >mdi-sync-alert</v-icon
                >
              </template>
              <span class="text-caption">
                {{
                  item.built ? t('projects.manifestChangedBuilt') : t('projects.manifestChanged')
                }}
              </span>
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

        <!-- Rebuild: the whole chain, for a project that already has an image.
             The control column's hammer only appears while `!built`, so once a
             project had been built there was no way to rebuild it from here at
             all — and that is exactly when it is needed, because changing the
             PHP version or an extension changes the *Dockerfile*.

             Not the same act as the stale-manifest icon beside the domain.
             That one regenerates files and stops; this regenerates, rebuilds
             the image and recreates the container. Restart is a third thing
             again — same container, same image, so a rebuilt configuration
             never reaches it. -->
        <template #item.rebuild="{ item }">
          <v-btn
            v-if="item.built"
            block
            size="small"
            color="info"
            variant="tonal"
            :aria-label="t('projectsView.rebuild')"
            :loading="ops.isBusy(item.name)"
            :disabled="!app.engineUp || !item.manifestValid"
            @click="act(item, (n) => api.projectBuild(n))"
          >
            <v-icon>mdi-hammer-wrench</v-icon>
            <v-tooltip activator="parent" location="top">
              {{ t('projectsView.rebuildHint') }}
            </v-tooltip>
          </v-btn>
        </template>

        <template #item.terminal="{ item }">
          <v-btn
            v-if="item.running"
            block
            size="small"
            color="info"
            variant="tonal"
            :aria-label="t('detail.externalTerminal')"
            @click="openTerminal(item)"
          >
            <v-icon>mdi-console</v-icon>
            <v-tooltip activator="parent">{{ t('detail.externalTerminal') }}</v-tooltip>
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
            @click="api.openInBrowser(`https://${item.domain}`)"
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

        <!-- Two empty states, not one.
             "Nothing here yet" and "nothing matched what you typed" are
             different situations with different next moves, and a single
             centred sentence answered neither: on a first run it named the
             problem and offered nothing, and after a typo it implied the
             projects were gone. Each now carries the action that resolves it. -->
        <template #no-data>
          <v-empty-state
            v-if="search"
            icon="mdi-magnify-close"
            :title="t('projects.noMatch')"
            :text="t('projects.noMatchText', { term: search })"
          >
            <template #actions>
              <v-btn variant="tonal" prepend-icon="mdi-close" @click="search = ''">
                {{ t('projects.clearSearch') }}
              </v-btn>
            </template>
          </v-empty-state>

          <v-empty-state
            v-else
            icon="mdi-folder-plus-outline"
            :title="t('projects.empty')"
            :text="t('projects.emptyText')"
          >
            <template #actions>
              <v-btn
                color="primary"
                variant="flat"
                prepend-icon="mdi-plus"
                :disabled="!app.hasWorkspace"
                @click="app.newProjectOpen = true"
              >
                {{ t('newProject.title') }}
              </v-btn>
            </template>
          </v-empty-state>
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
          <p class="text-body-2 mb-1">{{ t('newProject.deleteBody') }}</p>
          <!-- The rest of what goes, named. A dialog that says only "your
               source files stay" reads as a promise that nothing else moves. -->
          <p class="text-caption text-medium-emphasis mb-3">{{ t('newProject.deleteAlso') }}</p>
          <v-checkbox
            v-model="deleteFiles"
            :label="t('newProject.deleteFiles')"
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
/* Bounded so a checkout with many stray folders cannot push the table off the
   screen. The height is the panel body's, not the page's — adopting is
   something you do once, and it should not become the reason this page
   scrolls. */
/* A rule under it, so the panel reads as a band of the card rather than as a
   row of the table it sits above. */
.adopt-panels {
  border-bottom: thin solid rgba(var(--v-border-color), var(--v-border-opacity));
}

.adopt-body :deep(.v-expansion-panel-text__wrapper) {
  max-height: 240px;
  overflow-y: auto;
}

/* A heading, not a row of data: it carries the parent domain and a count, and
   should read as the label above the rows rather than as one of them. */
.group-row td {
  background: rgba(var(--v-theme-on-surface), 0.04);
}

/* Names a block inside the migration review, which has four of them. */
.section-head {
  font-size: 13px;
  font-weight: 600;
  opacity: 0.82;
}

/* A version, a domain or a container path — places where 8.0 and 8.O differ. */
.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

/* The proposed manifest, scrolling in its own box rather than stretching the
   dialog to the height of whatever the compose file turned out to imply. */
.migrate-json {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11.5px;
  line-height: 1.5;
  max-height: 260px;
  overflow: auto;
  margin: 0;
}

.domain-link {
  color: inherit;
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

/* Column labels are short phrases, and wrapping them onto a second line makes
   the header band twice the height of a row for no gain. They stay on one line
   and take the width they need. */
.table-wrap :deep(thead th) {
  white-space: nowrap;
}

.table-wrap :deep(.v-data-table-header__content) {
  flex-wrap: nowrap;
}

.adopt-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 0;
  min-width: 0;
}

.adopt-name {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px;
  font-weight: 600;
}

/* The evidence is the part that lets someone check the guess, so it truncates
   rather than wrapping the row into two lines per folder. */
.adopt-evidence {
  font-size: 12px;
  opacity: 0.7;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
</style>
