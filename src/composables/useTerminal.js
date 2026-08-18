import { ref, shallowRef, onUnmounted } from 'vue';
import { api, StackvoError } from '@/lib/ipc';
import { listenAll } from '@/lib/events';

/**
 * One PTY session: open it, route its output, take it down.
 *
 * The Rust half of this has existed since the port — `pty.rs`, 501 lines and
 * four commands, with `terminal:ready` / `:output` / `:closed` written into the
 * contract. Nothing in the interface had ever called it. `contracts:check`
 * reported the four wrappers as unused for ten months and the readiness report
 * filed them as "wrappers no view calls yet", which is how a shipped feature
 * stayed unreachable while the competitive analysis listed "container and host
 * PTY" as something this app has.
 *
 * The DOM half deliberately lives in the component. xterm owns a canvas, a
 * focus model and a resize observer, none of which belong in a composable, and
 * the split is what lets this file be tested without a renderer.
 *
 * ## Filtering by session
 *
 * Tauri events are global: every listener sees every `terminal:output`, whoever
 * opened it. So each payload is matched against this session's id and dropped
 * otherwise. Without that, two open terminals interleave each other's bytes —
 * and the second one to open would look like it was working, because the first
 * one's output arrives in the right shape.
 *
 * @param {(data: string) => void} onOutput — called with each chunk, in order.
 * @param {(exitCode: number) => void} [onClosed]
 */
export function useTerminal(onOutput, onClosed) {
  /** Non-null while a shell is attached. */
  const sessionId = shallowRef(null);
  const status = ref('idle');
  const error = ref(null);
  const exitCode = ref(null);

  /** The `listenAll` teardown, held so `close` can drop the subscriptions. */
  let unlisten = null;

  /**
   * True once this composable has been torn down.
   *
   * `open()` awaits twice — the listeners, then the command — and a view can be
   * left in between. The same class of leak `App.vue` was fixed for: the guard
   * is what stops a session being opened into a component that no longer
   * exists, which would leave a shell running with nobody able to close it.
   */
  let disposed = false;

  async function open(target, cols, rows) {
    if (sessionId.value || disposed) return null;

    status.value = 'opening';
    error.value = null;
    exitCode.value = null;

    try {
      // Subscribed *before* the command, not after. `pty_open` can emit
      // `terminal:output` before its own promise resolves — a shell that
      // prints a prompt immediately does exactly that — and a listener
      // attached afterwards silently loses the first chunk, which reads as a
      // terminal that opened without a prompt.
      unlisten = await listenAll(
        ['terminal:ready', 'terminal:output', 'terminal:closed'],
        (name, payload) => {
          // `sessionId`, not `session_id`. `pty.rs` emits camelCase — it builds
          // the payloads with `json!({ "sessionId": …, "exitCode": … })` — and
          // this read snake_case, so `payload.session_id` was `undefined` on
          // every event and the comparison below rejected all of them. Nothing
          // ever reached xterm: the pane opened a real shell, attached to it,
          // sent keystrokes to it, and drew an empty black box, because the
          // shell's answers were all dropped here. The tests passed because
          // they made up their own payloads in the shape this file expected
          // rather than the shape the backend sends.
          if (!payload || payload.sessionId !== sessionId.value) return;

          if (name === 'terminal:output') onOutput(payload.data ?? '');
          else if (name === 'terminal:ready') status.value = 'open';
          else if (name === 'terminal:closed') {
            exitCode.value = payload.exitCode ?? null;
            status.value = 'closed';
            onClosed?.(payload.exitCode ?? 0);
            stopListening();
            sessionId.value = null;
          }
        }
      );

      if (disposed) {
        stopListening();
        return null;
      }

      const id = await api.ptyOpen(target, cols, rows);

      // Checked again, on the far side of the await. The guard above only
      // covers a teardown during the *subscribe*; a view can just as easily go
      // away while `pty_open` is in flight, and the shell it returns is then
      // owned by a component that no longer exists — `onUnmounted` has already
      // run, so nothing will ever close it. Closing it here is the only
      // remaining chance.
      if (disposed) {
        stopListening();
        await api.ptyClose(id).catch(() => {});
        return null;
      }

      sessionId.value = id;

      // `terminal:ready` normally moves this on; setting it here as well
      // covers a shell that produced output before the event landed, which
      // would otherwise leave the pane saying "opening" over a live prompt.
      if (status.value === 'opening') status.value = 'open';
      return id;
    } catch (e) {
      error.value =
        e instanceof StackvoError ? e : new StackvoError({ message: String(e?.message ?? e) });
      status.value = 'error';
      stopListening();
      sessionId.value = null;
      return null;
    }
  }

  function stopListening() {
    unlisten?.();
    unlisten = null;
  }

  /** Keystrokes. Dropped rather than queued when no shell is attached. */
  async function write(data) {
    if (!sessionId.value) return;
    await api.ptyWrite(sessionId.value, data).catch(() => {});
  }

  async function resize(cols, rows) {
    if (!sessionId.value) return;
    await api.ptyResize(sessionId.value, cols, rows).catch(() => {});
  }

  async function close() {
    const id = sessionId.value;
    stopListening();
    sessionId.value = null;
    status.value = 'closed';
    if (id) await api.ptyClose(id).catch(() => {});
  }

  // A shell outlives its pane otherwise. `pty_close` is the only thing that
  // stops the process, and a view navigated away from is the most ordinary way
  // to leave one behind.
  onUnmounted(() => {
    disposed = true;
    close();
  });

  return { sessionId, status, error, exitCode, open, write, resize, close };
}
