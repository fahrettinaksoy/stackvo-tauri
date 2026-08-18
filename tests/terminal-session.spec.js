import { describe, it, expect, vi, beforeEach } from 'vitest';
import { defineComponent, h } from 'vue';
import { mount } from '@vue/test-utils';
import { readFileSync } from 'node:fs';

/**
 * The PTY session, which nothing had ever opened.
 *
 * `pty.rs` and its four commands shipped with the port and sat behind an
 * interface that never called them; `contracts:check` had been reporting the
 * wrappers as unused for ten months. So there is no regression to protect here
 * — these are the first assertions this surface has ever had, and they are
 * aimed at the three things that are wrong in a way nothing would report.
 */

const calls = [];
/** name -> handler, as registered by `listenAll`. */
let listeners = {};
let unlistened = 0;
let openResult = 'pty-1';

vi.mock('@/lib/ipc', () => ({
  StackvoError: class StackvoError extends Error {},
  api: {
    ptyOpen: (...args) => {
      calls.push(['ptyOpen', ...args]);
      return typeof openResult === 'function' ? openResult() : Promise.resolve(openResult);
    },
    ptyWrite: (...args) => (calls.push(['ptyWrite', ...args]), Promise.resolve()),
    ptyResize: (...args) => (calls.push(['ptyResize', ...args]), Promise.resolve()),
    ptyClose: (...args) => (calls.push(['ptyClose', ...args]), Promise.resolve()),
  },
}));

vi.mock('@/lib/events', () => ({
  listenAll: (names, handler) => {
    for (const name of names) listeners[name] = handler;
    return Promise.resolve(() => {
      unlistened += 1;
      listeners = {};
    });
  },
}));

const { useTerminal } = await import('@/composables/useTerminal');

/** Drive the composable inside a real component, so `onUnmounted` runs. */
function harness(onOutput = () => {}, onClosed = undefined) {
  let session;
  const wrapper = mount(
    defineComponent({
      setup() {
        session = useTerminal(onOutput, onClosed);
        return () => h('div');
      },
    })
  );
  return { session, wrapper };
}

/** Deliver an event the way Tauri would: to everyone listening. */
function emit(name, payload) {
  listeners[name]?.(name, payload);
}

beforeEach(() => {
  calls.length = 0;
  listeners = {};
  unlistened = 0;
  openResult = 'pty-1';
});

