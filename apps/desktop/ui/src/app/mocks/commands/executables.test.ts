import type { EffectiveExecutable, ExecutableCandidate } from '@features/nvapi-settings';
import { beforeEach, describe, expect, it } from 'vitest';

import { mockInvoker, resetMockDesktopState } from '../desktop';

const GAME_ID = 'steam:1091500';

describe('preview executable commands', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('lists candidates, resolves auto, persists an override, and clears it through the dispatcher', async () => {
    const candidates = await mockInvoker<ExecutableCandidate[]>('list_game_executable_candidates', {
      gameId: GAME_ID,
    });

    expect(candidates).toHaveLength(2);
    expect(candidates.every((candidate) => candidate.rejection === null)).toBe(true);
    const automatic = await mockInvoker<EffectiveExecutable | null>('resolve_game_executable', {
      gameId: GAME_ID,
    });
    expect(automatic).toEqual({
      file_name: candidates[0]?.file_name,
      absolute_path: candidates[0]?.absolute_path,
      source: 'auto',
    });

    const overridePath = candidates[1]?.absolute_path;
    if (!overridePath) {
      throw new Error('Fixture prerequisite: expected a second executable candidate.');
    }

    await mockInvoker<undefined>('set_game_executable_override', {
      gameId: GAME_ID,
      absolutePath: overridePath,
    });
    await expect(
      mockInvoker<EffectiveExecutable | null>('resolve_game_executable', { gameId: GAME_ID }),
    ).resolves.toEqual({
      file_name: candidates[1]?.file_name,
      absolute_path: overridePath,
      source: 'override',
    });

    await mockInvoker<undefined>('clear_game_executable_override', { gameId: GAME_ID });
    await expect(
      mockInvoker<EffectiveExecutable | null>('resolve_game_executable', { gameId: GAME_ID }),
    ).resolves.toEqual({
      file_name: candidates[0]?.file_name,
      absolute_path: candidates[0]?.absolute_path,
      source: 'auto',
    });
  });

  it('rejects an override that is not a supported candidate identity', async () => {
    await expect(
      mockInvoker<undefined>('set_game_executable_override', {
        gameId: GAME_ID,
        absolutePath: 'C:/untrusted/Unknown.exe',
      }),
    ).rejects.toThrow('must match a supported candidate');
  });
});
