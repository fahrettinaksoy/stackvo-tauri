// Vuetify runtime configuration.
//
// Ported from the web UI so the desktop app is visually the same product.
// Two things are deliberately dropped for Phase 1:
//   - the date-fns adapter: nothing here renders a date picker yet, and it is
//     two dependencies for zero features.
//   - the vue-i18n locale adapter: reinstated below, but without pulling
//     Vuetify's own strings through until there are components that need them.
import 'vuetify/styles';
import { createVuetify } from 'vuetify';
import { md3 } from 'vuetify/blueprints';

import { aliases, mdi } from 'vuetify/iconsets/mdi';
import '@mdi/font/css/materialdesignicons.css';

import { createVueI18nAdapter } from 'vuetify/locale/adapters/vue-i18n';
import { useI18n } from 'vue-i18n';
import { i18n } from '@/i18n';

const brand = {
  primary: '#1976D2',
  secondary: '#5C6BC0',
  accent: '#82B1FF',
  error: '#FF5252',
  info: '#2196F3',
  success: '#4CAF50',
  warning: '#FB8C00',
};

const sharedVariables = {
  'border-color': '#000000',
  'medium-emphasis-opacity': 0.68,
  'border-opacity': 0.12,
};

const dark = {
  dark: true,
  colors: {
    ...brand,
    background: '#0E1116',
    surface: '#161B22',
    'surface-bright': '#1E2530',
    'surface-variant': '#2A313C',
    'on-surface-variant': '#C9D1D9',
    'surface-light': '#21262D',
    'on-background': '#E6EDF3',
    'on-surface': '#E6EDF3',
  },
  variables: {
    ...sharedVariables,
    'border-color': '#FFFFFF',
    'theme-overlay-multiplier': 1.5,
  },
};

const light = {
  dark: false,
  colors: {
    ...brand,
    background: '#F5F7FA',
    surface: '#FFFFFF',
    'surface-bright': '#FFFFFF',
    'surface-variant': '#E7EAF0',
    'on-surface-variant': '#3B4252',
    'surface-light': '#EEF1F6',
    'on-background': '#1B2026',
    'on-surface': '#1B2026',
  },
  variables: { ...sharedVariables },
};

// Set the project's default look once so templates stop repeating the same
// props. Explicit props on a component still win.
const defaults = {
  global: { ripple: true },

  VCard: { rounded: 'lg' },
  VSheet: { rounded: 'lg' },
  VChip: { variant: 'tonal' },
  VAlert: { variant: 'tonal' },
  VBtnToggle: { variant: 'outlined' },
  VTooltip: { location: 'top' },

  // No `density` here on purpose: a component-level default outranks
  // `global.density`, which is the one knob the appearance setting turns.
  VTextField: { variant: 'outlined', hideDetails: 'auto' },
  VSelect: { variant: 'outlined', hideDetails: 'auto' },

  VToolbar: { VBtn: { variant: 'text' } },
};

export default createVuetify({
  blueprint: md3,
  icons: { defaultSet: 'mdi', aliases, sets: { mdi } },
  theme: { defaultTheme: 'dark', themes: { dark, light } },
  locale: { adapter: createVueI18nAdapter({ i18n, useI18n }) },
  defaults,
});
