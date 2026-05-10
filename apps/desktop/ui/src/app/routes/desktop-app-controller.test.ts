import { describe, expect, it } from 'vitest';
import type { Screen } from '@app/routes/screen';
import {
  DEFAULT_BACK_TARGET,
  DEFAULT_WORKSPACE_SCREEN,
  formatPartialScanWarning,
  getScreenAfterRollback,
  getSettingsBackTarget,
  isWorkspaceScreen,
  resolveBackTarget,
  type BackTarget,
  type WorkspaceScreen,
} from '@app/routes/desktop-app-controller';

type WorkspaceScreenGuardCase = {
  name: string;
  value: unknown;
  expected: boolean;
};

type SettingsBackTargetCase = {
  name: string;
  screen: Screen;
  hasSelectedGameDetails: boolean;
  expected: BackTarget;
};

type ResolveBackTargetCase = {
  name: string;
  backTarget: BackTarget;
  hasSelectedGameDetails: boolean;
  expected: BackTarget;
};

type RollbackScreenCase = {
  name: string;
  screen: Screen;
  expected: WorkspaceScreen;
};

type PartialScanWarningCase = {
  name: string;
  scanErrorCount: number;
  expected: string;
};

const workspaceScreenGuardCases = [
  {
    name: 'details workspace screen',
    value: 'details',
    expected: true,
  },
  {
    name: 'operations workspace screen',
    value: 'operations',
    expected: true,
  },
  {
    name: 'games screen',
    value: 'games',
    expected: false,
  },
  {
    name: 'settings screen',
    value: 'settings',
    expected: false,
  },
  {
    name: 'empty string',
    value: '',
    expected: false,
  },
  {
    name: 'null',
    value: null,
    expected: false,
  },
  {
    name: 'undefined',
    value: undefined,
    expected: false,
  },
  {
    name: 'number',
    value: 123,
    expected: false,
  },
  {
    name: 'plain object',
    value: {},
    expected: false,
  },
] satisfies readonly WorkspaceScreenGuardCase[];

const settingsBackTargetCases = [
  {
    name: 'keeps details when a game is selected',
    screen: 'details',
    hasSelectedGameDetails: true,
    expected: 'details',
  },
  {
    name: 'keeps operations when a game is selected',
    screen: 'operations',
    hasSelectedGameDetails: true,
    expected: 'operations',
  },
  {
    name: 'falls back from details when no game is selected',
    screen: 'details',
    hasSelectedGameDetails: false,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'falls back from operations when no game is selected',
    screen: 'operations',
    hasSelectedGameDetails: false,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'falls back from games even when a game is selected',
    screen: 'games',
    hasSelectedGameDetails: true,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'falls back from settings even when a game is selected',
    screen: 'settings',
    hasSelectedGameDetails: true,
    expected: DEFAULT_BACK_TARGET,
  },
] satisfies readonly SettingsBackTargetCase[];

const resolveBackTargetCases = [
  {
    name: 'keeps details when a game is selected',
    backTarget: 'details',
    hasSelectedGameDetails: true,
    expected: 'details',
  },
  {
    name: 'falls back from details when no game is selected',
    backTarget: 'details',
    hasSelectedGameDetails: false,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'keeps operations when a game is selected',
    backTarget: 'operations',
    hasSelectedGameDetails: true,
    expected: 'operations',
  },
  {
    name: 'falls back from operations when no game is selected',
    backTarget: 'operations',
    hasSelectedGameDetails: false,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'keeps games when a game is selected',
    backTarget: DEFAULT_BACK_TARGET,
    hasSelectedGameDetails: true,
    expected: DEFAULT_BACK_TARGET,
  },
  {
    name: 'keeps games when no game is selected',
    backTarget: DEFAULT_BACK_TARGET,
    hasSelectedGameDetails: false,
    expected: DEFAULT_BACK_TARGET,
  },
] satisfies readonly ResolveBackTargetCase[];

const rollbackScreenCases = [
  {
    name: 'keeps operations after rollback',
    screen: 'operations',
    expected: 'operations',
  },
  {
    name: 'uses default workspace screen after details rollback',
    screen: 'details',
    expected: DEFAULT_WORKSPACE_SCREEN,
  },
  {
    name: 'uses default workspace screen after games rollback',
    screen: 'games',
    expected: DEFAULT_WORKSPACE_SCREEN,
  },
  {
    name: 'uses default workspace screen after settings rollback',
    screen: 'settings',
    expected: DEFAULT_WORKSPACE_SCREEN,
  },
] satisfies readonly RollbackScreenCase[];

const partialScanWarningCases = [
  {
    name: 'single failed root',
    scanErrorCount: 1,
    expected: 'Some game libraries could not be scanned (1 root failed). Check logs for details.',
  },
  {
    name: 'two failed roots',
    scanErrorCount: 2,
    expected: 'Some game libraries could not be scanned (2 roots failed). Check logs for details.',
  },
  {
    name: 'many failed roots',
    scanErrorCount: 10,
    expected: 'Some game libraries could not be scanned (10 roots failed). Check logs for details.',
  },
] satisfies readonly PartialScanWarningCase[];

const invalidScanErrorCounts = [
  0,
  -0,
  -1,
  1.5,
  Number.NaN,
  Number.POSITIVE_INFINITY,
  Number.NEGATIVE_INFINITY,
  Number.MAX_SAFE_INTEGER + 1,
] as const;

describe('desktop-app-controller', () => {
  describe('defaults', () => {
    it('uses games as the default back target', () => {
      expect(DEFAULT_BACK_TARGET).toBe('games');
    });

    it('uses details as the default workspace screen', () => {
      expect(DEFAULT_WORKSPACE_SCREEN).toBe('details');
      expect(isWorkspaceScreen(DEFAULT_WORKSPACE_SCREEN)).toBe(true);
    });
  });

  describe('isWorkspaceScreen', () => {
    it.each(workspaceScreenGuardCases)('returns $expected for $name', ({ value, expected }) => {
      expect(isWorkspaceScreen(value)).toBe(expected);
    });
  });

  describe('getSettingsBackTarget', () => {
    it.each(settingsBackTargetCases)('$name', ({ screen, hasSelectedGameDetails, expected }) => {
      expect(getSettingsBackTarget(screen, hasSelectedGameDetails)).toBe(expected);
    });
  });

  describe('resolveBackTarget', () => {
    it.each(resolveBackTargetCases)('$name', ({ backTarget, hasSelectedGameDetails, expected }) => {
      expect(resolveBackTarget(backTarget, hasSelectedGameDetails)).toBe(expected);
    });
  });

  describe('getScreenAfterRollback', () => {
    it.each(rollbackScreenCases)('$name', ({ screen, expected }) => {
      expect(getScreenAfterRollback(screen)).toBe(expected);
    });
  });

  describe('formatPartialScanWarning', () => {
    it.each(partialScanWarningCases)('$name', ({ scanErrorCount, expected }) => {
      expect(formatPartialScanWarning(scanErrorCount)).toBe(expected);
    });

    it.each(invalidScanErrorCounts)(
      'throws RangeError for invalid scan error count: %s',
      (scanErrorCount) => {
        expect(() => formatPartialScanWarning(scanErrorCount)).toThrow(RangeError);
        expect(() => formatPartialScanWarning(scanErrorCount)).toThrow(
          'scanErrorCount must be a positive integer.',
        );
      },
    );
  });
});
