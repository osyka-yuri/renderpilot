import { beforeEach, describe, expect, it } from 'vitest';

import { setLanguageMode } from '@shared/i18n';

import { createShellNavigation } from './shell-navigation';

describe('createShellNavigation', () => {
  beforeEach(async () => {
    await setLanguageMode('en');
  });

  it.each([
    ['games', 'Games', [{ kind: 'page', label: 'Games' }]],
    ['libraries', 'Libraries', [{ kind: 'page', label: 'Libraries' }]],
    ['settings', 'Settings', [{ kind: 'page', label: 'Settings' }]],
    [
      'details',
      'Control',
      [
        { kind: 'link', label: 'Games' },
        { kind: 'page', label: 'Control' },
      ],
    ],
    [
      'operations',
      'Journal',
      [
        { kind: 'link', label: 'Games' },
        { kind: 'link', label: 'Control' },
        { kind: 'page', label: 'Journal' },
      ],
    ],
  ] as const)('creates labels and breadcrumbs for %s', (screen, pageLabel, breadcrumbs) => {
    const navigation = createShellNavigation(screen, 'Control');

    expect(navigation.pageLabel).toBe(pageLabel);
    expect(navigation.breadcrumbLabel).toBe('Breadcrumb');
    expect(navigation.primaryNavigationLabel).toBe('Primary navigation');
    expect(navigation.breadcrumbs.map(({ kind, label }) => ({ kind, label }))).toEqual(breadcrumbs);
  });

  it.each([
    ['games', ['page', undefined, undefined]],
    ['details', ['location', undefined, undefined]],
    ['operations', ['location', undefined, undefined]],
    ['libraries', [undefined, 'page', undefined]],
    ['settings', [undefined, undefined, 'page']],
  ] as const)('marks the primary navigation for %s', (screen, expectedCurrent) => {
    const navigation = createShellNavigation(screen, 'Control');

    expect(navigation.primaryNavigation.map((item) => item.ariaCurrent)).toEqual(expectedCurrent);
    expect(navigation.primaryNavigation.map((item) => item.isActive)).toEqual(
      expectedCurrent.map((current) => current !== undefined),
    );
  });
});
