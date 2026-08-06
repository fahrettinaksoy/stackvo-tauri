<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api, asList } from '@/lib/ipc';
import { useAppStore } from '@/stores/app';
import ErrorAlert from '@/components/ErrorAlert.vue';
import HostsDialog from '@/components/HostsDialog.vue';

/**
 * The screen the app opens on when something it depends on is missing.
 *
 * It replaces a gate that asked only "where is the checkout?", which was one of
 * six answers the app needs and the only one it bothered to ask for. The rest —
 * a running daemon, a compose plugin new enough for profiles, the shared
 * network, a projects directory, a shell — were discovered later, one failed
 * button at a time, each with a message about itself rather than about what to
 * do.
 *
 * Every row states what is wrong, what the machine actually said, and what to
 * do about it; the ones the app can settle itself carry a button that does.
 *
 * ## Why it is a numbered sequence and not a checklist
 *
 * The rows come back in dependency order — the daemon cannot be probed for the
 * network, the network cannot be created without the daemon — but as a flat
 * list of red crosses that order is invisible, and the screen reads as five
 * separate problems rather than one path with a first step. Numbering them and
 * naming the first actionable one turns "what is wrong" into "what to do now",
 * which is the only question a person on this screen has.
 *
 * App.vue hides the bar and both rails while this is up, so the numbers are the
 * only thing on the window.
 */
const { t } = useI18n();
const app = useAppStore();

const DOCS = 'https://stackvo.github.io/stackvo';

/**
 * Where to send someone the app cannot help directly.
 *
 * Docker's own pages, not StackVo's: a compose plugin too old for profiles is
 * not a StackVo problem, and the page that fixes it belongs to the people who
 * ship it. The engine gets one for the case its button cannot cover — a machine
 * with no Docker at all, where `open -a Docker` fails and "could not start
 * Docker" answers a question nobody asked.
 */
const HELP = {
  engine: 'https://docs.docker.com/desktop/',
  compose: 'https://docs.docker.com/compose/install/',
};

const busy = ref(null);
const rechecking = ref(false);

const ICONS = {
  workspace: 'mdi-folder-multiple-outline',
  engine: 'mdi-docker',
  compose: 'mdi-layers-outline',
  network: 'mdi-lan',
  hosts: 'mdi-web',
};

const STATE = {
  ok: { icon: 'mdi-check-circle', color: 'success' },
  warn: { icon: 'mdi-alert', color: 'warning' },
  fail: { icon: 'mdi-close-circle', color: 'error' },
  unknown: { icon: 'mdi-help-circle-outline', color: 'grey' },
};

const requirements = computed(() => app.preflight?.requirements ?? []);
const os = computed(() => app.preflight?.os ?? 'linux');
const blocking = computed(() => requirements.value.filter((r) => r.state === 'fail'));

const settled = computed(() => requirements.value.filter((r) => r.state === 'ok').length);
const total = computed(() => requirements.value.length);
const percent = computed(() => (total.value ? (settled.value / total.value) * 100 : 0));

/**
 * The one step to take now: the first that failed, or — when nothing has failed
 * and the rest simply could not be reached — the first of those, which is the
 * state a re-check exists for.
 */
const current = computed(
  () => blocking.value[0] ?? requirements.value.find((r) => r.state === 'unknown') ?? null
);

/**
 * A requirement that cannot be tested is not a requirement that failed — the
 * network cannot be looked for while the daemon is down. Say so rather than
 * offering a fix for it.
 */
const isBlocked = (requirement) => requirement.state === 'unknown';

/**
 * Warnings are not failures, but they are not nothing either.
 *
 * `ready` ignores them, so the app opens with a warning outstanding — and while
 * this screen is up they were rendered as a bare title with no explanation and
 * no button, which is how mkcert has been listed since it was added. A row that
 * says a thing is wrong and offers nothing to do about it is worse than absent.
 */
const isActionable = (r) => (r.state === 'fail' || r.state === 'warn') && r.fixable;
const hasHint = (r) => r.state === 'fail' || r.state === 'warn';

