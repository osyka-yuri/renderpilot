import { expect, test } from '@playwright/test';

import { expectNoDocumentOverflow, preparePage } from './support/accessibility';

test('game toolbar buttons follow viewport breakpoints after returning from operations', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await preparePage(page, 'ru');
  await page
    .getByRole('button', { name: /^Открыть подробности:/ })
    .first()
    .click();

  const journal = page.getByRole('button', { name: 'Журнал операций' });
  const journalLabel = journal.getByText('Журнал операций', { exact: true });
  const update = page.getByRole('button', { name: /Обновить всё/ });
  const updateLabel = update.locator('span');

  await expect(journalLabel).toBeVisible();
  await expect(updateLabel).toBeVisible();

  await page.setViewportSize({ width: 1100, height: 900 });
  await expect(journalLabel).toBeHidden();
  await expect(updateLabel).toBeVisible();

  await page.setViewportSize({ width: 960, height: 900 });
  await expect(journalLabel).toBeHidden();
  await expect(updateLabel).toBeHidden();
  await expect(journal).toHaveAttribute('aria-label', 'Журнал операций');
  await expect(update).toHaveAttribute('aria-label', /\S/);

  await page.setViewportSize({ width: 860, height: 900 });
  await expect(journal).toBeVisible();
  await expect(update).toBeVisible();
  await expect(journalLabel).toBeHidden();
  await expect(updateLabel).toBeHidden();

  // A wider tab set scrolls within its own region while the actions stay on one row.
  const tabList = page.getByRole('tablist');
  const tabRegion = tabList.locator('..');
  await tabList.evaluate((list) => {
    (list as HTMLElement).style.minWidth = '700px';
  });
  const tabsOverflow = await tabRegion.evaluate(
    (region) => region.scrollWidth > region.clientWidth,
  );
  expect(tabsOverflow).toBe(true);
  const tabCenter = await tabList.evaluate((list) => {
    const bounds = list.getBoundingClientRect();
    return bounds.top + bounds.height / 2;
  });
  const updateCenter = await update.evaluate((button) => {
    const bounds = button.getBoundingClientRect();
    return bounds.top + bounds.height / 2;
  });
  expect(Math.abs(updateCenter - tabCenter)).toBeLessThanOrEqual(3);
  await expectNoDocumentOverflow(page);

  await journal.click();
  await expect(page.locator('a[href="#details"]')).toBeVisible();
  await page.locator('a[href="#details"]').click();

  await expect(journalLabel).toBeHidden();
  await expect(updateLabel).toBeHidden();
});
