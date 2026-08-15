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

/** The pages a person actually lands on, by the route that opens them. */
const PAGES = [
  ['dashboard', '/'],
  ['projects', '/#/projects'],
  ['market', '/#/market'],
  ['settings', '/#/settings'],
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

    const results = await new AxeBuilder({ page })
      // The rendered application, not the scaffolding around it.
      .include('#app')
      .analyze();

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
