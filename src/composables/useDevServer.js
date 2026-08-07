import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * A Node project's dev server: whether the container runs it, under which
 * command, and whether the project's own config will accept the domain.
 *
 * Only meaningful for the `node` runtime — a PHP project has no dev server to
 * start, so `load` clears rather than asks. The caller passes the runtime in
 * instead of the composable reading the project, because the pane is mounted
 * inside a view that has already loaded it.
 *
 * Lifted out of `ProjectDetail.vue` with the Dev Server pane under §14.16.
 */
export function useDevServer(name) {
  const status = ref(null);
  const command = ref('');
  const error = ref(null);
  const busy = ref(false);
  const copied = ref(false);

  /**
   * On, mounted, and the project's own config still rejects the domain — the
   * state where the container is right and the site answers a flat 403.
   */
  const blocked = computed(() => status.value?.enabled && status.value.hostAllowed === false);

  async function load(runtime) {
    if (runtime !== 'node') {
      status.value = null;
      return null;
    }
    try {
      status.value = await api.devserverStatus(name.value);
      command.value = status.value.command;
    } catch {
      // Not an error the user can act on: an unbuilt project has no dev-server
      // state to report, and the pane simply has nothing to show.
      status.value = null;
    }
    return status.value;
  }

  async function toggle(enabled) {
    busy.value = true;
    error.value = null;
    try {
      status.value = await api.devserverSet(name.value, enabled, command.value || null);
      // The back end normalises the command — echo what it stored rather than
      // what was typed, or the field disagrees with what will actually run.
      command.value = status.value.command;
      return status.value;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      busy.value = false;
    }
  }

  /** The copy tick, cleared on its own. Returns false when there is no clipboard. */
  async function copySnippet(reset = 1500) {
    try {
      await navigator.clipboard.writeText(status.value.snippet);
      copied.value = true;
      setTimeout(() => (copied.value = false), reset);
      return true;
    } catch {
      // No clipboard — headless, or permission refused. The snippet is on
      // screen and selectable, so this is a missing convenience, not a failure.
      return false;
    }
  }

  return { status, command, busy, copied, error, blocked, load, toggle, copySnippet };
}
