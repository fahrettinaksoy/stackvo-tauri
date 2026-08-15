<script setup>
import { onMounted, ref, toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRelease } from '@/composables/useRelease';
import { api } from '@/lib/ipc';
import ErrorAlert from '@/components/ErrorAlert.vue';

/**
 * The production image this project can be shipped as.
 *
 * Second pane out of `ProjectDetail.vue` under §14.16, and one of the two that
 * came out as a single contiguous block — most of this view's sections are
 * split across three places in the file.
 */
const props = defineProps({
  name: { type: String, required: true },
});

const { t } = useI18n();

const { plan, tag, result, busy, error, load, build, save, loadBundle } = useRelease(
  toRef(props, 'name')
);

/** What `docker load` said it installed, so the answer is Docker's, not ours. */
const loaded = ref(null);

// Loaded on mount, and again when the route moves to another project. The
// view used to call this as part of one big `load()`; owning it here means the
// pane cannot be shown against the previous project's plan.
onMounted(load);
watch(() => props.name, load);

/** The dialog is opened here, not in the composable — see `useRelease.save`. */
async function saveTo() {
  const { save: choose } = await import('@tauri-apps/plugin-dialog');
  await save((defaultPath) => choose({ defaultPath }));
}

/**
 * The other direction, and the reason it is on this pane rather than a page of
 * its own: the bundle a machine receives is the artefact this pane produced.
 *
 * Offered whether or not this project has a plan — the receiving machine may
 * have no checkout at all, and refusing to open a tarball because the *local*
 * project is unbuilt would be answering the wrong question.
 */
async function loadFrom() {
  const { open: choose } = await import('@tauri-apps/plugin-dialog');
  loaded.value = await loadBundle(() =>
    choose({ multiple: false, filters: [{ name: 'Docker image', extensions: ['tar'] }] })
  );
}

/**
 * Pushing it, and something to run it with (H-1).
 *
 * The push plan is fetched after a successful build rather than offered
 * alongside the save button, because the two refusals it carries — an
 * unverified image, and a tag that names no registry — are the whole reason
 * pushing is safe, and both are sentences somebody has to read before there is
 * a button worth pressing.
 */
const pushPlan = ref(null);
const pushing = ref(false);
const recipe = ref('');

async function planPush() {
  pushPlan.value = null;
  try {
    pushPlan.value = await api.releasePushPlan(props.name, tag.value || null);
  } catch (e) {
    error.value = e;
  }
}

async function push() {
  pushing.value = true;
  try {
    pushPlan.value = await api.releasePush(props.name, tag.value || null);
  } catch (e) {
    error.value = e;
  } finally {
    pushing.value = false;
  }
}

async function showRecipe() {
  try {
    recipe.value = await api.releaseRecipe(props.name, tag.value || null);
  } catch (e) {
    error.value = e;
  }
}
</script>

