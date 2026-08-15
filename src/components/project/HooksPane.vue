<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * What this project runs when it starts, stops or is rebuilt (B-3).
 *
 * ## The screen exists because the commands come from a repository
 *
 * A hook is written by whoever wrote the repository, and pressing Start runs
 * it. The steps that run inside the project's own container are not the
 * problem — that container already runs the repository's code. The ones that
 * run on *this machine* are, and they run only after somebody has read them
 * here and approved them.
 *
 * So the commands are printed in full, one per row, with where each one runs
 * beside it. A summary — "3 hooks" — would be a screen that makes approving
 * easier than reading, which is the opposite of what it is for.
 *
 * ## Approval carries the digest back
 *
 * The button sends the digest the plan arrived with, not just the project name.
 * If the manifest changed between this being drawn and the button being
 * pressed, the backend refuses — so the approval is a receipt for the list on
 * screen rather than a vote of confidence in the project.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();

const plans = ref([]);
const error = ref(null);
const loading = ref(false);
const saving = ref(false);

/** Only events this project actually declares — an empty one is not a row. */
const declared = computed(() => asList(plans.value).filter((plan) => plan.steps?.length));

const anyHost = computed(() =>
  declared.value.some((plan) => plan.steps.some((step) => step.kind === 'host'))
);

/** The digest is the same across every plan; the first one that has it will do. */
const digest = computed(() => declared.value.find((plan) => plan.digest)?.digest ?? null);

const awaitingConsent = computed(() =>
  declared.value.some((plan) => plan.steps.some((step) => step.blocked === 'needs-consent'))
);

/** Policy blocks are not something the user can answer, so they are said once. */
const policyBlock = computed(() => {
  const blocked = declared.value
    .flatMap((plan) => plan.steps)
    .map((step) => step.blocked)
    .find((reason) => reason === 'policy-off' || reason === 'policy-host');
  return blocked ?? null;
});

async function load() {
  loading.value = true;
  error.value = null;
  try {
    plans.value = asList(await api.projectHooksPlan(props.name));
  } catch (e) {
    error.value = e;
  } finally {
    loading.value = false;
  }
}

async function decide(fn) {
  saving.value = true;
  error.value = null;
  try {
    plans.value = asList(await fn());
  } catch (e) {
    error.value = e;
  } finally {
    saving.value = false;
  }
}

const approve = () => decide(() => api.projectHooksApprove(props.name, digest.value));
const revoke = () => decide(() => api.projectHooksRevoke(props.name));

onMounted(load);
watch(() => props.name, load);
</script>

<template>
  <v-card v-if="declared.length || loading" variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-hook</v-icon>{{ t('hooks.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-3">{{ t('hooks.explain') }}</p>

    <ErrorAlert v-if="error" :error="error" class="mb-3" />

    <!-- An administrator's decision, which the person here cannot answer. Said
         once rather than on every row it applies to. -->
    <v-alert v-if="policyBlock" type="info" variant="tonal" density="compact" class="mb-3">
      <div class="text-caption">
        {{ policyBlock === 'policy-off' ? t('hooks.policyOff') : t('hooks.policyHost') }}
      </div>
    </v-alert>

    <div v-for="plan in declared" :key="plan.event" class="mb-4">
      <div class="text-caption text-medium-emphasis mb-1">{{ plan.event }}</div>
      <div v-for="(step, i) in plan.steps" :key="i" class="step">
        <v-chip size="x-small" :color="step.kind === 'host' ? 'warning' : ''" label class="mr-2">
          {{ step.kind === 'host' ? t('hooks.onThisMachine') : t('hooks.inContainer') }}
        </v-chip>
        <!-- Printed whole. A truncated command is one nobody can approve. -->
        <code class="step-command">{{ step.command }}</code>
        <v-icon v-if="step.blocked" size="14" class="ml-2 text-medium-emphasis">
          mdi-cancel
        </v-icon>
      </div>
    </div>

    <template v-if="anyHost && !policyBlock">
      <v-alert v-if="awaitingConsent" type="warning" variant="tonal" density="compact" class="mb-2">
        <div class="text-caption">{{ t('hooks.needsConsent') }}</div>
      </v-alert>
      <div v-else class="text-caption text-medium-emphasis mb-2">
        <v-icon size="14" class="mr-1">mdi-check</v-icon>{{ t('hooks.approved') }}
      </div>

      <v-btn
        v-if="awaitingConsent"
        size="small"
        color="warning"
        variant="flat"
        :loading="saving"
        :disabled="!digest"
        @click="approve"
      >
        {{ t('hooks.approve') }}
      </v-btn>
      <v-btn v-else size="small" variant="text" :loading="saving" @click="revoke">
        {{ t('hooks.revoke') }}
      </v-btn>
    </template>
  </v-card>
</template>

<style scoped>
.step {
  display: flex;
  align-items: center;
  padding: 3px 0;
  font-size: 0.75rem;
}

.step-command {
  min-width: 0;
  word-break: break-all;
}
</style>
