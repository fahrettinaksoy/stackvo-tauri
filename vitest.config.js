import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config.js';

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

      // Measured, not enforced — on purpose, for now.
      //
      // There are 13 spec files against 22k lines of front end, and nobody knew
      // which 22k. A threshold set today would either be so low it certifies the
      // gap or so high it fails the build on day one, and both teach the same
      // thing: write tests for the report. So this only reports. The number is
      // there to be looked at, argued about, and turned into a floor once it
      // means something — `thresholds: { lines: N }` is the one line to add.
      coverage: {
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
