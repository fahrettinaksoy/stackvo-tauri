import js from '@eslint/js';
import pluginVue from 'eslint-plugin-vue';
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting';

export default [
  { ignores: ['dist/**', 'src-tauri/**', 'node_modules/**'] },

  js.configs.recommended,
  ...pluginVue.configs['flat/recommended'],

  {
    files: ['**/*.{js,vue}'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        // Browser globals the webview provides.
        window: 'readonly',
        document: 'readonly',
        navigator: 'readonly',
        console: 'readonly',
        setTimeout: 'readonly',
        clearTimeout: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        requestAnimationFrame: 'readonly',
        ResizeObserver: 'readonly',
        localStorage: 'readonly',
        sessionStorage: 'readonly',
        location: 'readonly',
        TextDecoder: 'readonly',
        TextEncoder: 'readonly',
      },
    },
    rules: {
      // Unused code is how dead IPC wrappers accumulated before the contract
      // checker caught them; catch it here first, at edit time.
      //
      // `ignoreRestSiblings` keeps the omit idiom legal: destructuring the keys
      // you want gone and spreading the rest is how `stripDiagnostics` drops
      // validation fields before showing a manifest for editing.
      'no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', caughtErrors: 'none', ignoreRestSiblings: true },
      ],

      // Vuetify's data table names its cell slots `#item.domain` — a dot, which
      // this rule reads as a modifier. The alternative is 40 eslint-disable
      // comments describing a framework convention.
      'vue/valid-v-slot': ['error', { allowModifiers: true }],
      // Multi-word names are a Vue convention this codebase does not follow for
      // views (Dashboard, Projects, Settings) and does not need to.
      'vue/multi-word-component-names': 'off',
    },
  },

  {
    files: ['tests/**/*.js', 'src/**/*.spec.js', '*.config.js', 'tools/**/*.mjs'],
    languageOptions: {
      globals: { process: 'readonly', __dirname: 'readonly', console: 'readonly' },
    },
  },

  skipFormatting,
];
