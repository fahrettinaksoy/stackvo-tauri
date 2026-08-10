<script setup>
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { api } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The step between "everything it needs is here" and the app itself.
 *
 * The requirements gate answers *can* this run. It does not make it run, and
 * for one launch — the first — the difference is the whole experience: every
 * check went green and the app opened on a dashboard behind a proxy that was
 * not started, on a stack that had never been assembled. `stackvo.loc` did not
 * answer, and nothing on screen said it was supposed to be doing anything about
 * that.
 *
 * So the two things that have to happen once, happen here, in front of the
 * person waiting for them: the compose files get written, and the core stack
 * comes up.
 *
 * ## Once, and only on the way in
 *
 * Keyed on whether the app directory has ever been generated into, not on
 * whether the stack is up right now. Somebody who deliberately stopped
 * everything and reopened the app wants their dashboard, not to be walked
 * through a setup they have already done — and a screen that reappears every
 * time you stop the stack is a screen people learn to click past.
 */
const { t } = useI18n();
const app = useAppStore();

const emit = defineEmits(['done']);

// Order is not a preference. `up` is handed what `generate` writes, and the
// certificate comes last on purpose: issuing it while the proxy is already
// running is the case `certs::apply` reloads for, and a failure there leaves a
// started stack rather than nothing at all — HTTP works, HTTPS is untrusted,
// and the Certificates pane can finish the job.
const STEPS = ['generate', 'start', 'certificates', 'trust'];

const current = ref(0);
const failed = ref(null);
const error = ref(null);
/** Issued, but the machine was not persuaded to trust the issuer. */
const untrusted = ref(null);
/** A terminal is open and somebody is being asked for their password. */
const waitingForPassword = ref(false);

/**
 * Poll until the certificate is trusted, or until waiting stops being polite.
 *
 * There is no event for "the user typed their password in another window", so
 * the state is asked for. It resolves within a poll of the password landing —
 * it appeared not to, once, and that was the backend reading macOS's trust
 * dump wrongly rather than anything here.
 *
 * Bounded because the answer may be "they closed the terminal": sixty seconds
 * is long enough to find a password manager and short enough not to strand
 * somebody who has decided not to.
 */
async function waitForTrust() {
  for (let i = 0; i < 30; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    const status = await api.certStatus().catch(() => null);
    if (status?.caTrusted === true) return true;
  }
  return false;
}
/** Kept so a failure can be retried without another trip through preflight. */
const running = ref(false);

const progress = computed(() => (current.value / STEPS.length) * 100);

function state(index) {
  if (failed.value === index) return 'failed';
  if (index < current.value) return 'done';
  if (index === current.value && running.value) return 'running';
  return 'pending';
}

const ICONS = {
  done: 'mdi-check-circle',
  running: 'mdi-progress-clock',
  failed: 'mdi-close-circle',
  pending: 'mdi-circle-outline',
};
const COLORS = { done: 'success', running: 'primary', failed: 'error', pending: 'grey' };

async function run() {
  running.value = true;
  failed.value = null;
  error.value = null;

  try {
    // The compose files first: `up` is handed the files this writes, so the
    // order is not a preference.
    await api.generateRun('all');
    current.value = 1;

    // `minimal` is the profile Traefik is in — the proxy every domain resolves
    // through, including the two the gate insisted on. Not `all`: nobody asked
    // for twenty-five containers on first launch, and nothing is switched on by
    // default any more.
    await api.composeUp('minimal');
    current.value = 2;

    // Without this the whole thing looked finished and nothing answered.
    // `routes.yml` puts every router on `websecure` and points the TLS store at
    // `/certs/stackvo-wildcard.crt`; with no such file Traefik logs "failed to
    // find any PEM data" and builds no certificate store, so both names
    // resolved, reached the proxy and got a dropped connection.
    //
    // Installing the CA is idempotent — `certs::plan` asks for it only when the
    // trust store does not already have it — and it is what makes the browser
    // accept the result rather than warn about it.
    // Issue only. Trusting is the next step because it is a different job with
    // a different failure, and because it needs a password.
    await api.certApply(false);
    current.value = 3;

    // Trust, here, rather than by sending somebody to Settings afterwards.
    //
    // macOS grants the authorization for trust settings only interactively —
    // `sudo` waits for ever with no terminal, root through AppleScript is
    // refused outright, and the user-domain write exits 0 and changes nothing.
    // All three were measured. A terminal is the one place the password can be
    // asked for and answered, so the app opens one instead of explaining where
    // to go.
    if ((await api.certStatus().catch(() => null))?.caTrusted !== true) {
      waitingForPassword.value = true;
      await api.certTrustInTerminal();
      untrusted.value = (await waitForTrust()) ? null : 'not trusted';
      waitingForPassword.value = false;
    }
    current.value = 4;

    // Only now. The marker used to be a file the first step happened to leave
    // behind, which made a run that generated the compose files and then failed
    // to issue a certificate look finished for ever — the screen never offered
    // again, and the stack it left could not serve a single domain.
    await api.bootstrapComplete();

    await app.refreshWorkspace();
    emit('done');
  } catch (e) {
    error.value = e;
    failed.value = current.value;
  } finally {
    running.value = false;
  }
}

