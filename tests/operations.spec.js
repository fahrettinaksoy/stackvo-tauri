import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

/**
 * The operations store is the one piece of front-end state with real logic in
 * it: it turns a stream of events into addressable operations, and its busy
 * flags are what disable the buttons. The web UI had none of this — a build was
 * a single HTTP request held open for ten minutes — so there is no prior
 * behaviour to fall back on when it goes wrong.
 */

const listeners = [];
vi.mock('@/lib/events', async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...actual,
    listenAll: async (names, handler) => {
      listeners.push({ names, handler });
      return () => listeners.splice(listeners.indexOf(handler), 1);
    },
  };
});
vi.mock('@/lib/notify', () => ({ notify: vi.fn(), shouldNotify: () => false }));

const { useOperationsStore } = await import('@/stores/operations');

/** Feed an event to the store the way the Tauri listener would. */
function emit(name, payload) {
  for (const l of listeners) l.handler(name, payload);
}

beforeEach(() => {
  setActivePinia(createPinia());
  listeners.length = 0;
});

describe('busy flags', () => {
  it('are set by a lifecycle event and cleared by its terminal partner', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('project:starting', { project: 'shop' });
    expect(ops.isBusy('shop')).toBe(true);

    emit('project:started', { project: 'shop' });
    expect(ops.isBusy('shop')).toBe(false);
  });

  it('are per subject, so one project does not disable another', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('project:starting', { project: 'shop' });
    expect(ops.isBusy('shop')).toBe(true);
    expect(ops.isBusy('blog')).toBe(false);
  });

  it('fall back to the stack when an event names no subject', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('compose:progress', { operationId: 'up-1', line: 'pulling' });
    expect(ops.isBusy('stack')).toBe(true);
  });
});

describe('operations', () => {
  it('collects streamed lines under one addressable operation', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('build:start', { operationId: 'build-1', project: 'shop' });
    emit('build:progress', { operationId: 'build-1', project: 'shop', line: 'step 1' });
    emit('build:progress', { operationId: 'build-1', project: 'shop', line: 'step 2' });

    const op = ops.operations['build-1'];
    expect(op.state).toBe('running');
    expect(op.subject).toBe('shop');
    expect(op.lines).toEqual(['step 1', 'step 2']);
    expect(ops.active).toHaveLength(1);
  });

  it('bounds the buffer, because a Docker build emits thousands of lines', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('build:start', { operationId: 'b', project: 'shop' });
    for (let i = 0; i < 600; i++) {
      emit('build:progress', { operationId: 'b', project: 'shop', line: `line ${i}` });
    }

    const { lines } = ops.operations['b'];
    expect(lines).toHaveLength(500);
    // The tail is kept — that is the part anyone reads.
    expect(lines.at(-1)).toBe('line 599');
    expect(lines[0]).toBe('line 100');
  });

  it('treats `built` as a stage boundary, not the end of the build', async () => {
    // project_build is generate → build image → recreate container. Clearing
    // the busy flag at `built` would re-enable the buttons while the container
    // is still being recreated.
    const ops = useOperationsStore();
    await ops.bind();

    emit('build:start', { operationId: 'b', project: 'shop' });
    emit('build:built', { operationId: 'b', project: 'shop' });

    expect(ops.isBusy('shop')).toBe(true);
    expect(ops.operations['b'].state).toBe('running');
    expect(ops.operations['b'].lines.at(-1)).toContain('recreating');
  });

  it('marks failure when the payload says so, not only on an error event', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('build:start', { operationId: 'b', project: 'shop' });
    emit('build:success', { operationId: 'b', project: 'shop', success: false, error: 'no space' });

    const op = ops.operations['b'];
    expect(op.state).toBe('failed');
    expect(op.error).toBe('no space');
    expect(ops.isBusy('shop')).toBe(false);
  });

  it('records a duration even when the payload omits one', async () => {
    const ops = useOperationsStore();
    await ops.bind();

    emit('generate:start', { operationId: 'g', subject: 'projects' });
    emit('generate:done', { operationId: 'g', subject: 'projects' });

    expect(ops.operations['g'].durationMs).toBeGreaterThanOrEqual(0);
    expect(ops.operations['g'].state).toBe('done');
  });

  it('binds its listeners once, however often bind is called', async () => {
    const ops = useOperationsStore();
    await ops.bind();
    await ops.bind();

    emit('build:start', { operationId: 'b', project: 'shop' });
    emit('build:progress', { operationId: 'b', project: 'shop', line: 'once' });

    // A second binding would append every line twice.
    expect(ops.operations['b'].lines).toEqual(['once']);
  });
});
