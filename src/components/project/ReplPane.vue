<script setup>
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { api, asList } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * A snippet, the application it runs inside, and what came back.
 *
 * F-5, and §5.5 held it as a decision rather than a task: `quickcmd.rs` refused
 * an in-app REPL in writing — "a second, worse REPL next to the one they
 * already have configured" — and reversing a refusal is not something a commit
 * does quietly.
 *
 * ## Why this is not what was refused
 *
 * The refusal is about a *line* REPL in a pane, and it is right: no readline,
 * no history file, none of the colours somebody configured, and a terminal one
 * click away that does all of it better. `tinker` still opens the user's own
 * terminal from the command list, and this does not compete with it.
 *
 * A snippet is a different act. It is twenty lines you **edit** — write a
 * query, run it, change line three, run it again — which in a line REPL is
 * retyping. So the two sit next to each other here, above and below the
 * terminal pane, and the difference is visible rather than argued.
 *
 * ## Two tiers, on screen
 *
 * A booted runner has the application's models and config; a bare one is the
 * language on its own. Both are useful and they are not interchangeable, so
 * every row says which it is. A pane that showed them alike would let somebody
 * debug for ten minutes before finding out their models were never loaded.
 *
 * ## The editor is a textarea
 *
 * Deliberately, and it is the one place this pane could have grown a
 * dependency. CodeMirror is ~200 KB for syntax colouring on a text somebody
 * types for thirty seconds, and the bundle budget is a gate in this repository
 * (`tools/check-bundle.mjs`). What a workbench actually needs from an editor is
 * that the text survives being run, which a textarea does.
 */
const props = defineProps({
  name: { type: String, required: true },
  running: { type: Boolean, default: false },
});

const { t } = useI18n();

const runners = ref([]);
const runner = ref(null);
const code = ref('');
const result = ref(null);
const history = ref([]);
const busy = ref(false);
const error = ref(null);

const current = computed(() => runners.value.find((r) => r.id === runner.value) ?? null);

/**
 * Laravel's `--execute` does not echo the value of the last expression the way
 * the interactive REPL does — measured, and it is the one thing about this that
 * surprises somebody who knows `tinker`. Said for every booted runner rather
 * than only Laravel, because the same is true of `bin/rails runner` and
 * `manage.py shell -c`.
 */
const needsPrinting = computed(() => current.value?.booted === true);

const exitOk = computed(() => result.value?.exitCode === 0);

async function load() {
  error.value = null;
  try {
    runners.value = asList(await api.replRunners(props.name));
    if (!runners.value.some((r) => r.id === runner.value)) {
      runner.value = runners.value[0]?.id ?? null;
    }
    history.value = asList(await api.replHistory(props.name));
  } catch (e) {
    error.value = e;
    runners.value = [];
  }
}

async function run() {
  if (!runner.value || !code.value.trim() || busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    result.value = await api.replRun(props.name, runner.value, code.value);
    history.value = asList(await api.replHistory(props.name));
  } catch (e) {
    error.value = e;
    // The previous run's output is cleared with it: leaving it under a new
    // error would read as the result of the thing that just failed.
    result.value = null;
  } finally {
    busy.value = false;
  }
}

/** Put a remembered snippet back in the editor, with the runner it ran under. */
function recall(snippet) {
  code.value = snippet.code;
  if (runners.value.some((r) => r.id === snippet.runner)) runner.value = snippet.runner;
}

async function forget() {
  try {
    history.value = asList(await api.replHistoryClear(props.name));
  } catch (e) {
    error.value = e;
  }
}

/** Seconds since the epoch, as a local time somebody can compare to a log. */
function when(at) {
  return new Date(at * 1000).toLocaleString();
}

