<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import { bytes } from '@/lib/format';
import ErrorAlert from '@/components/ErrorAlert.vue';
import HostsDialog from '@/components/HostsDialog.vue';
import SettingsGroup from '@/components/SettingsGroup.vue';

/**
 * The doctor: one screen that says what is wrong and offers the repair.
 *
 * The boot gate answers "can the app run at all" and then gets out of the way.
 * The failures that arrive later — a port some other stack got to first, a
 * hosts file missing a domain, generated config older than the manifest it was
 * derived from, a disk quietly filling with dangling images — each surfaced
 * one failed `compose up` at a time, as an error about itself rather than
 * about what to do. This panel reads the whole `doctor` report and puts every
 * repair the app knows next to the finding it repairs.
 *
 * The repairs deliberately reuse the existing flows rather than shortcut them:
 * hosts go through the reviewed-diff dialog, never a blind write; pruning
 * volumes is opt-in behind its own warning, because the engine's "unused"
 * means "not currently mounted", and the database of a stopped project
 * qualifies.
 */
const { t } = useI18n();
const app = useAppStore();

const report = ref(null);
const error = ref(null);
const loading = ref(false);
/** Which repair is running — keys the one spinner that should spin. */
const busy = ref(null);

const hostsOpen = ref(false);
const pruneOpen = ref(false);
const pruneImages = ref(true);
const pruneVolumes = ref(false);
/**
 * Off by default, and a checkbox rather than a level, because only one of the
 * two levels is a decision worth surfacing here.
 *
 * `dangling` already happens on its own — deleting a project reclaims the
 * cache its image was holding. What is left is the shared part: every project
 * image builds from the same PHP base and the same extension installs, so
 * those layers are one cache serving all of them. Ticking this reclaims that,
 * and every project's next build runs in full.
 */
const pruneBuildCache = ref(false);
/** The last prune's outcome, shown until the next action. */
const pruneResult = ref(null);

const STATE = {
  ok: { icon: 'mdi-check-circle', color: 'success' },
  warn: { icon: 'mdi-alert', color: 'warning' },
  fail: { icon: 'mdi-close-circle', color: 'error' },
  unknown: { icon: 'mdi-help-circle-outline', color: 'grey' },
};

/**
 * Per-member disk attribution, loaded with the report.
 *
 * Separate call and separate state: the sizes come from `list_containers`
 * with size=true, which the engine computes on demand and can take seconds
 * on a large stack — the rest of the report should not wait for it.
 */
const owners = ref(null);

const requirements = computed(() => report.value?.preflight?.requirements ?? []);
const ports = computed(() => report.value?.ports ?? []);
const hostsMissing = computed(() => report.value?.hostsMissing ?? []);
const generated = computed(() => report.value?.generated ?? null);

/**
 * The containers every domain is routed through — Traefik, today.
 *
 * The gate says whether the app can run and the ports section says whether 80
 * is free; neither answers "the proxy is down", which is the state that makes a
 * correctly installed stack look broken.
 */
const core = computed(() => report.value?.core ?? []);
const coreDown = computed(() => core.value.some((c) => c.state === 'fail'));
const space = computed(() => report.value?.space ?? null);

/**
 * Extensions that will be dropped from a build without anyone being told.
 *
 * The generator skips one it cannot install and says nothing, so the symptom
 * arrives much later as a fatal `Call to undefined function` — with nothing
 * connecting it to a build that reported success. This is the only place the
 * app says it out loud.
 */
const extensions = computed(() => report.value?.extensions ?? []);

/** The default-set rows first: they break a project nobody has touched yet. */
const extensionsDefault = computed(() => extensions.value.filter((e) => e.isDefaultSet));
const extensionsProjects = computed(() => extensions.value.filter((e) => !e.isDefaultSet));

/** One row per extension, with the versions it fails on folded together —
 *  `imap` failing on both candidate defaults is one problem, not two. */
const extensionsDefaultRows = computed(() => {
  const byName = new Map();
  for (const e of extensionsDefault.value) {
    const row = byName.get(e.extension) ?? { ...e, versions: [] };
    row.versions.push(e.phpVersion);
    byName.set(e.extension, row);
  }
  return [...byName.values()];
});

