import { expect, test } from '@playwright/test';

import {
  expectNoAxeViolations,
  expectNoDocumentOverflow,
  preparePage,
} from './support/accessibility';

test('switches between cards and rows with visible selection and persisted preference', async ({
  page,
}) => {
  await preparePage(page);

  const cards = page.getByRole('radio', { name: 'Cards' });
  const list = page.getByRole('radio', { name: 'List' });
  const viewControl = page.getByRole('radiogroup', { name: 'Game view' });
  await expect(cards).toBeChecked();
  expect(await cards.evaluate((element) => element.textContent?.trim())).toBe('');
  expect(await list.evaluate((element) => element.textContent?.trim())).toBe('');
  expect(await viewControl.evaluate((element) => element.getBoundingClientRect().height)).toBe(
    await page
      .getByRole('button', { name: 'Filters' })
      .evaluate((element) => element.getBoundingClientRect().height),
  );
  await cards.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Cards');
  const cardsBackground = await cards.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  const inactiveBackground = await list.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  expect(cardsBackground).not.toBe(inactiveBackground);
  await expect(page.locator('[data-game-id]').first()).toBeVisible();

  await list.click();
  await expect(list).toBeChecked();
  await expect(list).toBeFocused();
  const listBackground = await list.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  expect(listBackground).not.toBe(
    await cards.evaluate((element) => getComputedStyle(element).backgroundColor),
  );
  await expect(page.locator('[data-game-layout="list"]').first()).toBeVisible();
  await list.click();
  await expect(list).toBeChecked();
  await expect(page.locator('[data-game-layout="list"]').first()).toBeVisible();

  await page.reload();
  await expect(page.getByRole('radio', { name: 'List' })).toBeChecked();
  await expect(page.locator('[data-game-layout="list"]').first()).toBeVisible();

  await cards.click();
  await expect(cards).toBeChecked();
  await expect(page.locator('[data-game-layout="list"]')).toHaveCount(0);
});

test('list row actions remain compact, accessible, and functional', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('radio', { name: 'List' }).click();

  const menuTrigger = page.getByRole('button', { name: /^Options for / }).first();
  await menuTrigger.click();
  await expect(page.getByRole('menu')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menuTrigger).toBeFocused();
  const detailsTrigger = page.getByRole('button', { name: /^Open details for / }).first();
  await expect(detailsTrigger).toBeVisible();
  await expect(detailsTrigger).toHaveText('');
  await expect(detailsTrigger.locator('svg')).toBeVisible();
  const detailsSize = await detailsTrigger.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return { width: bounds.width, height: bounds.height };
  });
  const menuSize = await menuTrigger.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return { width: bounds.width, height: bounds.height };
  });
  expect(detailsSize).toEqual(menuSize);
  await detailsTrigger.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Details');
  await page.mouse.move(0, 0);
  await expect(page.getByRole('tooltip')).toBeHidden();
  await menuTrigger.focus();
  await page.keyboard.press('Shift+Tab');
  await expect(detailsTrigger).toBeFocused();
  await expect(page.getByRole('tooltip')).toHaveText('Details');

  await page.getByRole('button', { name: 'Open details for Cyberpunk 2077' }).click();
  await expect(page.getByRole('tab', { name: 'Microsoft' })).toBeVisible();
});

test('switching views keeps the visible game in sight', async ({ page }) => {
  await page.setViewportSize({ width: 860, height: 500 });
  await preparePage(page);
  const viewport = page.locator('main [data-slot="scroll-area-viewport"]').first();
  await viewport.evaluate((element) => {
    element.scrollTop = Math.min(600, element.scrollHeight - element.clientHeight);
  });
  await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);

  const visibleGameId = await viewport.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return Array.from(element.querySelectorAll<HTMLElement>('[data-game-id]')).find((game) => {
      const gameBounds = game.getBoundingClientRect();
      return gameBounds.bottom > bounds.top + 2 && gameBounds.top < bounds.bottom;
    })?.dataset.gameId;
  });
  expect(visibleGameId).toBeTruthy();

  await page.getByRole('radio', { name: 'List' }).click();
  await expect
    .poll(() =>
      viewport.evaluate((element, gameId) => {
        const row = Array.from(
          element.querySelectorAll<HTMLElement>('[data-game-layout="list"]'),
        ).find((candidate) => candidate.dataset.gameId === gameId);
        if (!row) {
          return false;
        }
        const viewportBounds = element.getBoundingClientRect();
        const rowBounds = row.getBoundingClientRect();
        return rowBounds.bottom > viewportBounds.top + 20 && rowBounds.top < viewportBounds.bottom;
      }, visibleGameId),
    )
    .toBe(true);

  const listAnchorGameId = await viewport.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return Array.from(element.querySelectorAll<HTMLElement>('[data-game-layout="list"]')).find(
      (row) => {
        const rowBounds = row.getBoundingClientRect();
        return rowBounds.bottom > bounds.top + 2 && rowBounds.top < bounds.bottom;
      },
    )?.dataset.gameId;
  });
  expect(listAnchorGameId).toBeTruthy();

  await page.getByRole('radio', { name: 'Cards' }).click();
  await expect
    .poll(() =>
      viewport.evaluate((element, gameId) => {
        const card = Array.from(element.querySelectorAll<HTMLElement>('[data-game-id]')).find(
          (candidate) => candidate.dataset.gameId === gameId,
        );
        if (!card) {
          return false;
        }
        const viewportBounds = element.getBoundingClientRect();
        const cardBounds = card.getBoundingClientRect();
        return (
          cardBounds.bottom > viewportBounds.top + 20 && cardBounds.top < viewportBounds.bottom
        );
      }, listAnchorGameId),
    )
    .toBe(true);
});

