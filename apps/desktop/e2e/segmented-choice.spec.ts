import { expect, test } from '@playwright/test';

import {
  expectNoAxeViolations,
  expectNoDocumentOverflow,
  preparePage,
} from './support/accessibility';

test('theme choice stays selected on repeat activation and changes with arrow keys', async ({
  page,
}) => {
  await preparePage(page);
  await page.getByRole('link', { name: 'Settings' }).click();

  const themes = page.getByRole('radiogroup', { name: 'Theme' });
  const light = themes.getByRole('radio', { name: 'Light' });
  const dark = themes.getByRole('radio', { name: 'Dark' });

  await expect(light).toBeChecked();
  await light.click();
  await expect(light).toBeChecked();
  await light.press('Space');
  await expect(light).toBeChecked();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await dark.click();
  await expect(dark).toBeChecked();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await dark.press('ArrowRight');
  await expect(light).toBeChecked();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('library type remains selected on repeat activation', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('link', { name: 'Libraries' }).click();

  const types = page.getByRole('radiogroup', { name: 'Library types' });
  const selected = types.getByRole('radio', { checked: true });
  await expect(selected).toBeVisible();
  await selected.click();
  await expect(selected).toBeChecked();
  await selected.press('Space');
  await expect(selected).toBeChecked();
});

test('the full AMD type selector wraps without clipping at 320px', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('link', { name: 'Libraries' }).click();
  await page.getByRole('tab', { name: 'AMD' }).click();
  await page.setViewportSize({ width: 320, height: 720 });

  const types = page.getByRole('radiogroup', { name: 'Library types' });
  const options = types.getByRole('radio');
  await expect(options).toHaveCount(7);

  const positions = await options.evaluateAll((items) =>
    items.map((item) => {
      const rect = item.getBoundingClientRect();
      return { left: rect.left, right: rect.right, top: rect.top };
    }),
  );
  expect(new Set(positions.map(({ top }) => top)).size).toBeGreaterThan(1);
  expect(positions.every(({ left, right }) => left >= 0 && right <= 320)).toBe(true);
  await expectNoDocumentOverflow(page);

  const last = options.last();
  await last.click();
  await expect(last).toBeChecked();
});

test('Russian theme choices fit and remain readable at 320px', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 720 });
  await preparePage(page, 'ru');
  await page.locator('[data-slot="sidebar-trigger"]').click();
  const sidebar = page.getByRole('dialog');
  await sidebar.locator('a[href="#settings"]').click();
  await expect(sidebar).toBeHidden();

  const theme = page.getByRole('radiogroup', { name: 'Тема' });
  const items = theme.getByRole('radio');
  await expect(items).toHaveCount(3);

  const bounds = await items.evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, right: rect.right, top: rect.top };
    }),
  );
  expect(new Set(bounds.map(({ top }) => top)).size).toBe(1);
  expect(bounds.every(({ left, right }) => left >= 0 && right <= 320)).toBe(true);
  await expectNoAxeViolations(page, 'Russian 320px theme setting');
});
