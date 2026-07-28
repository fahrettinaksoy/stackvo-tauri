import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { listenAll, EVENTS } from '@/lib/events';
import { notify, shouldNotify } from '@/lib/notify';

const MAX_LINES = 500;

/**
 * Long-running operations and their streamed output.
 *
 * The web UI had nowhere to put this: a build ran inside a single HTTP request
 * held open for up to ten minutes, so there was no operation to track and no
 * output until it ended. Here every build, generate and compose run is an
 * addressable object with live lines attached.
 */
export const useOperationsStore = defineStore('operations', () => {
  /** operationId -> { id, kind, subject, state, lines, error, startedAt, durationMs } */
  const operations = ref({});
  /** Which subjects have an action in flight, so buttons can disable precisely. */
  const busy = ref({});

  let teardown = null;

  const active = computed(() =>
    Object.values(operations.value).filter((op) => op.state === 'running')
  );
  const latest = computed(() => {
    const all = Object.values(operations.value);
    return all.length ? all[all.length - 1] : null;
  });

  function isBusy(subject) {
    return !!busy.value[subject];
  }

  function markBusy(subject, value) {
    if (value) busy.value[subject] = true;
    else delete busy.value[subject];
  }

  function ensure(id, kind, subject) {
    if (!operations.value[id]) {
      operations.value[id] = {
        id,
        kind,
        subject,
        state: 'running',
        lines: [],
        error: null,
        startedAt: Date.now(),
        durationMs: null,
      };
    }
    return operations.value[id];
  }

  function appendLine(id, kind, subject, line) {
    const op = ensure(id, kind, subject);

    // Output after a finish means the operation was not over: one id can span
    // several stages, and only the last of them ends it. Enabling a service
    // generates the compose files (`generate:done`) and then brings the profile
    // up — so the panel used to sit on "done, 2.3s" while docker was still
    // pulling an image.
    if (op.state !== 'running') {
      op.state = 'running';
      op.durationMs = null;
      op.error = null;
    }

    op.lines.push(line);
    // Bounded: a Docker build emits thousands of lines and the panel only ever
    // shows the tail.
    if (op.lines.length > MAX_LINES) op.lines.splice(0, op.lines.length - MAX_LINES);
  }

  function finish(id, kind, subject, payload) {
    const op = ensure(id, kind, subject);
    op.state = payload?.success === false ? 'failed' : 'done';
    op.error = payload?.error ?? null;
    op.durationMs = payload?.durationMs ?? Date.now() - op.startedAt;
    markBusy(subject, false);
  }

  /** Wire up every event stream once, at app boot. */
  async function bind() {
    if (teardown) return;

    const names = [
      ...EVENTS.build,
      ...EVENTS.generate,
      ...EVENTS.compose,
      ...EVENTS.project,
      ...EVENTS.service,
    ];

    teardown = await listenAll(names, (name, payload) => {
      const [domain, verb] = name.split(':');
      const subject = payload?.subject ?? payload?.project ?? payload?.service ?? 'stack';
      const id = payload?.operationId;

      // Lifecycle events (starting/stopping/…) carry no operationId; they only
      // drive the per-subject busy flag.
      if (!id) {
        if (verb.endsWith('ing')) markBusy(subject, true);
        else markBusy(subject, false);
        return;
      }

      if (verb === 'start') {
        ensure(id, domain, subject);
        markBusy(subject, true);
        return;
      }
      if (verb === 'progress') {
        appendLine(id, domain, subject, payload.line ?? '');
        markBusy(subject, true);
        return;
      }
      // `built` ends a stage of project_build, not the whole operation, so it
      // must not clear the busy flag or mark the operation complete.
      if (verb === 'built') {
        appendLine(id, domain, subject, '— image built, recreating container —');
        return;
      }

      // Anything else carrying an operation id finishes it.
      //
      // This used to be a list of four verbs — built/done/success/error — which
      // missed `service:enabled`, the finished event of enabling a service. The
      // row's spinner is cleared by the finish, so a service could sit
      // "enabling" forever while its container was already up. The rule is not
      // which word was chosen: a runner emits exactly one finished event per
      // operation, whatever it is named.
      finish(id, domain, subject, payload);

      // Only worth interrupting someone about if the window is not in front.
      if (shouldNotify(name) && document.visibilityState !== 'visible') {
        const failed = verb === 'error' || payload?.success === false;
        notify(
          failed ? `${domain} failed — ${subject}` : `${domain} finished — ${subject}`,
          payload?.error ?? ''
        );
      }
    });
  }

  function unbind() {
    if (teardown) teardown();
    teardown = null;
  }

  function clear() {
    operations.value = {};
  }

  return { operations, busy, active, latest, isBusy, markBusy, bind, unbind, clear };
});
