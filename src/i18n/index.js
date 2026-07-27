import { createI18n } from 'vue-i18n';
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
  messages: { tr, en },
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
