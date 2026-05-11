import type { Screen } from './screen';
import { isString } from '@shared/utils';

const DETAILS_WORKSPACE_SCREEN = 'details' as const satisfies Screen;
const OPERATIONS_WORKSPACE_SCREEN = 'operations' as const satisfies Screen;
const GAMES_BACK_TARGET = 'games' as const;

const WORKSPACE_SCREENS = [
  DETAILS_WORKSPACE_SCREEN,
  OPERATIONS_WORKSPACE_SCREEN,
] as const satisfies readonly Screen[];

export type WorkspaceScreen = (typeof WORKSPACE_SCREENS)[number];

export const DEFAULT_BACK_TARGET = GAMES_BACK_TARGET;
export const DEFAULT_WORKSPACE_SCREEN = DETAILS_WORKSPACE_SCREEN satisfies WorkspaceScreen;

export type BackTarget = typeof DEFAULT_BACK_TARGET | WorkspaceScreen;

const WORKSPACE_SCREEN_SET: ReadonlySet<string> = new Set(WORKSPACE_SCREENS);

export function isWorkspaceScreen(value: unknown): value is WorkspaceScreen {
  return isString(value) && WORKSPACE_SCREEN_SET.has(value);
}

export function getSettingsBackTarget(screen: Screen, hasSelectedGameDetails: boolean): BackTarget {
  if (!hasSelectedGameDetails) {
    return DEFAULT_BACK_TARGET;
  }

  return toWorkspaceBackTarget(screen);
}

export function resolveBackTarget(
  backTarget: BackTarget,
  hasSelectedGameDetails: boolean,
): BackTarget {
  if (isUnavailableWorkspaceTarget(backTarget, hasSelectedGameDetails)) {
    return DEFAULT_BACK_TARGET;
  }

  return backTarget;
}

export function getScreenAfterRollback(screen: Screen): WorkspaceScreen {
  return screen === OPERATIONS_WORKSPACE_SCREEN
    ? OPERATIONS_WORKSPACE_SCREEN
    : DEFAULT_WORKSPACE_SCREEN;
}

function toWorkspaceBackTarget(screen: Screen): BackTarget {
  return isWorkspaceScreen(screen) ? screen : DEFAULT_BACK_TARGET;
}

function isUnavailableWorkspaceTarget(
  backTarget: BackTarget,
  hasSelectedGameDetails: boolean,
): boolean {
  return !hasSelectedGameDetails && isWorkspaceScreen(backTarget);
}