test('switching views at the catalog top keeps the launcher heading visible', async ({ page }) => {
  await preparePage(page);
  const viewport = page.locator('main [data-slot="scroll-area-viewport"]').first();
  const firstLauncherHeading = viewport.locator('h2').first();

  await viewport.evaluate((element) => {
    element.scrollTop = 0;
  });
  await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBe(0);
  await expect(firstLauncherHeading).toBeInViewport();

  await page.getByRole('radio', { name: 'List' }).click();
  await expect(page.getByRole('radio', { name: 'List' })).toBeChecked();
  await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBe(0);
  await expect(firstLauncherHeading).toBeInViewport();
});

test('rapid view changes restore the latest layout around the same game', async ({ page }) => {
  await page.setViewportSize({ width: 860, height: 500 });
  await preparePage(page);
  const viewport = page.locator('main [data-slot="scroll-area-viewport"]').first();
  await viewport.evaluate((element) => {
    element.scrollTop = Math.min(600, element.scrollHeight - element.clientHeight);
  });
  await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);

  const visibleGameId = await viewport.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    return Array.from(element.querySelectorAll<HTMLElement>('[data-game-id]')).find((game) => {
      const gameBounds = game.getBoundingClientRect();
      return gameBounds.bottom > bounds.top + 2 && gameBounds.top < bounds.bottom;
    })?.dataset.gameId;
  });
  expect(visibleGameId).toBeTruthy();

  await page.evaluate(async () => {
    const originalRequestAnimationFrame = window.requestAnimationFrame.bind(window);
    const deferredFrames: FrameRequestCallback[] = [];
    window.requestAnimationFrame = (callback) => {
      deferredFrames.push(callback);
      return deferredFrames.length;
    };

    const waitForDeferredFrame = async (): Promise<void> => {
      const timeoutAt = performance.now() + 1_000;
      while (deferredFrames.length === 0 && performance.now() < timeoutAt) {
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
      if (deferredFrames.length === 0) {
        throw new Error('View restoration did not reach its animation-frame boundary.');
      }
    };

    try {
      const list = document.querySelector<HTMLElement>('[role="radio"][aria-label="List"]');
      const cards = document.querySelector<HTMLElement>('[role="radio"][aria-label="Cards"]');
      if (!list || !cards) {
        throw new Error('Expected both catalog view controls.');
      }

      list.click();
      await waitForDeferredFrame();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      cards.click();
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    } finally {
      window.requestAnimationFrame = originalRequestAnimationFrame;
      for (const callback of deferredFrames) {
        callback(performance.now());
      }
    }
  });

  await expect(page.getByRole('radio', { name: 'Cards' })).toBeChecked();
  await expect(page.locator('[data-game-layout="list"]')).toHaveCount(0);
  await expect
    .poll(() =>
      viewport.evaluate((element, gameId) => {
        const game = Array.from(element.querySelectorAll<HTMLElement>('[data-game-id]')).find(
          (candidate) => candidate.dataset.gameId === gameId,
        );
        if (!game) {
          return false;
        }
        const viewportBounds = element.getBoundingClientRect();
        const gameBounds = game.getBoundingClientRect();
        return (
          gameBounds.bottom > viewportBounds.top + 20 && gameBounds.top < viewportBounds.bottom
        );
      }, visibleGameId),
    )
    .toBe(true);
});

test('Russian row view reflows within 320 CSS pixels', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 600 });
  await preparePage(page, 'ru');

  await page.getByRole('radio', { name: 'Список' }).click();
  const cyberpunkRow = page.locator('[data-game-layout="list"]').filter({
    has: page.getByRole('heading', { name: 'Cyberpunk 2077' }),
  });
  await expect(cyberpunkRow.getByText('DLSS RR', { exact: true })).toBeVisible();
  await expect(cyberpunkRow.getByText('DLSS SR', { exact: true })).toBeVisible();
  await expect(cyberpunkRow.getByText('D3D12 Agility', { exact: true })).toBeVisible();

  await page.setViewportSize({ width: 320, height: 720 });
  await expect(page.getByRole('tooltip')).toBeHidden();
  await expect(cyberpunkRow.getByText('+3', { exact: true })).toBeVisible();
  await expect(page.locator('[data-game-layout="list"]').first()).toBeVisible();
  await expect
    .poll(() =>
      page.locator('main [data-index]').evaluateAll((items) =>
        items.slice(1).every((item, index) => {
          const previous = items[index];
          return item.getBoundingClientRect().top >= previous.getBoundingClientRect().bottom - 1;
        }),
      ),
    )
    .toBe(true);
  await expectNoDocumentOverflow(page);
  await expectNoAxeViolations(page, 'Russian 320px game list');
});