onMounted(run);
</script>

<template>
  <div class="gate">
    <div class="gate-inner">
      <div class="brand">
        <span class="font-weight-bold">Stack</span><span class="font-weight-light">Vo</span>
      </div>

      <h1 class="text-h5 font-weight-bold text-center">{{ t('bootstrap.title') }}</h1>
      <p class="text-body-2 text-medium-emphasis text-center mt-2 mb-6">
        {{ t('bootstrap.subtitle') }}
      </p>

      <ErrorAlert :error="error" type="error" class="mb-4" />

      <v-alert
        v-if="untrusted"
        type="warning"
        variant="tonal"
        density="comfortable"
        class="mb-4"
        :text="t('bootstrap.untrusted')"
      />

      <v-card>
        <div class="px-4 pt-4 pb-3">
          <!-- Named for the same reason `RequirementsGate` names its bar, and
               `StatCard` now does: Vuetify emits `role="progressbar"` with a
               value and no name, so a screen reader announces a number with
               nothing attached to it. This one matters most of the three — it
               is the first-run screen, and the user is waiting on it. -->
          <v-progress-linear
            :model-value="progress"
            :indeterminate="running"
            :aria-label="t('bootstrap.title')"
            height="6"
            rounded
            color="primary"
          />
        </div>

        <v-divider />

        <div v-for="(step, index) in STEPS" :key="step" class="step">
          <v-icon :color="COLORS[state(index)]" size="20" class="mt-1">
            {{ ICONS[state(index)] }}
          </v-icon>
          <div class="min-w-0">
            <div class="text-body-2 font-weight-medium">{{ t(`bootstrap.${step}`) }}</div>
            <div class="text-caption text-medium-emphasis">
              {{ t(`bootstrap.${step}Detail`) }}
            </div>
            <div
              v-if="step === 'trust' && waitingForPassword"
              class="text-caption text-warning mt-1"
            >
              {{ t('bootstrap.waitingForPassword') }}
            </div>
          </div>
        </div>
      </v-card>

      <template v-if="failed !== null">
        <v-btn
          block
          size="large"
          color="primary"
          variant="flat"
          class="mt-5"
          prepend-icon="mdi-refresh"
          :loading="running"
          @click="run"
        >
          {{ t('bootstrap.retry') }}
        </v-btn>
      </template>
    </div>
  </div>
</template>

<style scoped>
.gate {
  display: flex;
  min-height: 100vh;
  min-height: 100dvh;
  padding: 32px 24px;
  box-sizing: border-box;
}

.gate-inner {
  margin: auto;
  width: 100%;
  max-width: 560px;
}

.brand {
  font-size: 2rem;
  line-height: 1.2;
  text-align: center;
  letter-spacing: -0.5px;
  margin-bottom: 20px;
}

.step {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px 16px;
}

.step + .step {
  border-top: thin solid rgba(var(--v-border-color), calc(var(--v-border-opacity) / 2));
}

.min-w-0 {
  min-width: 0;
}
</style>