describe('a terminal session', () => {
  it('subscribes before it opens, so the first prompt is not lost', async () => {
    // A shell prints its prompt immediately. `pty_open` can emit
    // `terminal:output` before its own promise resolves, and a listener
    // attached afterwards drops that chunk — which renders as a terminal that
    // opened with no prompt, blamed on the shell.
    const seen = [];
    let resolveOpen;
    openResult = () => new Promise((r) => (resolveOpen = r));

    const { session } = harness((data) => seen.push(data));
    const opening = session.open({ kind: 'container', name: 'stackvo-shop' }, 80, 24);

    await Promise.resolve();
    expect(Object.keys(listeners), 'listeners are up before the command returns').toContain(
      'terminal:output'
    );

    resolveOpen('pty-1');
    await opening;

    emit('terminal:output', { sessionId: 'pty-1', data: '$ ' });
    expect(seen).toEqual(['$ ']);
  });

  it('ignores output belonging to another session', async () => {
    // Tauri events are global: every listener sees every `terminal:output`.
    // Without the filter, two open terminals interleave — and the newer one
    // looks like it is working, because the other's bytes arrive in the right
    // shape.
    const seen = [];
    const { session } = harness((data) => seen.push(data));
    await session.open({ kind: 'host', cwd: null }, 80, 24);

    emit('terminal:output', { sessionId: 'pty-OTHER', data: 'not mine' });
    emit('terminal:output', { sessionId: 'pty-1', data: 'mine' });

    expect(seen).toEqual(['mine']);
  });

  it('reports the exit and stops listening when the shell closes', async () => {
    const closed = vi.fn();
    const { session } = harness(() => {}, closed);
    await session.open({ kind: 'container', name: 'stackvo-shop' }, 80, 24);

    emit('terminal:closed', { sessionId: 'pty-1', exitCode: 130 });

    expect(closed).toHaveBeenCalledWith(130);
    expect(session.exitCode.value).toBe(130);
    expect(session.status.value).toBe('closed');
    expect(session.sessionId.value, 'the id is dropped with the shell').toBe(null);
    expect(unlistened, 'the subscriptions go with it').toBe(1);
  });

  it('closes the shell when the view goes away', async () => {
    // `pty_close` is the only thing that stops the process. Navigating away
    // from a project is the most ordinary way to leave one running, and
    // nothing else would ever reach it: the id lived in the component.
    const { session, wrapper } = harness();
    await session.open({ kind: 'container', name: 'stackvo-shop' }, 80, 24);
    calls.length = 0;

    wrapper.unmount();
    await Promise.resolve();

    expect(calls.map(([name]) => name)).toContain('ptyClose');
  });

  it('does not leave a shell behind when the view unmounts mid-open', async () => {
    // The same shape of leak `App.vue` was fixed for: `open()` awaits twice, and
    // a component can be gone by the time the command returns. A session opened
    // into a dead component has nobody left to close it.
    let resolveOpen;
    openResult = () => new Promise((r) => (resolveOpen = r));

    const { session, wrapper } = harness();
    const opening = session.open({ kind: 'container', name: 'stackvo-shop' }, 80, 24);

    await Promise.resolve();
    wrapper.unmount();
    calls.length = 0;
    resolveOpen('pty-1');
    await opening;

    expect(session.sessionId.value, 'no session is adopted after teardown').toBe(null);
    // The assertion that matters. A null id only proves this composable forgot
    // the shell; the shell itself is a process, and forgetting it is the leak
    // rather than the fix.
    expect(calls, 'the shell that arrived too late is closed').toContainEqual([
      'ptyClose',
      'pty-1',
    ]);
  });

  it('drops keystrokes rather than throwing when no shell is attached', async () => {
    const { session } = harness();

    await session.write('ls\r');
    await session.resize(120, 40);

    expect(calls, 'nothing is sent without a session').toEqual([]);
  });

  it('surfaces a refused open instead of looking idle', async () => {
    openResult = () =>
      Promise.reject(Object.assign(new Error('no such container'), { code: 'NOT_FOUND' }));

    const { session } = harness();
    await session.open({ kind: 'container', name: 'stackvo-gone' }, 80, 24);

    expect(session.status.value).toBe('error');
    expect(session.error.value).toBeTruthy();
    expect(session.sessionId.value).toBe(null);
  });

  /**
   * The payloads above are the ones `pty.rs` actually emits.
   *
   * They were not. Every test in this file invented its own event bodies in
   * `session_id` / `exit_code`, which is what the composable read — while the
   * backend has always built them with `json!({ "sessionId": …, "exitCode": …
   * })`. So `payload.session_id` was `undefined` on every real event, the
   * session filter rejected all of them, and the terminal pane opened a live
   * shell into a black box that never printed a character. Seven green tests,
   * one dead feature: they agreed with each other about a shape neither shared
   * with the program.
   *
   * Read from the Rust source, because that is the only place the two halves
   * meet — there is no schema between them, and a mock cannot disagree with
   * itself.
   */
  it('filters on the field name the backend actually emits', () => {
    const pty = readFileSync('src-tauri/src/pty.rs', 'utf8');

    for (const key of ['sessionId', 'data', 'exitCode']) {
      expect(pty, `pty.rs emits no "${key}" — this composable filters on it`).toContain(`"${key}"`);
    }

    // And the snake_case spelling this file used for ten months is not in it.
    expect(pty).not.toContain('"session_id"');
  });
});
