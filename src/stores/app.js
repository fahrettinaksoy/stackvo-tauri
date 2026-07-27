import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api, StackvoError } from '@/lib/ipc';

/**
 * Workspace + engine state — the two things every other view depends on.
 *
 * The web UI had no equivalent. It could not report a stopped Docker daemon,
 * because the dashboard was itself a container that Docker had to be running to
 * serve. Here both are first-class, observable state, and the app renders fine
 * with either of them missing.
 */
export const useAppStore = defineStore('app', () => {
  const workspace = ref(null);
  const engine = ref(null);
  const booting = ref(true);
  const startingEngine = ref(false);
  const error = ref(null);

  const hasWorkspace = computed(() => !!workspace.value?.valid);
  const engineUp = computed(() => !!engine.value?.reachable);

  /** True once we know enough to render the real UI rather than a blocker. */
  const ready = computed(() => hasWorkspace.value && engineUp.value);

  async function refreshWorkspace() {
    try {
      workspace.value = await api.workspaceGet();
    } catch (e) {
      error.value = e instanceof StackvoError ? e : new StackvoError({ message: String(e) });
    }
  }

  async function setWorkspace(path) {
    error.value = null;
    try {
      workspace.value = await api.workspaceSet(path);
      return true;
    } catch (e) {
      error.value = e;
      return false;
    }
  }

  async function refreshEngine() {
    try {
      engine.value = await api.engineStatus();
    } catch (e) {
      // engine_status is infallible by contract; a throw here means the command
      // itself is broken, which is worth surfacing rather than swallowing.
      error.value = e;
    }
  }

  async function startEngine() {
    startingEngine.value = true;
    try {
      await api.engineStart();
      // The daemon takes a while to accept connections; poll until it does or
      // we give up, instead of leaving the button spinning forever.
      for (let i = 0; i < 40; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        await refreshEngine();
        if (engineUp.value) return true;
      }
      return false;
    } catch (e) {
      error.value = e;
      return false;
    } finally {
      startingEngine.value = false;
    }
  }

  async function boot() {
    booting.value = true;
    await Promise.all([refreshWorkspace(), refreshEngine()]);
    booting.value = false;
  }

  return {
    workspace,
    engine,
    booting,
    startingEngine,
    error,
    hasWorkspace,
    engineUp,
    ready,
    boot,
    refreshWorkspace,
    setWorkspace,
    refreshEngine,
    startEngine,
  };
});
