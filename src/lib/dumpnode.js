/**
 * Reading the tree the debug bridge captures.
 *
 * The bridge walks a dumped value and emits typed nodes rather than a
 * formatted block (`debugbridge.rs`, `__stackvo_capture`). Everything here is
 * the other half of that: what a node says when it has one line to say it,
 * what it says when pasted into an issue, and whether it has anything more to
 * show than the one line.
 *
 * ## Why not just format it in the bridge
 *
 * Because a type cannot be recovered from text. `"[\n  0 => …"` and a string
 * that happens to contain a newline and two spaces are the same bytes, so a
 * pane parsing the block would fold, colour and count the wrong things on
 * exactly the values people dump when something has gone wrong. The bridge is
 * also the worst place to decide how wide a summary is: it runs inside the
 * request it is observing and has no idea how much room the row has.
 *
 * ## Nodes
 *
 *   {t:'null'}                            {t:'bool', v}       {t:'fn'}
 *   {t:'num', v}   |  {t:'num', s}        NAN and INF arrive as `s`
 *   {t:'str', v, len, cut?}               `v` is bounded, `len` is the truth
 *   {t:'arr', n, items:[{k, v}]}          `n` is the size, `items` the sample
 *   {t:'obj', class, n, items:[{k, v}]}
 *   {t:'deep'}                            the depth bound, not an empty value
 *   {t:'other', v}                        a resource, by its type name
 *
 * A plain string is also a node, and means an event written by the bridge as
 * it was before it captured trees. Those are on disk and inside queue workers
 * that have not been restarted, so every function here takes one — rendered as
 * the block it already is, which is exactly what the old pane did with it.
 */

/** How far a summary is allowed to run before the row would rather ellipsize. */
const SUMMARY_LIMIT = 160;

/** Beyond this a one-line string is worth expanding rather than truncating. */
const FLAT_STRING_LIMIT = 120;

/** Symfony's `VarDumper` marks, which is where a PHP developer has seen them. */
const MARK = { public: '+', protected: '#', private: '-' };

export function isLegacy(node) {
  return typeof node === 'string';
}

/**
 * Split an object property key into a name and a visibility.
 *
 * Casting an object to an array in PHP NUL-pads the keys of everything that is
 * not public: `"\0*\0size"` for protected, `"\0App\Models\User\0size"` for
 * private. The bridge replaces the NULs with `·` on the way out, because a NUL
 * inside a JSON string is legal and nothing downstream enjoys it.
 *
 * Without this the pane shows the whole padded key, which is how a screen ends
 * up reading `·App\Services\Observability\HealthCheckService·infrastructure`
 * for a property called `infrastructure`.
 */
export function propName(key) {
  const raw = String(key ?? '');
  const parts = /^·(.*?)·(.*)$/.exec(raw);
  if (!parts) return { name: raw, visibility: 'public', owner: '' };
  const owner = parts[1];
  return {
    name: parts[2],
    visibility: owner === '*' ? 'protected' : 'private',
    // Kept for the tooltip: a private property inherited from a parent class
    // belongs to that class, and which one is the answer sometimes.
    owner: owner === '*' ? '' : owner,
  };
}

export function mark(visibility) {
  return MARK[visibility] ?? '';
}

/** A captured string, with the bridge's truncation made visible. */
function quoted(node) {
  return `"${node.v ?? ''}${node.cut ? '…' : ''}"`;
}

/** How many of `n` this node is actually carrying. */
export function hidden(node) {
  if (!node || typeof node !== 'object') return 0;
  const shown = node.items?.length ?? 0;
  return Math.max(0, (node.n ?? shown) - shown);
}

/**
 * The one line the collapsed row shows.
 *
 * A type and a size, never the first line of a formatting. The old pane took
 * the first line of the block, which for every array in existence was `[` —
 * the row was there, took a row's worth of height, and carried nothing.
 */
export function summary(node) {
  if (isLegacy(node)) return oneLine(node.split('\n')[0]);
  if (!node || typeof node !== 'object') return String(node ?? '');

  switch (node.t) {
    case 'null':
      return 'null';
    case 'bool':
      return node.v ? 'true' : 'false';
    case 'num':
      return node.s ?? String(node.v);
    case 'str':
      return oneLine(quoted(node));
    case 'arr':
      return node.n ? `array:${node.n} [ … ]` : '[]';
    case 'obj':
      return node.n ? `${node.class} { … }` : `${node.class} {}`;
    case 'fn':
      return 'Closure';
    case 'deep':
      return '…';
    default:
      return String(node.v ?? node.t ?? '');
  }
}

/**
 * Flatten for a single-line context.
 *
 * A dumped string can hold newlines and tabs, and pasting them raw into a row
 * that is one line tall gets them rendered as spaces of unpredictable width.
 */
function oneLine(text) {
  const flat = String(text ?? '').replace(/\s+/g, ' ');
  return flat.length > SUMMARY_LIMIT ? `${flat.slice(0, SUMMARY_LIMIT)}…` : flat;
}

/**
 * Is the summary already the whole value?
 *
 * What decides whether a row gets a disclosure control at all. `dump(503)` was
 * a row that said `503`, opened onto a panel that said `503`, and charged a
 * click for it.
 */
export function isFlat(node) {
  if (isLegacy(node)) return !node.includes('\n') && node.length <= FLAT_STRING_LIMIT;
  if (!node || typeof node !== 'object') return true;

  switch (node.t) {
    case 'arr':
    case 'obj':
      return !(node.items?.length ?? 0) && !hidden(node);
    case 'str':
      // A long string has more to show even with nowhere to fold: the row can
      // only ellipsize it, and the expanded view wraps it.
      return (
        !node.cut &&
        !String(node.v ?? '').includes('\n') &&
        String(node.v ?? '').length <= FLAT_STRING_LIMIT
      );
    default:
      return true;
  }
}

/**
 * The whole value as text — what gets copied, and what search runs over.
 *
 * Deliberately close to the block the bridge used to emit: it is the shape a
 * PHP developer expects in a pasted dump, and it is what the copy button
 * promised before any of this changed.
 */
export function text(node, depth = 0) {
  if (isLegacy(node)) return node;
  if (!node || typeof node !== 'object') return String(node ?? '');

  const pad = '  '.repeat(depth + 1);
  const close = '  '.repeat(depth);

  switch (node.t) {
    case 'str':
      return quoted(node);
    case 'arr':
    case 'obj': {
      const head = node.t === 'arr' ? `array:${node.n ?? 0} [` : `${node.class} {`;
      const tail = node.t === 'arr' ? ']' : '}';
      const items = node.items ?? [];
      if (!items.length && !hidden(node)) return `${head}${tail}`;

      const lines = items.map((item) => {
        const key =
          node.t === 'arr'
            ? `${typeof item.k === 'number' ? item.k : JSON.stringify(item.k)} =>`
            : `${keyLabel(item.k)}:`;
        return `${pad}${key} ${text(item.v, depth + 1)}`;
      });
      const rest = hidden(node);
      if (rest) lines.push(`${pad}… ${rest} more`);
      return `${head}\n${lines.join(',\n')}\n${close}${tail}`;
    }
    default:
      return summary(node);
  }
}

function keyLabel(key) {
  const { name, visibility } = propName(key);
  return `${mark(visibility)}${name}`;
}
