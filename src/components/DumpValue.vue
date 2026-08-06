<script setup>
import { computed, ref } from 'vue';
import { hidden, isLegacy, mark, propName, summary } from '@/lib/dumpnode';

/**
 * One captured value, drawn as the tree the bridge captured.
 *
 * Recursive, and the whole component is the argument for having changed the
 * bridge: a type that survived the trip can be coloured, counted and folded,
 * and one that was formatted into a block on the way out cannot be any of
 * those things without guessing.
 *
 * ## What folds, and what is open to begin with
 *
 * The first two levels are open and everything under them is closed. A dumped
 * Eloquent model is a class name, twenty scalars and four relations; opening
 * all of it is the wall of text this replaces, and opening none of it makes
 * every dump cost two clicks before it says anything. Two levels is where the
 * scalars of the thing you dumped are visible and its graph is not.
 *
 * ## Colour carries the type, not the emphasis
 *
 * Strings, numbers, booleans, null and class names each get one, and they are
 * the same in every row — so a `"8.1"` never reads as an `8.1` again. They
 * come from the theme rather than being written here, because this pane is one
 * of two dark surfaces in the app and a hard-coded palette is legible on
 * exactly one of them.
 */
const props = defineProps({
  node: { type: [Object, String, Number, Boolean], default: null },
  /** Depth in the tree, which decides what starts open. */
  depth: { type: Number, default: 0 },
});

const AUTO_OPEN_DEPTH = 2;

const open = ref(props.depth < AUTO_OPEN_DEPTH);

const branch = computed(() => {
  const n = props.node;
  return !isLegacy(n) && n && typeof n === 'object' && (n.t === 'arr' || n.t === 'obj');
});

const items = computed(() => props.node?.items ?? []);
const rest = computed(() => hidden(props.node));

/** `array:8 [` or `App\Models\User {`, without the contents. */
const head = computed(() =>
  props.node?.t === 'arr' ? `array:${props.node.n ?? 0} [` : `${props.node?.class ?? ''} {`
);
const tail = computed(() => (props.node?.t === 'arr' ? ']' : '}'));

/** Empty is a leaf: there is no disclosure for a branch with nothing under it. */
const empty = computed(() => !items.value.length && !rest.value);

/** The class that carries the type's colour, for a leaf. */
const leafClass = computed(() => {
  if (isLegacy(props.node)) return 'tok-legacy';
  const t = props.node?.t;
  if (t === 'str') return 'tok-str';
  if (t === 'num') return 'tok-num';
  if (t === 'bool') return 'tok-bool';
  if (t === 'null' || t === 'deep') return 'tok-null';
  return 'tok-other';
});

function keyOf(item) {
  if (props.node?.t === 'arr') {
    return typeof item.k === 'number' ? `${item.k} =>` : `${JSON.stringify(item.k)} =>`;
  }
  const { name, visibility } = propName(item.k);
  return `${mark(visibility)}${name}:`;
}

function keyTitle(item) {
  if (props.node?.t === 'arr') return '';
  const { owner, visibility } = propName(item.k);
  return owner ? `${visibility} — ${owner}` : visibility;
}

function keyClass(item) {
  if (props.node?.t === 'arr') return 'tok-index';
  return `tok-prop tok-${propName(item.k).visibility}`;
}
</script>

<template>
  <!-- A value the older bridge already formatted. Nothing to fold and nothing
       to colour: what arrived is the rendering. -->
  <pre v-if="isLegacy(node)" class="tok-legacy legacy-block">{{ node }}</pre>

  <span v-else-if="!branch" :class="leafClass">{{ summary(node) }}</span>

  <span v-else-if="empty" class="tok-punct">{{ head }}{{ tail }}</span>

  <div v-else class="branch">
    <button type="button" class="twist" :aria-expanded="open" @click="open = !open">
      <span class="chev" :class="{ 'chev-open': open }">▸</span>
      <span :class="node.t === 'obj' ? 'tok-class' : 'tok-punct'">{{ head }}</span>
      <span v-if="!open" class="tok-punct"> … {{ tail }}</span>
    </button>

    <template v-if="open">
      <div class="children">
        <div v-for="(item, i) in items" :key="i" class="child">
          <span :class="keyClass(item)" :title="keyTitle(item)">{{ keyOf(item) }}</span>
          <!-- The recursion. Vue resolves a single-file component by its own
               filename, so this is the same component without an import. -->
          <DumpValue :node="item.v" :depth="depth + 1" />
        </div>
        <!-- The bridge stops at fifty entries per level. Saying so is the
             difference between a bounded view and a wrong one. -->
        <div v-if="rest" class="child tok-null">… {{ rest }} more</div>
      </div>
      <span class="tok-punct">{{ tail }}</span>
    </template>
  </div>
</template>

<style scoped>
.branch {
  min-width: 0;
}

/* The disclosure is the head itself, not a separate hit target beside it: the
   class name is what somebody aims at, and a 12px chevron is not. */
.twist {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  border: 0;
  padding: 0;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
  text-align: left;
}

.twist:hover .chev {
  opacity: 1;
}

.chev {
  display: inline-block;
  opacity: 0.5;
  transition: transform 120ms ease;
  font-size: 10px;
}

.chev-open {
  transform: rotate(90deg);
}

/* One indent per level, drawn as a rule: at four levels the eye needs the line
   to tell which key a value belongs to, and whitespace alone stops working. */
.children {
  margin-left: 6px;
  padding-left: 10px;
  border-left: thin solid rgba(var(--v-border-color), var(--v-border-opacity));
}

.child {
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
}

.legacy-block {
  margin: 0;
  font: inherit;
  white-space: pre-wrap;
  word-break: break-word;
}

/* Types, from the theme. `warning`, `success` and `info` are the three the app
   already uses for "a value, a good thing, a fact", and they are the three
   that were checked against both themes. */
.tok-str {
  color: rgb(var(--v-theme-success));
  word-break: break-word;
}

.tok-num {
  color: rgb(var(--v-theme-info));
}

.tok-bool {
  color: rgb(var(--v-theme-warning));
}

.tok-null {
  opacity: 0.55;
}

.tok-class {
  color: rgb(var(--v-theme-primary));
}

.tok-index,
.tok-punct,
.tok-other {
  opacity: 0.75;
}

.tok-prop {
  color: rgb(var(--v-theme-warning));
  white-space: nowrap;
}

/* Visibility reads as weight rather than as a second colour: the mark already
   says which it is, and a fourth colour in a key column is noise. */
.tok-public {
  opacity: 0.95;
}

.tok-protected {
  opacity: 0.8;
}

.tok-private {
  opacity: 0.65;
}
</style>
