import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The Dockerfile this project would be built from, rendered by the Rust
 * generator and compared against what Bash writes today.
 *
 * `compat` reproduces what Bash actually writes; `strict` refuses where Bash
 * silently drops an extension. Held as state rather than fired by two
 * unlabelled buttons: which of the two you are looking at changes what the
 * comparison chip below it means.
 *
 * Lifted out of `ProjectDetail.vue` with the Dockerfile pane under §14.16.
 */
export function useDockerfilePreview(name) {
  const preview = ref(null);
  const mode = ref('compat');
  const loading = ref(false);
  const error = ref(null);

  /** Numbered, because a Dockerfile is read by line as often as it is read. */
  const lines = computed(() => preview.value?.dockerfile?.split('\n') ?? []);

  async function load(next = mode.value) {
    mode.value = next;
    // Cleared first: leaving the previous render on screen while the other mode
    // is fetched shows one mode's file under the other mode's heading.
    preview.value = null;
    error.value = null;
    loading.value = true;
    try {
      preview.value = await api.projectDockerfilePreview(name.value, next === 'strict');
      return preview.value;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      loading.value = false;
    }
  }

  return { preview, mode, loading, error, lines, load };
}
