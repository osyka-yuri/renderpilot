import { expect, test, type Locator, type Page } from '@playwright/test';

import { expectNoDocumentOverflow, preparePage } from './support/accessibility';

type Point = { x: number; y: number };
type LauncherMoveMetadata = { label: string; position: number; total: number };

function gameOptionsTrigger(page: Page): Locator {
  return page.getByRole('button', { name: /^Options for / }).first();
}

function gameDetailsTrigger(page: Page): Locator {
  return page.getByRole('button', { name: /^Open details for / }).first();
}

function launcherList(dialog: Locator): Locator {
  return dialog.getByRole('list').first();
}

function launcherRows(dialog: Locator): Locator {
  return launcherList(dialog).getByRole('listitem');
}

function launcherMoveButtons(dialog: Locator): Locator {
  return launcherList(dialog).getByRole('button', { name: /^Move .+, position \d+ of \d+$/ });
}

async function orderedLauncherNames(dialog: Locator): Promise<string[]> {
  return (await launcherMoveMetadata(dialog)).map(({ label }) => label);
}

async function projectedLauncherNames(dialog: Locator): Promise<string[]> {
  return launcherList(dialog)
    .locator('li')
    .evaluateAll((rows) =>
      rows
        .map((row) => {
          const name = row.querySelector('button[aria-label^="Move "]')?.getAttribute('aria-label');
          return name?.match(/^Move (.+), position \d+ of \d+$/)?.[1] ?? null;
        })
        .filter((label): label is string => label !== null),
    );
}

async function launcherMoveMetadata(dialog: Locator): Promise<LauncherMoveMetadata[]> {
  return launcherMoveButtons(dialog).evaluateAll((buttons) =>
    buttons.map((button) => {
      const name = button.getAttribute('aria-label');
      const match = name?.match(/^Move (.+), position (\d+) of (\d+)$/);

      if (!match) {
        throw new Error(`Expected a semantic launcher move name, received ${name ?? 'none'}.`);
      }

      const [, label, positionText, totalText] = match;
      return { label, position: Number(positionText), total: Number(totalText) };
    }),
  );
}

async function launcherItemLabels(dialog: Locator): Promise<string[]> {
  return launcherRows(dialog).evaluateAll((rows) =>
    rows.map((row) => row.getAttribute('aria-label')?.trim() ?? ''),
  );
}

async function requireAtLeastTwoLaunchers(dialog: Locator): Promise<void> {
  const count = await launcherRows(dialog).count();

  if (count < 2) {
    throw new Error(`Fixture prerequisite: expected at least two launchers, received ${count}.`);
  }
}

async function expectLauncherMoveTargetsMeetMinimumSize(dialog: Locator): Promise<void> {
  const sizes = await launcherMoveButtons(dialog).evaluateAll((buttons) =>
    buttons.map((button) => {
      const { offsetWidth: width, offsetHeight: height } = button;

      return { width, height };
    }),
  );

  expect(sizes.length).toBeGreaterThan(0);

  for (const [index, size] of sizes.entries()) {
    expect(size.width, `Launcher move control ${index} width`).toBeGreaterThanOrEqual(24);
    expect(size.height, `Launcher move control ${index} height`).toBeGreaterThanOrEqual(24);
  }
}

async function rowForLauncher(dialog: Locator, label: string): Promise<Locator> {
  const row = launcherRows(dialog).filter({ hasText: label });
  await expect(row).toHaveCount(1);
  return row;
}

async function locatorCenter(locator: Locator): Promise<Point> {
  const box = await locator.boundingBox();

  if (!box) {
    throw new Error('Expected a visible launcher pointer target.');
  }

  return {
    x: box.x + box.width / 2,
    y: box.y + box.height / 2,
  };
}

async function beginPointerDrag(page: Page, dialog: Locator, moveButton: Locator): Promise<void> {
  const source = await locatorCenter(moveButton);
  await page.mouse.move(source.x, source.y);
  await page.mouse.down();
  await page.mouse.move(source.x, source.y + 12, { steps: 3 });
  await expect(dialog.locator('[data-is-dnd-shadow-item-hint="true"]')).toHaveCount(1);
}

