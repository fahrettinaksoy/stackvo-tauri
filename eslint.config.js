import js from '@eslint/js';
import pluginVue from 'eslint-plugin-vue';
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting';

export default [
  // `coverage/**` is the v8 reporter's own HTML bundle — hundreds of generated
  // files carrying their own eslint-disable comments, which this config then
  // reports as unused directives. Linting a report about the code is not
  // linting the code.
  { ignores: ['dist/**', 'src-tauri/**', 'node_modules/**', 'coverage/**'] },

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
      // A template referencing a binding the script does not define.
      //
      // Not hypothetical: extracting the Domain pane deleted three consts the
      // Servers pane still used, and nothing complained. `vue/valid-*` checks
      // syntax, the unit tests only mount what has a test, and the app throws
      // at render — on a tab nobody opened during the change. This rule is the
      // only layer that sees it.
      'vue/no-undef-properties': 'error',

      // Multi-word names are a Vue convention this codebase does not follow for
      // views (Dashboard, Projects, Settings) and does not need to.
      'vue/multi-word-component-names': 'off',
    },
  },

  {
    files: ['tests/**/*.js', 'src/**/*.spec.js', '*.config.js', 'tools/**/*.mjs'],
    languageOptions: {
      globals: {
        process: 'readonly',
        __dirname: 'readonly',
        console: 'readonly',
        // `tests/setup.js` fills in the browser APIs jsdom lacks, so it names
        // the very globals the app is never allowed to assume — and one it
        // patches a prototype on.
        globalThis: 'readonly',
        SVGElement: 'readonly',
        // Node 17+, and the honest way to build a fixture a test then mutates
        // without the mutation leaking into the next test.
        structuredClone: 'readonly',
      },
    },
  },

  skipFormatting,
];
