import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { stage } from './stage.js';

/**
 * axe over the whole page, in an engine that has laid it out.
 *
 * `a11y-axe.spec.js` beside this already runs axe — over four components, in
 * jsdom. That is worth having and it is not this: jsdom computes no colours, no
 * boxes and no stacking, so the rules it can decide are the ones about markup.
 * Contrast, focus order, an element covered by another, a label whose control
 * is somewhere else on screen — none of those exist until something has done
 * layout.
 *
 * This is also what §3 #25 is waiting for. An accessibility statement is a
 * claim about the product, and a claim needs a measurement; the note in that
 * row says the statement "cannot be produced without #12", and this is the half
 * of #12 that produces it.
 *
 * ## Serious and critical only, and why that is not a dodge
 *
 * axe grades its rules, and the two lower grades are largely advice whose right
 * answer depends on the design — "this landmark could be labelled", "this
 * heading order is unusual". Failing a build on those trains people to add
 * `disableRules` until the check means nothing. The two top grades are the ones
 * where a person is actually blocked, and those are held at zero.
 *
 * The counts of the lower two are printed rather than asserted, so the number
 * is visible on every run and a drift is something a reader can see without a
 * gate having to decide what it means.
 */

/**
 * Every route in the application, by the address that opens it.
 *
 * It was four of the nine, and the four were the ones somebody thought of. That
 * is a fine start and a poor basis for a conformance statement, which is a
 * claim about the product rather than about a sample of it — so the list is now
 * the router's, and `accessibility-claims.spec.js` fails if the two come apart.
 */
const PAGES = [
  ['dashboard', '/'],
  ['projects', '/#/projects'],
  ['project detail', '/#/projects/shop'],
  ['market', '/#/market'],
  ['logs', '/#/logs'],
  ['dumps', '/#/dumps'],
  ['mail', '/#/mail'],
  ['settings', '/#/settings'],
  ['about', '/#/about'],
];

for (const [name, route] of PAGES) {
  test(`${name} has no serious or critical axe violations`, async ({ page }) => {
    await stage(page);
    await page.goto(route);

    // The shell renders before its data arrives, and axe on a spinner is axe on
    // a page nobody sees. Waiting for the heading is waiting for the view to
    // have decided what it is.
    await expect(page.getByRole('main')).toBeVisible();
    await page.waitForLoadState('networkidle');

    // The whole document, and that is a correction rather than a widening for
    // its own sake. This scoped itself to `#app` — "the rendered application,
    // not the scaffolding around it" — and Vuetify's overlay container is a
    // sibling of `#app`, not a child of it. Every tooltip, menu, dialog and
    // side sheet in this application lives in that container, so the run that
    // reported "zero serious" had never looked at any of them. It was hiding
    // one serious rule (`aria-tooltip-name`, four nodes on the dashboard alone)
    // and the two page-level rules that only exist against `<html>`.
    const results = await new AxeBuilder({ page }).analyze();

    const bad = results.violations.filter((v) => ['serious', 'critical'].includes(v.impact));
    const rest = results.violations.filter((v) => !['serious', 'critical'].includes(v.impact));

    // Printed, not asserted — see the header.
    if (rest.length) {
      console.log(`${name}: ${rest.length} minor/moderate — ${rest.map((v) => v.id).join(', ')}`);
    }

    expect(
      bad.map((v) => `${v.id} (${v.impact}) × ${v.nodes.length}: ${v.help}`),
      `${name} blocks somebody`
    ).toEqual([]);
  });
}
