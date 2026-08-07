import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The four php.ini directives a project can override.
 *
 * Deliberately not a manifest form: these are not manifest keys and cannot
 * become them (the schema is `additionalProperties: false`), and they land in a
 * real ini file mounted into the container.
 *
 * Lifted out of `ProjectDetail.vue` with the PHP pane under §14.16.
 */
export const PHP_INI_FIELDS = [
  'memory_limit',
  'upload_max_filesize',
  'post_max_size',
  'max_execution_time',
];

export function usePhpIni(name) {
  const status = ref(null);
  const busy = ref(false);
  const error = ref(null);

  /**
   * Local edit state rather than a v-model onto `status.values`: an empty field
   * has to mean "remove this directive", and binding straight at the status
   * object would make every keystroke look like a pending removal.
   */
  const draft = ref({});

  function resetDraft() {
    const values = status.value?.values ?? {};
    draft.value = Object.fromEntries(PHP_INI_FIELDS.map((k) => [k, values[k] ?? '']));
  }

  const dirty = computed(() => {
    const values = status.value?.values ?? {};
    return PHP_INI_FIELDS.some((k) => (draft.value[k] ?? '') !== (values[k] ?? ''));
  });

  /** Every field cleared and nothing unmanaged left — the whole file goes. */
  const wouldRemoveFile = computed(
    () =>
      PHP_INI_FIELDS.every((k) => !(draft.value[k] ?? '').trim()) &&
      !Object.keys(status.value?.unmanaged ?? {}).length
  );

  async function load(runtime) {
    if (runtime !== 'php') {
      status.value = null;
      resetDraft();
      return null;
    }
    try {
      status.value = await api.phpIniStatus(name.value);
    } catch {
      status.value = null;
    }
    resetDraft();
    return status.value;
  }

  /**
   * Save only what changed.
   *
   * Sending the unchanged fields too would rewrite lines the user may have
   * commented next to, for no reason.
   */
  async function save() {
    busy.value = true;
    error.value = null;
    try {
      const values = status.value?.values ?? {};
      const patch = {};
      for (const key of PHP_INI_FIELDS) {
        const next = (draft.value[key] ?? '').trim();
        const now = values[key] ?? '';
        if (next === now) continue;
        // An empty field is a removal, not an empty value: this file is an
        // override layer, and `memory_limit =` with nothing after it is a
        // directive PHP reads as zero.
        patch[key] = next === '' ? null : next;
      }
      status.value = await api.phpIniSet(name.value, patch);
      resetDraft();
      return patch;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      busy.value = false;
    }
  }

  return { status, draft, busy, error, dirty, wouldRemoveFile, load, save, resetDraft };
}
