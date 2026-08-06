import { computed, ref, watch } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The per-server directive file, edited here rather than only on disk.
 *
 * A text area and not a set of fields: what goes in is nginx's own grammar, and
 * pretending otherwise would mean a form that can express a fraction of it and
 * silently drops the rest.
 *
 * Lifted out of `Settings.vue` with its pane. The state is per call — one
 * component reads it — and the tab watcher lives here rather than in the view,
 * which is the part that was easy to get wrong: switching servers has to reload
 * the file, and a component that forgot would show nginx's directives under the
 * caddy tab.
 */

/** The three servers that have a generated config to append to. */
export const CONFIGURABLE_SERVERS = ['nginx', 'caddy', 'frankenphp'];

export function useServerConfig() {
  const server = ref(CONFIGURABLE_SERVERS[0]);
  const text = ref('');
  const busy = ref(false);
  const error = ref(null);

  /**
   * What is on disk, so "changed" is a comparison rather than a flag somebody
   * has to remember to set and clear.
   */
  const saved = ref('');
  const dirty = computed(() => text.value !== saved.value);

  async function load() {
    busy.value = true;
    error.value = null;
    try {
      text.value = await api.serverConfigGet(server.value);
      saved.value = text.value;
    } catch (e) {
      error.value = e;
    } finally {
      busy.value = false;
    }
  }

  /**
   * Returns the keys that changed, for the caller's "regenerate to apply"
   * notice. Directives reach a container only through a regenerate — saying so
   * is the difference between a feature that worked and one the user believes
   * did nothing.
   */
  async function save() {
    busy.value = true;
    error.value = null;
    try {
      await api.serverConfigSet(server.value, text.value);
      saved.value = text.value;
      return ['SERVER_CONFIG'];
    } catch (e) {
      error.value = e;
      return [];
    } finally {
      busy.value = false;
    }
  }

  // Switching the tab is switching the file. Without this the pane shows one
  // server's directives under another server's name.
  watch(server, load);

  return { server, text, busy, error, saved, dirty, load, save };
}
