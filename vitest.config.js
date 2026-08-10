import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config.js';
// The floors live outside this file because CI holds the same ones against the
// Rust report, and a policy written twice is a policy that will disagree with
// itself. `tools/coverage-floors.mjs` also carries why each number is what it
// is, including why `functions` is deliberately not floored.
import { floors } from './tools/coverage-floors.mjs';

// Reuses the app's own resolve aliases and plugins so a test imports exactly
// what the app imports. A separate alias table is a second source of truth and
// drifts silently.
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      include: ['src/**/*.spec.js', 'tests/**/*.spec.js'],
      // Browser APIs jsdom lacks. Without them Vuetify throws inside `setup()`
      // and whole pages simply cannot be mounted — which is how `src/views/`
      // stayed at 0%. `tests/setup.js` explains what each stub does and does
      // not promise.
      setupFiles: ['tests/setup.js'],
      // Vuetify components pull in .css from node_modules; without this the
      // transform pipeline treats them as modules to execute.
      server: { deps: { inline: ['vuetify'] } },

      // Measured *and* enforced, in that order.
      //
      // This block reported and never failed for four rounds of work, on the
      // argument that a threshold picked before anyone has seen the number is
      // either low enough to bless the gap or high enough to fail on the first
      // run. That was right at 30.70%, when 13 spec files faced 22k lines of
      // front end and `src/views/` was at zero. It stopped being right once the
      // number had been watched across those rounds: 30.70 → 50.71 → 73.85 →
      // 89.65. Reporting answers "how much"; only a floor answers "did that
      // just get worse", and the second question is the one a build can ask.
      coverage: {
        // Set from measurement, with headroom for platform drift and for the
        // commit that adds a module before its tests. See the file for both.
        thresholds: floors.frontend,
        provider: 'v8',
        // `text` for the terminal, `json-summary` for a CI step that wants the
        // number, `lcov` for an editor gutter or an upload later.
        reporter: ['text', 'json-summary', 'lcov'],
        reportsDirectory: 'coverage',
        // Every source file, including the ones no spec imports — the default
        // only counts files a test touched, which reports the covered subset of
        // the covered subset and always looks healthy.
        all: true,
        include: ['src/**/*.{js,vue}'],
        exclude: [
          'src/**/*.spec.js',
          // Generated or declarative surfaces with no branches to exercise:
          // counting them moves the percentage without telling anyone anything.
          'src/main.js',
          'src/i18n/**',
        ],
      },
    },
  })
);
