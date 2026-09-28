import { expect, test } from '@playwright/test';

import {
  expectNoDocumentOverflow,
  preparePage,
  waitForFiniteAnimations,
} from './support/accessibility';

test('long executable confirmation remains usable at 860 by 600', async ({ page }) => {
  await page.setViewportSize({ width: 860, height: 600 });
  await preparePage(page, 'en');
  await page.getByRole('button', { name: 'Open details for Cyberpunk 2077' }).click();
  await page.getByRole('tab', { name: 'Microsoft' }).click();
  await page.getByRole('button', { name: 'v1.606.4' }).click();
  await page.getByRole('option', { name: '1.619.1' }).click();

  const dialog = page.getByRole('dialog', { name: 'Change game executable' });
  await expect(dialog).toBeVisible();
  await waitForFiniteAnimations(page);
  const content = dialog.getByRole('region', { name: 'Change game executable' });
  const actionList = dialog.getByRole('list');
  await expect(actionList.getByRole('listitem')).toHaveCount(1);

  // The preview has one managed executable; duplicate its real card to exercise a long batch.
  await actionList.evaluate((list) => {
    const template = list.firstElementChild;
    if (!(template instanceof HTMLElement)) {
      throw new Error('Expected the prepared executable action');
    }

    for (let index = 0; index < 11; index += 1) {
      const item = template.cloneNode(true) as HTMLElement;
      const path = item.querySelector('code');
      if (path) {
        path.textContent = `C:/Games/Test/executable-${index}.exe`;
      }
      list.append(item);
    }
  });

  await expect(actionList.getByRole('listitem')).toHaveCount(12);
  await expectNoDocumentOverflow(page);

  const layout = async () =>
    dialog.evaluate((element) => {
      const bounds = (selector: string) => {
        const target = element.querySelector<HTMLElement>(selector);
        if (!target) {
          throw new Error(`Missing dialog element: ${selector}`);
        }
        const rect = target.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom };
      };
      const region = element.querySelector<HTMLElement>('[role="region"]');
      if (!region) {
        throw new Error('Missing confirmation scroll region');
      }

      return {
        dialog: element.getBoundingClientRect().toJSON(),
        header: bounds('[data-slot="dialog-header"]'),
        footer: bounds('[data-slot="dialog-footer"]'),
        scrollTop: region.scrollTop,
        scrollHeight: region.scrollHeight,
        clientHeight: region.clientHeight,
      };
    });

  const beforeScroll = await layout();
  expect(beforeScroll.dialog.top).toBeGreaterThanOrEqual(0);
  expect(beforeScroll.dialog.bottom).toBeLessThanOrEqual(600);
  expect(beforeScroll.dialog.height).toBeLessThanOrEqual(568);
  expect(beforeScroll.header.top).toBeGreaterThanOrEqual(beforeScroll.dialog.top);
  expect(beforeScroll.footer.bottom).toBeLessThanOrEqual(beforeScroll.dialog.bottom);
  expect(beforeScroll.scrollHeight).toBeGreaterThan(beforeScroll.clientHeight);
  expect(await actionList.evaluate((list) => list.scrollHeight === list.clientHeight)).toBe(true);

  await expect(content).toHaveAttribute('tabindex', '0');
  await content.focus();
  await page.keyboard.press('PageDown');

  const afterScroll = await layout();
  expect(afterScroll.scrollTop).toBeGreaterThan(0);
  expect(afterScroll.header.top).toBe(beforeScroll.header.top);
  expect(afterScroll.footer.bottom).toBe(beforeScroll.footer.bottom);
});
