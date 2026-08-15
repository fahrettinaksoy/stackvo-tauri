import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api, StackvoError } from '@/lib/ipc';
import { loadLocalePacks, syncLocale } from '@/i18n';

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
  /**
   * Is the "new project" panel open?
   *
   * Here rather than in either view because there is exactly one panel and two
   * buttons that open it — the rail's and the project list's. As a drawer it
   * has to be mounted at the app level (a `v-navigation-drawer` is absolutely
   * positioned, so one rendered inside a page is clipped by the page's own
   * `overflow: hidden`), which leaves the flag as the only thing the two
   * triggers can share.
   */
  const newProjectOpen = ref(false);
  const startingEngine = ref(false);
  const error = ref(null);

  /**
   * The prerequisite report from `preflight`.
   *
   * Null until the first check. `ready` there — not `hasWorkspace` — is what
   * decides whether the app renders: a chosen folder is one of six things that
   * have to hold, and the other five used to be discovered one failed click at
   * a time.
   */
  const preflight = ref(null);

  /**
   * The configured domain suffix, from `DEFAULT_TLD_SUFFIX`.
   *
   * Here rather than fetched per view because three of them build a hostname
   * from it and each was inventing its own. The project form was the one that
   * mattered: it defaulted a new project to `name.loc` while the routing
   * labels, the certificate and the services list all used the configured
   * suffix. The project came up, the container ran, and the address in the
   * card resolved to nothing.
   */
  const tld = ref('');
  /** Whether the stack serves HTTPS — decides what a `.dev` domain costs. */
  const sslEnabled = ref(false);

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
    const fresh = !workspace.value || workspace.value.root !== path;
    try {
      workspace.value = await api.workspaceSet(path);
      // Choosing an empty folder now *creates* the workspace. Saying so is the
      // difference between "nothing happened" and "thirty-six files were
      // written into the directory you picked".
      if (fresh && workspace.value?.root) {
        const { toastSuccess } = await import('@/lib/toast');
        const { i18n } = await import('@/i18n');
        toastSuccess(i18n.global.t('preflight.workspaceInstalled', { path: workspace.value.root }));
      }
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

  async function checkRequirements() {
    try {
      preflight.value = await api.preflight();
    } catch (e) {
      error.value = e;
    }
    return preflight.value;
  }

  /**
   * Ask the app to do the one thing a requirement needs, then re-check.
   *
   * Re-checking rather than assuming: creating the network can succeed and the
   * daemon still be gone by the time the next thing needs it.
   */
  async function fixRequirement(id) {
    error.value = null;
    try {
      if (id === 'workspace') {
        const picked = await api.workspacePick();
        if (picked) workspace.value = picked;
      } else if (id === 'engine') {
        await startEngine();
      } else {
        await api.preflightFix(id);
      }
    } catch (e) {
      error.value = e;
    }
    await Promise.all([refreshWorkspace(), refreshEngine()]);
    return checkRequirements();
  }

  /** Best-effort: with no workspace there is no .env, and no suffix to read. */
  async function refreshTld() {
    const env = await api.envGet().catch(() => null);
    tld.value = env?.DEFAULT_TLD_SUFFIX ?? '';
    sslEnabled.value = env?.SSL_ENABLE === 'true';
  }

  async function boot() {
    booting.value = true;
    // The language first, and alongside the rest rather than before it: every
    // screen the boot can land on — the requirements gate, the first-run setup
    // — is one somebody reads, and reading it in the wrong language is worst on
    // exactly the launch where nothing has been chosen yet.
    // Packs before the language is settled (M-7): `syncLocale` checks the
    // resolved tag against the registered locales, and a pack that has not
    // been loaded yet is a language the app would decline to open in — one
    // frame of the user's choice, then English.
    await loadLocalePacks().catch(() => []);
    await Promise.all([
      syncLocale().catch(() => {}),
      refreshWorkspace(),
      refreshEngine(),
      checkRequirements(),
    ]);
    await refreshTld();
    booting.value = false;
  }

  return {
    workspace,
    engine,
    preflight,
    tld,
    sslEnabled,
    refreshTld,
    checkRequirements,
    fixRequirement,
    newProjectOpen,
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
