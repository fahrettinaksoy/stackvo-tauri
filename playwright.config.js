import { defineConfig, devices } from '@playwright/test';

/**
 * The front end in a real browser engine, against a real layout.
 *
 * ## Why this exists beside 700 vitest specs
 *
 * Those run in jsdom, which has no layout: `getBoundingClientRect` is zeroes,
 * nothing scrolls, nothing overflows, and `:focus-visible` never matches. That
 * is not a gap somebody forgot to fill — it is the class of bug this repository
 * keeps finding by looking at the screen. A `v-btn` whose icon prop is ignored
 * because its slot is full renders blank and passes every mount test; a page
 * that bounds nothing compresses its children instead of scrolling and passes
 * every mount test. Both shipped, both were caught by a person, and both got a
 * source-reading guard afterwards — `button-icons.spec.js` and
 * `page-scroll.spec.js` — because there was no engine to ask.
 *
 * This is the engine. What it can check that neither jsdom nor a grep can: that
 * a thing is visible, has a size, is reachable by keyboard, and that axe agrees
 * about the rendered tree rather than about a tree with no boxes in it.
 *
 * ## Why not `tauri-driver`
 *
 * Because it does not run here. Tauri's own documentation is explicit: "only
 * Windows and Linux are supported on desktop, as macOS has no WKWebView driver
 * tool available". This application is developed on macOS, so a `tauri-driver`
 * suite would be a suite its authors cannot run — the exact arrangement that
 * lets a test rot until CI is the only thing that has ever seen it pass.
 *
 * So this drives the front end and stubs the boundary, and the honest name for
 * what it covers is *the webview half*. It does not start the Rust binary and
 * does not prove an IPC command exists — `contracts/ipc.json` and the contract
 * tests do that, and they do it on every platform. What is genuinely still
 * missing is the whole binary under a driver on Linux CI; §3 #12 says so.
 *
 * ## Why stubbing the boundary is not a cheat here
 *
 * There is exactly one function the data layer passes through — `ipc.js` →
 * `call()`, which §7 measures and a test enforces — and underneath it exactly
 * one global, `window.__TAURI_INTERNALS__.invoke`. Replacing that global is
 * replacing the process boundary at the same seam ADR 0001 draws it, not
 * reaching inside a component to make it behave.
 */
export default defineConfig({
  testDir: 'tests/e2e',
  // Not `**/*.spec.js`: vitest owns that suffix everywhere else in this
  // repository, and two runners collecting each other's files is a failure
  // whose message names neither of them.
  testMatch: '**/*.e2e.js',

  // A browser is slower than jsdom and these are few; a whole suite that takes
  // longer than this has hung rather than been slow.
  timeout: 30_000,
  expect: { timeout: 5_000 },

  fullyParallel: true,
  // A `.only` left in a file passes locally and silently narrows CI to one
  // test. Refused there rather than trusted.
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],

  use: {
    // `localhost`, not `127.0.0.1`: `vite preview` binds the hostname rather
    // than the address, so the numeric form is refused while the name answers.
    // Measured, after a 180-second timeout that reported nothing else.
    baseURL: 'http://localhost:4173',
    trace: 'on-first-retry',
    // A failure in a browser is a picture. Kept only for failures, because a
    // screenshot per passing test is a megabyte per run of nothing.
    screenshot: 'only-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // The preview server, not the dev one: HMR has no place here, and the built
  // bundle is what a user's webview actually loads — a test against the dev
  // server would not have caught a plugin that behaves differently in build.
  webServer: {
    command: 'npm run build && npx vite preview --port 4173 --strictPort',
    url: 'http://localhost:4173',
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
