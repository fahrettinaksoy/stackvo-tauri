import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The Dockerfile this project would be built from, rendered from the manifest
 * and never written.
 *
 * `compat` is what the generator actually writes — an extension it cannot build
 * is dropped and the render carries on. `strict` refuses instead, and names the
 * extension. Held as state rather than fired by two unlabelled buttons: which
 * of the two you are looking at changes what the chip beside the heading means,
 * and in strict mode there is nothing for it to mean at all.
 *
 * The pair is left over from a port that was verified against a Bash generator
 * — `compat` meant "what Bash writes". That generator is gone; the two renders
 * kept their value because "what will be built" and "what would be refused" are
 * still different questions.
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
