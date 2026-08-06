import { describe, expect, it } from 'vitest';
import { hidden, isFlat, propName, summary, text } from './dumpnode';

/**
 * The three things the pane got wrong before the bridge captured trees, plus
 * the compatibility that keeps events written by the old one readable.
 */
describe('summary', () => {
  /**
   * The regression this whole change started from. The bridge used to render
   * an array to a block beginning with `[`, and the row took the first line —
   * so every array in the pane was a row whose entire content was one bracket.
   */
  it('says what an array is and how big, not what its first line looks like', () => {
    const node = { t: 'arr', n: 8, items: [{ k: 0, v: { t: 'num', v: 1 } }] };
    expect(summary(node)).toBe('array:8 [ … ]');
    expect(summary({ t: 'arr', n: 0, items: [] })).toBe('[]');
  });

  it('names the class of an object', () => {
    const node = { t: 'obj', class: 'App\\Models\\User', n: 3, items: [] };
    expect(summary(node)).toBe('App\\Models\\User { … }');
  });

  /** A row is one line tall; a captured string is under no such obligation. */
  it('flattens whitespace so a multi-line string cannot break the row', () => {
    const node = { t: 'str', v: 'first\n  second', len: 14 };
    expect(summary(node)).toBe('"first second"');
  });

  it('marks a string the bridge truncated', () => {
    expect(summary({ t: 'str', v: 'abc', len: 900, cut: true })).toBe('"abc…"');
  });

  it('carries a float json cannot', () => {
    expect(summary({ t: 'num', s: 'INF' })).toBe('INF');
  });
});

describe('isFlat', () => {
  /**
   * `dump(503)` was a row saying `503` that opened onto a panel saying `503`.
   * A scalar has nothing behind it, so it gets no disclosure.
   */
  it('is true for a scalar, which is already its own summary', () => {
    expect(isFlat({ t: 'num', v: 503 })).toBe(true);
    expect(isFlat({ t: 'bool', v: false })).toBe(true);
    expect(isFlat({ t: 'null' })).toBe(true);
    expect(isFlat({ t: 'str', v: 'HealthCheckController invoked', len: 29 })).toBe(true);
  });

  it('is false for anything with contents', () => {
    expect(isFlat({ t: 'arr', n: 2, items: [{ k: 0, v: { t: 'null' } }] })).toBe(false);
    expect(isFlat({ t: 'obj', class: 'X', n: 1, items: [{ k: 'a', v: { t: 'null' } }] })).toBe(
      false
    );
  });

  /** An empty container has contents to show only in the sense that it has none. */
  it('is true for an empty container', () => {
    expect(isFlat({ t: 'arr', n: 0, items: [] })).toBe(true);
    expect(isFlat({ t: 'obj', class: 'X', n: 0, items: [] })).toBe(true);
  });

  /** The row can only ellipsize a long string; expanded, it wraps. */
  it('is false for a string the row could only truncate', () => {
    expect(isFlat({ t: 'str', v: 'x'.repeat(400), len: 400 })).toBe(false);
    expect(isFlat({ t: 'str', v: 'a\nb', len: 3 })).toBe(false);
    expect(isFlat({ t: 'str', v: 'abc', len: 900, cut: true })).toBe(false);
  });
});

/**
 * Casting an object to an array in PHP NUL-pads every non-public key, and the
 * bridge swaps the NULs for `·`. Left alone, a property called `infrastructure`
 * is shown as `·App\Services\Observability\HealthCheckService·infrastructure`.
 */
describe('propName', () => {
  it('reads a protected property', () => {
    expect(propName('·*·connection')).toEqual({
      name: 'connection',
      visibility: 'protected',
      owner: '',
    });
  });

  it('reads a private property and keeps the class that owns it', () => {
    expect(propName('·App\\Models\\User·secret')).toEqual({
      name: 'secret',
      visibility: 'private',
      owner: 'App\\Models\\User',
    });
  });

  it('leaves a public property alone', () => {
    expect(propName('id')).toEqual({ name: 'id', visibility: 'public', owner: '' });
  });
});

/**
 * The bridge stops at fifty entries per level. The pane can only say so if the
 * real size came with the sample.
 */
describe('hidden', () => {
  it('is the difference between the size and the sample', () => {
    expect(
      hidden({ t: 'arr', n: 120, items: new Array(50).fill({ k: 0, v: { t: 'null' } }) })
    ).toBe(70);
    expect(hidden({ t: 'arr', n: 2, items: [1, 2] })).toBe(0);
  });
});

describe('text', () => {
  it('renders a tree as the block a pasted dump is expected to look like', () => {
    const node = {
      t: 'obj',
      class: 'App\\Models\\User',
      n: 2,
      items: [
        { k: 'id', v: { t: 'num', v: 420 } },
        { k: '·*·connection', v: { t: 'str', v: 'mysql', len: 5 } },
      ],
    };
    expect(text(node)).toBe('App\\Models\\User {\n  +id: 420,\n  #connection: "mysql"\n}');
  });

  it('says what it is not showing', () => {
    const node = { t: 'arr', n: 3, items: [{ k: 0, v: { t: 'num', v: 1 } }] };
    expect(text(node)).toBe('array:3 [\n  0 => 1,\n  … 2 more\n]');
  });
});

/**
 * A queue worker started before this app updated keeps the old bridge loaded
 * for as long as it lives, and the events it already wrote are on disk. Those
 * values are formatted strings, and they still have to render.
 */
describe('a value from the older bridge', () => {
  const block = 'App\\Models\\User {\n  +id: 420\n}';

  it('summarises as its first line, which is what the old pane showed', () => {
    expect(summary(block)).toBe('App\\Models\\User {');
  });

  it('is not flat, because there is more of it below the first line', () => {
    expect(isFlat(block)).toBe(false);
    expect(isFlat('503')).toBe(true);
  });

  it('copies as itself: it is already a rendering', () => {
    expect(text(block)).toBe(block);
  });
});
