import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * `lib/ipc.js` is the whole boundary between the front end and Rust, and its
 * only real job is turning a rejected `invoke` back into something a `catch`
 * block can branch on. Getting that wrong is silent: the app keeps running and
 * every error reads as "Unknown error", which is exactly the class of failure
 * the contract's error codes exist to prevent.
 */

const invoke = vi.fn();

/**
 * `mockRejectedValue` stores an eagerly-created rejected promise, which Node
 * reports as unhandled before `call` gets a chance to await it. Throwing from
 * the implementation creates the rejection only when the call actually happens.
 */
const rejectWith = (value) =>
  invoke.mockImplementation(async () => {
    throw value;
  });
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args) => invoke(...args) }));

const { call, api, StackvoError } = await import('@/lib/ipc');

// Braced deliberately: `() => invoke.mockReset()` returns the mock, and Vitest
// treats a function returned from beforeEach as a teardown callback — so it
// would call the mock after every test, firing the throwing implementation into
// an unhandled rejection that fails the test it belongs to.
beforeEach(() => {
  invoke.mockReset();
});

describe('call', () => {
  it('passes the payload through untouched on success', async () => {
    // No { success, data } envelope to unwrap — that was the HTTP shape.
    invoke.mockResolvedValue({ name: 'shop', running: true });

    await expect(call('project_get', { name: 'shop' })).resolves.toEqual({
      name: 'shop',
      running: true,
    });
    expect(invoke).toHaveBeenCalledWith('project_get', { name: 'shop' });
  });

  it('defaults the argument object so a no-arg command is still valid', async () => {
    invoke.mockResolvedValue([]);
    await call('projects_list');
    expect(invoke).toHaveBeenCalledWith('projects_list', {});
  });

  it('rebuilds the Rust error struct into a branchable error', async () => {
    rejectWith({
      code: 'ENGINE_UNREACHABLE',
      message: 'Docker is not running',
      hint: 'Start Docker Desktop.',
      details: { endpoint: '/var/run/docker.sock' },
    });

    const error = await call('engine_status').catch((e) => e);

    expect(error).toBeInstanceOf(StackvoError);
    expect(error.code).toBe('ENGINE_UNREACHABLE');
    expect(error.message).toBe('Docker is not running');
    expect(error.hint).toBe('Start Docker Desktop.');
    expect(error.details).toEqual({ endpoint: '/var/run/docker.sock' });
    // Instances of Error, so existing catch blocks and Vue error handling work.
    expect(error).toBeInstanceOf(Error);
  });

  it('survives a panic or a missing command, which arrive as a bare string', async () => {
    rejectWith('command project_nope not found');

    const error = await call('project_nope').catch((e) => e);

    expect(error).toBeInstanceOf(StackvoError);
    expect(error.code).toBe('UNKNOWN');
    expect(error.message).toBe('command project_nope not found');
  });

  it('never leaves a caller without a message', async () => {
    rejectWith({ code: 'IO_ERROR' });
    const error = await call('anything').catch((e) => e);
    expect(error.message).toBe('Unknown error');
  });
});

describe('StackvoError shortcuts', () => {
  it('identifies the two states the web UI could never report', () => {
    const down = new StackvoError({ code: 'ENGINE_UNREACHABLE', message: 'x' });
    expect(down.isEngineDown).toBe(true);
    expect(down.needsWorkspace).toBe(false);

    const unset = new StackvoError({ code: 'NO_WORKSPACE', message: 'x' });
    expect(unset.needsWorkspace).toBe(true);
    expect(unset.isEngineDown).toBe(false);
  });

  it('falls back to UNKNOWN rather than an undefined code', () => {
    expect(new StackvoError({ message: 'x' }).code).toBe('UNKNOWN');
  });
});

describe('the api surface', () => {
  it('maps every wrapper to a snake_case command name', () => {
    // The Rust side only registers snake_case; a camelCase string here would
    // fail at runtime on a screen nobody opens during development.
    const offenders = [];
    for (const [name, fn] of Object.entries(api)) {
      invoke.mockReset();
      invoke.mockResolvedValue(null);
      try {
        fn('a', 'b', 'c');
      } catch {
        continue;
      }
      const command = invoke.mock.calls[0]?.[0];
      if (command && !/^[a-z][a-z0-9_]*$/.test(command)) offenders.push(`${name} -> ${command}`);
    }
    expect(offenders).toEqual([]);
  });

  it('names arguments, so Tauri can bind them by name', async () => {
    invoke.mockResolvedValue(null);
    await api.projectDelete('shop', true);
    expect(invoke).toHaveBeenCalledWith('project_delete', { name: 'shop', removeFiles: true });
  });
});

/**
 * The field that decides whether a hint is translated.
 *
 * `src-tauri/src/hints.rs` sends `hintKey` beside the English `hint`, and
 * `ErrorAlert` renders `errorHints.<hintKey>` from the locale. `StackvoError`
 * is the only path a real error takes to that component — the constructor
 * destructures the payload, so a field it does not name is a field the UI never
 * sees.
 *
 * It was omitted, and nothing noticed: every test of the rendering passed a
 * plain object literal, which has whatever fields the test wrote. The whole
 * translation would have been a no-op in the built app while the suite stayed
 * green. That is why this asserts on the class rather than on a shape.
 */
describe('the fields StackvoError carries across', () => {
  it('keeps hintKey, which is what makes a hint translatable', () => {
    const error = new StackvoError({
      code: 'ENGINE_UNREACHABLE',
      message: 'no socket',
      hint: 'Start Docker Desktop and try again.',
      hintKey: 'startDocker',
    });

    expect(error.hintKey).toBe('startDocker');
    expect(error.hint).toBe('Start Docker Desktop and try again.');
  });

  it('carries it through `call`, which is where a real one is built', async () => {
    rejectWith({
      code: 'NO_WORKSPACE',
      message: 'none selected',
      hint: 'Choose an empty folder.',
      hintKey: 'chooseWorkspace',
      details: { field: 'root' },
    });

    const error = await call('workspace_get').catch((e) => e);

    expect(error).toBeInstanceOf(StackvoError);
    expect(error.hintKey).toBe('chooseWorkspace');
    expect(error.details).toEqual({ field: 'root' });
  });

  it('leaves hintKey undefined when Rust sent none — the runtime-built hints', () => {
    const error = new StackvoError({
      code: 'GENERATE_FAILED',
      message: 'could not run git',
      hint: '`git` is not on PATH.',
    });

    expect(error.hintKey).toBeUndefined();
    expect(error.hint).toBe('`git` is not on PATH.');
  });
});

/**
 * The guard three separate places needed before it was made one function.
 */
describe('asList', () => {
  it('passes an array through untouched', async () => {
    const { asList } = await import('@/lib/ipc');
    const list = [{ id: 'mysql' }];
    expect(asList(list)).toBe(list);
  });

  it('turns everything the boundary can answer badly into an empty list', async () => {
    const { asList } = await import('@/lib/ipc');
    for (const bad of [null, undefined, 0, 'nope', { projects: [] }, true]) {
      expect(asList(bad)).toEqual([]);
    }
  });
});
