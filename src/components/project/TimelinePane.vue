<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * One page load, from both ends.
 *
 * F-2. `dd($user)` says what the code thought it had; the query log says what
 * it actually asked the database for. Both were already on this page, in two
 * panes, and reading them together meant comparing clocks by eye.
 *
 * ## Two kinds of row, and the difference is stated rather than smoothed over
 *
 * A dump knows the request it happened in, so dumps group and the group is
 * named. A query does not: nothing in a general log says which HTTP request
 * caused a statement, and inferring it from what sits either side of it would
 * be wrong the first time two requests overlap — silently. So queries sit on
 * the axis by time, outside the groups, and the legend says so. The alternative
 * is asking the application to tag its own queries, which is code in somebody's
 * project and the thing this feature was built to avoid needing.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();

const timeline = ref(null);
const service = ref(null);
const targets = ref([]);
const error = ref(null);
const loading = ref(false);

const moments = computed(() => asList(timeline.value?.moments));
const requests = computed(() => asList(timeline.value?.requests));

/** Only the databases whose log this can read — the same three `querylog` names. */
const usable = computed(() =>
  asList(targets.value).filter((target) =>
    ['mysql', 'mariadb', 'postgres', 'mongo'].includes(target.service)
  )
);

/**
 * Seconds from the first moment, so the axis reads as a duration rather than as
 * a wall clock. A page load is interesting in milliseconds since it started;
 * what time it was is not the question.
 */
const zero = computed(() => (moments.value.length ? moments.value[0].at : 0));
const offsetOf = (moment) => `+${((moment.at - zero.value) * 1000).toFixed(0)}ms`;

async function load() {
  loading.value = true;
  error.value = null;
  try {
    timeline.value = await api.requestTimeline(props.name, service.value);
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  try {
    targets.value = asList(await api.dbTargets());
    if (usable.value.length) service.value = usable.value[0].service;
  } catch {
    // A workspace with no database is a timeline of dumps, which is still one.
  }
  await load();
});

watch(() => props.name, load);
watch(service, load);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-timeline-clock-outline</v-icon>{{ t('timeline.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('timeline.explain') }}</p>

    <div class="d-flex align-center ga-3 flex-wrap mb-3">
      <v-select
        v-if="usable.length"
        v-model="service"
        :items="usable.map((target) => ({ value: target.service, title: target.service }))"
        :label="t('timeline.database')"
        density="compact"
        hide-details
        prepend-inner-icon="mdi-database"
        style="max-width: 220px"
      />
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-refresh"
        :loading="loading"
        @click="load"
      >
        {{ t('app.refresh') }}
      </v-btn>
    </div>

    <!-- The database half being off is a different state from the database
         having been asked nothing, and only one of them has an answer. -->
    <v-alert
      v-if="usable.length && !timeline?.queriesRecording"
      type="info"
      variant="tonal"
      class="mb-3"
    >
      <div class="text-caption">{{ t('timeline.notRecording') }}</div>
    </v-alert>

    <div v-if="!moments.length" class="text-caption text-medium-emphasis">
      {{ t('timeline.empty') }}
    </div>

    <template v-else>
      <!-- The requests the dumps named. Queries are absent from this list on
           purpose — see the script comment. -->
      <div v-if="requests.length" class="text-caption text-medium-emphasis mb-2">
        {{ t('timeline.requests') }}
        <v-chip v-for="request in requests" :key="request" size="x-small" class="ml-1">
          {{ request }}
        </v-chip>
      </div>

      <div class="axis">
        <div v-for="(moment, i) in moments" :key="i" class="moment" :class="moment.source">
          <span class="moment-at">{{ offsetOf(moment) }}</span>
          <v-icon size="14" class="moment-icon">
            {{
              moment.source === 'dump'
                ? 'mdi-bug-outline'
                : moment.source === 'mail'
                  ? 'mdi-email-outline'
                  : 'mdi-database-search'
            }}
          </v-icon>
          <div class="moment-body">
            <code class="moment-summary">{{ moment.summary }}</code>
            <span v-if="moment.request" class="moment-request">{{ moment.request }}</span>
          </div>
        </div>
      </div>
    </template>
  </v-card>
</template>

<style scoped>
/* Bounded and scrolled: `PageLayout` is a fixed-height flex column, so an
   unbounded child is compressed rather than scrolled — the bug
   `page-scroll.spec.js` exists for. */
.axis {
  max-height: 420px;
  overflow-y: auto;
  border-left: 2px solid rgb(var(--v-border-color), var(--v-border-opacity));
  padding-left: 10px;
}

.moment {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 4px 0;
  font-size: 0.75rem;
}

.moment-at {
  flex: 0 0 62px;
  text-align: right;
  color: rgb(var(--v-theme-on-surface-variant));
  opacity: 0.7;
  font-variant-numeric: tabular-nums;
}

.moment-icon {
  flex: 0 0 auto;
}

.moment.query .moment-icon {
  color: rgb(var(--v-theme-info));
}

.moment.dump .moment-icon {
  color: rgb(var(--v-theme-warning));
}

.moment.mail .moment-icon {
  color: rgb(var(--v-theme-success));
}

.moment-body {
  min-width: 0;
}

.moment-summary {
  word-break: break-all;
}

.moment-request {
  display: block;
  opacity: 0.6;
}
</style>
