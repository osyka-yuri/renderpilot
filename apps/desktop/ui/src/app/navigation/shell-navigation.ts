import type { Screen } from './screen';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';

type PrimaryScreen = Extract<Screen, 'games' | 'libraries' | 'settings'>;

export type ShellBreadcrumbEntry =
  | {
      id: string;
      kind: 'link';
      label: string;
      target: Screen;
    }
  | {
      id: string;
      kind: 'page';
      label: string;
    };

export type ShellPrimaryNavigationItem = {
  screen: PrimaryScreen;
  label: string;
  isActive: boolean;
  ariaCurrent: 'page' | 'location' | undefined;
};

export type ShellNavigation = {
  pageLabel: string;
  breadcrumbLabel: string;
  primaryNavigationLabel: string;
  breadcrumbs: ShellBreadcrumbEntry[];
  primaryNavigation: ShellPrimaryNavigationItem[];
};

const PRIMARY_NAVIGATION = [
  { screen: 'games', labelKey: 'nav.games' },
  { screen: 'libraries', labelKey: 'nav.libraries' },
  { screen: 'settings', labelKey: 'nav.settings' },
] as const satisfies readonly { screen: PrimaryScreen; labelKey: MessageKeyWithoutParams }[];

export function createShellNavigation(screen: Screen, gameTitle: string): ShellNavigation {
  return {
    pageLabel: pageLabelForScreen(screen, gameTitle),
    breadcrumbLabel: t('nav.breadcrumbLabel'),
    primaryNavigationLabel: t('nav.primaryLabel'),
    breadcrumbs: breadcrumbsForScreen(screen, gameTitle),
    primaryNavigation: PRIMARY_NAVIGATION.map((item) => ({
      screen: item.screen,
      label: t(item.labelKey),
      isActive: isPrimaryNavigationActive(screen, item.screen),
      ariaCurrent: primaryNavigationCurrent(screen, item.screen),
    })),
  };
}

function pageLabelForScreen(screen: Screen, gameTitle: string): string {
  switch (screen) {
    case 'details':
      return gameTitle;
    case 'operations':
      return t('nav.operations');
    case 'libraries':
      return t('nav.libraries');
    case 'settings':
      return t('nav.settings');
    default:
      return t('nav.games');
  }
}

function breadcrumbsForScreen(screen: Screen, gameTitle: string): ShellBreadcrumbEntry[] {
  switch (screen) {
    case 'games':
      return [{ id: 'games-page', kind: 'page', label: t('nav.games') }];
    case 'settings':
      return [{ id: 'settings-page', kind: 'page', label: t('nav.settings') }];
    case 'libraries':
      return [{ id: 'libraries-page', kind: 'page', label: t('nav.libraries') }];
    case 'details':
      return [
        { id: 'games-link', kind: 'link', label: t('nav.games'), target: 'games' },
        { id: 'game-page', kind: 'page', label: gameTitle },
      ];
    case 'operations':
      return [
        { id: 'games-link', kind: 'link', label: t('nav.games'), target: 'games' },
        { id: 'game-link', kind: 'link', label: gameTitle, target: 'details' },
        { id: 'operations-page', kind: 'page', label: t('nav.operations') },
      ];
  }
}

function isPrimaryNavigationActive(screen: Screen, target: PrimaryScreen): boolean {
  return (
    screen === target || (target === 'games' && (screen === 'details' || screen === 'operations'))
  );
}

function primaryNavigationCurrent(
  screen: Screen,
  target: PrimaryScreen,
): 'page' | 'location' | undefined {
  if (screen === target) {
    return 'page';
  }

  return target === 'games' && (screen === 'details' || screen === 'operations')
    ? 'location'
    : undefined;
}
