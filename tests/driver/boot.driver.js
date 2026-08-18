/**
 * The whole binary, under a driver. §3 #12's other half.
 *
 * Every assertion in this file is one no other suite in this repository can
 * make, and the list is short on purpose. The Playwright suite already owns
 * layout, roles and axe; repeating any of that here would buy a slower copy of
 * an answer we have, running on the one platform its author cannot watch.
 *
 * What is left after removing everything Playwright covers is the boundary
 * itself:
 *
 * 1. the built bundle loads in the real webview, under the CSP `tauri.conf
 *    .json` declares and over Tauri's asset protocol — `vite preview` serves
 *    over http and proves neither;
 * 2. the IPC boundary underneath `ipc.js` → `call()` is the real one rather
 *    than the global the Playwright stage replaces;
 * 3. a command named in `contracts/ipc.json` is actually registered and
 *    answers in its declared shape;
 * 4. a failure comes back as ADR 0004's `{code, message}` with a code from the
 *    contract's closed set — end to end, through serde and through Tauri.
 *
 * Run: `npm run test:driver`, on Linux, with `tauri-driver` on PATH and the
 * app built. On macOS every test below skips with the reason printed.
 */

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { launch, whyNotHere, REPO } from './launch.js';

const blocked = whyNotHere();

/** The contract's closed set of error codes, read rather than copied. */
const CODES = Object.keys(
  JSON.parse(await readFile(join(REPO, 'contracts', 'ipc.json'), 'utf8')).errors.codes
);

let app;

before(
  async () => {
    if (blocked) return;
    app = await launch();
  },
  { timeout: 120_000 }
);

after(async () => {
  if (app) await app.stop();
});

/**
 * Poll an expression in the webview until it answers truthily.
 *
 * The app boots through its gates asynchronously, so the first script can land
 * before Vue has mounted. This waits for a stated condition rather than for a
 * duration — a `sleep(2000)` is a test that passes on a fast runner and gets
 * marked flaky on a slow one.
 */