async function moveButtonForLauncher(dialog: Locator, label: string): Promise<Locator> {
  const button = (await rowForLauncher(dialog, label)).getByRole('button', {
    name: new RegExp(`^Move ${label}, position \\d+ of \\d+$`),
  });
  await expect(button).toHaveCount(1);
  return button;
}

async function dropAtRow(
  page: Page,
  dialog: Locator,
  row: Locator,
  expectedOrder: readonly string[],
): Promise<void> {
  const target = await locatorCenter(row);
  await page.mouse.move(target.x, target.y, { steps: 10 });
  await expect.poll(() => projectedLauncherNames(dialog)).toEqual(expectedOrder);
  await page.mouse.up();
  await expect(dialog.locator('[data-is-dnd-shadow-item-hint="true"]')).toHaveCount(0);
}

async function movePointerOutsideList(page: Page, list: Locator): Promise<void> {
  const box = await list.boundingBox();

  if (!box) {
    throw new Error('Expected a visible launcher list boundary.');
  }

  await page.mouse.move(box.x + box.width + 32, box.y - 16, { steps: 12 });
}

async function expectLauncherOrder(dialog: Locator, expected: readonly string[]): Promise<void> {
  await expect.poll(() => orderedLauncherNames(dialog)).toEqual(expected);
  await expect(launcherRows(dialog)).toHaveCount(expected.length);
}

async function visibleLibraryVersionOrder(table: Locator): Promise<string[]> {
  const rowTexts = await table.getByRole('row').allTextContents();

  return rowTexts
    .map((text) => text.trim().match(/^\d+\.\d+\.\d+/)?.[0])
    .filter((version): version is string => version !== undefined);
}

test('English keyboard and overlay interactions retain focus and state', async ({ page }) => {
  await page.context().grantPermissions(['clipboard-write']);
  await preparePage(page);

  const skipLink = page.getByRole('link', { name: 'Skip to content' });
  await skipLink.focus();
  await expect(skipLink).toBeFocused();
  const hashBeforeSpace = await page.evaluate(() => location.hash);
  await page.keyboard.press('Space');
  await expect(skipLink).toBeFocused();
  await expect.poll(() => page.evaluate(() => location.hash)).toBe(hashBeforeSpace);
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main')).toBeFocused();

  const menuTrigger = gameOptionsTrigger(page);
  await menuTrigger.focus();
  await page.keyboard.press('Enter');
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menuTrigger).toBeFocused();

  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  const initialLauncherOrder = await orderedLauncherNames(filters);
  const moveHandleLabel = initialLauncherOrder[0]!;
  const moveHandle = await moveButtonForLauncher(filters, moveHandleLabel);
  const moveRow = await rowForLauncher(filters, moveHandleLabel);
  await moveHandle.focus();
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowDown');
  await expect(moveRow).toBeFocused();
  await page.keyboard.press('Enter');
  await expectLauncherOrder(filters, [
    initialLauncherOrder[1]!,
    initialLauncherOrder[0]!,
    ...initialLauncherOrder.slice(2),
  ]);
  await moveHandle.focus();
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Escape');
  await expect(filters).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Filters' })).toBeFocused();

  await gameDetailsTrigger(page).click();
  const executableTrigger = page.getByRole('button', { name: /^Game executable:/ });
  await executableTrigger.click();
  const executables = page.getByRole('radiogroup');
  await expect(executables).toBeVisible();
  const radioMetadata = await executables.getByRole('radio').evaluateAll((radios) =>
    radios.map((radio, index) => ({
      index,
      checked: radio.getAttribute('aria-checked') === 'true',
      value: radio.getAttribute('data-value'),
    })),
  );
  const uncheckedRadio = radioMetadata.find((radio) => !radio.checked && radio.value?.trim());
  if (!uncheckedRadio) {
    throw new Error('Fixture prerequisite: expected an unchecked executable radio with data-value');
  }

  const executablePath = uncheckedRadio.value;
  await executables.getByRole('radio').nth(uncheckedRadio.index).click();
  await expect(executables).toBeHidden();
  await executableTrigger.click();
  await expect(executables).toBeVisible();
  await expect
    .poll(() =>
      executables
        .getByRole('radio')
        .evaluateAll((radios) =>
          radios
            .filter((radio) => radio.getAttribute('aria-checked') === 'true')
            .map((radio) => radio.getAttribute('data-value')),
        ),
    )
    .toEqual([executablePath]);

  await page.getByRole('link', { name: 'Libraries' }).click();
  const copyHash = page.getByRole('button', { name: /^Copy hash for / }).first();
  await copyHash.click();
  await expect(page.getByText('Hash copied to clipboard')).toBeVisible();
});

