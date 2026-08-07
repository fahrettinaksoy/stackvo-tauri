import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * Stack presets — which services are on, and at which versions.
 *
 * That is the part of a StackVo configuration a teammate does not get from a
 * clone: `stackvo.json` is already in the repository, `.env` is not, because
 * `.env` is also where every password lives.
 *
 * Import is plan-then-apply, like the hosts file and the certificate — you see
 * the diff before anything is written over your own stack.
 *
 * ## The load that never ran
 *
 * Lifted out of `Settings.vue`, where the export half **never populated**. The
 * view loaded it from a `watch` on the active tab:
 *
 * ```js
 * if (value === 'sharing' && !stackPreset.value) loadStackPreset();
 * ```
 *
 * There is no `sharing` section. The folder, the compose verbs and the preset
 * were merged into one `workspace` pane and the key was left behind, so the
 * only surviving path to `loadStackPreset` was the one *after* an import
 * succeeded. Opening the pane showed an empty JSON box and "0 services
 * enabled", and nothing about the code looked wrong — a `watch` comparing a
 * string to a string that no longer exists is not a mistake any tool reports.
 *
 * The pane loads on mount now, which is the same moment: each section is behind
 * a `v-if`, so mounting it *is* opening it, and there is no key to keep in step
 * with anything.
 */
export function useStackPreset() {
  const name = ref('');
  const preset = ref(null);
  const plan = ref(null);
  const path = ref('');
  const busy = ref(false);
  const applied = ref(false);
  const error = ref(null);

  const json = computed(() => (preset.value ? JSON.stringify(preset.value, null, 2) : ''));

  /** How many services the current stack has on, for the summary line. */
  const enabledCount = computed(
    () => Object.values(preset.value?.services ?? {}).filter((s) => s.enabled).length
  );

  async function load() {
    try {
      preset.value = await api.presetExport();
      error.value = null;
    } catch (e) {
      error.value = e;
    }
  }

  /** Write the current stack out. The path comes from the system save dialog. */
  async function exportTo(save) {
    const suggested = name.value.trim() || 'stack';
    const target = await save(`${suggested}.stackvo-preset.json`);
    if (!target) return false;

    busy.value = true;
    error.value = null;
    try {
      await api.presetSave(target, name.value.trim() || null);
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = false;
    }
  }

  /** Read a file and show what applying it would do. Nothing is written yet. */
  async function planFrom(open) {
    const chosen = await open();
    if (!chosen) return;

    busy.value = true;
    error.value = null;
    applied.value = false;
    try {
      plan.value = await api.presetPlan(chosen);
      path.value = chosen;
    } catch (e) {
      // A file that is not a preset is an error, not an empty plan — clear the
      // pane so a previous review cannot be mistaken for this file's.
      plan.value = null;
      path.value = '';
      error.value = e;
    } finally {
      busy.value = false;
    }
  }

  async function apply() {
    busy.value = true;
    error.value = null;
    try {
      plan.value = await api.presetApply(path.value);
      applied.value = true;
      // The stack this pane describes is the stack that just changed.
      await load();
    } catch (e) {
      error.value = e;
    } finally {
      busy.value = false;
    }
  }

  function clearPlan() {
    plan.value = null;
    path.value = '';
    applied.value = false;
  }

  return {
    name,
    preset,
    plan,
    path,
    busy,
    applied,
    error,
    json,
    enabledCount,
    load,
    exportTo,
    planFrom,
    apply,
    clearPlan,
  };
}

/**
 * Does what is on disk still match what the generator would write?
 *
 * A workspace can drift: someone edits a generated file, or a template override
 * lands and nothing regenerates. The answer is only useful on request, so it is
 * a button rather than something read on mount.
 */
export function useGeneratorCheck() {
  const report = ref(null);
  const verifying = ref(false);
  const error = ref(null);

  async function verify() {
    verifying.value = true;
    error.value = null;
    try {
      report.value = await api.generatorVerify();
    } catch (e) {
      error.value = e;
    } finally {
      verifying.value = false;
    }
  }

  /**
   * Regenerate, then re-check.
   *
   * One button, because "the files are stale" and "write them again" are the
   * same thought — offering the check without the fix would leave the user to
   * find the regenerate somewhere else on the same screen.
   */
  async function regenerateAndVerify() {
    verifying.value = true;
    error.value = null;
    try {
      await api.generateRun('all');
      report.value = await api.generatorVerify();
    } catch (e) {
      error.value = e;
    } finally {
      verifying.value = false;
    }
  }

  return { report, verifying, error, verify, regenerateAndVerify };
}
