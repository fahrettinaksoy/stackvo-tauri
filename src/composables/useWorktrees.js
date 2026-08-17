import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * N — a branch with an environment of its own.
 *
 * One composable for both halves of the feature, because they are two views of
 * one subject and a project is only ever in one of them: an ordinary git
 * project *has* worktrees, and a worktree *is* one. Splitting them would mean
 * two composables that both call `worktreeSupport` and disagree about which
 * fields of it matter.
 *
 * ## Why the plan is state and not a return value
 *
 * `worktreePlan` has no side effects, so it runs while somebody is still
 * choosing — as the branch changes, as the database mode changes. What it
 * answers is what the dialog shows: the name the project will have, the
 * hostname it will answer on, the database it will be given, and the one
 * sentence saying why it cannot be done. Holding it here means the dialog reads
 * a value rather than deriving four strings the Rust side has already derived,
 * which is the class of mistake `domain_label` exists to prevent.
 */
export function useWorktrees(name) {
  const support = ref(null);
  const plan = ref(null);
  const loading = ref(false);
  const planning = ref(false);
  const busy = ref(null);
  const error = ref(null);

  /** This project's own record, when it is a worktree rather than has them. */
  const record = computed(() => support.value?.record ?? null);
  const isWorktree = computed(() => record.value !== null);
  const worktrees = computed(() => asList(support.value?.worktrees));
  const branches = computed(() => asList(support.value?.branches));
  const instances = computed(() => asList(support.value?.instances));

  /**
   * Can a worktree be created here at all?
   *
   * `reason` is a sentence from the boundary, never assembled here: the same
   * check runs again inside `worktreeCreate`, and two implementations of
   * "is this allowed" is how a screen offers a button that then refuses.
   */
  const available = computed(() => Boolean(support.value) && !support.value.reason);
  const reason = computed(() => support.value?.reason ?? null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      support.value = await api.worktreeSupport(name.value);
    } catch (e) {
      error.value = e;
      support.value = null;
    } finally {
      loading.value = false;
    }
    return support.value;
  }

  /**
   * Ask what creating this would do.
   *
   * A refusal is not an error: it is the answer, and it belongs on the form
   * beside the fields that caused it rather than in the page's error alert.
   * Only a boundary failure — no workspace, an unreadable manifest — reaches
   * `error`.
   */
  async function preview(branch, options) {
    if (!branch) {
      plan.value = null;
      return null;
    }
    planning.value = true;
    try {
      plan.value = await api.worktreePlan(name.value, branch, options);
    } catch (e) {
      error.value = e;
      plan.value = null;
    } finally {
      planning.value = false;
    }
    return plan.value;
  }

  async function create(branch, options) {
    busy.value = 'create';
    error.value = null;
    try {
      await api.worktreeCreate(name.value, branch, options);
      await load();
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = null;
    }
  }

  /**
   * Remove one, by its own name rather than this project's.
   *
   * The pane calls this from two places — a parent removing one of its
   * worktrees, and a worktree removing itself — so the target is an argument.
   * Reloading afterwards is right in the first case and pointless in the
   * second, where the project the page is about no longer exists; the caller
   * navigates away and the reload never lands.
   */
  async function remove(target, options) {
    busy.value = target;
    error.value = null;
    try {
      await api.worktreeRemove(target, options);
      if (target !== name.value) await load();
      return true;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      busy.value = null;
    }
  }

  async function saveEnv(env) {
    busy.value = 'env';
    error.value = null;
    try {
      const saved = await api.worktreeEnvSet(name.value, env);
      await load();
      return saved;
    } catch (e) {
      error.value = e;
      return null;
    } finally {
      busy.value = null;
    }
  }

  return {
    support,
    plan,
    record,
    isWorktree,
    worktrees,
    branches,
    instances,
    available,
    reason,
    loading,
    planning,
    busy,
    error,
    load,
    preview,
    create,
    remove,
    saveEnv,
  };
}
