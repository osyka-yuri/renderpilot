import { expect, test, type Locator, type Page } from '@playwright/test';

import {
  expectNoAxeViolations,
  expectNoDocumentOverflow,
  libraryTableRegion,
  primaryNavigation,
  preparePage,
  setThemeMode,
} from './support/accessibility';

const MOBILE_VIEWPORT = { width: 320, height: 720 };

function gameOptionsTrigger(page: Page): Locator {
  return page.getByRole('button', { name: /^Options for / }).first();
}

type FocusStyle = {
  outlineStyle: string;
  outlineWidth: number;
  outlineAlpha: number;
};

async function getFocusStyle(target: Locator): Promise<FocusStyle> {
  return target.evaluate((element) => {
    const style = getComputedStyle(element);
    const canvas = document.createElement('canvas');
    canvas.width = 1;
    canvas.height = 1;
    const context = canvas.getContext('2d');

    if (context === null) {
      throw new Error('Expected a 2D canvas context for focus-color verification.');
    }

    context.fillStyle = style.outlineColor;
    context.fillRect(0, 0, 1, 1);

    return {
      outlineStyle: style.outlineStyle,
      outlineWidth: Number.parseFloat(style.outlineWidth),
      outlineAlpha: context.getImageData(0, 0, 1, 1).data[3] / 255,
    };
  });
}

async function expectTabFocus(page: Page, before: Locator, target: Locator): Promise<void> {
  await before.focus();
  await page.keyboard.press('Tab');
  await expect(target).toBeFocused();

  const focusStyle = await getFocusStyle(target);
  expect(focusStyle.outlineStyle).toBe('solid');
  expect(focusStyle.outlineWidth).toBeGreaterThanOrEqual(2);
}

test('English light and dark overlays remain reachable and layered', async ({ page }) => {
  await preparePage(page);

  for (const theme of ['light', 'dark'] as const) {
    await setThemeMode(page, theme);
    const menuTrigger = gameOptionsTrigger(page);
    await menuTrigger.click();
    await expect(page.getByRole('menu')).toBeVisible();
    await page.keyboard.press('Escape');

    await page.getByRole('button', { name: 'Filters' }).click();
    await expect(page.getByRole('dialog', { name: 'Filters' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expectNoDocumentOverflow(page);
  }
});

test('reduced motion and forced colors preserve focus and bounds', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce', forcedColors: 'active' });
  await preparePage(page);

  const hiddenGames = page.getByRole('button', { name: 'Hidden games' });
  const filtersTrigger = page.getByRole('button', { name: 'Filters' });
  await expectTabFocus(page, hiddenGames, filtersTrigger);

  const settingsLink = page.getByRole('link', { name: 'Settings' });
  const sidebarTrigger = page.locator("[data-slot='sidebar-trigger']");
  await expectTabFocus(page, settingsLink, sidebarTrigger);

  const donateButton = page.getByRole('button', { name: 'Donate' });
  await expectTabFocus(page, sidebarTrigger, donateButton);

  const trigger = gameOptionsTrigger(page);
  await trigger.click();
  await expect(page.getByRole('menu')).toBeVisible();
  await page.keyboard.press('Escape');

  await filtersTrigger.click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  const launcherList = filters.getByRole('list').first();
  const firstMoveButton = launcherList.getByRole('button', { name: /^Move / }).first();
  await expectTabFocus(page, launcherList, firstMoveButton);
  await filters.getByRole('button', { name: 'Cancel' }).click();
  await expect(filters).toBeHidden();

  await expectNoDocumentOverflow(page);
  await expectNoAxeViolations(page, 'forced colors and reduced motion');
});

test('mobile Filters dialog keeps scrolling inside the labeled region', async ({ page }) => {
  await page.setViewportSize(MOBILE_VIEWPORT);
  await preparePage(page);

  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  const metrics = await filters.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return {
      top: rect.top,
      bottom: rect.bottom,
      viewportHeight: window.innerHeight,
      clientHeight: element.clientHeight,
    };
  });
  const region = filters.getByRole('region', { name: 'Filters' });
  const scrollMetrics = await region.evaluate((element) => ({
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
    overflowY: getComputedStyle(element).overflowY,
  }));

  expect(metrics.top).toBeGreaterThanOrEqual(-2);
  expect(metrics.bottom).toBeLessThanOrEqual(metrics.viewportHeight + 2);
  expect(metrics.clientHeight).toBeLessThanOrEqual(metrics.viewportHeight - 30);
  expect(scrollMetrics.scrollHeight).toBeGreaterThan(scrollMetrics.clientHeight);
  expect(scrollMetrics.overflowY).toMatch(/auto|scroll/);
  await expectNoDocumentOverflow(page);
});

test('Libraries table viewport keeps an opaque focus outline in light and dark themes', async ({
  page,
}) => {
  await preparePage(page);

  for (const theme of ['light', 'dark'] as const) {
    await setThemeMode(page, theme);

    await primaryNavigation(page).getByRole('link', { name: 'Libraries' }).click();

    const beforeViewport = page.locator('[data-slot="toggle-group-item"][tabindex="0"]');
    const viewport = libraryTableRegion(page);
    await expect(beforeViewport).toHaveCount(1);
    await expect(viewport).toHaveCount(1);

    await expectTabFocus(page, beforeViewport, viewport);

    const focusStyle = await getFocusStyle(viewport);
    expect(focusStyle.outlineAlpha).toBe(1);
  }
});
