<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The fourth of the pattern, and the only one that is not skippable.
 *
 * ADR 0016. StackVo used to render its services from `.env` and a set of
 * templates compiled into the binary; it renders them from `instances.json` and
 * a package tree now, and as of this version the first branch is gone. A
 * workspace that still keeps its services in `.env` therefore cannot build a
 * stack at all — not a degraded one, none — so this screen is a wall where
 * `CatalogueGate` is a door.
 *
 * ## Why not a banner
 *
 * Because a banner is for something that can be ignored. The three answers §5
 * weighed were a forced migration, two paths for one release, and a silent one;
 * two paths is what was already happening and is what left the catalogue with
 * two lists that disagree, and a silent migration changes somebody's service
 * definitions without asking — which this codebase refuses elsewhere for
 * smaller things than this (`env_reveal` makes *reading* a password an act).
 *
 * ## What is offered, and what happens if it is declined
 *
 * The plan is shown before anything is written: which services, which versions,
 * which ports and volumes they will keep. `.env` is copied to
 * `.env.pre-market.bak` first and its service lines are commented out, so the
 * whole thing is reversible and the Market page keeps the panel that reverses
 * it.
 *
 * Declining leaves the app open with no services — and that is deliberately not
 * nothing: without them StackVo is still a reverse proxy, a certificate
 * authority and a project runner, which is exactly the argument `CatalogueGate`
 * makes about a machine with no catalogue. What declining does not do is let
 * the old stack come back, because the code that built it no longer exists.
 *
 * ## Blockers are shown, not swallowed
 *
 * A plan can be inapplicable — a service enabled in `.env` that the catalogue
 * has no package for, a port that something else has taken since. Those are
 * listed with names, because the repair is the user's and naming it is the
 * whole of the help this screen can give.
 */
const { t } = useI18n();
const emit = defineEmits(['done', 'skip']);

const preview = ref(null);
const busy = ref(false);
const error = ref(null);

// `HandoverPreview` is flat — no `plan` object under it. Read from the
// contract rather than guessed at, which is the lesson the browser suite taught
// three times in one afternoon.
const instances = computed(() => preview.value?.instances ?? []);
const blockers = computed(() => preview.value?.blockers ?? []);
const notes = computed(() => preview.value?.notes ?? []);
const missing = computed(() => preview.value?.missing ?? []);
const canApply = computed(
  () => instances.value.length > 0 && blockers.value.length === 0 && missing.value.length === 0
);

async function load() {
  error.value = null;
  try {
    preview.value = await api.handoverPreview();
  } catch (e) {
    error.value = e;
  }
}

async function apply() {
  busy.value = true;
  error.value = null;
  try {
    await api.handoverApply();
    emit('done');
  } catch (e) {
    error.value = e;
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <div class="gate">
    <div class="gate-inner">
      <h1 class="text-h4 font-weight-bold mb-1">
        <span class="font-weight-bold">Stack</span><span class="font-weight-light">Vo</span>
      </h1>
      <h2 class="text-h6 font-weight-bold mb-2">{{ t('migration.title') }}</h2>
      <p class="text-body-2 text-medium-emphasis mb-1">{{ t('migration.lead') }}</p>
      <p class="text-caption text-medium-emphasis mb-4">{{ t('migration.reversible') }}</p>

      <ErrorAlert v-if="error" :error="error" class="mb-4" />

      <v-card variant="outlined" class="mb-4">
        <v-card-text>
          <!-- The plan, before anything is written. A migration that showed a
               spinner and then a result would be asking for trust it has not
               earned yet. -->
          <div v-if="!preview" class="d-flex align-center ga-3">
            <v-progress-circular indeterminate size="18" width="2" color="primary" />
            <span class="text-caption text-medium-emphasis">{{ t('migration.reading') }}</span>
          </div>

          <template v-else>
            <div class="text-caption text-medium-emphasis mb-2">
              {{ t('migration.willKeep', { count: instances.length }) }}
            </div>

            <v-list density="compact" class="py-0">
              <v-list-item
                v-for="instance in instances"
                :key="instance.id"
                :title="`${instance.service} ${instance.version}`"
                :subtitle="instance.id"
                prepend-icon="mdi-package-variant-closed"
              />
            </v-list>

            <!-- Named, because the repair is the user's. -->
            <v-alert v-if="blockers.length" type="error" variant="tonal" class="mt-3">
              <div class="text-caption font-weight-medium mb-1">
                {{ t('migration.blocked') }}
              </div>
              <div v-for="(b, i) in blockers" :key="i" class="text-caption">
                <strong>{{ b.subject }}</strong> — {{ b.detail }}
              </div>
            </v-alert>

            <v-alert v-if="notes.length" type="info" variant="tonal" class="mt-3">
              <div v-for="(n, i) in notes" :key="i" class="text-caption">
                <strong>{{ n.subject }}</strong> — {{ n.detail }}
              </div>
            </v-alert>

            <!-- Not a blocker and not a note: the handover needs a package
                 this machine does not have, and the answer is to install it
                 rather than to repair anything. -->
            <v-alert v-if="missing.length" type="warning" variant="tonal" class="mt-3">
              <div class="text-caption font-weight-medium mb-1">{{ t('migration.missing') }}</div>
              <div v-for="m in missing" :key="`${m.service}@${m.version}`" class="text-caption">
                {{ m.service }} {{ m.version }}
                <span v-if="!m.installable">— {{ t('migration.notInCatalogue') }}</span>
              </div>
            </v-alert>

            <div
              v-if="!instances.length && !blockers.length && !missing.length"
              class="text-caption mt-2"
            >
              {{ t('migration.nothing') }}
            </div>
          </template>
        </v-card-text>
      </v-card>

      <div class="d-flex ga-2 flex-wrap">
        <v-btn
          color="primary"
          variant="flat"
          prepend-icon="mdi-swap-horizontal"
          :loading="busy"
          :disabled="!canApply"
          @click="apply"
        >
          {{ t('migration.apply') }}
        </v-btn>
        <!-- "Not now" and not "skip": the app opens without services and the
             old stack does not come back, so the word has to be about timing
             rather than about opting out. -->
        <v-btn variant="text" :disabled="busy" @click="emit('skip')">
          {{ t('migration.later') }}
        </v-btn>
      </div>

      <p class="text-caption text-medium-emphasis mt-4">{{ t('migration.laterHint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.gate {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  padding: 24px;
}

.gate-inner {
  width: 100%;
  max-width: 560px;
  text-align: left;
}
</style>
