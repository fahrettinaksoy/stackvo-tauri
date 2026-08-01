import { describe, it, expect, vi } from 'vitest';

/**
 * The projects table opens its groups on first sight and then leaves them
 * alone. The table keeps that state internally — no prop seeds it, nothing is
 * exposed to reach it — so the only handle is the `toggleGroup` handed to the
 * group-header slot, which means the open has to be asked for during render.
 *
 * Two things have to hold, and neither is obvious from reading the call site:
 * it fires once per group, so a group the user collapsed stays collapsed; and
 * it is deferred, because toggling during render mutates state that same
 * render is reading.
 */
function makeOpener(nextTick) {
  const seen = new Set();
  return function openByDefault(item, isGroupOpen, toggleGroup) {
    const open = isGroupOpen(item);
    if (!open && !seen.has(item.id)) {
      seen.add(item.id);
      nextTick(() => toggleGroup(item));
    }
    return open;
  };
}

describe('opening table groups by default', () => {
  it('asks once, after the render that noticed', () => {
    const deferred = [];
    const open = makeOpener((fn) => deferred.push(fn));
    const toggle = vi.fn();
    const group = { id: 'ajans.loc' };

    expect(open(group, () => false, toggle)).toBe(false);
    // Deferred: nothing has been toggled during the render itself.
    expect(toggle).not.toHaveBeenCalled();
    deferred.forEach((fn) => fn());
    expect(toggle).toHaveBeenCalledTimes(1);
  });

  it('leaves a group the user collapsed alone', () => {
    const deferred = [];
    const open = makeOpener((fn) => deferred.push(fn));
    const toggle = vi.fn();
    const group = { id: 'ajans.loc' };

    open(group, () => false, toggle);
    deferred.forEach((fn) => fn());
    deferred.length = 0;

    // The user collapses it; the next renders must not reopen it.
    open(group, () => false, toggle);
    open(group, () => false, toggle);
    expect(deferred).toHaveLength(0);
    expect(toggle).toHaveBeenCalledTimes(1);
  });

  it('does nothing at all for a group already open', () => {
    const deferred = [];
    const open = makeOpener((fn) => deferred.push(fn));
    const toggle = vi.fn();

    expect(open({ id: 'x' }, () => true, toggle)).toBe(true);
    expect(deferred).toHaveLength(0);
    expect(toggle).not.toHaveBeenCalled();
  });
});
