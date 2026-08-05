import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import DumpValue from './DumpValue.vue';

/**
 * The tree renderer, and mostly one question about it: does the recursion
 * work?
 *
 * A single-file component referring to itself in its own template resolves by
 * filename, with no import and nothing to fail at build time — a rename or a
 * `defineOptions({ name })` that disagrees with the file turns every nested
 * value into a silent blank. The bounds tests are here for the same reason: a
 * dump the bridge truncated must say so, and "shows fewer entries than it has"
 * is indistinguishable from "shows all of them" unless it is checked.
 */
const nested = {
  t: 'obj',
  class: 'App\\Models\\User',
  n: 2,
  items: [
    { k: 'id', v: { t: 'num', v: 420 } },
    {
      k: '·*·connection',
      v: {
        t: 'arr',
        n: 1,
        items: [{ k: 'driver', v: { t: 'str', v: 'mysql', len: 5 } }],
      },
    },
  ],
};

describe('DumpValue', () => {
  it('renders a value nested inside another one', () => {
    const w = mount(DumpValue, { props: { node: nested } });
    // Reached only through the component rendering itself twice.
    expect(w.text()).toContain('"mysql"');
    expect(w.text()).toContain('App\\Models\\User {');
  });

  it('names a protected property without the padding PHP put on the key', () => {
    const w = mount(DumpValue, { props: { node: nested } });
    expect(w.text()).toContain('#connection:');
    expect(w.text()).not.toContain('·');
  });

  it('folds and unfolds a branch', async () => {
    const w = mount(DumpValue, { props: { node: nested } });
    expect(w.text()).toContain('+id:');

    await w.find('button.twist').trigger('click');
    expect(w.text()).not.toContain('+id:');
    // Folded, the head still says what is in there.
    expect(w.text()).toContain('App\\Models\\User {');
  });

  /**
   * Two levels open, deeper closed. A dumped model is its own scalars plus a
   * graph; the scalars are what somebody dumped it for.
   */
  it('starts closed below the second level', () => {
    const deep = { ...nested, items: [{ k: 'a', v: { t: 'arr', n: 1, items: nested.items } }] };
    const w = mount(DumpValue, { props: { node: deep, depth: 1 } });
    expect(w.text()).not.toContain('+id:');
  });

  it('says how many entries the bridge did not send', () => {
    const w = mount(DumpValue, {
      props: { node: { t: 'arr', n: 120, items: [{ k: 0, v: { t: 'num', v: 1 } }] } },
    });
    expect(w.text()).toContain('119 more');
  });

  it('renders a value from the older bridge as the block it already is', () => {
    const w = mount(DumpValue, { props: { node: 'App\\Models\\User {\n  +id: 420\n}' } });
    expect(w.find('pre').exists()).toBe(true);
    expect(w.text()).toContain('+id: 420');
  });
});
