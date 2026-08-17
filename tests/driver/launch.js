/**
 * Starting the real application under `tauri-driver`, and taking it down again.
 *
 * ## What this proves that nothing else in this repository does
 *
 * The Playwright suite drives the front end in a real engine and stubs the one
 * global underneath `ipc.js` → `call()`. That is honest and it is half. It does
 * not start the Rust binary, so it cannot tell anyone whether a command named
 * in `contracts/ipc.json` is actually registered, whether the built bundle
 * loads under the CSP `tauri.conf.json` declares, or whether a failure comes
 * back in ADR 0004's shape. Every one of those has exactly one way to be
 * checked: run the binary and ask it.
 *
 * ## Where it runs, and why the skip is loud
 *
 * Linux CI. Not macOS — Tauri's documentation is explicit that there is no
 * WKWebView driver — and that is a fact about the platform rather than a
 * preference, so [`whyNotHere`] returns prose and the tests skip with it
 * printed. On Linux there is no skip path at all: a missing `tauri-driver` or
 * a missing binary fails, because "the suite was green" and "the suite never
 * ran" must not look the same on the one platform that can tell them apart.
 *
 * ## The machine it runs on is not this machine
 *
 * The application is pointed at a temporary `STACKVO_ROOT` and
 * `STACKVO_CONFIG_DIR`, so a driver run reads and writes a directory that is
 * deleted afterwards. `hosts_roundtrip.rs` made the same argument for
 * `STACKVO_HOSTS_PATH` (§3 #35): a test that touches the developer's real
 * workspace is a test somebody eventually switches off.
 */

import { spawn } from 'node:child_process';
import { connect } from 'node:net';
import { mkdtemp, rm, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Session } from './webdriver.js';

const HERE = dirname(fileURLToPath(import.meta.url));
export const REPO = resolve(HERE, '..', '..');

/** `tauri-driver` proxies WebDriver on this port and speaks to the native one on the next. */
export const PORT = Number(process.env.TAURI_DRIVER_PORT || 4444);
export const NATIVE_PORT = Number(process.env.TAURI_DRIVER_NATIVE_PORT || 4445);
export const BASE = `http://127.0.0.1:${PORT}`;

/**
 * The reason this suite cannot run here, or `null` when it can.
 *
 * Returned as prose rather than a boolean because it is printed: a skipped
 * test whose reason is `false` teaches the next reader nothing, and this
 * particular skip is one they will meet on their own machine.
 */
export function whyNotHere(platform = process.platform) {
  if (platform === 'darwin') {
    return 'tauri-driver does not support macOS — there is no WKWebView driver. This suite runs on Linux CI (§3 #12).';
  }
  return null;
}

/**
 * Where `cargo build` leaves the application.
 *
 * `debug` by default and that is deliberate: this suite asks whether the app
 * boots and answers IPC, and a release build costs several minutes of CI to
 * answer the same question. `STACKVO_DRIVER_BINARY` overrides it for the run
 * that wants to ask a packaged binary instead.
 */
export function binaryPath({
  repo = REPO,
  platform = process.platform,
  profile = process.env.STACKVO_DRIVER_PROFILE || 'debug',
  override = process.env.STACKVO_DRIVER_BINARY,
} = {}) {
  if (override) return override;
  const name = platform === 'win32' ? 'stackvo-desktop.exe' : 'stackvo-desktop';
  return join(repo, 'src-tauri', 'target', profile, name);
}

/**
 * Wait until something is listening, by connecting to it.
 *
 * Not by polling an HTTP endpoint: `tauri-driver` is a proxy that starts the
 * native driver lazily, so `/status` before the first session is a question
 * about a process that does not exist yet. A TCP connect asks the only thing
 * worth knowing here — is the proxy up — and nothing else.
 */
export async function waitForPort(port, { timeoutMs = 20_000, everyMs = 100 } = {}) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const open = await new Promise((done) => {
      const socket = connect({ port, host: '127.0.0.1' });
      const settle = (value) => {
        socket.destroy();
        done(value);
      };
      socket.once('connect', () => settle(true));
      socket.once('error', () => settle(false));
      socket.setTimeout(1_000, () => settle(false));
    });
    if (open) return;
    if (Date.now() > deadline) {
      throw new Error(
        `nothing listened on ${port} within ${timeoutMs}ms — is tauri-driver installed?`
      );
    }
    await new Promise((r) => setTimeout(r, everyMs));
  }
}

/**
 * Start `tauri-driver`, open a session against the application, and hand back
 * both the session and the one function that undoes all of it.
 *
 * The teardown closes the session *before* killing the driver, in that order
 * and with the session close guarded: a driver killed while it still owns a
 * session leaves the application it started running, and on a CI runner that
 * is a job that hangs at the end rather than a job that fails.
 */
export async function launch({ application = binaryPath(), args = [] } = {}) {
  await access(application).catch(() => {
    throw new Error(
      `no application at ${application} — build it first: cargo build --manifest-path src-tauri/Cargo.toml`
    );
  });

  const root = await mkdtemp(join(tmpdir(), 'stackvo-driver-'));

  const driver = spawn(
    'tauri-driver',
    ['--port', String(PORT), '--native-port', String(NATIVE_PORT)],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        // The application inherits these through the driver, which is what
        // keeps a run off the developer's real workspace. `STACKVO_ROOT` moves
        // the app root (`appdir.rs`) and `STACKVO_CONFIG_DIR` the settings.
        STACKVO_ROOT: join(root, 'app'),
        STACKVO_CONFIG_DIR: join(root, 'config'),
        // A policy file that does not exist reads as "no policy" rather than
        // as whatever this machine has under /Library or /etc. ADR 0009 says
        // the layer is not a security boundary; it is still an input, and an
        // input a test should not inherit from the host it runs on.
        STACKVO_POLICY_FILE: join(root, 'policy.json'),
      },
    }
  );

  // Kept, not printed: `tauri-driver`'s own diagnostics are the only place a
  // "cannot find WebKitWebDriver" ends up, and a failed launch that reports
  // only "nothing listened on 4444" sends the reader to the wrong question.
  const noise = [];
  driver.stdout.on('data', (chunk) => noise.push(String(chunk)));
  driver.stderr.on('data', (chunk) => noise.push(String(chunk)));
  const died = new Promise((_, reject) => {
    driver.once('error', (error) =>
      reject(new Error(`tauri-driver would not start: ${error.message}`))
    );
    driver.once('exit', (code) =>
      reject(new Error(`tauri-driver exited with ${code}\n${noise.join('')}`))
    );
  });

  let session;
  try {
    await Promise.race([waitForPort(PORT), died]);
    session = await Promise.race([Session.open(BASE, { application, args }), died]);
    // Generous, and bounded. The first script runs while the app is still
    // booting its gates; 30 seconds is long enough for a cold CI runner and
    // short enough that a hang is a failure rather than a job timeout.
    await session.timeouts({ script: 30_000, pageLoad: 30_000, implicit: 0 });
  } catch (error) {
    driver.kill('SIGKILL');
    await rm(root, { recursive: true, force: true });
    error.message = `${error.message}${noise.length ? `\n--- tauri-driver said ---\n${noise.join('')}` : ''}`;
    throw error;
  }

  const stop = async () => {
    await session.close().catch(() => {});
    driver.kill();
    await rm(root, { recursive: true, force: true });
  };

  return { session, stop, root, driverOutput: () => noise.join('') };
}
