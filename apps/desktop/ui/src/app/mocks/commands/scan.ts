import type { ScanManualFolderResult, AutoScanResponse } from '@entities/game';
import {
  mockState,
  getOrCreateManualGameId,
  findGameSummary,
  upsertGameSummary,
  createManualPreviewDetails,
  createGameSummaryFromDetails,
  hasAvailableBackup,
  getLatestOperationStatus,
  requireGameDetails,
} from '../desktop-state';
import { clone, lastPathSegment, normalizeInstallPath, resolveMock } from '../desktop-utils';

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
        update_count: details.candidate_groups.length,
        risk_level: 'medium',
        backup_available: hasAvailableBackup(details),
        last_operation_status: getLatestOperationStatus(details),
      }),
      cover_updated_at_ms: previousSummary?.cover_updated_at_ms ?? null,
    });

    return {
      games: [clone(details)],
    };
  });
}

export function mockScanAutoLibraries(): Promise<AutoScanResponse> {
  return resolveMock(() => {
    const games = [...mockState.autoGameIds].map((gameId) => requireGameDetails(gameId));

    return {
      games: clone(games),
      errors: [],
    };
  });
}
