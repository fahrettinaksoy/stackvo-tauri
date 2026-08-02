import { createI18n } from 'vue-i18n';
import { en as vuetifyEn, tr as vuetifyTr } from 'vuetify/locale';
import tr from './locales/tr';
import en from './locales/en';

const STORAGE_KEY = 'stackvo.locale';

function initialLocale() {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === 'tr' || saved === 'en') return saved;
  return navigator.language?.startsWith('tr') ? 'tr' : 'en';
}

export const i18n = createI18n({
  // Composition API mode — required by Vuetify's vue-i18n adapter.
  legacy: false,
  locale: initialLocale(),
  fallbackLocale: 'en',
  /**
   * Vuetify's own strings live under `$vuetify`, merged in here.
   *
   * `createVueI18nAdapter` makes Vuetify resolve its internal labels through
   * this instance instead of its own locale store — so every one of them,
   * `$vuetify.dismiss` for a snackbar's close button through
   * `$vuetify.noDataText` for an empty table, became a lookup vue-i18n had no
   * answer for. A missing key is returned verbatim, and the button's
   * text-transform then shouted it: **$VUETIFY.DISMISS**.
   *
   * Taken from `vuetify/locale`, which ships both languages this app speaks,
   * rather than written out here — these are the library's strings, not the
   * app's, and the app's own files stay the only place its own copy lives.
   */
  messages: {
    tr: { ...tr, $vuetify: vuetifyTr },
    en: { ...en, $vuetify: vuetifyEn },
  },
});

export async function setLocale(locale) {
  i18n.global.locale.value = locale;
  localStorage.setItem(STORAGE_KEY, locale);

  // The tray menu is built in Rust and cannot see localStorage, so the choice
  // has to reach preferences.json as well — and the menu has to be re-labelled,
  // or it keeps the old language until the next launch and reads as broken.
  const { api } = await import('@/lib/ipc');
  await api.prefsSet({ locale }).catch(() => {});
  await api.trayRelabel().catch(() => {});
}