onMounted(load);
watch(() => props.name, load);
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-console-line</v-icon>{{ t('repl.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('repl.explain') }}</p>

    <!-- Nothing this can load. Said plainly rather than shown as a disabled
         editor: a static site has no application to boot, and that is not a
         fault to report. -->
    <v-alert v-if="!runners.length" type="info" variant="tonal">
      <div class="text-caption">{{ t('repl.noRunner') }}</div>
    </v-alert>

    <template v-else>
      <div class="d-flex align-center ga-3 flex-wrap mb-2">
        <v-select
          v-if="runners.length > 1"
          v-model="runner"
          :items="runners.map((r) => ({ value: r.id, title: r.display }))"
          :label="t('repl.runner')"
          density="compact"
          hide-details
          prepend-inner-icon="mdi-play-box-outline"
          style="max-width: 280px"
        />
        <code v-else class="repl-runner">{{ current?.display }}</code>

        <!-- The tier, beside the runner rather than in a tooltip: it is the
             difference between `User::count()` meaning something and meaning
             nothing. -->
        <v-chip
          v-if="current"
          size="x-small"
          :color="current.booted ? 'primary' : undefined"
          variant="tonal"
        >
          {{ current.booted ? t('repl.booted') : t('repl.bare') }}
        </v-chip>
        <span class="text-caption text-medium-emphasis">{{ current?.about }}</span>
      </div>

      <v-textarea
        v-model="code"
        :label="t('repl.snippet')"
        :placeholder="t('repl.placeholder')"
        :disabled="busy"
        class="mono-input"
        rows="6"
        auto-grow
        max-rows="18"
        density="compact"
        hide-details
        persistent-placeholder
        @keydown.enter.meta.prevent="run"
        @keydown.enter.ctrl.prevent="run"
      />

      <div class="d-flex align-center ga-3 flex-wrap mt-3">
        <v-btn
          color="primary"
          size="small"
          prepend-icon="mdi-play"
          :loading="busy"
          :disabled="!running || !code.trim()"
          @click="run"
        >
          {{ t('repl.run') }}
        </v-btn>
        <span class="text-caption text-medium-emphasis">{{ t('repl.shortcut') }}</span>
        <!-- Why the button is off, where the button is. -->
        <span v-if="!running" class="text-caption text-warning">{{ t('repl.needsRunning') }}</span>
      </div>

      <div v-if="needsPrinting" class="text-caption text-medium-emphasis mt-2">
        {{ t('repl.printYourself') }}
      </div>

      <!-- RESULT ---------------------------------------------------------- -->
      <template v-if="result">
        <div class="section-head mt-4 mb-1">{{ t('repl.output') }}</div>

        <div class="d-flex align-center ga-2 flex-wrap mb-2">
          <v-chip size="x-small" :color="exitOk ? 'success' : 'error'" variant="tonal">
            {{ exitOk ? t('repl.ok') : t('repl.exit', { code: result.exitCode ?? '—' }) }}
          </v-chip>
          <span class="text-caption text-medium-emphasis">{{ result.ms }} ms</span>
          <v-chip v-if="result.timedOut" size="x-small" color="warning" variant="tonal">
            {{ t('repl.timedOut') }}
          </v-chip>
          <v-chip v-if="result.truncated" size="x-small" variant="tonal">
            {{ t('repl.truncated') }}
          </v-chip>
        </div>

        <!-- The one thing the app cannot fix and must not hide: without the
             in-container limit, a snippet that never finishes is still going
             after this pane stopped waiting. -->
        <v-alert v-if="!result.limited" type="warning" variant="tonal" class="mb-2">
          <div class="text-caption">{{ t('repl.notLimited') }}</div>
        </v-alert>

        <pre v-if="result.stdout" class="snippet repl-out">{{ result.stdout }}</pre>
        <!-- Kept apart from stdout, and both are shown: a PHP fatal is written
             to stdout and a Node one to stderr, so a pane that showed only one
             of them would be blank for half the languages it offers. -->
        <pre v-if="result.stderr" class="snippet repl-out repl-err">{{ result.stderr }}</pre>
        <div v-if="!result.stdout && !result.stderr" class="text-caption text-medium-emphasis">
          {{ t('repl.noOutput') }}
        </div>
      </template>

      <!-- HISTORY --------------------------------------------------------- -->
      <template v-if="history.length">
        <div class="d-flex align-center justify-space-between mt-4 mb-1">
          <div class="section-head">{{ t('repl.history') }}</div>
          <v-btn size="x-small" variant="text" prepend-icon="mdi-broom" @click="forget">
            {{ t('repl.forget') }}
          </v-btn>
        </div>
        <div class="text-caption text-medium-emphasis mb-2">{{ t('repl.historyKeeps') }}</div>

        <div
          v-for="(snippet, i) in history"
          :key="`${snippet.at}-${i}`"
          class="repl-past"
          role="button"
          tabindex="0"
          @click="recall(snippet)"
          @keydown.enter="recall(snippet)"
          @keydown.space.prevent="recall(snippet)"
        >
          <div class="d-flex align-center ga-2">
            <v-chip size="x-small" variant="tonal">{{ snippet.runner }}</v-chip>
            <span class="text-caption text-medium-emphasis">{{ when(snippet.at) }}</span>
          </div>
          <code class="repl-past-code">{{ snippet.code }}</code>
        </div>
      </template>
    </template>
  </v-card>
</template>

<style scoped>
.repl-runner {
  font-size: 0.8rem;
}

/* Bounded and scrolled rather than let to run off the page: `PageLayout` is a
   fixed-height flex column, so an unbounded child is compressed instead —
   `page-scroll.spec.js` is the guard that class of bug earned. */
.repl-out {
  max-height: 320px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.repl-err {
  margin-top: 8px;
  color: rgb(var(--v-theme-error));
}

.repl-past {
  padding: 8px 0;
  border-bottom: 1px solid rgb(var(--v-border-color), var(--v-border-opacity));
  cursor: pointer;
}

.repl-past-code {
  display: block;
  margin-top: 2px;
  font-size: 0.75rem;
  /* One line, because the list is an index and not a second editor. */
  white-space: pre;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
