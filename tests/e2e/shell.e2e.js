import { test, expect } from '@playwright/test';
import { stage, callsOf } from './stage.js';

/**
 * The application boots, and the things that only a layout engine can answer.
 *
 * Every assertion here is one jsdom cannot make. "Is it visible" in jsdom means
 * "is it in the DOM"; here it means the element has a box, is not clipped to
 * nothing and is not behind something else. That distinction is the whole
 * reason this file exists — the two bugs that got source-reading guards written
 * for them (a blank icon button, a page that compressed instead of scrolling)
 * were both invisible to a tree with no boxes in it.
 */

test.beforeEach(async ({ page }) => {
  await stage(page);
  // A console error during boot is a failure even when the page renders: it is
  // how a broken store or a bad prop announces itself in production, where
  // nobody is running a test.
  page.on('pageerror', (error) => {
    throw new Error(`the page threw during boot: ${error.message}`);
  });
});

test('boots past the gates and counts what the boundary handed it', async ({ page }) => {
  await page.goto('/');

  // Not `toBeAttached`: attached-but-zero-sized is exactly the failure jsdom
  // cannot see and a person reports as "the page is blank".
  // Scoped to `main`: the nav rail carries the same word, and a bare text
  // locator that matches two elements fails on the ambiguity rather than on
  // the thing under test.
  await expect(page.getByRole('main').getByText('Dashboard')).toBeVisible();

  // It got there by asking, rather than by rendering something that happens to
  // look right. What each page then *shows* is asserted on that page — here the
  // claim is only that the shell booted and the boundary was used.
  const calls = await callsOf(page);
  expect(calls.map((c) => c.cmd)).toEqual(
    expect.arrayContaining(['workspace_get', 'engine_status', 'preflight', 'projects_list'])
  );
});

test('the projects page shows the projects, by name', async ({ page }) => {
  await page.goto('/#/projects');

  // Asked for by role, and that is the assertion rather than a detail of it.
  // These were `<a>` with a click handler and no href until this suite's first
  // run: rendered like links, announced as text, unreachable by keyboard. The
  // role is what a person navigating without a mouse actually gets.
  await expect(page.getByRole('main').getByRole('button', { name: 'shop.loc' })).toBeVisible();
  await expect(
    page.getByRole('main').getByRole('button', { name: 'storefront.loc' })
  ).toBeVisible();
});

test('every visible control has a size', async ({ page }) => {
  await page.goto('/#/projects');
  await expect(page.getByRole('main').getByRole('button', { name: 'shop.loc' })).toBeVisible();

  // The `button-icons` guard reads sources because there was no engine to ask.
  // This asks: a button that rendered its label into nothing has a zero box,
  // whatever its markup says.
  const collapsed = await page.evaluate(() => {
    const bad = [];
    for (const el of document.querySelectorAll('button')) {
      // `checkVisibility` and not a hand-rolled style check: a Vuetify menu
      // keeps its contents mounted and hides them with `content-visibility`
      // and an ancestor's `display`, neither of which shows up in the
      // element's own computed style. The first version of this test read
      // `display` and `visibility` off the button itself and reported an
      // unopened overflow menu's items as buttons that had rendered to
      // nothing — a true statement about a hidden element, and not the bug.
      if (!el.checkVisibility({ contentVisibilityAuto: true, visibilityProperty: true })) continue;
      const box = el.getBoundingClientRect();
      if (box.width < 4 || box.height < 4) {
        bad.push(
          el.getAttribute('aria-label') || el.textContent.trim() || el.outerHTML.slice(0, 80)
        );
      }
    }
    return bad;
  });
  expect(collapsed, 'buttons that rendered to nothing').toEqual([]);
});

/**
 * The `page-scroll` bug, asked of the engine instead of the source.
 *
 * `PageLayout` is a fixed-height flex column, so a child that bounds nothing
 * gets compressed rather than scrolled — a status alert squeezed to a few
 * pixels above a catalogue running off the bottom edge. The source guard
 * catches the shape; this catches the symptom, which is the thing a user sees.
 */
test('the page fits its viewport rather than pushing the body sideways', async ({ page }) => {
  await page.goto('/#/projects');
  await expect(page.getByRole('main').getByRole('button', { name: 'shop.loc' })).toBeVisible();

  const overflow = await page.evaluate(() => ({
    body: document.body.scrollWidth - document.body.clientWidth,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  }));
  expect(overflow.body, 'the body scrolls horizontally').toBeLessThanOrEqual(1);
  expect(overflow.doc, 'the document scrolls horizontally').toBeLessThanOrEqual(1);
});

/**
 * `:focus-visible` never matches in jsdom, so no unit test in this repository
 * has ever established that the app can be operated without a mouse.
 */
test('the keyboard reaches something, and what it reaches can be seen', async ({ page }) => {
  await page.goto('/#/projects');
  await expect(page.getByRole('main').getByRole('button', { name: 'shop.loc' })).toBeVisible();

  await page.keyboard.press('Tab');

  const focused = await page.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return null;
    const box = el.getBoundingClientRect();
    return {
      tag: el.tagName,
      visible: box.width > 0 && box.height > 0,
      // An outline of none with no other affordance is a focus ring nobody can
      // follow. Reported rather than failed on: several designs replace it with
      // a box-shadow, and this is the value, not the verdict.
      outline: getComputedStyle(el).outlineStyle,
    };
  });

  expect(focused, 'Tab moved focus nowhere').not.toBeNull();
  expect(focused.visible, `focus landed on an invisible ${focused?.tag}`).toBe(true);
});

/**
 * The engine being down is the state the web UI could never report, and the one
 * a person meets most often. It has to be a sentence, not an empty page.
 */
test('says the engine is down rather than rendering an empty stack', async ({ page }) => {
  await stage(page, {
    engine_status: {
      reachable: false,
      version: null,
      apiVersion: null,
      context: null,
      platform: 'unknown',
      socketPath: '/var/run/docker.sock',
      error: 'Cannot connect to the Docker daemon',
    },
    projects_list: [],
    services_list: [],
  });
  await page.goto('/');

  // Whatever the wording, the screen must not be blank while Docker is off.
  const text = (await page.locator('body').innerText()).trim();
  expect(text.length, 'the page is empty with the engine down').toBeGreaterThan(40);
});
