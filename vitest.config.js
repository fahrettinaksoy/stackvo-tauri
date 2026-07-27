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
      // Vuetify components pull in .css from node_modules; without this the
      // transform pipeline treats them as modules to execute.
      server: { deps: { inline: ['vuetify'] } },
    },
  })
);
