import { ref } from 'vue';
import { api } from '@/lib/ipc';
import {
  isEnabled as autostartEnabled,
  enable as enableAutostart,
  disable as disableAutostart,
} from '@tauri-apps/plugin-autostart';

/**
 * The app's own preferences: editor, terminal, browser, close behaviour,
 * autostart.
 *
 * Distinct from the `.env` editor beside it — these are facts about this
 * installation, not about the stack, and they live in `preferences.json` rather
 * than in a file a teammate clones.
 *
 * Module-scoped: the view reads `locale` and `closeBehaviour` out of it while
 * the pane edits them, and two instances would mean the window acting on a
 * preference the pane had already changed.
 */

const prefs = ref(null);
const autostart = ref(false);

export function usePreferences() {
  async function load() {
    try {
      prefs.value = await api.prefsGet();
      autostart.value = await autostartEnabled().catch(() => false);
      return prefs.value;
    } catch {
      // A preferences file that cannot be read is a fresh start, not a pane
      // that refuses to open — `commands::prefs_get` already backs up a corrupt
      // one and answers with defaults.
      prefs.value = {};
      return prefs.value;
    }
  }

  async function set(patch) {
    prefs.value = await api.prefsSet(patch);
    return prefs.value;
  }

  /**
   * The OS registration first.
   *
   * If the launch agent refuses — a managed machine, a sandbox — the stored
   * preference must not claim otherwise, so the flag is re-read from the OS
   * rather than assumed from the switch.
   */
  async function toggleAutostart(value) {
    if (value) await enableAutostart();
    else await disableAutostart();
    autostart.value = await autostartEnabled();
    await set({ autostart: autostart.value });
    return autostart.value;
  }

  /** For tests, which must not inherit another test's read. */
  function reset() {
    prefs.value = null;
    autostart.value = false;
  }

  return { prefs, autostart, load, set, toggleAutostart, reset };
}

/**
 * What an app picker shows when the user has never touched it.
 *
 * An empty box said nothing about what "Open in terminal" would start, while
 * the back end has always fallen back to the first installed entry. Showing
 * that entry is honest — it is what the button does — and it is deliberately
 * **not** written to `preferences.json`: the fallback should keep tracking what
 * is installed rather than freeze the first answer it ever gave.
 */
export const appDefault = (list) => list?.find((a) => a.default)?.id ?? null;
export const appChoice = (stored, list) => stored ?? appDefault(list);
