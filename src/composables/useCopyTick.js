import { ref } from 'vue';

/**
 * Copy to the clipboard, and remember which button did it for a moment.
 *
 * A page has several copy buttons — a URL, a container name, a path — and a
 * bare boolean would tick all of them at once. The key is what makes the tick
 * land on the button that was pressed.
 *
 * Module-scoped, because the page shows *two* confirmations for one copy: the
 * icon on the button swaps to a check, and the view raises a snackbar. Those
 * live in different components, and per-instance state would leave the snackbar
 * watching a value nothing ever sets.
 *
 * Clipboard failure is swallowed on purpose: the value is on screen and
 * selectable, so this is a missing convenience rather than an error worth
 * putting a red alert on the page for.
 */

/** How long the tick stays up. */
export const COPY_HOLD = 1200;

const copied = ref(null);

export function useCopyTick() {
  async function copy(value, key) {
    try {
      await navigator.clipboard.writeText(value);
      copied.value = key;
      setTimeout(() => (copied.value = null), COPY_HOLD);
      return true;
    } catch {
      return false;
    }
  }

  /** For tests, which must not inherit another test's tick. */
  function reset() {
    copied.value = null;
  }

  return { copied, copy, reset };
}