async function until(expression, what, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    const box = await app.session.evaluate(expression);
    last = box;
    if (box.ok && box.value) return box.value;
    if (Date.now() > deadline) {
      throw new Error(`${what} — last answer: ${JSON.stringify(last)}${await diagnosis()}`);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
}

/**
 * What the page and the driver have to say, appended to a failing assertion.
 *
 * The first Linux run of this suite reported `the app root never rendered any
 * children — last answer: {"ok":true,"value":null}` and nothing else, four
 * times. That sentence says the condition was not met and refuses to say why:
 * an empty `#app` looks the same whether the bundle never loaded, the CSP
 * refused the script, Vue threw during mount, or the binary is serving a stale
 * `dist`. Four different fixes behind one message.
 *
 * `tauri-driver`'s output was already captured and was only ever printed when
 * the driver DIED — so on the failure that actually happened it was collected
 * and thrown away.
 *
 * Every probe is wrapped: this runs while something is already wrong, and a
 * diagnostic that throws replaces the real failure with its own.
 */
async function diagnosis() {
  const ask = async (label, expression) => {
    try {
      const box = await app.session.evaluate(expression);
      return `\n  ${label}: ${JSON.stringify(box?.value ?? box)}`;
    } catch (error) {
      return `\n  ${label}: <unreadable: ${error.message}>`;
    }
  };

  let out = '\n--- what the page says ---';
  out += await ask('url', '() => location.href');
  out += await ask('readyState', '() => document.readyState');
  out += await ask('title', '() => document.title');
  out += await ask('#app present', '() => !!document.querySelector("#app")');
  out += await ask('scripts', '() => [...document.scripts].map((s) => s.src || "inline")');
  out += await ask('stylesheets', '() => document.styleSheets.length');
  out += await ask('ipc bridge', '() => typeof window.__TAURI_INTERNALS__');
  out += await ask('body', '() => document.body && document.body.innerHTML.slice(0, 200)');

  const said = app.driverOutput?.() ?? '';
  if (said.trim()) out += `\n--- tauri-driver said ---\n${said}`;
  return out;
}

test('the built bundle renders inside the real webview', { skip: blocked }, async () => {
  const shell = await until(
    `() => {
       const app = document.querySelector('#app');
       if (!app || app.children.length === 0) return null;
       return {
         children: app.children.length,
         // Under the app's CSP, style-src is 'self' — a stylesheet count of
         // zero means the bundle's CSS was refused rather than that the page
         // is unstyled, and those two look identical in a screenshot.
         stylesheets: document.styleSheets.length,
         protocol: location.protocol,
       };
     }`,
    'the app root never rendered any children'
  );

  assert.ok(shell.children > 0, 'the app mounted nothing');
  assert.ok(shell.stylesheets > 0, 'no stylesheet applied — check style-src in tauri.conf.json');
  // Not http: the Playwright suite loads over `vite preview`, so this is the
  // one assertion that says "this is not that suite".
  assert.notEqual(shell.protocol, 'http:', 'this is the preview server, not the app');
});

test('the IPC boundary is the real one', { skip: blocked }, async () => {
  const box = await app.session.evaluate(
    `() => ({
       internals: typeof window.__TAURI_INTERNALS__,
       invoke: typeof window.__TAURI_INTERNALS__?.invoke,
     })`
  );
  assert.ok(box.ok, `evaluating the boundary failed: ${JSON.stringify(box.error)}`);
  // `box.value`, not `box`. `evaluate` answers with the envelope
  // `{ ok, value }` — every other test here reads through `until`, which
  // unwraps it, and this one read the envelope's own properties and compared
  // `undefined` against `'object'`. It could not have passed, and nothing said
  // so until the suite was run: the four failures around it all had the same
  // message, and this one was a typo wearing their clothes.
  assert.equal(box.value.internals, 'object');
  assert.equal(box.value.invoke, 'function');
});

test('a contract command is registered and answers in its shape', { skip: blocked }, async () => {
  const box = await app.session.evaluate(
    `() => window.__TAURI_INTERNALS__.invoke('workspace_get')`
  );
  assert.ok(box.ok, `workspace_get rejected: ${JSON.stringify(box.error)}`);

  const workspace = box.value;
  assert.equal(typeof workspace, 'object');
  // Field for field from `workspace::Workspace`, and `valid` is the one the
  // shell gates on. Its *value* is not asserted: a CI runner has no workspace
  // and `false` is the correct answer there. That the field exists and is a
  // boolean is the claim — a missing one is what makes a gate never open.
  assert.equal(typeof workspace.valid, 'boolean', 'workspace.valid is not a boolean');
  assert.ok('root' in workspace, 'workspace.root is missing');
  assert.ok('projectsDir' in workspace, 'workspace.projectsDir is missing');
});

test('a failure arrives as a code, not a string', { skip: blocked }, async () => {
  // Read-only and certain to fail on a machine with no workspace and no such
  // project — whichever of the two it hits, ADR 0004 says the answer has the
  // same shape, and the shape is what is under test.
  const box = await app.session.evaluate(
    `() => window.__TAURI_INTERNALS__.invoke('project_get', { name: 'no-such-project-e6f2' })`
  );
  assert.equal(box.ok, false, 'project_get resolved for a project that does not exist');

  const error = box.error;
  assert.equal(typeof error, 'object', `the failure was not an object: ${JSON.stringify(error)}`);
  assert.ok(
    CODES.includes(error.code),
    `code ${JSON.stringify(error.code)} is not in the contract's closed set`
  );
  assert.equal(typeof error.message, 'string');
  assert.ok(error.message.length > 0, 'the message was empty');
  // `hintKey` is optional (error.rs says why) — asserted only when present, so
  // this does not quietly become a requirement the Rust side never made.
  if ('hintKey' in error) assert.equal(typeof error.hintKey, 'string');
});

test(
  'an unregistered command fails differently, which is what makes the test above mean something',
  {
    skip: blocked,
  },
  async () => {
    // The control. If Tauri answered every unknown command with a plausible
    // object, the previous test would pass against an app with no commands at
    // all. It does not: an unregistered command fails at the framework, and the
    // framework's failure is not a StackvoError.
    const box = await app.session.evaluate(
      `() => window.__TAURI_INTERNALS__.invoke('command_that_is_not_in_the_contract')`
    );
    assert.equal(box.ok, false, 'an unregistered command resolved');
    assert.ok(
      typeof box.error !== 'object' || box.error === null || !CODES.includes(box.error?.code),
      'an unregistered command produced a StackvoError, so error codes prove nothing'
    );
  }
);
