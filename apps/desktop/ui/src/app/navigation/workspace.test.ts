import { describe, expect, it } from 'vitest';
import type { Screen } from './screen';
import {
  DEFAULT_WORKSPACE_SCREEN,
  getScreenAfterRollback,
  isWorkspaceScreen,
  type WorkspaceScreen,
} from './workspace';

type WorkspaceScreenGuardCase = {
  name: string;
  value: unknown;
  expected: boolean;
};

type RollbackScreenCase = {
  name: string;
  screen: Screen;
  expected: WorkspaceScreen;
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

describe('workspace navigation', () => {
  describe('defaults', () => {
    it('uses details as the default workspace screen', () => {
      expect(DEFAULT_WORKSPACE_SCREEN).toBe('details');
      expect(isWorkspaceScreen(DEFAULT_WORKSPACE_SCREEN)).toBe(true);
    });
  });

  describe('isWorkspaceScreen', () => {
    it.each(workspaceScreenGuardCases)('$name', ({ value, expected }) => {
      expect(isWorkspaceScreen(value)).toBe(expected);
    });
  });

  describe('getScreenAfterRollback', () => {
    it.each(rollbackScreenCases)('$name', ({ screen, expected }) => {
      expect(getScreenAfterRollback(screen)).toBe(expected);
    });
  });
});
