import { describe, expect, it, vi } from 'vitest';
import {
  withManualCoverBusy,
  isCoverOperationBusy,
  shouldCloseOpenMenu,
  pruneCoverMenuState,
} from './cover-ops';
import type { ManualCoverBusyParams } from './cover-ops';

const GAME_1 = 'game-1';
const GAME_2 = 'game-2';
const GAME_3 = 'game-3';

describe('cover-ops', () => {
  describe('withManualCoverBusy', () => {
    it('runs successful command in the expected order', async () => {
      const calls: string[] = [];
      const onCoverError = vi.fn();
      const onSuccess = vi.fn(() => {
        calls.push('success');
      });

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');
            return Promise.resolve();
          },
          onClearError: () => {
            calls.push('clear-error');
          },
          onReloadCards: () => {
            calls.push('reload');
            return Promise.resolve();
          },
          onSuccess,
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual([
        'busy:game-1',
        'task',
        'clear-error',
        'reload',
        'success',
        'busy:none',
        'focus:game-1',
      ]);
      expect(onSuccess).toHaveBeenCalledTimes(1);
      expect(onCoverError).not.toHaveBeenCalled();
    });

    it('skips command when another manual action is already running', async () => {
      const task = vi.fn(() => Promise.resolve());
      const setManualCoverBusyFor = vi.fn();
      const onClearError = vi.fn();
      const onReloadCards = vi.fn(() => Promise.resolve());
      const onCoverError = vi.fn();
      const focusMenuTrigger = vi.fn();

      await withManualCoverBusy(
        createManualCoverBusyParams({
          manualCoverBusyFor: GAME_2,
          setManualCoverBusyFor,
          task,
          onClearError,
          onReloadCards,
          onCoverError,
          focusMenuTrigger,
        }),
      );

      expect(task).not.toHaveBeenCalled();
      expect(setManualCoverBusyFor).not.toHaveBeenCalled();
      expect(onClearError).not.toHaveBeenCalled();
      expect(onReloadCards).not.toHaveBeenCalled();
      expect(onCoverError).not.toHaveBeenCalled();
      expect(focusMenuTrigger).not.toHaveBeenCalled();
    });

    it('reports task error and still restores busy state and focus', async () => {
      const calls: string[] = [];
      const error = new Error('failed');

      const onClearError = vi.fn(() => {
        calls.push('clear-error');
      });
      const onReloadCards = vi.fn(() => {
        calls.push('reload');
        return Promise.resolve();
      });
      const onSuccess = vi.fn(() => {
        calls.push('success');
      });
      const onCoverError = vi.fn((message: string) => {
        calls.push(`error:${message}`);
      });

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');
            return Promise.reject(error);
          },
          onClearError,
          onReloadCards,
          onSuccess,
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual(['busy:game-1', 'task', 'error:failed', 'busy:none', 'focus:game-1']);
      expect(onClearError).not.toHaveBeenCalled();
      expect(onReloadCards).not.toHaveBeenCalled();
      expect(onSuccess).not.toHaveBeenCalled();
      expect(onCoverError).toHaveBeenCalledWith('failed');
    });

    it('reports reload error and still restores busy state and focus', async () => {
      const calls: string[] = [];
      const reloadError = new Error('reload failed');

      const onClearError = vi.fn(() => {
        calls.push('clear-error');
      });
      const onSuccess = vi.fn(() => {
        calls.push('success');
      });
      const onCoverError = vi.fn((message: string) => {
        calls.push(`error:${message}`);
      });

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');
            return Promise.resolve();
          },
          onClearError,
          onReloadCards: () => {
            calls.push('reload');
            return Promise.reject(reloadError);
          },
          onSuccess,
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual([
        'busy:game-1',
        'task',
        'clear-error',
        'reload',
        'error:reload failed',
        'busy:none',
        'focus:game-1',
      ]);
      expect(onClearError).toHaveBeenCalledTimes(1);
      expect(onSuccess).not.toHaveBeenCalled();
      expect(onCoverError).toHaveBeenCalledWith('reload failed');
    });
  });

  describe('isCoverOperationBusy', () => {
    const cases: [
      caseName: string,
      gameId: string,
      manualCoverBusyFor: string | null,
      coversAutoFetchingIds: ReadonlySet<string>,
      expected: boolean,
    ][] = [
      ['manual operation targets the game', GAME_1, GAME_1, new Set(), true],
      ['auto operation targets the game', GAME_1, null, new Set([GAME_1]), true],
      ['operations target other games', GAME_1, GAME_2, new Set([GAME_3]), false],
    ];

    it.each(cases)(
      '%s',
      (_caseName, gameId, manualCoverBusyFor, coversAutoFetchingIds, expected) => {
        expect(isCoverOperationBusy(gameId, manualCoverBusyFor, coversAutoFetchingIds)).toBe(
          expected,
        );
      },
    );
  });

  describe('shouldCloseOpenMenu', () => {
    const cases: [
      caseName: string,
      menuOpenFor: string | null,
      manualCoverBusyFor: string | null,
      coversAutoFetchingIds: ReadonlySet<string>,
      expected: boolean,
    ][] = [
      ['menu is already closed', null, null, new Set([GAME_1]), false],
      ['any manual cover operation is running', GAME_1, GAME_2, new Set(), true],
      ['auto operation targets the open menu game', GAME_1, null, new Set([GAME_1]), true],
      ['auto operation targets another game', GAME_1, null, new Set([GAME_2]), false],
    ];

    it.each(cases)(
      '%s',
      (_caseName, menuOpenFor, manualCoverBusyFor, coversAutoFetchingIds, expected) => {
        expect(shouldCloseOpenMenu(menuOpenFor, manualCoverBusyFor, coversAutoFetchingIds)).toBe(
          expected,
        );
      },
    );
  });

  describe('pruneCoverMenuState', () => {
    it('prunes stale refs and clears stale open menu id', () => {
      const refs = {
        [GAME_1]: { id: 1 },
        [GAME_2]: { id: 2 },
      };

      const result = pruneCoverMenuState(refs, GAME_2, [GAME_1]);

      expect(result.refs).toEqual({
        [GAME_1]: refs[GAME_1],
      });
      expect(result.menuOpenFor).toBeNull();
    });

    it('keeps refs object identity when nothing was pruned', () => {
      const refs = {
        [GAME_1]: { id: 1 },
        [GAME_2]: { id: 2 },
      };

      const result = pruneCoverMenuState(refs, GAME_1, [GAME_1, GAME_2]);

      expect(result.refs).toBe(refs);
      expect(result.menuOpenFor).toBe(GAME_1);
    });

    it('clears open menu id even when refs do not need pruning', () => {
      const refs = {
        [GAME_1]: { id: 1 },
      };

      const result = pruneCoverMenuState(refs, GAME_2, [GAME_1]);

      expect(result.refs).toBe(refs);
      expect(result.menuOpenFor).toBeNull();
    });
  });
});

function createManualCoverBusyParams(
  overrides: Partial<ManualCoverBusyParams> = {},
): ManualCoverBusyParams {
  return {
    gameId: GAME_1,
    manualCoverBusyFor: null,
    setManualCoverBusyFor: vi.fn(),
    task: vi.fn(() => Promise.resolve()),
    onClearError: vi.fn(),
    onReloadCards: vi.fn(() => Promise.resolve()),
    onSuccess: vi.fn(),
    onCoverError: vi.fn(),
    describeError: (error) => (error instanceof Error ? error.message : 'unknown'),
    focusMenuTrigger: vi.fn(),
    ...overrides,
  };
}
