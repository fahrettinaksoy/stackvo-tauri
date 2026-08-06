import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * Which shipped templates this workspace has taken over.
 *
 * The app renders from the copies compiled into its binary and reads the
 * workspace first, so a file under `core/` is an override and nothing else —
 * installing writes none. That is what makes this list answerable, and the
 * question it answers is a real one: an edit made months ago is invisible until
 * the stack stops matching what the documentation says it does.
 *
 * Lifted out of `Settings.vue` with the pane it belongs to. Unlike
 * `useCertificates`, the state is created per call rather than module-scoped:
 * one component reads it, so sharing would buy nothing and would leak between
 * two panes if the settings view ever showed both.
 */
export function useTemplates() {
  const templates = ref([]);
  const busy = ref(false);
  const error = ref(null);

  /** The path currently being copied in or reverted, or null. */
  const working = ref(null);

  /** The path chosen in the picker, not yet taken over. */
  const chosen = ref(null);

  /** The path a confirmation dialog is open for. */
  const revertTarget = ref(null);

  const overridden = computed(() => templates.value.filter((f) => f.overridden));
  const shipped = computed(() => templates.value.filter((f) => !f.overridden));

  /**
   * Is this particular template the one being worked on?
   *
   * **The emptiness check is the point.** The binding used to be
   * `templateBusy === templateToOverride`, which reads correctly and is wrong
   * for the state the pane opens in: both are null, null equals null, and the
   * button sat there spinning before anyone had chosen a file — and again after
   * every successful override, which clears the selection back to null. Idle is
   * not a path, so it can never be the busy one.
   */
  const busyWith = (path) => !!path && working.value === path;

  async function load() {
    busy.value = true;
    error.value = null;
    try {
      templates.value = asList(await api.templatesList());
    } catch (e) {
      error.value = e;
    } finally {
      busy.value = false;
    }
  }

  /**
   * Copy the shipped file in, then open it in the user's own editor.
   *
   * Not a textarea in this pane: these are compose fragments and server
   * configs, and the tool for editing YAML is the one they already have open.
   */
  async function override() {
    const path = chosen.value;
    if (!path) return;

    working.value = path;
    error.value = null;
    try {
      const absolute = await api.templateOverride(path);
      await load();
      chosen.value = null;
      await api.openInEditor(absolute).catch(() => {});
    } catch (e) {
      error.value = e;
    } finally {
      working.value = null;
    }
  }

  function open(path, root) {
    if (root) api.openInEditor(`${root}/${path}`).catch(() => {});
  }

  /** Deletes the user's edit. Confirmed in the dialog, not here. */
  async function revert() {
    const path = revertTarget.value;
    revertTarget.value = null;
    if (!path) return;

    working.value = path;
    error.value = null;
    try {
      await api.templateRevert(path);
      await load();
    } catch (e) {
      error.value = e;
    } finally {
      working.value = null;
    }
  }

  return {
    templates,
    busy,
    error,
    working,
    chosen,
    revertTarget,
    overridden,
    shipped,
    busyWith,
    load,
    override,
    open,
    revert,
  };
}
