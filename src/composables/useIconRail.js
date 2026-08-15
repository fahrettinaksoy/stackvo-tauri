import { computed } from 'vue';
import { useDisplay } from 'vuetify';

/**
 * When a side rail of labelled sections keeps only its icons.
 *
 * Two pages have one: Settings, whose rail is 220px, and Project detail, whose
 * rail is 240px. Both truncate before they shrink — "Kimlik bilgileri nerede
 * t…", "Çalışma zamanı ay…" — which is a label that has stopped being one, on
 * exactly the window where the pane beside it was already the narrower half.
 *
 * Measured here rather than in a media query per view, for two reasons. The
 * number is shared, and a breakpoint written twice is a breakpoint that will
 * be changed once. And the labels are not hidden — a `display: none` title is
 * absent from the accessible name too, so a CSS-only version leaves a column
 * of buttons called nothing. Deciding it in script lets the view drop the
 * title, move the name to `aria-label` and raise the tooltip in one breath.
 */
export const RAIL_BELOW = 1100;

/**
 * @param min the width under which the caller's layout stops being a rail at
 *   all — Settings turns its into a strip above the pane at 900, where there
 *   is width to spare and the labels are what make a wrapped row readable.
 *   Left at 0 the rail simply stays a rail all the way down.
 */
export function useIconRail(min = 0) {
  const display = useDisplay();
  return computed(() => display.width.value >= min && display.width.value < RAIL_BELOW);
}
