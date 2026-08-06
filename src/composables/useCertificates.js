import { computed, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The certificate the stack serves HTTPS with: its state, and the two things a
 * user can do about it.
 *
 * Lifted out of `Settings.vue`, which was 3,433 lines and **0% covered**. The
 * readiness review's §2.3 named this: behaviour was verified in a *copy* of the
 * pane inside `tests/certificates-pane.spec.js`, kept honest by reading the
 * real file as text and asserting the copy still matched. That was a creative
 * answer to an untestable component and it had the cost creative answers have —
 * a whitespace change broke the test, and a real regression escaped unless
 * somebody remembered to mirror it.
 *
 * ## Why the state is module-scoped
 *
 * Two places need it and they need the *same* answer: the pane renders it, and
 * the settings rail badges the "certificate is stale" entry — which has to be
 * visible before you navigate to the pane that would explain it. Two calls to
 * this function must therefore share one fetch, or the badge and the pane
 * disagree and the app makes the same request twice.
 *
 * That is what a store is, and Pinia is right there. It is not used here
 * because this state has no cross-page lifecycle worth registering: nothing
 * subscribes, nothing resets it on workspace change, and `reset()` exists only
 * so tests start from a known point. If a third consumer appears, promote it.
 */

const certs = ref(null);
const plan = ref(null);
const error = ref(null);
const busy = ref(false);

/**
 * The certificate was reissued but the running proxy is still serving the old
 * one — see `reload_proxy` in `certs.rs`.
 *
 * Cleared by the next reissue, because the state it describes belongs to the
 * last one. Silence here is what let the bug survive: the reissue reports
 * success either way.
 */
const notReloaded = ref(false);

export function useCertificates() {
  /**
   * Read the status and the plan.
   *
   * Both, not just the status: the plan is what says which names a reissue
   * would *drop*, and a user who deleted a project and watches its domain
   * vanish from the certificate should have been told first.
   */
  async function load() {
    try {
      certs.value = await api.certStatus();
      plan.value = await api.certPlan(certs.value.caTrusted !== true);
      error.value = null;
    } catch (e) {
      // A missing workspace is already reported by the requirements gate; a
      // second copy of it here would be noise.
      certs.value = null;
      plan.value = null;
      error.value = e?.needsWorkspace ? null : e;
    }
  }

  /**
   * Reissue for the domains the projects actually have.
   *
   * The plan is on screen before this runs, so there is no confirmation step —
   * the button's label is the plan.
   */
  async function reissue() {
    busy.value = true;
    error.value = null;
    notReloaded.value = false;
    try {
      // `false`: the trust write is its own button. Asking for it here made
      // every reissue return an error about something nobody had asked for.
      const applied = await api.certApply(false);
      // A certificate nothing serves is not a certificate the user has.
      notReloaded.value = applied?.reloaded === false;
      await load();
    } catch (e) {
      error.value = e;
    } finally {
      busy.value = false;
    }
  }

  /**
   * Trust the CA by opening a terminal.
   *
   * The app cannot do it itself: macOS grants the authorization for trust
   * settings only interactively, and a background child of a windowed app is
   * not somewhere it will ask. Opening a terminal is honest, and it works.
   */
  async function trustInTerminal() {
    try {
      await api.certTrustInTerminal();
    } catch (e) {
      error.value = e;
    }
  }

  /** The one fact worth surfacing outside the pane — it badges the rail. */
  const stale = computed(() => certs.value?.sslEnabled && certs.value?.stale);

  /** Expiry as a date; the Rust side sends epoch seconds. */
  function expiry(locale) {
    const seconds = certs.value?.notAfter;
    if (!seconds) return null;
    return new Date(seconds * 1000).toLocaleDateString(locale);
  }

  /** For tests, which must not inherit another test's fetch. */
  function reset() {
    certs.value = null;
    plan.value = null;
    error.value = null;
    busy.value = false;
    notReloaded.value = false;
  }

  return {
    certs,
    plan,
    error,
    busy,
    notReloaded,
    stale,
    expiry,
    load,
    reissue,
    trustInTerminal,
    reset,
  };
}
