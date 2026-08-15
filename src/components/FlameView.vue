<script setup>
import { computed, ref } from 'vue';

/**
 * A profile as nested bars: what called what, and how much each branch cost.
 *
 * F-3. The table beside this answers "where did the time go"; it cannot answer
 * "what called that", which is the question that turns a slow page into a
 * fixed one.
 *
 * ## An icicle, not a flame
 *
 * Root at the top and growing downward. A classic flame graph is inverted —
 * root at the bottom — which reads well as an image and badly in a scrolling
 * pane: the thing you look at first ends up wherever the deepest stack happens
 * to put it. Downward means the entry point is where the eye starts and depth
 * is where the scroll goes.
 *
 * ## Width is share of the parent, not of the total
 *
 * A branch that is 90% of its caller reads as 90% wide inside it, whatever the
 * caller is a share of. That is what makes a deep tree readable at all: widths
 * relative to the root would make everything below the third level a hairline,
 * which is exactly when somebody stops being able to click it.
 *
 * ## Rendered as divs, not SVG or canvas
 *
 * The rows are text with a background, and text in a div is selectable,
 * searchable with the browser's own find, and reachable by keyboard. A canvas
 * would need every one of those written by hand, and this repository has a
 * browser suite (§3 #12) that would then have nothing to assert against — a
 * canvas is one node with no accessible name.
 */
const props = defineProps({
  /** `Frame[]` from `profiler_tree`. */
  frames: { type: Array, default: () => [] },
  /** Formats a cost into the profile's own unit. */
  format: { type: Function, default: (v) => String(v) },
});

/** How deep to render before a branch has to be opened by hand. */
const OPEN_TO = 3;

/** Branches the reader has folded or unfolded, by path. */
const toggled = ref(new Set());

const rows = computed(() => flatten(props.frames, 0, '', []));

/**
 * Depth-first into a flat list, because a nested `v-for` of components cannot
 * be keyboard-navigated in order and cannot be virtualised later.
 *
 * `share` is of the parent — see the header. The root's share is 1.
 */
function flatten(frames, depth, prefix, out) {
  const total = frames.reduce((sum, f) => sum + f.value, 0) || 1;
  for (const frame of frames) {
    const path = `${prefix}/${frame.name}`;
    const open = isOpen(path, depth);
    out.push({
      path,
      depth,
      name: frame.name,
      value: frame.value,
      recursive: frame.recursive,
      share: frame.value / total,
      children: frame.children.length,
      open,
    });
    if (open && frame.children.length) {
      flatten(frame.children, depth + 1, path, out);
    }
  }
  return out;
}

function isOpen(path, depth) {
  const flipped = toggled.value.has(path);
  return depth < OPEN_TO ? !flipped : flipped;
}

function toggle(row) {
  if (!row.children) return;
  const next = new Set(toggled.value);
  if (next.has(row.path)) next.delete(row.path);
  else next.add(row.path);
  toggled.value = next;
}

/**
 * A colour per function name, stable between renders.
 *
 * Hue from a hash of the name so the same function is the same colour every
 * time the profile is opened — and so two adjacent branches are usually
 * distinguishable. Fixed saturation and a light-mode-safe lightness rather than
 * anything clever: the bar is a background for text that has to stay readable.
 */
function hue(name) {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) % 360;
  return h;
}
</script>

<template>
  <div v-if="!rows.length" class="text-caption text-medium-emphasis">
    <slot name="empty" />
  </div>

  <div v-else class="flame">
    <button
      v-for="row in rows"
      :key="row.path"
      type="button"
      class="frame"
      :class="{ leaf: !row.children }"
      :style="{
        marginLeft: `${row.depth * 12}px`,
        width: `calc(100% - ${row.depth * 12}px)`,
        '--share': row.share,
        '--hue': hue(row.name),
      }"
      :aria-expanded="row.children ? row.open : undefined"
      @click="toggle(row)"
    >
      <span class="frame-bar" />
      <span class="frame-text">
        <v-icon v-if="row.children" size="12" class="mr-1">
          {{ row.open ? 'mdi-menu-down' : 'mdi-menu-right' }}
        </v-icon>
        <span class="frame-name">{{ row.name }}</span>
        <!-- Marked rather than hidden: a recursive call is a fact about the
             program, and a branch that simply stopped would read as a bug. -->
        <span v-if="row.recursive" class="frame-note">↻</span>
        <span class="frame-cost">{{ format(row.value) }}</span>
      </span>
    </button>
  </div>
</template>

<style scoped>
.flame {
  max-height: 460px;
  overflow-y: auto;
}

/* A button, not a div with a click handler: it takes focus, it is announced as
   a control, and `aria-expanded` means something on it. The repository has a
   source guard for exactly the other shape. */
.frame {
  position: relative;
  display: block;
  appearance: none;
  background: none;
  border: 0;
  padding: 0;
  text-align: left;
  font: inherit;
  cursor: pointer;
  height: 18px;
  margin-bottom: 1px;
}

.frame.leaf {
  cursor: default;
}

/* The bar is the share; the text sits over it at full width so a narrow branch
   is still readable. A width that clipped its own label would be a picture of
   the problem rather than a way to find it. */
.frame-bar {
  position: absolute;
  inset: 0;
  width: calc(var(--share) * 100%);
  min-width: 2px;
  background: hsl(var(--hue) 65% 62% / 0.55);
  border-radius: 2px;
}

.frame-text {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 100%;
  padding: 0 6px;
  font-size: 0.7rem;
  white-space: nowrap;
  overflow: hidden;
}

.frame-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

.frame-note {
  opacity: 0.7;
}

.frame-cost {
  margin-left: auto;
  opacity: 0.65;
  font-variant-numeric: tabular-nums;
}
</style>
