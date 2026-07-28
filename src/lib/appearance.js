import vuetify from '@/plugins/vuetify';
import { i18n } from '@/i18n';

/**
 * The look of the app, as data.
 *
 * Vuetify can be re-themed at runtime — `theme.themes` is a writable ref and
 * the stylesheet it generates is recomputed from it — so everything here is a
 * value the user picks rather than a build-time constant. What Vuetify cannot
 * do at runtime is the SASS half: corner radius and typography are compiled to
 * fixed pixels, so those two ride on CSS custom properties instead, with the
 * override layer in `styles/global.css`.
 *
 * Persisted under `appearance` in preferences.json, applied by the store in
 * `stores/appearance.js` at boot — not by the settings page, which is opened
 * long after the first paint.
 */
export const DEFAULT_APPEARANCE = {
  /** 'system' is Vuetify's own: it follows prefers-color-scheme and updates live. */
  theme: 'system',
  primary: '#1976D2',
  neutral: 'graphite',
  radius: 12,
  fontFamily: 'system',
  /** Root px. Vuetify's type scale is in rem, so this scales the whole UI. */
  fontSize: 16,
  highContrast: false,
  rtl: false,
  /** Which set of colours carries running / stopped / failed. */
  statusPalette: 'default',
  /**
   * How tight the controls are. Every `density` prop in the app was removed in
   * favour of this: with them in place the setting moved nothing, because a
   * prop written on the component outranks any default.
   */
  density: 'compact',
  /** Terminals and log views keep a dark surface whatever the app theme is. */
  darkConsoles: true,
  reduceMotion: false,
  /** Take the accent from the desktop instead of the swatch grid. */
  useSystemAccent: false,
  /**
   * Language for the log and terminal panels: 'app' follows the interface, a
   * locale code pins them. Pinning to English is what makes an error message
   * pasted into an issue readable by someone who does not share your UI
   * language.
   */
  consoleLocale: 'app',
};

/**
 * The colours that mean "running", "stopped", "failed".
 *
 * This is the one palette in the app that is not decoration. A dashboard whose
 * primary signal is a red or green dot is unreadable to the ~8% of men with a
 * red-green deficiency, and no amount of theming fixes that if the only two
 * states differ by hue alone.
 *
 * `colorblind` is Okabe-Ito, the palette designed to stay distinguishable under
 * every common form of colour vision deficiency: blue-green against vermillion
 * rather than green against red.
 */
export const STATUS_PALETTES = [
  {
    id: 'default',
    colors: { success: '#4CAF50', warning: '#FB8C00', error: '#FF5252', info: '#2196F3' },
  },
  {
    id: 'colorblind',
    colors: { success: '#009E73', warning: '#E69F00', error: '#D55E00', info: '#56B4E9' },
  },
  {
    id: 'muted',
    colors: { success: '#5E8C61', warning: '#B58B34', error: '#B4544E', info: '#4F7CAC' },
  },
];

/**
 * The accent, offered as swatches rather than a colour picker.
 *
 * A free picker lets you choose a primary that white text cannot sit on, and
 * every one of these has been checked against both surface families. Vuetify
 * derives `on-primary` from the value automatically, so the text on a button
 * stays legible without a second choice being asked for.
 */
export const PRIMARY_SWATCHES = [
  '#5E35B1',
  '#7C4DFF',
  '#3F51B5',
  '#1E5FD8',
  '#1976D2',
  '#0288D1',
  '#00ACC1',
  '#009688',
  '#00A870',
  '#2E7D32',
  '#7CB342',
  '#F5A623',
  '#FB8C00',
  '#F4511E',
  '#E53935',
  '#D81B60',
  '#EC407A',
  '#D500F9',
  '#8E24AA',
  '#546E7A',
];

/**
 * Surface families. Only the greys move — the accent is chosen separately, and
 * a palette that changed both at once would make the two choices inseparable.
 *
 * `on-surface-variant` travels with them because it is the colour of secondary
 * text on those surfaces; leaving it fixed makes the warm palettes look muddy.
 */
