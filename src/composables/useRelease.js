import { ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The production image a project can be shipped as.
 *
 * Three verbs over one plan: read what would be built, build it, and write the
 * result out as a tarball. The plan is read first and its `tag` becomes the
 * default — the user is shown the name the build will actually use rather than
 * an empty box that silently means "whatever the plan says".
 *
 * Lifted out of `ProjectDetail.vue` with the Release pane under §14.16.
 */
export function useRelease(name) {
  const plan = ref(null);
  const tag = ref('');
  const result = ref(null);
  const error = ref(null);

  /** `''` when idle, otherwise the verb running: `build` or `save`. */
  const busy = ref('');

  /**
   * Read the plan.
   *
   * A project with nothing to release answers `NOT_FOUND`, which is a state
   * rather than a failure — it is what an unbuilt project looks like, and
   * reporting it would put an error on a page the user has just opened.
   */
  async function load() {
    try {
      plan.value = await api.releasePlan(name.value, tag.value || null);
      if (!tag.value) tag.value = plan.value.tag;
      return true;
    } catch (e) {
      plan.value = null;
      if (e?.code && e.code !== 'NOT_FOUND') error.value = e;
      return false;
    }
  }

  async function build() {
    busy.value = 'build';
    error.value = null;
    result.value = null;
    try {
      result.value = await api.releaseBuild(name.value, tag.value || null);
      return result.value;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      busy.value = '';
    }
  }

  /**
   * Write the image out. The path comes from the system save dialog, passed in
   * rather than reached for here — choosing a destination is the user's act,
   * not this logic's, and a composable that imports a Tauri plugin cannot be
   * exercised without one.
   */
  async function save(choosePath) {
    const path = await choosePath(`${name.value}-production.tar`);
    if (!path) return false;

    busy.value = 'save';
    error.value = null;
    try {
      await api.releaseSave(name.value, path, tag.value || null);
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = '';
    }
  }

  return { plan, tag, result, busy, error, load, build, save };
}
