import AxeBuilder from '@axe-core/playwright';
import { expect, type Locator, type Page } from '@playwright/test';

export const LOCALES = ['en', 'ru', 'es', 'fr', 'de', 'ja', 'zh-Hans', 'zh-Hant'] as const;

const WCAG_22_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'];

export async function preparePage(
  page: Page,
  locale: (typeof LOCALES)[number] = 'en',
  theme: 'light' | 'dark' = 'light',
): Promise<void> {
  await page.addInitScript(
    ({ language, selectedTheme }) => {
      localStorage.setItem(
        'renderpilot.language-mode',
        JSON.stringify({ version: 2, mode: language }),
      );
      if (localStorage.getItem('renderpilot.theme-mode') === null) {
        localStorage.setItem('renderpilot.theme-mode', selectedTheme);
      }
    },
    { language: locale, selectedTheme: theme },
  );
  await page.goto('/');
  await expectAppliedTheme(page, theme);
}

export async function setThemeMode(page: Page, theme: 'light' | 'dark'): Promise<void> {
  await page.evaluate((selectedTheme) => {
    localStorage.setItem('renderpilot.theme-mode', selectedTheme);
  }, theme);
  await page.reload();
  await expectAppliedTheme(page, theme);
}

async function expectAppliedTheme(page: Page, theme: 'light' | 'dark'): Promise<void> {
  await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
}

export async function waitForFiniteAnimations(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const finiteAnimations = document.getAnimations().filter((animation) => {
      const endTime = animation.effect?.getComputedTiming().endTime;
      return typeof endTime === 'number' && Number.isFinite(endTime) && endTime > 0;
    });

    await Promise.race([
      Promise.allSettled(finiteAnimations.map((animation) => animation.finished)),
      new Promise<void>((resolve) => window.setTimeout(resolve, 750)),
    ]);
  });
}

export async function expectNoAxeViolations(page: Page, context: string): Promise<void> {
  await waitForFiniteAnimations(page);
  const results = await new AxeBuilder({ page }).withTags(WCAG_22_TAGS).analyze();
  const report = results.violations
    .map(
      (violation) =>
        `${violation.id} (${violation.impact ?? 'unknown'}): ${violation.help}\n${violation.nodes
          .map((node) => `  ${node.target.join(' ')}: ${node.failureSummary ?? ''}`)
          .join('\n')}`,
    )
    .join('\n\n');

  expect(results.violations, `${context}\n${report}`).toEqual([]);
}

export function primaryNavigation(page: Page): Locator {
  return page.getByRole('navigation').filter({ has: page.locator('a[href="#settings"]') });
}

export function libraryTableRegion(page: Page): Locator {
  return page
    .getByRole('region')
    .filter({ has: page.getByRole('table') })
    .filter({ hasNot: page.getByRole('heading', { level: 1 }) });
}

export async function expectPageLandmarks(page: Page): Promise<void> {
  await expect(page.getByRole('main')).toHaveCount(1);
  await expect(page.getByRole('main').getByRole('heading', { level: 1 })).toHaveCount(1);
  const navigation = page.getByRole('navigation');
  await expect(navigation).toHaveCount(1);
  const navigationNames = await navigation.evaluateAll((elements) =>
    elements.map((element) => element.getAttribute('aria-label')?.trim() ?? ''),
  );
  expect(navigationNames.every((name) => name.length > 0)).toBe(true);
  expect(new Set(navigationNames).size).toBe(1);
  await expect(page.getByRole('main')).toHaveAccessibleName(/\S/);
}

export async function expectNoDocumentOverflow(page: Page): Promise<void> {
  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    document: document.documentElement.scrollWidth,
    body: document.body.scrollWidth,
  }));

  expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport);
  expect(dimensions.body).toBeLessThanOrEqual(dimensions.viewport);
}
