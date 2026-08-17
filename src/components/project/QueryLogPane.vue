<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * What the database was asked while you were looking, and what it was asked
 * three hundred times.
 *
 * F-1 — the row §2 calls the largest product gap. Three competitors sell this
 * and none of them do it without an agent; MySQL and MariaDB have had a
 * collector all along, and it is switchable at runtime from the connection this
 * app already has.
 *
 * ## The pane is a session, not a feed
 *
 * Recording is off, you turn it on, you reload the page you are investigating,
 * you look, you turn it off. That shape is the whole design: the general log is
 * unsampled and costs write throughput on every statement, so leaving it on is
 * not a smaller version of this feature — it is a different and worse one. The
 * switch says so, and stopping clears what was collected.
 *
 * ## Repeats first, statements second
 *
 * The list of every statement is the evidence; the repeated shapes are the
 * finding. A pane that led with four hundred rows of SQL would be a log viewer,
 * and the question people actually have is "what is this page doing twice".
 */
const { t } = useI18n();

/**
 * The databases this workspace runs, asked for here rather than passed in.
 *
 * `db_targets` is a workspace-level answer — which database services exist and
 * whether they are running — and threading it through the page would make this
 * pane's correctness depend on a parent remembering to fetch it.
 */
const targets = ref([]);

const service = ref(null);
const session = ref(null);
const busy = ref(false);
const error = ref(null);

/**
 * Only the ones that can answer, which is now all four this app runs.
 *
 * The list is still a filter rather than a greyed row: anything else in the
 * workspace — Redis, RabbitMQ, Memcached — keeps no statement log to switch on,
 * and a disabled row invites "why not" for an answer that is a paragraph.
 */
const usable = computed(() =>
  asList(targets.value).filter((target) =>
    ['mysql', 'mariadb', 'postgres', 'mongo'].includes(target.service)
  )
);

const recording = computed(() => session.value?.recording === true);
const repeats = computed(() => asList(session.value?.repeats));
const entries = computed(() => asList(session.value?.entries));

async function load() {
  if (!service.value) return;
  error.value = null;
  try {
    session.value = await api.queryLog(service.value);
  } catch (e) {
    error.value = e;
  }
}

async function toggle(on) {
  busy.value = true;
  error.value = null;
  try {
    session.value = await api.queryLogRecord(service.value, on);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

async function clear() {
  busy.value = true;
  error.value = null;
  try {
    session.value = await api.queryLogClear(service.value);
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

// The first usable target, so the pane opens on something rather than on an
// empty select somebody has to discover.
onMounted(async () => {
  try {
    targets.value = asList(await api.dbTargets());
  } catch (e) {
    error.value = e;
  }
});

watch(
  usable,
  (list) => {
    if (!service.value && list.length) {
      service.value = list[0].service;
      load();
    }
  },
  { immediate: true }
);
watch(service, load);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-database-search</v-icon>{{ t('queryLog.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('queryLog.explain') }}</p>

    <!-- Nothing this can attach to. Said plainly rather than shown as an empty
         control: the project may simply not use a SQL database. -->
    <v-alert v-if="!usable.length" type="info" variant="tonal">
      <div class="text-caption">{{ t('queryLog.noTarget') }}</div>
    </v-alert>

    <template v-else>
      <v-select
        v-if="usable.length > 1"
        v-model="service"
        :items="usable.map((target) => ({ value: target.service, title: target.service }))"
        :label="t('queryLog.database')"
        density="compact"
        prepend-inner-icon="mdi-database"
        class="mb-2"
      />

      <div class="d-flex align-center ga-3 flex-wrap">
        <v-switch
          :model-value="recording"
          :loading="busy"
          :disabled="busy"
          color="primary"
          density="compact"
          hide-details
          :label="t('queryLog.record')"
          @update:model-value="toggle($event)"
        />
        <!-- Shown for every database now. Postgres was excluded here because
             its log belongs to the server and this app must not rewrite it —
             true, and it was the wrong conclusion: "cannot delete" is not
             "cannot start again". Clearing writes a watermark into the log and
             the reader stops at it, so the button does on Postgres exactly what
             the person pressing it means. -->
        <v-btn
          v-if="recording"
          size="small"
          variant="tonal"
          prepend-icon="mdi-broom"
          :disabled="busy"
          @click="clear"
        >
          {{ t('queryLog.clear') }}
        </v-btn>
        <v-btn
          v-if="recording"
          size="small"
          variant="text"
          prepend-icon="mdi-refresh"
          :disabled="busy"
          @click="load"
        >
          {{ t('app.refresh') }}
        </v-btn>
      </div>

      <!-- Said while it is on, not once in the docs: the cost is real and the
           failure mode is forgetting. -->
      <v-alert v-if="recording" type="warning" variant="tonal" class="mt-3">
        <div class="text-caption">{{ t('queryLog.cost') }}</div>
        <!-- The one cost that is not the same on all four. MySQL and Mongo
             collect into something this app truncates or drops; Postgres writes
             every statement into the server's own log file, and no button here
             takes it back out. Said where the switch is rather than in a
             document nobody reads at the moment it matters. -->
        <div v-if="service === 'postgres'" class="text-caption mt-2">
          {{ t('queryLog.costPostgres') }}
        </div>
      </v-alert>

      <div v-else class="text-caption text-medium-emphasis mt-3">
        {{ t('queryLog.howTo') }}
      </div>

      <template v-if="recording">
        <!-- The finding, above the evidence. -->
        <div class="section-head mt-4 mb-1">{{ t('queryLog.repeats') }}</div>

        <div v-if="!repeats.length" class="text-caption text-medium-emphasis">
          {{ entries.length ? t('queryLog.noRepeats') : t('queryLog.nothingYet') }}
        </div>

        <div v-for="repeat in repeats" :key="repeat.shape" class="repeat">
          <div class="d-flex align-center ga-2">
            <v-chip size="x-small" color="warning" variant="tonal">×{{ repeat.count }}</v-chip>
            <code class="repeat-shape">{{ repeat.shape }}</code>
          </div>
          <div class="text-caption text-medium-emphasis mt-1">
            {{ t('queryLog.example') }} <code>{{ repeat.example }}</code>
          </div>
        </div>

        <div class="section-head mt-4 mb-1">
          {{ t('queryLog.statements', { count: entries.length }) }}
        </div>
        <div class="entries">
          <div v-for="(entry, i) in entries" :key="i" class="entry">
            <span class="entry-at">{{ entry.at }}</span>
            <code class="entry-sql">{{ entry.sql }}</code>
          </div>
        </div>
      </template>
    </template>
  </v-card>
</template>

<style scoped>
.repeat {
  padding: 8px 0;
  border-bottom: 1px solid rgb(var(--v-border-color), var(--v-border-opacity));
}

.repeat-shape {
  font-size: 0.8rem;
  word-break: break-all;
}

/* Bounded and scrolled rather than let to run off the page: `PageLayout` is a
   fixed-height flex column, so an unbounded child is compressed instead —
   `page-scroll.spec.js` is the guard that class of bug earned. */
.entries {
  max-height: 320px;
  overflow-y: auto;
}

.entry {
  display: flex;
  gap: 8px;
  padding: 3px 0;
  font-size: 0.75rem;
}

.entry-at {
  flex: 0 0 auto;
  color: rgb(var(--v-theme-on-surface-variant));
  opacity: 0.7;
}

.entry-sql {
  word-break: break-all;
}
</style>