async function load() {
  loading.value = true;
  error.value = null;
  try {
    report.value = await api.doctor();
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
  // After the report, not alongside it — and silently: with the engine down
  // the report already says so, and this list is simply absent.
  try {
    owners.value = report.value?.space ? await api.dockerDiskUsage() : null;
  } catch {
    owners.value = null;
  }
}

/** Route a requirement's repair through the store — it already knows that
 *  `workspace` means the folder picker and `engine` means start-and-poll. */
async function fixRequirement(id) {
  busy.value = id;
  try {
    await app.fixRequirement(id);
    await load();
  } finally {
    busy.value = null;
  }
}

/**
 * Take one unbuildable extension out of one selection.
 *
 * No confirmation dialog, deliberately: the extension is already being dropped
 * from the build, so this removes nothing the container ever had — it makes the
 * manifest agree with reality. The row itself names exactly what goes.
 */
async function dropExtension(row) {
  busy.value = `ext-${row.subject}-${row.extension}`;
  error.value = null;
  try {
    report.value = await api.doctorDropExtension(row.subject, row.extension);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

/**
 * Bring the core containers up — the same `minimal` mode the first-run setup
 * uses, which is the `core` profile and nothing else.
 *
 * Not `all`: the finding is "the proxy is not running", and answering it by
 * starting every enabled service would be a repair larger than the problem.
 */
async function startCore() {
  busy.value = 'core';
  error.value = null;
  try {
    await api.composeUp('minimal');
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

/**
 * What one core-container row should say.
 *
 * "Missing" and "stopped" are deliberately different sentences. `compose down`
 * removes containers, so the two states arrive from different actions and want
 * different repairs — and reporting both as "not running" is what sent somebody
 * looking for a stopped container that had never existed.
 */
function coreLabel(c) {
  if (c.state === 'unknown') return t('doctor.coreUnknown');
  if (c.running) return c.image || t('doctor.coreRunning');
  return c.exists ? t('doctor.coreStopped') : t('doctor.coreMissing');
}

async function regenerate() {
  busy.value = 'generated';
  error.value = null;
  try {
    await api.generateRun('all');
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

async function prune() {
  busy.value = 'prune';
  error.value = null;
  try {
    pruneResult.value = await api.dockerPrune(
      pruneImages.value,
      pruneVolumes.value,
      pruneBuildCache.value ? 'all' : 'keep'
    );
    pruneOpen.value = false;
    await load();
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = null;
  }
}

/** What one port row should say, in the user's language. */
function portLabel(p) {
  if (p.state === 'unknown') return t('doctor.portUnknown');
  if (p.ours) return t('doctor.portOurs', { name: p.process });
  if (p.state === 'ok') return t('doctor.portFree');
  if (p.process && p.pid) return t('doctor.portHeldPid', { process: p.process, pid: p.pid });
  if (p.process) return t('doctor.portHeld', { process: p.process });
  return t('doctor.portHeldUnknown');
}

onMounted(load);
</script>

<template>
  <ErrorAlert :error="error" type="error" class="mb-4" />

  <!-- ---- the boot gate's rows, re-checkable ------------------------------ -->
  <SettingsGroup
    icon="mdi-clipboard-pulse-outline"
    :title="t('doctor.requirements')"
    :description="t('doctor.requirementsDesc')"
  >
    <template #append>
      <v-btn
        size="x-small"
        variant="text"
        icon="mdi-refresh"
        :aria-label="t('preflight.recheck')"
        :loading="loading"
        @click="load"
      />
    </template>

    <div v-if="!report && loading" class="text-caption text-medium-emphasis">
      {{ t('doctor.loading') }}
    </div>

    <div v-for="r in requirements" :key="r.id" class="row">
      <v-icon :color="STATE[r.state].color" size="18">{{ STATE[r.state].icon }}</v-icon>
      <div class="min-w-0">
        <span class="text-body-2">{{ t(`preflight.${r.id}`) }}</span>
        <div v-if="r.detail" class="text-caption text-medium-emphasis detail">{{ r.detail }}</div>
      </div>
      <v-spacer />
      <v-btn
        v-if="r.state === 'fail' && r.fixable"
        size="small"
        color="primary"
        variant="tonal"
        :loading="busy === r.id"
        @click="fixRequirement(r.id)"
      >
        {{ t(`preflight.${r.id}Action`) }}
      </v-btn>
    </div>
  </SettingsGroup>

  <!-- ---- the containers everything is routed through ---------------------- -->
  <!-- Above ports on purpose: "the proxy is not running" is the finding, and
       "port 80 is free" is a symptom of it rather than a separate problem. -->
  <SettingsGroup
    v-if="core.length"
    icon="mdi-swap-horizontal-bold"
    :title="t('doctor.coreTitle')"
    :description="t('doctor.coreDesc')"
    class="mt-4"
  >
    <div v-for="c in core" :key="c.service" class="row">
      <v-icon :color="STATE[c.state].color" size="18">{{ STATE[c.state].icon }}</v-icon>
      <div class="min-w-0">
        <span class="text-body-2">{{ c.container }}</span>
        <div class="text-caption text-medium-emphasis detail">{{ coreLabel(c) }}</div>
      </div>
    </div>

    <v-btn
      v-if="coreDown"
      size="small"
      color="primary"
      variant="tonal"
      class="mt-3"
      prepend-icon="mdi-play"
      :loading="busy === 'core'"
      :disabled="!app.engineUp"
      @click="startCore"
    >
      {{ t('doctor.coreStart') }}
    </v-btn>
  </SettingsGroup>

  <!-- ---- ports ----------------------------------------------------------- -->
  <SettingsGroup
    icon="mdi-lan-pending"
    :title="t('doctor.portsTitle')"
    :description="t('doctor.portsDesc')"
    class="mt-4"
  >
    <div v-if="report && !ports.length" class="text-caption text-medium-emphasis">
      {{ t('doctor.portsNone') }}
    </div>

    <div v-for="p in ports" :key="p.port" class="row">
      <v-icon :color="STATE[p.state].color" size="18">{{ STATE[p.state].icon }}</v-icon>
      <div class="min-w-0">
        <span class="text-body-2">
          <code class="port">{{ p.port }}</code>
          <span class="text-medium-emphasis"> · {{ p.requiredBy }}</span>
        </span>
        <div class="text-caption text-medium-emphasis">{{ portLabel(p) }}</div>
      </div>
    </div>
  </SettingsGroup>

  <!-- ---- hosts file ------------------------------------------------------- -->
  <SettingsGroup
    icon="mdi-file-document-outline"
    :title="t('doctor.hostsTitle')"
    :description="t('doctor.hostsDesc')"
    class="mt-4"
  >
    <div v-if="report && !hostsMissing.length" class="row">
      <v-icon color="success" size="18">mdi-check-circle</v-icon>
      <span class="text-body-2">{{ t('doctor.hostsOk') }}</span>
    </div>

    <template v-if="hostsMissing.length">
      <div class="row">
        <v-icon color="warning" size="18">mdi-alert</v-icon>
        <div class="min-w-0">
          <span class="text-body-2">
            {{ t('doctor.hostsMissing', { count: hostsMissing.length }) }}
          </span>
          <div class="mt-1 d-flex ga-1 flex-wrap">
            <v-chip v-for="d in hostsMissing" :key="d" size="x-small" variant="tonal">
              {{ d }}
            </v-chip>
          </div>
        </div>
        <v-spacer />
        <!-- Through the reviewed diff, never a blind write: the dialog shows
             exactly what changes before the one elevated operation. -->
        <v-btn size="small" color="primary" variant="tonal" @click="hostsOpen = true">
          {{ t('doctor.hostsRepair') }}
        </v-btn>
      </div>
    </template>
  </SettingsGroup>

  <!-- ---- php extensions ---------------------------------------------------- -->
  <!-- The generator drops an extension it cannot install and says nothing, so
       the failure surfaces much later as a fatal "undefined function" with
       nothing tying it back to a build that reported success. -->
  <SettingsGroup
    icon="mdi-puzzle-outline"
    :title="t('doctor.extTitle')"
    :description="`${t('doctor.extDesc')} ${t('doctor.extRemoveHint')}`"
    class="mt-4"
  >
    <div v-if="report && !extensions.length" class="row">
      <v-icon color="success" size="18">mdi-check-circle</v-icon>
      <span class="text-body-2">{{ t('doctor.extOk') }}</span>
    </div>

    <!-- The default set first: this one breaks a project nobody has touched. -->
    <div v-for="row in extensionsDefaultRows" :key="`d-${row.extension}`" class="row">
      <v-icon color="error" size="18">mdi-alert-circle</v-icon>
      <div class="min-w-0">
        <span class="text-body-2">
          {{ t('doctor.extDefault', { ext: row.extension, detail: row.detail }) }}
        </span>
        <div class="text-caption text-medium-emphasis">
          {{ t('doctor.extDefaultWhy', { versions: row.versions.join(', ') }) }}
        </div>
      </div>
      <v-spacer />
      <v-btn
        size="small"
        color="primary"
        variant="tonal"
        :loading="busy === `ext-${row.subject}-${row.extension}`"
        @click="dropExtension(row)"
      >
        {{ t('doctor.extRemove') }}
      </v-btn>
    </div>

    <div v-for="row in extensionsProjects" :key="`${row.subject}-${row.extension}`" class="row">
      <v-icon color="warning" size="18">mdi-alert</v-icon>
      <div class="min-w-0">
        <span class="text-body-2">
          {{ t('doctor.extProject', { ext: row.extension, project: row.subject }) }}
        </span>
        <div class="text-caption text-medium-emphasis">{{ row.detail }}</div>
      </div>
      <v-spacer />
      <v-btn
        size="small"
        variant="text"
        :to="{ name: 'ProjectDetail', params: { name: row.subject } }"
      >
        {{ t('doctor.extOpen') }}
      </v-btn>
      <!-- Safe without a confirmation: the extension is already absent from
           the built container, so this removes nothing it ever had. -->
      <v-btn
        size="small"
        color="primary"
        variant="tonal"
        :loading="busy === `ext-${row.subject}-${row.extension}`"
        @click="dropExtension(row)"
      >
        {{ t('doctor.extRemove') }}
      </v-btn>
    </div>
  </SettingsGroup>

  <!-- ---- generated config -------------------------------------------------- -->
  <SettingsGroup
    icon="mdi-file-refresh-outline"
    :title="t('doctor.generatedTitle')"
    :description="t('doctor.generatedDesc')"
    class="mt-4"
  >
    <div v-if="generated" class="row">
      <v-icon :color="STATE[generated.state].color" size="18">
        {{ STATE[generated.state].icon }}
      </v-icon>
      <div class="min-w-0">
        <span class="text-body-2">
          {{
            generated.state === 'ok'
              ? t('doctor.generatedOk')
              : generated.state === 'warn'
                ? t('doctor.generatedStale', { file: generated.detail })
                : generated.state === 'fail'
                  ? t('doctor.generatedMissing')
                  : t('doctor.generatedUnknown')
          }}
        </span>
      </div>
      <v-spacer />
      <v-btn
        v-if="generated.state === 'warn' || generated.state === 'fail'"
        size="small"
        color="primary"
        variant="tonal"
        :loading="busy === 'generated'"
        @click="regenerate"
      >
        {{ t('doctor.regenerate') }}
      </v-btn>
    </div>
  </SettingsGroup>

  <!-- ---- disk ---------------------------------------------------------------- -->
  <SettingsGroup
    icon="mdi-harddisk"
    :title="t('doctor.spaceTitle')"
    :description="t('doctor.spaceDesc')"
    class="mt-4"
  >
    <div v-if="report && !space" class="text-caption text-medium-emphasis">
      {{ t('doctor.spaceUnknown') }}
    </div>

    <template v-if="space">
      <div class="row">
        <v-icon :color="space.images.unused ? 'warning' : 'success'" size="18">
          {{ space.images.unused ? 'mdi-alert' : 'mdi-check-circle' }}
        </v-icon>
        <span class="text-body-2">
          {{ t('doctor.spaceImages', { count: space.images.unused }) }}
        </span>
        <v-chip size="x-small" variant="tonal">{{ bytes(space.images.size) }}</v-chip>
      </div>
      <div class="row">
        <v-icon :color="space.volumes.unused ? 'warning' : 'success'" size="18">
          {{ space.volumes.unused ? 'mdi-alert' : 'mdi-check-circle' }}
        </v-icon>
        <span class="text-body-2">
          {{ t('doctor.spaceVolumes', { count: space.volumes.unused }) }}
        </span>
        <v-chip size="x-small" variant="tonal">{{ bytes(space.volumes.size) }}</v-chip>
        <v-spacer />
        <v-btn
          size="small"
          color="primary"
          variant="tonal"
          :disabled="!space.images.unused && !space.volumes.unused"
          @click="pruneOpen = true"
        >
          {{ t('doctor.reclaim') }}
        </v-btn>
      </div>

      <div v-if="pruneResult" class="text-caption text-medium-emphasis mt-2">
        {{
          t('doctor.pruneResult', {
            images: pruneResult.imagesDeleted,
            volumes: pruneResult.volumesDeleted,
            caches: pruneResult.cachesDeleted,
            size: bytes(pruneResult.spaceReclaimed),
          })
        }}
      </div>

      <!-- Who holds the bytes. The totals above say how much; this says which
           project — the question a full disk actually raises. -->
      <template v-if="owners?.length">
        <div class="text-caption font-weight-medium mt-4 mb-1">
          {{ t('doctor.ownersTitle') }}
        </div>
        <v-table density="compact" class="owners">
          <thead>
            <tr>
              <th>{{ t('doctor.ownerCol') }}</th>
              <th>{{ t('doctor.ownerImage') }}</th>
              <th class="text-right">{{ t('doctor.ownerImageSize') }}</th>
              <th class="text-right">{{ t('doctor.ownerRw') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="o in owners" :key="o.id + (o.image ?? '')">
              <td>
                <span class="text-body-2">{{ o.id }}</span>
                <v-chip
                  v-if="o.image && !o.running && o.imageDedicated && !o.containerRw"
                  size="x-small"
                  color="warning"
                  variant="tonal"
                  class="ml-2"
                >
                  {{ t('doctor.ownerOrphan') }}
                </v-chip>
              </td>
              <td class="text-caption text-medium-emphasis">
                {{ o.image ?? '—' }}
                <span v-if="o.image && !o.imageDedicated"> · {{ t('doctor.ownerShared') }}</span>
              </td>
              <td class="text-right text-caption">{{ bytes(o.imageSize) }}</td>
              <td class="text-right text-caption">{{ bytes(o.containerRw) }}</td>
            </tr>
          </tbody>
        </v-table>
      </template>
    </template>
  </SettingsGroup>

  <HostsDialog v-model="hostsOpen" :add="hostsMissing" @applied="load" />

  <!-- Confirmation before anything is deleted. Volumes carry their own
       warning: "unused" is the engine's word for "not currently mounted",
       which includes the data of a project that is merely stopped. -->
  <v-dialog v-model="pruneOpen" max-width="520">
    <v-card>
      <v-card-item>
        <template #prepend><v-icon color="warning">mdi-delete-sweep-outline</v-icon></template>
        <v-card-title class="text-body-1">{{ t('doctor.pruneTitle') }}</v-card-title>
      </v-card-item>

      <v-card-text>
        <v-checkbox
          v-model="pruneImages"
          density="compact"
          hide-details
          :label="
            t('doctor.pruneImagesLabel', {
              count: space?.images.unused ?? 0,
              size: bytes(space?.images.size ?? 0),
            })
          "
        />
        <v-checkbox
          v-model="pruneVolumes"
          density="compact"
          hide-details
          :label="
            t('doctor.pruneVolumesLabel', {
              count: space?.volumes.unused ?? 0,
              size: bytes(space?.volumes.size ?? 0),
            })
          "
        />
        <v-checkbox
          v-model="pruneBuildCache"
          density="compact"
          hide-details
          :label="t('doctor.pruneBuildCacheLabel')"
        />
        <v-alert v-if="pruneVolumes" type="warning" variant="tonal" class="mt-3">
          <div class="text-caption">{{ t('doctor.pruneVolumesWarning') }}</div>
        </v-alert>
        <!-- Its own warning, for its own reason: this one costs no data, it
             costs every project's next build. -->
        <v-alert v-if="pruneBuildCache" type="warning" variant="tonal" class="mt-3">
          <div class="text-caption">{{ t('doctor.pruneBuildCacheWarning') }}</div>
        </v-alert>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="pruneOpen = false">{{ t('app.cancel') }}</v-btn>
        <v-btn
          color="primary"
          variant="flat"
          :disabled="!pruneImages && !pruneVolumes && !pruneBuildCache"
          :loading="busy === 'prune'"
          @click="prune"
        >
          {{ t('doctor.pruneConfirm') }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 0;
}

.row + .row {
  border-top: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

.port {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.detail {
  word-break: break-all;
}

.min-w-0 {
  min-width: 0;
}
</style>
