import { computed, ref } from 'vue';
import { api, asList } from '@/lib/ipc';

/**
 * Projects somebody pinned to the top of the list (M-1).
 *
 * ## Why this is a preference and not a manifest key
 *
 * A favourite is about the person, not the project. `stackvo.json` is committed
 * and shared — writing one there would put "Ali likes this project" in a
 * teammate's diff — and the manifest schema is `additionalProperties: false`
 * anyway. `preferences.json` is per-machine and per-user, which is exactly what
 * this is.
 *
 * `prefs_set` merges shallowly, so a whole array is sent under one key rather
 * than a patch: the list is the value, and a bad merge of a list is a favourite
 * that comes back after being removed.
 *
 * ## Sorting rather than filtering
 *
 * The starred ones move to the top; nothing is hidden. A list that hid the rest
 * would be a mode somebody can get stuck in — the inventory store already makes
 * a point of not hiding broken projects, and this follows it.
 */
export function useFavourites() {
  const names = ref([]);
  const loaded = ref(false);

  async function load() {
    try {
      const prefs = await api.prefsGet();
      // Only strings: the file is editable by hand, and a number in there would
      // otherwise become a name nothing matches and nobody can remove.
      names.value = asList(prefs?.favourites).filter((n) => typeof n === 'string');
    } catch {
      names.value = [];
    } finally {
      loaded.value = true;
    }
  }

  const isFavourite = (name) => names.value.includes(name);

  async function toggle(name) {
    const next = isFavourite(name)
      ? names.value.filter((n) => n !== name)
      : [...names.value, name].sort();
    // Optimistic: the star is a click away from being undone, and waiting for a
    // file write to redraw it makes the button feel broken.
    names.value = next;
    try {
      await api.prefsSet({ favourites: next });
    } catch {
      // Put it back rather than leaving the screen claiming something the file
      // does not say.
      await load();
    }
  }

  /** Starred first, everything else in the order it arrived. */
  function sorted(projects) {
    const starred = names.value;
    return [...projects].sort((a, b) => {
      const left = starred.includes(a.name) ? 0 : 1;
      const right = starred.includes(b.name) ? 0 : 1;
      return left - right;
    });
  }

  return {
    names,
    loaded,
    count: computed(() => names.value.length),
    load,
    isFavourite,
    toggle,
    sorted,
  };
}
