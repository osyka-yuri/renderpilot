import type { ScanManualFolderResult, AutoScanResponse } from '@entities/game';
import { createGameSummaryFromDetails, getLatestOperationStatus } from '../build-game-summary';
import {
  mockState,
  getOrCreateManualGameId,
  findGameSummary,
  upsertGameSummary,
} from '../desktop-state';
import { clone, lastPathSegment, normalizeInstallPath, resolveMock } from '../desktop-utils';
import { createManualPreviewDetails } from '../fixtures';

export function mockScanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return resolveMock(() => {
    const installPath = normalizeInstallPath(path);
    const gameId = getOrCreateManualGameId(installPath);
    const title = lastPathSegment(installPath) || 'Manual Game';

    const previousDetails = mockState.detailsByGameId.get(gameId);
    const previousSummary = findGameSummary(gameId);

    const details = createManualPreviewDetails(gameId, title, installPath);
    details.operations = previousDetails ? clone(previousDetails.operations) : [];

    mockState.detailsByGameId.set(gameId, details);

    upsertGameSummary({
      ...createGameSummaryFromDetails(details, {
        risk_level: 'medium',
        rollback_available: false,
        last_operation_status: getLatestOperationStatus(details),
      }),
      cover_updated_at_ms: previousSummary?.cover_updated_at_ms ?? null,
    });

    return {
      addedGameIds: previousDetails ? [] : [gameId],
      updatedGameIds: previousDetails ? [gameId] : [],
      changedGameIds: [gameId],
      removedGameIds: [],
    };
  });
}

export function mockScanAutoLibraries(): Promise<AutoScanResponse> {
  return resolveMock(() => {
    return {
      addedGameIds: [],
      updatedGameIds: [...mockState.autoGameIds],
      changedGameIds: [...mockState.autoGameIds],
      removedGameIds: [],
      errors: [],
    };
  });
}
