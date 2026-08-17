import { describe, it, expect } from 'vitest';
import {
  sessionPayload,
  sessionIdOf,
  unwrap,
  asyncScript,
  Session,
  WebDriverError,
} from './driver/webdriver.js';
import { whyNotHere, binaryPath } from './driver/launch.js';

/**
 * The half of the driver suite that can be watched from here.
 *
 * `tauri-driver` does not run on macOS, which is where this repository is
 * developed, so `tests/driver/boot.driver.js` is a file its author can write
 * and never see pass (§3 #12). That is a real hazard and it has a name: code
 * whose first execution is in CI is code whose first execution is in front of
 * everybody.
 *
 * So the client was split. Building a request, reading a response envelope,
 * wrapping a script in the callback protocol and resolving a binary path are
 * all pure, and all of them are places a silent mistake would survive to CI
 * and then look like an environment problem — a wrong capability key reads as
 * "the driver ignored the application", a mis-read error envelope reads as
 * "the app crashed". Those are asserted here, on every platform, in the same
 * runner as the other 700 specs.
 *
 * What is deliberately *not* faked: there is no fake WebKitWebDriver. A stub
 * that agreed with this client about the protocol would prove the two halves
 * of one opinion agree, which `tui_probe.rs` already paid this repository the
 * lesson for. The live protocol is CI's question; the shapes are this file's.
 */

describe('the capability payload tauri-driver reads', () => {
  it('names the application under the vendor-prefixed key, in both shapes', () => {
    const payload = sessionPayload({ application: '/opt/stackvo' });

    expect(payload.capabilities.alwaysMatch['tauri:options']).toEqual({
      application: '/opt/stackvo',
    });
    // The JSON-Wire mirror. Sent because a proxy in front of a native driver
    // is not this repository's to date-stamp; dropping it is a change that
    // can only be tested by breaking CI.
    expect(payload.desiredCapabilities['tauri:options'].application).toBe('/opt/stackvo');
    expect(payload.capabilities.firstMatch).toEqual([{}]);
  });

  it('omits args rather than sending an empty list', () => {
    // `cli.rs` makes argv a surface. An empty array is a claim that the app
    // was given arguments and got none, which is a different sentence.
    expect(
      sessionPayload({ application: '/opt/x' }).capabilities.alwaysMatch['tauri:options']
    ).not.toHaveProperty('args');
    expect(
      sessionPayload({ application: '/opt/x', args: [] }).capabilities.alwaysMatch['tauri:options']
    ).not.toHaveProperty('args');
  });

  it('copies the args it is given, so a caller cannot mutate a sent payload', () => {
    const args = ['--project', 'shop'];
    const payload = sessionPayload({ application: '/opt/x', args });
    args.push('--oops');
    expect(payload.capabilities.alwaysMatch['tauri:options'].args).toEqual(['--project', 'shop']);
  });
});

describe('reading the session id back', () => {
  it('finds it where W3C puts it', () => {
    expect(sessionIdOf({ value: { sessionId: 'abc' } })).toBe('abc');
  });

  it('finds it where a JSON-Wire driver puts it', () => {
    // Not politeness to old drivers: a session id this client fails to read is
    // a session it cannot close, and an unclosed session leaves the
    // application process running on the runner.
    expect(sessionIdOf({ sessionId: 'abc' })).toBe('abc');
  });

  it('reports nothing rather than an empty string', () => {
    expect(sessionIdOf({ value: { sessionId: '' } })).toBeNull();
    expect(sessionIdOf({ value: {} })).toBeNull();
    expect(sessionIdOf(null)).toBeNull();
  });
});

