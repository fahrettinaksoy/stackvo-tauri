import { computed, inject, provide, ref } from 'vue';
import { api } from '@/lib/ipc';

/**
 * The `.env` editor six Settings panes share.
 *
 * This is the machine the readiness review's §2.3 called `useEnvEditor()`, and
 * it is why the split stalled at three panes: the appearance, domain, PHP,
 * server-limits, services and workspace panes all edit the same file through
 * the same four refs, so none of them could leave `Settings.vue` on its own.
 *
 * ## The three-layer read is the whole design
 *
 * A value is whatever the user has typed, or what the file holds, or what the
 * binary ships — in that order. Keeping the layers apart is what lets the form
 * say "this is the default" and offer to go back to it, which is the difference
 * between a settings screen and a wall of populated text fields. Collapsing
 * them into one map at load time would lose that on the first render.
 *
 * ## Edits are a diff, not a copy
 *
 * `edit()` *removes* a key when the value returns to what the file holds, so
 * `dirty` is "is there anything to write" rather than "has anything been
 * touched", and a save writes only what changed. Typing a character and typing
 * it back leaves nothing behind.
 */

/**
 * Keys whose change does not reach the running stack until the files are
 * regenerated.
 *
 * The suffix rewrites every routing label and moves what the certificate has to
 * cover; none of that happens on save. Saving and staying silent is how a
 * setting looks like it did nothing.
 */
const ROUTING_KEYS = [
  'DEFAULT_TLD_SUFFIX',
  'DOCKER_DEFAULT_NETWORK',
  'SSL_ENABLE',
  'REDIRECT_TO_HTTPS',
];

/** How long the "saved" confirmation stays up. */
const SAVED_FOR_MS = 2500;

export function useEnvEditor() {
  /** What the file holds. */
  const env = ref({});
  /** What the binary ships, for "this is the default". */
  const defaults = ref({});
  /** What the user has changed and not yet written. */
  const edits = ref({});

  const error = ref(null);
  const saving = ref(false);
  const saved = ref(false);

  /** The keys the last save wrote, so a pane can say what has to happen next. */
  const lastSaved = ref([]);

  const dirty = computed(() => Object.keys(edits.value).length > 0);
  const changedCount = computed(() => Object.keys(edits.value).length);

  const routingChanged = computed(() => lastSaved.value.some((k) => ROUTING_KEYS.includes(k)));
  const suffixChanged = computed(() => lastSaved.value.includes('DEFAULT_TLD_SUFFIX'));

  async function loadDefaults() {
    defaults.value = await api.envDefaults().catch(() => ({}));
  }

  async function load() {
    error.value = null;
    edits.value = {};
    try {
      env.value = await api.envGet();
    } catch (e) {
      error.value = e;
      env.value = {};
    }
  }

  /**
   * Record a change, or forget one.
   *
   * The `delete` is the part that matters: without it, typing a value and then
   * typing the original back leaves the key in `edits`, the save button stays
   * lit, and the save writes a value identical to the one already on disk —
   * which for a routing key would put a "regenerate to apply" notice on screen
   * for a change nobody made.
   */
  function edit(key, value) {
    if (value === env.value[key]) delete edits.value[key];
    else edits.value[key] = value;
  }

  /** Typed, then on disk, then shipped. */
  const effective = (key) => edits.value[key] ?? env.value[key] ?? defaults.value[key] ?? '';
  const isDefault = (key) => effective(key) === defaults.value[key];
  const resetToDefault = (key) => edit(key, defaults.value[key] ?? '');

  // `.env` has two boolean spellings and they are not interchangeable: compose
  // reads `true`/`false` for its own conditionals and the generated nginx and
  // php.ini fragments read `on`/`off`. Writing the wrong one produces a file
  // that parses and does the opposite of what the switch said.
  const boolOf = (key) => effective(key) === 'true';
  const setBool = (key, on) => edit(key, on ? 'true' : 'false');
  const onOff = (key) => effective(key) === 'on';
  const setOnOff = (key, value) => edit(key, value ? 'on' : 'off');

  const listOf = (key) =>
    effective(key)
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

  const setList = (key, items) =>
    edit(
      key,
      items
        .map((s) => String(s).trim())
        .filter(Boolean)
        .join(',')
    );

  /**
   * Write the diff, then re-read.
   *
   * Re-reading rather than merging the edits into `env` locally: the Rust side
   * normalises what it writes, and a local merge would show the user their own
   * input where the file holds something else.
   */
  async function save(onSaved) {
    saving.value = true;
    error.value = null;
    saved.value = false;
    try {
      const keys = Object.keys(edits.value);
      await api.envSet({ ...edits.value });
      lastSaved.value = keys;
      await load();
      await onSaved?.();
      saved.value = true;
      setTimeout(() => (saved.value = false), SAVED_FOR_MS);
      return keys;
    } catch (e) {
      error.value = e;
      return [];
    } finally {
      saving.value = false;
    }
  }

  /** Called once the regenerate that applies a routing change has succeeded. */
  function clearPending() {
    lastSaved.value = [];
  }

  return {
    env,
    defaults,
    edits,
    error,
    saving,
    saved,
    lastSaved,
    dirty,
    changedCount,
    routingChanged,
    suffixChanged,
    loadDefaults,
    load,
    edit,
    effective,
    isDefault,
    resetToDefault,
    boolOf,
    setBool,
    onOff,
    setOnOff,
    listOf,
    setList,
    save,
    clearPending,
  };
}

/**
 * The key `Settings.vue` shares one editor under.
 *
 * Six panes edit the same file, so they must see the same instance — two calls
 * to `useEnvEditor()` would be two diffs over one `.env`, and whichever saved
 * last would silently drop the other's changes. Passing it down as a prop would
 * work and would mean threading it through every pane signature for a value
 * none of them chooses; injection is the idiom for exactly this.
 */
const ENV_EDITOR = Symbol('stackvo:env-editor');

/** Called once, by the view that owns the editor. */
export function provideEnvEditor(editor) {
  provide(ENV_EDITOR, editor);
  return editor;
}

/**
 * Called by a pane. Falls back to its own editor when nothing provided one,
 * so a pane can be mounted on its own in a test without a host component —
 * which is the whole reason these panes are being extracted.
 */
export function useSharedEnvEditor() {
  return inject(ENV_EDITOR, null) ?? useEnvEditor();
}
