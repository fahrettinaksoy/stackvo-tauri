import { createI18n } from 'vue-i18n';
import { en as vuetifyEn, tr as vuetifyTr } from 'vuetify/locale';
import tr from './locales/tr';
import en from './locales/en';

const STORAGE_KEY = 'stackvo.locale';

/**
 * The language for the very first paint, before anything can be asked.
 *
 * `localStorage` only, and only as a cache of a decision already made
 * elsewhere: the authority is `preferences.json`, which the tray reads too, and
 * reaching it means an IPC round trip that the module's own evaluation cannot
 * wait for. `syncLocale` below reconciles the two as soon as the app boots.
 *
 * `navigator.language` used to be the fallback here and is deliberately gone.
 * In a WKWebView it answers from the app bundle's localised resources — this
 * app has none — so it is not a reading of the system setting, it just often
 * resembles one. The real reading happens in Rust and arrives a moment later.
 */
function initialLocale() {
  const saved = localStorage.getItem(STORAGE_KEY);
  return saved === 'tr' || saved === 'en' ? saved : 'en';
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

/**
 * Settle on the language Rust resolved: the stored choice, else this machine's.
 *
 * Called once at boot. Deliberately **not** `setLocale`: writing the answer
 * back would turn a detected language into a stored choice, and from then on
 * the app would keep opening in whatever the machine happened to be set to on
 * first run even after the user changed the machine. A guess must stay a guess
 * until somebody picks.
 *
 * The `localStorage` write is the exception, and it is a cache rather than a
 * decision — it is what stops the next launch from painting English for a
 * frame before the round trip lands.
 */
export async function syncLocale() {
  const { api } = await import('@/lib/ipc');
  const resolved = await api.localeGet().catch(() => null);
  if (resolved !== 'tr' && resolved !== 'en') return;

  localStorage.setItem(STORAGE_KEY, resolved);
  if (i18n.global.locale.value !== resolved) i18n.global.locale.value = resolved;
}