describe('the response envelope', () => {
  it('returns the value', () => {
    expect(unwrap(200, { value: { root: '/w' } })).toEqual({ root: '/w' });
  });

  it('keeps falsy values, and only collapses a missing one', () => {
    // The bug this exists to stop: a script returning 0 reading as "the driver
    // answered nothing", which gets diagnosed as flakiness rather than as a
    // truthiness check in a client.
    expect(unwrap(200, { value: 0 })).toBe(0);
    expect(unwrap(200, { value: false })).toBe(false);
    expect(unwrap(200, { value: '' })).toBe('');
    expect(unwrap(200, { value: null })).toBeNull();
    expect(unwrap(200, {})).toBeNull();
  });

  it('throws the driver’s own error code, not an HTTP status', () => {
    let thrown;
    try {
      unwrap(404, { value: { error: 'no such element', message: 'nothing matched #app' } });
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(WebDriverError);
    // `error` is the field a caller would branch on; the message is prose.
    expect(thrown.error).toBe('no such element');
    expect(thrown.message).toContain('nothing matched #app');
  });

  it('still throws when the body is unreadable', () => {
    // A driver that died mid-response answers with no JSON at all. Returning
    // null there would hand the test a value it would then assert against.
    expect(() => unwrap(500, null)).toThrow(WebDriverError);
  });
});

describe('the async script wrapper', () => {
  /** Run the generated body the way a WebDriver executor does: as a function whose last argument is the callback. */
  const run = (expression, args = []) =>
    new Promise((done) => {
      // The executor, reproduced. `asyncScript` produces source text for a
      // function body by construction — this is the only way to find out
      // whether that text does what it says without a driver to run it.
      new Function(asyncScript(expression))(...args, done);
    });

  it('boxes a resolved value', async () => {
    await expect(run('() => 42')).resolves.toEqual({ ok: true, value: 42 });
  });

  it('boxes undefined as null, because JSON has no undefined', async () => {
    await expect(run('() => undefined')).resolves.toEqual({ ok: true, value: null });
  });

  it('passes arguments through, minus the callback', async () => {
    await expect(run('(a, b) => a + b', [2, 3])).resolves.toEqual({ ok: true, value: 5 });
  });

  it('boxes a rejection with a plain object intact', async () => {
    // The reason the box exists. A rejected `invoke` carries a StackvoError —
    // `{code, message, hintKey}` (ADR 0004) — and letting it reach the driver
    // would flatten it to the string "javascript error", losing the one field
    // `boot.driver.js` is there to look at.
    const boxed = await run(`() => Promise.reject({ code: 'NOT_FOUND', message: 'gone' })`);
    expect(boxed).toEqual({ ok: false, error: { code: 'NOT_FOUND', message: 'gone' } });
  });

  it('reduces a thrown Error to something that survives JSON', async () => {
    const boxed = await run(`() => { throw new TypeError('bad'); }`);
    expect(boxed.ok).toBe(false);
    expect(boxed.error).toEqual({ name: 'TypeError', message: 'bad' });
  });

  it('boxes a synchronous throw as well as a rejection', async () => {
    const boxed = await run(`() => { throw { code: 'CONFLICT', message: 'busy' }; }`);
    expect(boxed).toEqual({ ok: false, error: { code: 'CONFLICT', message: 'busy' } });
  });
});

describe('the session talks to the endpoints it says it does', () => {
  /** A fetch that records and answers, so request shape can be asserted without a driver. */
  const recorder = (answer = { value: null }) => {
    const calls = [];
    const fetchImpl = async (url, init) => {
      calls.push({ url, method: init.method, body: init.body ? JSON.parse(init.body) : undefined });
      return { status: 200, json: async () => answer };
    };
    return { calls, fetchImpl };
  };

  it('opens a session and keeps the id it was given', async () => {
    const { calls, fetchImpl } = recorder({ value: { sessionId: 's1' } });
    const session = await Session.open(
      'http://127.0.0.1:4444/',
      { application: '/opt/x' },
      { fetchImpl }
    );

    expect(calls[0].url).toBe('http://127.0.0.1:4444/session');
    expect(calls[0].method).toBe('POST');
    expect(session.id).toBe('s1');
    // The trailing slash is not carried into every subsequent path: `//session`
    // is a 404 on some drivers and a redirect on others.
    expect(session.base).toBe('http://127.0.0.1:4444');
  });

  it('refuses a session it cannot close', async () => {
    const { fetchImpl } = recorder({ value: {} });
    await expect(
      Session.open('http://127.0.0.1:4444', { application: '/opt/x' }, { fetchImpl })
    ).rejects.toThrow(/session id/);
  });

  it('sends each verb to its W3C path', async () => {
    const { calls, fetchImpl } = recorder();
    const session = new Session('http://d', 's1', { fetchImpl });

    await session.title();
    await session.timeouts({ script: 30_000 });
    await session.execute('return 1', []);
    await session.evaluate('() => 1');
    await session.close();

    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      'GET http://d/session/s1/title',
      'POST http://d/session/s1/timeouts',
      'POST http://d/session/s1/execute/sync',
      'POST http://d/session/s1/execute/async',
      'DELETE http://d/session/s1',
    ]);
    // `evaluate` wraps; `execute` does not. A wrapper applied twice is a
    // script that hands the callback to itself and times out with no reason.
    expect(calls[2].body.script).toBe('return 1');
    expect(calls[3].body.script).toContain('arguments[arguments.length - 1]');
  });
});

describe('where the suite can run, and against what', () => {
  it('says why macOS is not it, in words a person can read', () => {
    // A skip whose reason is `false` teaches the next reader nothing, and this
    // is the skip they will meet first on their own machine.
    expect(whyNotHere('darwin')).toMatch(/WKWebView/);
    expect(whyNotHere('linux')).toBeNull();
    expect(whyNotHere('win32')).toBeNull();
  });

  it('resolves the binary cargo actually writes', () => {
    expect(
      binaryPath({ repo: '/r', platform: 'linux', profile: 'debug', override: undefined })
    ).toBe('/r/src-tauri/target/debug/stackvo-desktop');
    expect(
      binaryPath({ repo: '/r', platform: 'win32', profile: 'release', override: undefined })
    ).toBe('/r/src-tauri/target/release/stackvo-desktop.exe');
  });

  it('lets a packaged binary be named instead', () => {
    expect(binaryPath({ repo: '/r', platform: 'linux', override: '/usr/bin/stackvo' })).toBe(
      '/usr/bin/stackvo'
    );
  });
});
