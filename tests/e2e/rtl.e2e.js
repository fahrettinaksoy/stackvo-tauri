import { test, expect } from '@playwright/test';
import { stage } from './stage.js';

/**
 * Right-to-left, in an engine that has laid the window out.
 *
 * `tests/appearance-rtl.spec.js` beside this holds the wiring: that the switch
 * reaches the map Vuetify reads rather than the computed over it. That test was
 * written after the setting had been shipped for months doing nothing, and it
 * is still the right test — but jsdom has no layout, so what it cannot ask is
 * whether anything on screen actually moved.
 *
 * §3 #24 stood at half done with the reason "no rtl configuration in
 * `vuetify.js`". That was wrong twice: the configuration was in
 * `appearance.js`, where it belongs, and the real gap was in a third place
 * again. Measured here rather than reasoned about, and all three were only
 * visible with boxes:
 *
 *   - `<html>` carried no `dir`, so the document's own direction never changed;
 *   - Vuetify's overlay container is a **sibling of `#app`**, so every dialog,
 *     side sheet, menu and tooltip stayed left-to-right inside a mirrored
 *     window — the same structural fact that had been hiding half the axe run;
 *   - the two navigation drawers were pinned to `location="left"`, a physical
 *     side, so the primary navigation stayed on the wrong edge for a
 *     right-to-left reader while its own contents mirrored.
 */

/** The rail is 64px wide, and which edge it sits on is the whole question. */
const RAIL = 64;

test('mirrors the window, the document and the overlays', async ({ page }) => {
  await stage(page, { prefs_get: { appearance: { rtl: true } } });
  await page.goto('/#/projects');
  await page.waitForLoadState('networkidle');

  const seen = await page.evaluate(() => {
    const app = document.querySelector('.v-application');
    const drawer = document.querySelector('.nav-drawer');
    const overlays = document.querySelector('.v-overlay-container');
    return {
      documentDir: document.documentElement.getAttribute('dir'),
      appDirection: getComputedStyle(app).direction,
      // `null` rather than a guess if Vuetify ever stops rendering the host:
      // an assertion against 'ltr' would then pass for the wrong reason.
      overlayDirection: overlays ? getComputedStyle(overlays).direction : null,
      railLeft: Math.round(drawer.getBoundingClientRect().left),
      width: window.innerWidth,
    };
  });

  expect(seen.documentDir, 'the document itself has to turn round').toBe('rtl');
  expect(seen.appDirection).toBe('rtl');
  expect(
    seen.overlayDirection,
    'the overlay container is outside `#app` and inherits from the document'
  ).toBe('rtl');
  expect(
    seen.railLeft,
    'the navigation belongs on the reading edge, which is the right one here'
  ).toBeGreaterThan(seen.width - RAIL - 8);
});

test('leaves a left-to-right window alone', async ({ page }) => {
  await stage(page);
  await page.goto('/#/projects');
  await page.waitForLoadState('networkidle');

  const seen = await page.evaluate(() => ({
    documentDir: document.documentElement.getAttribute('dir'),
    railLeft: Math.round(document.querySelector('.nav-drawer').getBoundingClientRect().left),
  }));

  // Written rather than absent: an attribute that is sometimes there is one a
  // user stylesheet has to guess about.
  expect(seen.documentDir).toBe('ltr');
  expect(seen.railLeft).toBe(0);
});
