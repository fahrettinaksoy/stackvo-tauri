/**
 * The process boundary, replaced at the seam the architecture already draws.
 *
 * `ipc.js` → `call()` is the one function the data layer passes through — §7 of
 * `docs/durum.md` measures that and a test enforces it — and underneath it
 * there is exactly one global: `window.__TAURI_INTERNALS__.invoke`. Everything
 * here replaces that global and nothing else. No component is reached into, no
 * store is pre-filled, no module is mocked: the app boots the way it boots and
 * asks the questions it asks, and this answers them.
 *
 * The answers are written in the shapes `contracts/ipc.json` declares. That is
 * the part worth being careful about — a stage that answered a shape the real
 * boundary never produces would let a page pass against a fiction. Where a
 * shape here is wrong, the fix is the contract, not this file.
 */

/**
 * A manifest as `manifest::read` returns one — every field, so a component that
 * digs into it does not meet an `undefined` the real boundary never produces.
 */
const MANIFEST = (name, runtime) => ({
  name,
  domain: `${name}.loc`,
  runtime,
  server: runtime === 'php' ? 'nginx' : null,
  documentRoot: runtime === 'php' ? 'public' : null,
  aliases: [],
  lanShare: false,
  services: [],
  php: runtime === 'php' ? { version: '8.4', extensions: [] } : null,
  node:
    runtime === 'node'
      ? {
          version: '22',
          install: 'npm install',
          build: null,
          start: 'npm start',
          port: 3000,
          packageManager: null,
        }
      : null,
  lang: null,
  valid: true,
  errors: [],
  warnings: [],
});

/**
 * A machine with a workspace, a running engine, two projects and two services.
 *
 * Deliberately *not* an empty install: an empty one renders the onboarding
 * gates and nothing else, so every page assertion would be about a screen that
 * says "choose a folder". The interesting screens need something on them.
 */