export const NEUTRALS = [
  {
    id: 'graphite',
    dark: {
      background: '#0E1116',
      surface: '#161B22',
      'surface-bright': '#1E2530',
      'surface-light': '#21262D',
      'surface-variant': '#2A313C',
      'on-surface-variant': '#C9D1D9',
    },
    light: {
      background: '#F5F7FA',
      surface: '#FFFFFF',
      'surface-bright': '#FFFFFF',
      'surface-light': '#EEF1F6',
      'surface-variant': '#E7EAF0',
      'on-surface-variant': '#3B4252',
    },
  },
  {
    id: 'carbon',
    dark: {
      background: '#0B0B0C',
      surface: '#151516',
      'surface-bright': '#1F1F21',
      'surface-light': '#1A1A1C',
      'surface-variant': '#2B2B2E',
      'on-surface-variant': '#CFCFD2',
    },
    light: {
      background: '#F4F4F5',
      surface: '#FFFFFF',
      'surface-bright': '#FFFFFF',
      'surface-light': '#ECECEE',
      'surface-variant': '#E3E3E6',
      'on-surface-variant': '#3F3F46',
    },
  },
  {
    id: 'midnight',
    dark: {
      background: '#080D1A',
      surface: '#101728',
      'surface-bright': '#182238',
      'surface-light': '#141C30',
      'surface-variant': '#233048',
      'on-surface-variant': '#C3CEE4',
    },
    light: {
      background: '#F2F5FC',
      surface: '#FFFFFF',
      'surface-bright': '#FFFFFF',
      'surface-light': '#E9EFFA',
      'surface-variant': '#DFE7F6',
      'on-surface-variant': '#33415C',
    },
  },
  {
    id: 'forest',
    dark: {
      background: '#0A1210',
      surface: '#111C19',
      'surface-bright': '#182722',
      'surface-light': '#14211D',
      'surface-variant': '#22352E',
      'on-surface-variant': '#C4D5CD',
    },
    light: {
      background: '#F2F7F4',
      surface: '#FFFFFF',
      'surface-bright': '#FFFFFF',
      'surface-light': '#E9F2ED',
      'surface-variant': '#DEEBE4',
      'on-surface-variant': '#33473E',
    },
  },
  {
    id: 'warm',
    dark: {
      background: '#12100E',
      surface: '#1B1815',
      'surface-bright': '#26221E',
      'surface-light': '#201C18',
      'surface-variant': '#332D27',
      'on-surface-variant': '#D8CEC4',
    },
    light: {
      background: '#FAF7F3',
      surface: '#FFFFFF',
      'surface-bright': '#FFFFFF',
      'surface-light': '#F3EEE8',
      'surface-variant': '#EBE4DB',
      'on-surface-variant': '#4A4239',
    },
  },
];

/**
 * Font stacks, not fonts.
 *
 * The app bundles one webfont — the icon set — and nothing else, so offering
 * "Roboto" would name a face that is simply absent on most machines and fall
 * back silently to something else. Every stack here starts with a face the
 * platform ships.
 */
export const FONT_FAMILIES = [
  {
    id: 'system',
    stack:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, "Helvetica Neue", Arial, sans-serif',
  },
  { id: 'grotesk', stack: '"Helvetica Neue", Helvetica, Arial, "Segoe UI", sans-serif' },
  { id: 'serif', stack: 'Georgia, "Times New Roman", "Noto Serif", serif' },
  { id: 'mono', stack: '"SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace' },
];

const byId = (list, id) => list.find((item) => item.id === id) ?? list[0];

/**
 * Push a settings object into the running app.
 *
 * Idempotent and total: every field is written on every call, so applying a
 * partial change and applying the whole object land in the same place. That is
 * what lets the settings page call this on every keystroke of a slider without
 * tracking what actually changed.
 */
export function applyAppearance(appearance, systemAccent = null) {
  const a = { ...DEFAULT_APPEARANCE, ...appearance };
  // Falls back to the chosen swatch when the desktop could not be read, rather
  // than to a hard-coded blue: the swatch is at least something the user picked.
  const primary = (a.useSystemAccent && systemAccent) || a.primary;
  const neutral = byId(NEUTRALS, a.neutral);
  const status = byId(STATUS_PALETTES, a.statusPalette);

  vuetify.theme.change(a.theme);

  for (const name of ['light', 'dark']) {
    const theme = vuetify.theme.themes.value[name];
    if (!theme) continue;

    theme.colors.primary = primary;
    Object.assign(theme.colors, neutral[name], status.colors);

    // Contrast is a pair of opacities rather than a second palette: Vuetify
    // renders secondary text and every divider through these two variables, so
    // raising them lifts the whole interface without inventing new colours.
    theme.variables['medium-emphasis-opacity'] = a.highContrast ? 0.9 : 0.68;
    theme.variables['border-opacity'] = a.highContrast ? 0.3 : 0.12;
  }

  // `global`, not per-component: Vuetify reads defaults through a proxy on
  // props, so only components that actually declare `density` ever see it —
  // there are no stray attributes on the ones that do not.
  vuetify.defaults.value = {
    ...vuetify.defaults.value,
    global: { ...vuetify.defaults.value.global, density: a.density },
  };

  // An attribute rather than a class: the stylesheet keys off it, and it is one
  // value to read back when debugging why nothing on screen is animating.
  document.documentElement.dataset.reduceMotion = String(!!a.reduceMotion);

  const root = document.documentElement.style;
  root.setProperty('--app-radius', `${a.radius}px`);
  root.setProperty('--app-font-family', byId(FONT_FAMILIES, a.fontFamily).stack);
  root.setProperty('--app-font-size', `${a.fontSize}px`);

  // Layout direction is Vuetify's, not a stylesheet of ours: every component
  // mirrors itself from it.
  //
  // Written into the per-locale map rather than onto `isRtl`, which is a
  // computed derived from that map — assigning to it is silently discarded.
  // Every locale the app ships gets the flag, so the choice survives switching
  // language; the locales Vuetify already knows to be RTL are left alone.
  for (const name of i18n.global.availableLocales) {
    vuetify.locale.rtl.value[name] = a.rtl;
  }
}
