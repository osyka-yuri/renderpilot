import { expect, test } from '@playwright/test';

import {
  expectNoAxeViolations,
  expectNoDocumentOverflow,
  expectPageLandmarks,
  LOCALES,
  preparePage,
} from './support/accessibility';

for (const locale of LOCALES) {
  test(`${locale}: language, landmarks, one games axe scan, and 320px bounds`, async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 720 });
    await preparePage(page, locale);

    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    await expect(page).toHaveTitle(/RenderPilot/);
    await expectPageLandmarks(page);
    await expect(page.getByRole('main').getByRole('heading', { level: 1 })).toHaveText(/\S/);
    await expectNoDocumentOverflow(page);
    await expectNoAxeViolations(page, `${locale}: games`);
  });
}