export const DEFAULT_STAGE = {
  // ---- the shell decides what to render from these three -----------------
  // Field for field from `workspace::Workspace`. `valid` is the one the shell
  // gates on — `hasWorkspace` reads it and nothing else, so a stage with a
  // plausible-looking `exists` boots into an app that never asks for a project.
  workspace_get: {
    root: '/Users/dev/Library/Application Support/StackVo',
    projectsDir: '/Users/dev/StackVo/projects',
    valid: true,
    bootstrapped: true,
    catalogueFetched: true,
    source: 'settings',
    stackvoVersion: '0.1.0',
    envFile: '/Users/dev/StackVo/.env',
  },
  // `reachable`, not `running` — `engineUp` reads that one.
  engine_status: {
    reachable: true,
    version: '29.7.2',
    apiVersion: '1.48',
    context: 'desktop-linux',
    platform: 'dockerDesktop',
    socketPath: '/var/run/docker.sock',
    error: null,
  },
  // `ready` is the field the shell gates on — `App.vue` renders
  // `RequirementsGate` whenever `preflight` exists and `ready` is false. The
  // first version of this stage answered `{ ok, findings }`, which is nobody's
  // shape: `ready` came back undefined, the gate opened over the whole app, and
  // the screen said "0 requirements are not met" while refusing to continue.
  // Exactly the kind of thing this suite is for, found on its own first run.
  preflight: { os: 'macos', requirements: [], ready: true },

  prefs_get: { appearance: 'system' },
  prefs_set: null,
  system_accent: null,
  env_get: {},
  locale_get: 'en',

  cert_status: {
    sslEnabled: true,
    mkcertAvailable: true,
    mkcertVersion: 'v1.4.4',
    caRoot: '/Users/dev/Library/ca',
    caPath: '/Users/dev/Library/ca/rootCA.pem',
    caTrusted: true,
    covered: ['stackvo.loc', '*.stackvo.loc'],
    missing: [],
    stale: [],
  },

  // Field for field from `commands::Project`. The first version of this stage
  // invented `valid`, `aliases` and `services` and left out `domainConfigured`
  // — so every domain button rendered disabled, because `!undefined` is true.
  // The suite's own third finding, and the reason this file says at the top
  // that a wrong shape here lets a page pass against a fiction.
  projects_list: [
    {
      name: 'shop',
      domain: 'shop.loc',
      runtime: 'php',
      path: '/Users/dev/StackVo/projects/shop',
      containerName: 'stackvo-shop',
      running: true,
      built: true,
      manifest: MANIFEST('shop', 'php'),
      manifestValid: true,
      domainConfigured: true,
      generatedStale: false,
      ports: [],
    },
    {
      name: 'storefront',
      domain: 'storefront.loc',
      runtime: 'node',
      path: '/Users/dev/StackVo/projects/storefront',
      containerName: 'stackvo-storefront',
      running: false,
      built: true,
      manifest: MANIFEST('storefront', 'node'),
      manifestValid: true,
      domainConfigured: true,
      generatedStale: false,
      ports: [],
    },
  ],

  services_list: [
    {
      id: 'mysql',
      containerName: 'stackvo-mysql-8-0',
      enabled: true,
      running: true,
      built: true,
      health: 'healthy',
      url: null,
      hostPort: 3306,
      ports: [{ container: 3306, host: 3306, protocol: 'tcp' }],
      declaredPorts: [],
      aliases: ['stackvo-mysql-8-0'],
      support: null,
      eolDate: null,
      companions: [],
      credentials: [],
      required: [],
      optional: [],
      unmetDependencies: [],
    },
    {
      id: 'redis',
      containerName: 'stackvo-redis-8-10',
      enabled: true,
      running: false,
      built: true,
      health: null,
      url: null,
      hostPort: 6379,
      ports: [{ container: 6379, host: 6379, protocol: 'tcp' }],
      declaredPorts: [],
      aliases: ['stackvo-redis-8-10'],
      support: null,
      eolDate: null,
      companions: [],
      credentials: [],
      required: [],
      optional: [],
      unmetDependencies: [],
    },
  ],

  // Enough of the market for its page to render a catalogue rather than the
  // "nothing has been fetched here" gate.
  market_status: { source: 'https://example.test/registry.json', sequence: 13, fetchedAt: null },
  market_registry: { sequence: 13, packages: [] },
  market_installed: [],
  policy_status: { market: {} },

  updater_status: { available: false },
};

// Anything not named above answers `null` rather than rejecting, and the choice
// matters: the shell fires a dozen optional calls on boot — its own state, a
// tray relabel, an accent colour — and a stage that rejected them would fill
// the console with failures that say nothing about the test. A command whose
// absence would change what a test asserts has to be named in `DEFAULT_STAGE`
// or in the test's own overrides, and the assertion is what catches it.

/**
 * Install the boundary into a page before any application code runs.
 *
 * `addInitScript` and not a route handler: the app reads
 * `window.__TAURI_INTERNALS__` during module evaluation, which is before any
 * navigation callback would have fired.
 */
export async function stage(page, overrides = {}) {
  const replies = { ...DEFAULT_STAGE, ...overrides };

  await page.addInitScript((table) => {
    // Every call the page makes, in order, so a test can assert on what was
    // asked rather than only on what was drawn.
    window.__CALLS__ = [];

    window.__TAURI_INTERNALS__ = {
      invoke(cmd, args) {
        window.__CALLS__.push({ cmd, args });
        const reply = Object.prototype.hasOwnProperty.call(table, cmd) ? table[cmd] : null;
        return Promise.resolve(reply);
      },
      // The event API's two halves. Nothing here emits, so a listener is
      // registered and never fires — which is the truthful stand-in for a quiet
      // machine, and quiet is the state every one of these tests is in.
      transformCallback(callback) {
        const id = Math.floor(Math.random() * 1e9);
        window[`_${id}`] = callback;
        return id;
      },
      unregisterCallback() {},
      convertFileSrc(path) {
        return path;
      },
    };
  }, replies);
}

/** What the page asked for, in order. */
export const callsOf = (page) => page.evaluate(() => window.__CALLS__ ?? []);
