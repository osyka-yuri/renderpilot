import type { EffectiveExecutable, ExecutableCandidate } from '@features/nvapi-settings';
import { clearGameExecutableOverride, mockState, requireGameDetails } from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockListGameExecutableCandidates(gameId: string): Promise<ExecutableCandidate[]> {
  return resolveMock(() => clone(executableCandidatesForGame(gameId)));
}

export function mockResolveGameExecutable(gameId: string): Promise<EffectiveExecutable | null> {
  return resolveMock(() => {
    const candidates = executableCandidatesForGame(gameId);
    const override = mockState.executableOverrideByGameId.get(gameId);
    const effective = override
      ? candidates.find((candidate) => candidate.absolute_path === override)
      : candidates[0];

    if (!effective) {
      return null;
    }

    return clone({
      file_name: effective.file_name,
      absolute_path: effective.absolute_path,
      source: override ? 'override' : 'auto',
    });
  });
}

export function mockSetGameExecutableOverride(gameId: string, absolutePath: string): Promise<void> {
  return resolveMock(() => {
    const path = requireNonEmptyText(absolutePath, 'executable absolute path');
    const supported = executableCandidatesForGame(gameId).some(
      (candidate) => candidate.absolute_path === path,
    );

    if (!supported) {
      throw new Error('Mock preview executable override must match a supported candidate.');
    }

    mockState.executableOverrideByGameId.set(gameId, path);
  });
}

export function mockClearGameExecutableOverride(gameId: string): Promise<void> {
  return resolveMock(() => {
    requireGameDetails(gameId);
    clearGameExecutableOverride(gameId);
  });
}

function executableCandidatesForGame(gameId: string): ExecutableCandidate[] {
  const details = requireGameDetails(gameId);
  const installPath = requireNonEmptyText(details.game.install_path, 'game install path');

  return [
    {
      relative_path: 'Game.exe',
      file_name: 'Game.exe',
      absolute_path: `${installPath}/Game.exe`,
      size_bytes: 134_217_728,
      depth: 0,
      rank_score: 100,
      rejection: null,
      rejection_token: null,
    },
    {
      relative_path: 'bin/Win64/Game-Win64-Shipping.exe',
      file_name: 'Game-Win64-Shipping.exe',
      absolute_path: `${installPath}/bin/Win64/Game-Win64-Shipping.exe`,
      size_bytes: 125_829_120,
      depth: 2,
      rank_score: 90,
      rejection: null,
      rejection_token: null,
    },
  ];
}