<template>
  <ErrorAlert v-if="error" :error="error" class="mb-4" />

  <v-card variant="flat" class="pane">
    <div class="section-head mb-1">
      <v-icon size="18" class="mr-2">mdi-package-variant-closed</v-icon>
      {{ t('release.title') }}
    </div>
    <p class="text-caption text-medium-emphasis mb-4">{{ t('release.explain') }}</p>

    <template v-if="plan">
      <div class="d-flex ga-2 align-start">
        <v-text-field
          v-model="tag"
          :label="t('release.tag')"
          :hint="t('release.tagHint', { base: plan.baseImage })"
          persistent-hint
          density="comfortable"
          variant="outlined"
          :disabled="!!busy"
        />
        <v-btn
          color="primary"
          variant="flat"
          :loading="busy === 'build'"
          :disabled="!tag.trim() || !!busy"
          @click="build"
        >
          {{ t('release.build') }}
        </v-btn>
      </div>

      <!-- Everything the result will be true of, before the build rather
           than after. None of these is a decision to make silently. -->
      <v-alert type="info" variant="tonal" class="mt-4">
        <div v-for="line in plan.warnings" :key="line" class="text-caption">• {{ line }}</div>
      </v-alert>

      <div class="section-head mt-5 mb-2">
        <v-icon size="18" class="mr-2">mdi-eye-off-outline</v-icon>
        {{ t('release.excluded') }}
      </div>
      <v-table density="compact">
        <tbody>
          <tr v-for="[pattern, reason] in plan.excluded" :key="pattern">
            <td class="mono">{{ pattern }}</td>
            <td class="text-medium-emphasis text-caption">{{ reason }}</td>
          </tr>
        </tbody>
      </v-table>

      <v-expansion-panels v-if="plan.dockerfile" variant="accordion" class="mt-4">
        <v-expansion-panel :title="t('release.dockerfile')">
          <v-expansion-panel-text>
            <pre class="snippet">{{ plan.dockerfile }}</pre>
          </v-expansion-panel-text>
        </v-expansion-panel>
      </v-expansion-panels>

      <!-- Read out of the built image, not inferred from the Dockerfile:
           this guarantee is easy to state and easy to get wrong. -->
      <template v-if="result">
        <div class="section-head mt-5 mb-2">
          <v-icon size="18" class="mr-2">mdi-shield-check-outline</v-icon>
          {{ t('release.checked') }}
        </div>

        <v-alert :type="result.verification.clean ? 'success' : 'error'" variant="tonal">
          <div class="text-caption">
            {{
              result.verification.clean
                ? t('release.clean', { tag: result.plan.tag })
                : t('release.notClean')
            }}
          </div>
          <ul class="text-caption mt-2 pl-4">
            <li v-if="result.verification.envFiles.length">
              {{
                t('release.leaked', {
                  files: result.verification.envFiles.join(', '),
                })
              }}
            </li>
            <li v-else>{{ t('release.noEnv') }}</li>
            <li v-if="result.verification.xdebugActive === true">
              {{ t('release.xdebugOn') }}
            </li>
            <li v-else-if="result.verification.xdebugActive === false">
              {{ t('release.xdebugOff') }}
            </li>
            <li v-if="!result.verification.hasApp">{{ t('release.noApp') }}</li>
          </ul>
        </v-alert>

        <v-btn
          v-if="result.verification.clean"
          class="mt-3"
          variant="tonal"
          prepend-icon="mdi-download-outline"
          :loading="busy === 'save'"
          :disabled="!!busy"
          @click="saveTo"
        >
          {{ t('release.save') }}
        </v-btn>

        <!-- PUSH ------------------------------------------------------- -->
        <template v-if="result.verification.clean">
          <v-divider class="my-4" />
          <div class="text-caption text-medium-emphasis mb-2">{{ t('release.pushExplain') }}</div>

          <div class="d-flex ga-2 mb-2">
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-cloud-search-outline"
              :disabled="!!busy || pushing"
              @click="planPush"
            >
              {{ t('release.pushCheck') }}
            </v-btn>
            <v-btn
              size="small"
              color="primary"
              variant="flat"
              prepend-icon="mdi-cloud-upload-outline"
              :disabled="!pushPlan?.possible || pushing"
              :loading="pushing"
              @click="push"
            >
              {{ t('release.push') }}
            </v-btn>
            <v-spacer />
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-file-document-outline"
              @click="showRecipe"
            >
              {{ t('release.recipe') }}
            </v-btn>
          </div>

          <!-- The refusal, in full. It names which check failed, and that is
               the sentence that stops an .env reaching a registry. -->
          <v-alert
            v-if="pushPlan && !pushPlan.possible"
            type="error"
            variant="tonal"
            density="compact"
            class="mb-2"
          >
            <div class="text-caption">{{ pushPlan.refused }}</div>
          </v-alert>
          <v-alert
            v-else-if="pushPlan?.warnings?.length"
            type="warning"
            variant="tonal"
            density="compact"
            class="mb-2"
          >
            <div v-for="(note, i) in pushPlan.warnings" :key="i" class="text-caption">
              {{ note }}
            </div>
          </v-alert>

          <!-- Shown, not written. Where a deployment recipe belongs is the
               user's decision, so it is text they can copy. -->
          <pre v-if="recipe" class="snippet mt-2">{{ recipe }}</pre>
        </template>
      </template>
    </template>

    <!-- LOAD ------------------------------------------------------------- -->
    <!-- Outside the plan/build/verify chain above, deliberately: this is the
         step that runs on the machine that received the bundle, and that
         machine has nothing to plan. -->
    <v-divider class="my-4" />
    <div class="text-caption text-medium-emphasis mb-2">{{ t('release.loadExplain') }}</div>
    <v-btn
      variant="tonal"
      prepend-icon="mdi-upload-outline"
      :loading="busy === 'loadBundle'"
      :disabled="!!busy"
      @click="loadFrom"
    >
      {{ t('release.load') }}
    </v-btn>
    <v-alert v-if="loaded?.length" type="success" variant="tonal" density="compact" class="mt-3">
      {{ t('release.loaded') }}
      <ul class="mt-1">
        <li v-for="image in loaded" :key="image">
          <code>{{ image }}</code>
        </li>
      </ul>
    </v-alert>
  </v-card>
</template>
