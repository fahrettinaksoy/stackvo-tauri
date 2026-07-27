import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import vuetify from 'vite-plugin-vuetify';
import { fileURLToPath, URL } from 'node:url';

// Tauri drives the dev server, so the port is fixed and HMR must be reachable
// from the webview. Failing loudly on a taken port beats Vite silently moving
// to 1421 while tauri.conf.json still points at 1420.
const host = process.env.TAURI_DEV_HOST;


/**
 * Ship one font format instead of four.
 *
 * The Material Design Icons stylesheet declares eot, woff2, woff and ttf, and
 * Vite bundles every file a stylesheet references. That is 3.4 MB of fonts in
 * the app bundle to deliver one typeface: .eot exists for Internet Explorer,
 * and .woff/.ttf are fallbacks for browsers that predate woff2.
 *
 * This app runs in exactly three engines — WKWebView, WebView2 and WebKitGTK —
 * and all three have supported woff2 for years. There is no browser to fall
 * back for, so the fallbacks are pure weight in a bundle that gets written to
 * the user's disk.
 *
 * Rewriting the src descriptor rather than deleting files: unreferenced assets
 * are simply never emitted.
 */
function mdiWoff2Only() {
  return {
    name: 'mdi-woff2-only',
    enforce: 'pre',
    transform(code, id) {
      if (!id.includes('materialdesignicons.css')) return null;

      // Two src declarations, not one: the stylesheet emits a bare
      // `src: url(...eot)` for IE8 and then a second `src:` listing eot?#iefix,
      // woff2, woff and ttf. Replacing only the first leaves every file still
      // referenced, so both have to go.
      let replaced = 0;
      const trimmed = code.replace(/src:\s*url\([^)]*\)[^;]*;/g, () => {
        replaced += 1;
        return replaced === 1
          ? 'src: url("../fonts/materialdesignicons-webfont.woff2") format("woff2");'
          : '';
      });

      if (replaced !== 2) {
        // The stylesheet changed shape upstream. Failing loudly beats silently
        // shipping four formats again while the comment above claims otherwise.
        this.error(
          `mdi-woff2-only: expected 2 src declarations to rewrite, found ${replaced}`
        );
      }
      return { code: trimmed, map: null };
    },
  };
}

export default defineConfig({
  plugins: [
    mdiWoff2Only(),
    vue(),
    vuetify({
      autoImport: { labs: true },
      styles: { configFile: 'src/styles/settings.scss' },
    }),
  ],

  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },

  css: {
    preprocessorOptions: {
      // Vuetify's entry stylesheet is main.sass (indented syntax), so both the
      // scss and sass keys must be configured or half the compilation keeps
      // emitting deprecation noise.
      scss: { api: 'modern-compiler', quietDeps: true },
      sass: { api: 'modern-compiler', quietDeps: true },
    },
  },

  // Tauri expects a fixed port and treats a failure to bind as fatal.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },

  // Match the Rust side's target so we don't ship polyfills the webview
  // (WKWebView / WebView2 / WebKitGTK) does not need.
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      output: {
        manualChunks: { vue: ['vue', 'vue-router', 'pinia', 'vue-i18n'] },
      },
    },
  },
});
