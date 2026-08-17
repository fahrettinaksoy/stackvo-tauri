/**
 * A W3C WebDriver client, without a WebDriver library.
 *
 * ## Why not WebdriverIO or selenium-webdriver
 *
 * Tauri's own documentation offers both, and both were measured before this
 * file was written: `webdriverio` brings 89 packages into `package-lock.json`,
 * `selenium-webdriver` brings 24 plus a `mocha` to run it under. What this
 * suite asks a driver to do is: open a session, run four scripts, read a title,
 * close the session. That is six endpoints of a protocol that is JSON over
 * HTTP, and `fetch` has been in Node since 18.
 *
 * ADR 0019 made this same trade for the TUI and wrote down the rule it used:
 * measure the packages first, and only claim "zero new packages" after
 * counting. Counted here too — `package.json` gains nothing.
 *
 * ## What is deliberately *not* here
 *
 * No implicit waits, no element-interaction retries, no shadow-DOM traversal,
 * no page-object layer. Those are the parts of a WebDriver library that are
 * genuinely hard, and the day this suite needs them is the day the trade above
 * stops being true — at which point the honest move is to add the dependency,
 * not to grow a worse copy of it here.
 *
 * ## The split this file is written around
 *
 * Everything above `Session` is pure: it builds a request, or reads a response.
 * That half is unit-tested in `tests/driver-client.spec.js`, which runs on
 * every platform including the one this repository is developed on. The half
 * below it needs a live driver and only runs on Linux CI (§3 #12).
 *
 * That split is not tidiness. `tauri-driver` cannot run on macOS — Tauri's
 * documentation is explicit that macOS has no WKWebView driver — so the person
 * writing this file can never watch it pass. The pure half is the part they
 * *can* watch, and it is where a wrong capability shape or a mis-read error
 * envelope would otherwise sit unnoticed until CI.
 */

/**
 * A failure the driver reported, rather than one this client caused.
 *
 * `error` is the W3C error code (`no such element`, `javascript error`, …) and
 * is the field to branch on; `message` is prose from the driver.
 */
export class WebDriverError extends Error {
  constructor(error, message, stacktrace) {
    super(message ? `${error}: ${message}` : error);
    this.name = 'WebDriverError';
    this.error = error;
    this.stacktrace = stacktrace ?? null;
  }
}

/**
 * The body of `POST /session`.
 *
 * `tauri:options` is the vendor-prefixed capability `tauri-driver` reads to
 * learn which binary to start. The W3C shape (`capabilities.alwaysMatch`) is
 * what it parses; `desiredCapabilities` is the JSON-Wire mirror that every
 * WebDriver client still sends alongside it, and sending it costs one key.
 *
 * `args` is omitted rather than sent empty: an empty array is a claim that the
 * application takes arguments and got none, and this app's argv is a surface
 * (`cli.rs`) that a stray `[]` would be a confusing way to touch.
 */
export function sessionPayload({ application, args }) {
  const tauri = { application };
  if (args && args.length) tauri.args = [...args];
  const capability = { 'tauri:options': tauri };
  return {
    capabilities: { alwaysMatch: capability, firstMatch: [{}] },
    desiredCapabilities: capability,
  };
}

/**
 * Read the session id out of whatever shape came back.
 *
 * W3C puts it at `value.sessionId`. Drivers that still speak JSON-Wire put it
 * at the top level, and `tauri-driver` is a proxy in front of a native driver
 * whose vintage is not this repository's to pick. Both are read rather than
 * one being assumed, because the failure mode of assuming is a session that
 * was really opened, never closed, and leaves an application process behind.
 */
export function sessionIdOf(body) {
  const fromW3C = body?.value?.sessionId;
  if (typeof fromW3C === 'string' && fromW3C) return fromW3C;
  const legacy = body?.sessionId;
  if (typeof legacy === 'string' && legacy) return legacy;
  return null;
}

/**
 * Turn one HTTP response into a value or a throw.
 *
 * A missing `value` key and a `value` of `null` are the same thing to a caller
 * and are collapsed here; `false`, `0` and `''` are not, which is why this is
 * a key check rather than a truthiness one. A script that returns `0` reading
 * as "the driver answered nothing" is the kind of bug that gets diagnosed as
 * a flaky test.
 */