/** Domains with no line in the hosts file, from the `hosts` requirement. */
const hostsOpen = ref(false);
const missingDomains = ref([]);

async function fix(id) {
  // Rewriting a system file goes through the same review dialog as every other
  // hosts change in the app: the diff is shown, and only then does the OS ask
  // for a password. `preflight_fix('hosts')` would skip straight to the prompt.
  //
  // `hostsMissingCore`, not `hostsMissing`: this row blocks on two names, so
  // its button writes two names. It used to write every missing entry, which
  // meant a machine that needed `stackvo.loc` and `traefik.stackvo.loc` got
  // those plus the admin UI of every enabled service — four lines from a
  // prompt that had been opened for two.
  if (id === 'hosts') {
    missingDomains.value = asList(await api.hostsMissingCore().catch(() => []));
    hostsOpen.value = missingDomains.value.length > 0;
    return;
  }

  busy.value = id;
  try {
    await app.fixRequirement(id);
  } finally {
    busy.value = null;
  }
}

async function recheck() {
  rechecking.value = true;
  try {
    await app.checkRequirements();
  } finally {
    rechecking.value = false;
  }
}
</script>

<template>
  <div class="gate">
    <div class="gate-inner">
      <!-- The wordmark lives in the app bar, which is not on screen here. A
           window with no chrome and no logo does not say whose it is. -->
      <div class="brand">
        <span class="font-weight-bold">Stack</span><span class="font-weight-light">Vo</span>
      </div>

      <h1 class="text-h5 font-weight-bold text-center">{{ t('preflight.title') }}</h1>
      <p class="text-body-2 text-medium-emphasis text-center mt-2">
        {{ t('preflight.subtitle', { count: blocking.length }) }}
      </p>
      <p class="text-body-2 text-medium-emphasis text-center mt-1 mb-6">
        {{ t('preflight.lead') }}
      </p>

      <ErrorAlert :error="app.error" type="error" class="mb-4" />

      <v-card>
        <div class="px-4 pt-4 pb-3">
          <div class="text-caption text-medium-emphasis mb-2">
            {{ t('preflight.progress', { done: settled, total }) }}
          </div>
          <v-progress-linear
            :model-value="percent"
            height="6"
            rounded
            color="primary"
            :aria-label="t('preflight.progress', { done: settled, total })"
          />
        </div>

        <v-divider />

        <div
          v-for="(r, i) in requirements"
          :key="r.id"
          class="step"
          :class="{ 'is-current': r.id === current?.id, 'is-done': r.state === 'ok' }"
        >
          <!-- The number is the sequence; a check replaces it once the step is
               behind us, so progress is legible without reading any text. -->
          <div class="badge" :class="`badge--${r.state}`">
            <v-icon v-if="r.state === 'ok'" size="16" :color="STATE[r.state].color">
              {{ STATE.ok.icon }}
            </v-icon>
            <span v-else>{{ i + 1 }}</span>
          </div>

          <div class="min-w-0">
            <div class="d-flex align-center ga-2 flex-wrap">
              <v-icon size="16" class="text-medium-emphasis">{{ ICONS[r.id] }}</v-icon>
              <span class="text-body-2 font-weight-medium">{{ t(`preflight.${r.id}`) }}</span>
              <v-chip
                v-if="r.id === current?.id"
                size="x-small"
                color="primary"
                variant="flat"
                label
              >
                {{ t('preflight.nextStep') }}
              </v-chip>
            </div>

            <!-- The machine's own words: a version, a path, the daemon's error. -->
            <div v-if="r.detail" class="text-caption text-medium-emphasis detail">
              {{ r.detail }}
            </div>

            <!-- Instructions only where they are needed, and only the ones that
                 apply to this operating system. -->
            <div v-if="hasHint(r)" class="text-caption mt-1">
              {{ t(`preflight.${r.id}Hint.${os}`) }}
            </div>
            <div v-else-if="isBlocked(r)" class="text-caption text-medium-emphasis mt-1">
              {{ t('preflight.blocked') }}
            </div>

            <!-- A failure with no button is not the same as one with a button
                 nobody pressed; without this the row looks unfinished rather
                 than delegated. -->
            <div v-if="hasHint(r) && !r.fixable" class="text-caption text-warning mt-1">
              <v-icon size="14" class="mr-1">mdi-hand-back-right-outline</v-icon>
              {{ t('preflight.manual') }}
            </div>

            <!-- "Install it yourself" is only an instruction if it comes with
                 the page that says how. -->
            <v-btn
              v-if="hasHint(r) && HELP[r.id]"
              size="x-small"
              variant="text"
              color="primary"
              class="px-1 mt-1 ml-n1"
              append-icon="mdi-open-in-new"
              @click="openUrl(HELP[r.id])"
            >
              {{ t('preflight.help') }}
            </v-btn>
          </div>

          <v-spacer />

          <v-btn
            v-if="isActionable(r)"
            size="small"
            :color="r.state === 'warn' ? 'warning' : 'primary'"
            :variant="r.id === current?.id ? 'flat' : 'tonal'"
            :loading="busy === r.id"
            @click="fix(r.id)"
          >
            {{ t(`preflight.${r.id}Action`) }}
          </v-btn>
        </div>
      </v-card>

      <!-- The same action as the current row's button, at the size and position
           a person looks for one. A step nobody can automate sends them to the
           documentation instead of leaving the space empty. -->
      <template v-if="current">
        <v-btn
          v-if="current.fixable"
          block
          size="large"
          color="primary"
          variant="flat"
          class="mt-5"
          :prepend-icon="ICONS[current.id]"
          :loading="busy === current.id"
          @click="fix(current.id)"
        >
          {{ t(`preflight.${current.id}Action`) }}
        </v-btn>
        <v-btn
          v-else
          block
          size="large"
          variant="tonal"
          class="mt-5"
          prepend-icon="mdi-open-in-new"
          @click="openUrl(HELP[current.id] ?? DOCS)"
        >
          {{ HELP[current.id] ? t('preflight.help') : t('app.documentation') }}
        </v-btn>
      </template>

      <div class="d-flex justify-center ga-2 mt-3">
        <v-btn
          variant="text"
          size="small"
          prepend-icon="mdi-book-open-variant"
          @click="openUrl(DOCS)"
        >
          {{ t('app.documentation') }}
        </v-btn>
        <v-btn
          variant="text"
          size="small"
          prepend-icon="mdi-refresh"
          :loading="rechecking"
          @click="recheck"
        >
          {{ t('preflight.recheck') }}
        </v-btn>
      </div>

      <!-- The same review-then-elevate dialog the dashboard and the project
           pages use, so a hosts write looks identical wherever it is started. -->
      <HostsDialog v-model="hostsOpen" :add="missingDomains" @applied="recheck" />
    </div>
  </div>
</template>

<style scoped>
/*
 * Centred with `margin: auto` rather than `align-items: center`, which clips
 * the top of a flex child taller than its container instead of letting it
 * scroll — and this card grows with the number of failing steps.
 */
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
  max-width: 620px;
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

.step.is-current {
  background: rgba(var(--v-theme-primary), 0.06);
  box-shadow: inset 3px 0 0 rgb(var(--v-theme-primary));
}

.step.is-done {
  opacity: 0.65;
}

.badge {
  flex: 0 0 auto;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.75rem;
  font-weight: 600;
  border: thin solid rgba(var(--v-border-color), var(--v-border-opacity));
  color: rgb(var(--v-theme-on-surface));
}

.badge--fail {
  background: rgb(var(--v-theme-error));
  border-color: rgb(var(--v-theme-error));
  color: rgb(var(--v-theme-on-error));
}

.badge--warn {
  background: rgb(var(--v-theme-warning));
  border-color: rgb(var(--v-theme-warning));
  color: rgb(var(--v-theme-on-warning));
}

.badge--unknown {
  opacity: 0.5;
}

.detail {
  word-break: break-all;
}

.min-w-0 {
  min-width: 0;
}
</style>
