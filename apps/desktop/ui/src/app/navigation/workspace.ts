import type { Screen } from './screen';
import { isString } from '@shared/validation';

const DETAILS_WORKSPACE_SCREEN = 'details' as const satisfies Screen;
const OPERATIONS_WORKSPACE_SCREEN = 'operations' as const satisfies Screen;

const WORKSPACE_SCREENS = [
  DETAILS_WORKSPACE_SCREEN,
  OPERATIONS_WORKSPACE_SCREEN,
] as const satisfies readonly Screen[];

export type WorkspaceScreen = (typeof WORKSPACE_SCREENS)[number];

export const DEFAULT_WORKSPACE_SCREEN = DETAILS_WORKSPACE_SCREEN satisfies WorkspaceScreen;

const WORKSPACE_SCREEN_SET: ReadonlySet<string> = new Set(WORKSPACE_SCREENS);

export function isWorkspaceScreen(value: unknown): value is WorkspaceScreen {
  return isString(value) && WORKSPACE_SCREEN_SET.has(value);
}

export function getScreenAfterRollback(screen: Screen): WorkspaceScreen {
  return screen === OPERATIONS_WORKSPACE_SCREEN
    ? OPERATIONS_WORKSPACE_SCREEN
    : DEFAULT_WORKSPACE_SCREEN;
}
