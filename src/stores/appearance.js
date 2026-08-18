import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/lib/ipc';
import { DEFAULT_APPEARANCE, applyAppearance } from '@/lib/appearance';

/**
 * The appearance settings, loaded once at boot and applied app-wide.
 *
 * Not owned by the settings page: the theme has to be right on the first paint,
 * and the settings page is opened — if ever — long after it. Before this, a
 * saved theme was applied by `Settings.vue` on mount, so the app started dark
 * whatever the user had chosen and corrected itself only if they happened to
 * open Settings.
 *
 * Writes go to preferences.json under a single `appearance` key. `prefs_set`
 * merges shallowly at the top level, so the whole object is sent every time —
 * sending one field would replace the rest with nothing.
 */
export const useAppearanceStore = defineStore('appearance', () => {
  const value = ref({ ...DEFAULT_APPEARANCE });
  /** Named snapshots, so a look can be kept without being the current one. */
  const presets = ref([]);
  /** The desktop's accent, re-read on demand — it can change while we run. */
  const systemAccent = ref(null);
  const loaded = ref(false);

  async function load() {
    const [prefs, accent] = await Promise.all([
      api.prefsGet().catch(() => null),
      api.systemAccent().catch(() => null),
    ]);
    systemAccent.value = accent?.available ? accent.hex : null;

    // `theme` used to live at the top level. Carry it over rather than resetting
    // to system: a user who chose light and never opens this page again would
    // otherwise silently lose the choice.
    const legacy = prefs?.theme ? { theme: prefs.theme } : {};

    value.value = { ...DEFAULT_APPEARANCE, ...legacy, ...(prefs?.appearance ?? {}) };
    presets.value = Array.isArray(prefs?.appearancePresets) ? prefs.appearancePresets : [];
    apply();
    loaded.value = true;
  }

  function apply() {
    applyAppearance(value.value, systemAccent.value);
  }

  /** Re-read the desktop accent; called when the window regains focus. */
  async function refreshSystemAccent() {
    const accent = await api.systemAccent().catch(() => null);
    const hex = accent?.available ? accent.hex : null;
    if (hex === systemAccent.value) return;

    systemAccent.value = hex;
    if (value.value.useSystemAccent) apply();
  }

  /** Apply first, persist second: the UI must not wait on a disk write. */
  async function set(patch) {
    value.value = { ...value.value, ...patch };
    apply();
    await api.prefsSet({ appearance: value.value }).catch(() => {});
  }

  /**
   * Apply without writing, for controls that fire continuously.
   *
   * A slider emits on every pixel of a drag; persisting each one would be a
   * few hundred writes to preferences.json for one gesture. The view previews
   * during the drag and calls `commit` when the handle is released.
   */
  function preview(patch) {
    value.value = { ...value.value, ...patch };
    applyAppearance(value.value, systemAccent.value);
  }

  async function commit() {
    await api.prefsSet({ appearance: value.value }).catch(() => {});
  }

  function reset() {
    return set({ ...DEFAULT_APPEARANCE });
  }

  /**
   * Save the current look under a name, or overwrite the one already using it.
   *
   * Overwriting rather than allowing duplicates: two presets called "Demo" are
   * indistinguishable in the list, and the second one silently wins whenever
   * anyone clicks either.
   */
  async function savePreset(name) {
    const trimmed = String(name ?? '').trim();
    if (!trimmed) return;

    const entry = { name: trimmed, values: { ...value.value } };
    const rest = presets.value.filter((p) => p.name !== trimmed);
    presets.value = [...rest, entry].sort((a, b) => a.name.localeCompare(b.name));
    await api.prefsSet({ appearancePresets: presets.value }).catch(() => {});
  }

  function applyPreset(name) {
    const preset = presets.value.find((p) => p.name === name);
    // Merged over the defaults, not applied raw: a preset saved by an older
    // version has no field for a setting added since, and the current value
    // would otherwise leak into a look that never contained it.
    return preset ? set({ ...DEFAULT_APPEARANCE, ...preset.values }) : undefined;
  }

  async function deletePreset(name) {
    presets.value = presets.value.filter((p) => p.name !== name);
    await api.prefsSet({ appearancePresets: presets.value }).catch(() => {});
  }

  /**
   * The app bar's light/dark button.
   *
   * Resolves 'system' to whatever it is currently showing before flipping, so
   * the first press always changes something — toggling out of 'system' into
   * the mode you were already looking at would read as a dead button.
   */
  function toggleTheme(isDark) {
    return set({ theme: isDark ? 'light' : 'dark' });
  }

  return {
    value,
    presets,
    systemAccent,
    refreshSystemAccent,
    loaded,
    load,
    set,
    preview,
    commit,
    reset,
    savePreset,
    applyPreset,
    deletePreset,
    toggleTheme,
  };
});