export function unwrap(status, body) {
  if (status >= 200 && status < 300) {
    if (body && typeof body === 'object' && 'value' in body) return body.value;
    return null;
  }
  const failure = (body && typeof body === 'object' && body.value) || body || {};
  throw new WebDriverError(
    failure.error || `http ${status}`,
    failure.message || '',
    failure.stacktrace
  );
}

/**
 * Wrap an expression in the callback protocol `execute/async` speaks.
 *
 * WebDriver's async script gets a callback as its last argument and the script
 * body is a function body, so `arguments` is the real thing — an arrow closing
 * over it is reading the script's own arguments, not its own.
 *
 * The result is boxed into `{ok, value}` / `{ok, error}` rather than letting a
 * rejection reach the driver, and that box is the point of this function. A
 * rejected `invoke` carries a `StackvoError` — a plain object with `code`,
 * `message` and `hint_key` (ADR 0004) — and handing it to the driver's own
 * error path would flatten it into the string `javascript error`, losing the
 * exact field this suite exists to look at.
 */
export function asyncScript(expression) {
  return `
    const done = arguments[arguments.length - 1];
    const args = Array.prototype.slice.call(arguments, 0, -1);
    Promise.resolve()
      .then(() => (${expression}).apply(null, args))
      .then(
        (value) => done({ ok: true, value: value === undefined ? null : value }),
        (error) => done({
          ok: false,
          error: error instanceof Error ? { name: error.name, message: error.message } : error,
        })
      );
  `;
}

/**
 * An open WebDriver session.
 *
 * One instance owns one session id and closes it in [`close`]. Nothing here
 * retries: a retry inside a client is how a test that is measuring a race
 * reports a pass, and every wait in this suite is written where it can say
 * what it is waiting for.
 */
export class Session {
  constructor(base, id, { fetchImpl = fetch } = {}) {
    this.base = base.replace(/\/+$/, '');
    this.id = id;
    this.fetchImpl = fetchImpl;
  }

  /**
   * Start the application and attach to its webview.
   *
   * The application is started *by the driver*, not by this process. That is
   * the whole difference between this suite and the Playwright one: there is a
   * real binary, a real webview and a real IPC boundary on the other side of
   * this call.
   */
  static async open(base, options, { fetchImpl = fetch } = {}) {
    const url = `${base.replace(/\/+$/, '')}/session`;
    const response = await fetchImpl(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(sessionPayload(options)),
    });
    const body = await response.json().catch(() => null);
    unwrap(response.status, body);
    const id = sessionIdOf(body);
    if (!id) {
      throw new WebDriverError(
        'session not created',
        'the driver answered without a session id; nothing to close and nothing to drive'
      );
    }
    return new Session(base, id, { fetchImpl });
  }

  async send(method, path, body) {
    const response = await this.fetchImpl(`${this.base}/session/${this.id}${path}`, {
      method,
      headers: body === undefined ? {} : { 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const parsed = await response.json().catch(() => null);
    return unwrap(response.status, parsed);
  }

  /**
   * How long the driver waits for an async script before giving up.
   *
   * Set explicitly rather than left at the default, which differs between
   * drivers — a suite whose timeout depends on which WebKitWebDriver the
   * runner image happens to ship is a suite that fails for a reason nobody
   * can read off the log.
   */
  timeouts(values) {
    return this.send('POST', '/timeouts', values);
  }

  title() {
    return this.send('GET', '/title');
  }

  url() {
    return this.send('GET', '/url');
  }

  execute(script, args = []) {
    return this.send('POST', '/execute/sync', { script, args });
  }

  /**
   * Run an async expression in the webview and get its settled result.
   *
   * `expression` is source text for a function — `'() => invoke("x")'` — not a
   * function object: it is serialised to another process, and a closure would
   * arrive with none of what it closed over.
   */
  async evaluate(expression, args = []) {
    const boxed = await this.send('POST', '/execute/async', {
      script: asyncScript(expression),
      args,
    });
    return boxed;
  }

  async close() {
    await this.send('DELETE', '');
  }
}