test('pointer launcher reordering commits only valid in-zone terminal drafts', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  await requireAtLeastTwoLaunchers(filters);

  const initial = await orderedLauncherNames(filters);
  const [first, second] = initial;
  const firstAfterSecond = [second, first, ...initial.slice(2)];
  await beginPointerDrag(page, filters, await moveButtonForLauncher(filters, first));
  await dropAtRow(page, filters, await rowForLauncher(filters, second), firstAfterSecond);
  await expectLauncherOrder(filters, firstAfterSecond);

  await beginPointerDrag(page, filters, await moveButtonForLauncher(filters, second));
  await movePointerOutsideList(page, launcherList(filters));
  await page.mouse.up();
  await expectLauncherOrder(filters, firstAfterSecond);
});

test('launcher labels toggle once without beginning a pointer reorder', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  await requireAtLeastTwoLaunchers(filters);

  const orderBeforeClick = await orderedLauncherNames(filters);
  const label = orderBeforeClick[0]!;
  const row = await rowForLauncher(filters, label);
  const launcherSwitch = row.getByRole('switch', { name: label });
  const checkedBeforeClick = await launcherSwitch.getAttribute('aria-checked');
  if (checkedBeforeClick !== 'true' && checkedBeforeClick !== 'false') {
    throw new Error('Expected the launcher switch to expose a boolean checked state.');
  }
  await row.getByText(label, { exact: true }).click();
  await expect(launcherSwitch).toHaveAttribute(
    'aria-checked',
    checkedBeforeClick === 'true' ? 'false' : 'true',
  );
  await expectLauncherOrder(filters, orderBeforeClick);
});

test('Cancel restores and Apply retains launcher-order drafts on reopen', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  await requireAtLeastTwoLaunchers(filters);

  const initial = await orderedLauncherNames(filters);
  const [first, second] = initial;
  const reordered = [second, first, ...initial.slice(2)];

  await beginPointerDrag(page, filters, await moveButtonForLauncher(filters, first));
  await dropAtRow(page, filters, await rowForLauncher(filters, second), reordered);
  await expectLauncherOrder(filters, reordered);
  await filters.getByRole('button', { name: 'Cancel' }).click();
  await expect(filters).toBeHidden();

  await page.getByRole('button', { name: 'Filters' }).click();
  await expect(filters).toBeVisible();
  await expectLauncherOrder(filters, initial);

  await beginPointerDrag(page, filters, await moveButtonForLauncher(filters, first));
  await dropAtRow(page, filters, await rowForLauncher(filters, second), reordered);
  await expectLauncherOrder(filters, reordered);
  await filters.getByRole('button', { name: 'Apply' }).click();
  await expect(filters).toBeHidden();

  await page.getByRole('button', { name: 'Filters' }).click();
  await expect(filters).toBeVisible();
  await expectLauncherOrder(filters, reordered);
});

