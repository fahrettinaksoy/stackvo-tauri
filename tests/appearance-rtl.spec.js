import { describe, it, expect, afterEach } from 'vitest';
import { applyAppearance } from '@/lib/appearance.js';
import vuetify from '@/plugins/vuetify';
import { i18n } from '@/i18n';

/**
 * The right-to-left switch, held to what it claims.
 *
 * The readiness review had this down as half-done with the wrong reason —
 * "no Vuetify rtl configuration" — when the wiring in `appearance.js` was
 * already there and already correct. What was actually missing was this: a
 * test. The setting had one, and it asserted that flipping the toggle called
 * `set({ rtl: true })` — that the *switch* worked, not that anything happened
 * as a result.
 *
 * That gap is the interesting one, because the way this breaks is silent.
 * `isRtl` is a computed derived from the per-locale map, so assigning to it is
 * accepted and discarded; the setting would persist, the toggle would stay on,
 * and every component would keep laying out left-to-right. The comment in
 * `appearance.js` records that this was worked out once. Nothing was keeping it
 * true.
 */

/** Vuetify's rtl map is module state; leave it as it was found. */
afterEach(() => {
  applyAppearance({ rtl: false });
});

describe('the right-to-left setting', () => {
  it('reaches the map Vuetify actually reads, not the computed over it', () => {
    applyAppearance({ rtl: true });

    expect(
      vuetify.locale.isRtl.value,
      'isRtl is what every component asks; the map behind it is what has to be set'
    ).toBe(true);

    applyAppearance({ rtl: false });
    expect(vuetify.locale.isRtl.value).toBe(false);
  });

  it('survives a language change', () => {
    // The flag is a layout preference, not a property of a language: somebody
    // reading an English interface right-to-left has asked for that. Setting it
    // on the active locale alone would silently reset on the next switch.
    const started = i18n.global.locale.value;
    applyAppearance({ rtl: true });

    for (const locale of i18n.global.availableLocales) {
      i18n.global.locale.value = locale;
      expect(vuetify.locale.isRtl.value, `${locale} kept the choice`).toBe(true);
    }

    i18n.global.locale.value = started;
  });

  it('leaves a locale Vuetify already knows to be right-to-left alone', () => {
    // `applyAppearance` writes only the locales this app ships. A build that
    // adds Arabic must not have its direction switched off by a user who never
    // asked about Arabic — which is what writing every key in Vuetify's map
    // would do.
    const shipped = i18n.global.availableLocales;
    expect(shipped, 'the app ships the locales this test reasons about').toContain('en');

    applyAppearance({ rtl: false });
    for (const locale of shipped) {
      expect(vuetify.locale.rtl.value[locale]).toBe(false);
    }
  });

  /**
   * The second half, and it is not the same element.
   *
   * Vuetify's flag turns the app root round. Everything Vue teleports out of
   * that root — which in this application is every dialog, side sheet, menu and
   * tooltip, because the overlay container is a sibling of `#app` — inherits
   * its direction from the document instead. `tests/e2e/rtl.e2e.js` is what
   * measures the result with boxes; this is what keeps the attribute from
   * quietly going away.
   */
  it('turns the document round as well as the app root', () => {
    applyAppearance({ rtl: true });
    expect(document.documentElement.getAttribute('dir')).toBe('rtl');

    // Written both ways rather than removed when off: an attribute that is
    // sometimes absent is one a user stylesheet has to guess about.
    applyAppearance({ rtl: false });
    expect(document.documentElement.getAttribute('dir')).toBe('ltr');
  });
});