test('Russian launcher reorder keeps localized zone, item, and move labels', async ({ page }) => {
  await preparePage(page, 'ru');
  await page.getByRole('button', { name: 'Фильтры' }).click();
  const filters = page.getByRole('dialog', { name: 'Фильтры' });
  await expect(filters).toBeVisible();

  const list = filters.getByRole('list', { name: 'Зона сортировки лаунчеров' });
  await expect(list).toHaveCount(1);
  await expect(list.getByRole('listitem').first()).toHaveAttribute('aria-label', /Лаунчер/);

  const initial = await launcherItemLabels(filters);
  const itemLabel = initial[0]!;
  const zoneLabel = 'Зона сортировки лаунчеров';
  const liveRegion = page.locator('#dnd-action-aria-alert[role="alert"]');
  const firstMove = list.getByRole('button').first();
  await expect(firstMove).toHaveAttribute('aria-label', /^Переместить /);
  await firstMove.focus();
  await page.keyboard.press('Space');
  await expect(liveRegion).toHaveText(
    `${itemLabel} захвачен в зоне ${zoneLabel}, позиция 1 из ${initial.length}.`,
  );
  await page.keyboard.press('ArrowDown');
  await expect(liveRegion).toHaveText(`${itemLabel} перемещён на позицию 2 из ${initial.length}.`);
  await page.keyboard.press('Enter');
  await expect(liveRegion).toHaveText(
    `${itemLabel} размещён в зоне ${zoneLabel}, позиция 2 из ${initial.length}.`,
  );
  await expect
    .poll(() => launcherItemLabels(filters))
    .toEqual([initial[1]!, initial[0]!, ...initial.slice(2)]);
});

test('Tab traversal reaches launcher move controls and switches in sequence', async ({ page }) => {
  await preparePage(page);
  await page.getByRole('button', { name: 'Filters' }).click();
  const filters = page.getByRole('dialog', { name: 'Filters' });
  await expect(filters).toBeVisible();
  await requireAtLeastTwoLaunchers(filters);
  await expectLauncherMoveTargetsMeetMinimumSize(filters);

  const [first, second] = await orderedLauncherNames(filters);
  const firstMove = launcherMoveButtons(filters).nth(0);
  const secondMove = launcherMoveButtons(filters).nth(1);
  const firstSwitch = (await rowForLauncher(filters, first)).getByRole('switch', { name: first });

  await firstMove.focus();
  await expect(firstMove).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(firstSwitch).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(secondMove).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(firstSwitch).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(firstMove).toBeFocused();
});

test('library version sorting exposes aria-sort and visible row order in both directions', async ({
  page,
}) => {
  await preparePage(page);
  await page.getByRole('link', { name: 'Libraries' }).click();

  const table = page.getByRole('table');
  const versionHeader = table.getByRole('columnheader', { name: 'Version' });
  const sortButton = versionHeader.getByRole('button', { name: 'Sort by Version' });

  await expect(versionHeader).toHaveAttribute('aria-sort', 'descending');

  await sortButton.focus();
  await page.keyboard.press('Enter');
  await expect(versionHeader).toHaveAttribute('aria-sort', 'none');
  await expect(sortButton).toBeFocused();
  await expect.poll(() => visibleLibraryVersionOrder(table)).toHaveLength(3);

  await page.keyboard.press('Enter');
  await expect(versionHeader).toHaveAttribute('aria-sort', 'ascending');
  await expect(sortButton).toBeFocused();
  await expect.poll(() => visibleLibraryVersionOrder(table)).toEqual(['2.0.1', '3.8.0', '3.10.0']);

  await page.keyboard.press('Enter');
  await expect(versionHeader).toHaveAttribute('aria-sort', 'descending');
  await expect(sortButton).toBeFocused();
  await expect.poll(() => visibleLibraryVersionOrder(table)).toEqual(['3.10.0', '3.8.0', '2.0.1']);
});

test('mobile sidebar closes before destination focus', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 720 });
  await preparePage(page);

  await page.getByRole('button', { name: 'Toggle sidebar' }).click();
  const sidebar = page.getByRole('dialog', { name: 'Navigation' });
  await expect(sidebar).toBeVisible();
  await sidebar.getByRole('link', { name: 'Libraries' }).click();
  await expect(sidebar).toBeHidden();
  await expect(page.getByRole('main', { name: 'Libraries' })).toBeFocused();
  await expectNoDocumentOverflow(page);
});
