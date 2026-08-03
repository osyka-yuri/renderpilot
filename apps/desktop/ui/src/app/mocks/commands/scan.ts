import type { AutoScanResponse } from '@entities/game';
import type { AddGameInspection, AddGameResult } from '@features/scan-libraries';
import { DesktopCommandError } from '@shared/errors';
import { createGameSummaryFromDetails, getLatestOperationStatus } from '../build-game-summary';
import {
  mockState,
  getOrCreateManualGameId,
  findGameSummary,
  upsertGameSummary,
} from '../desktop-state';
import {
  clone,
  createInstallPathKey,
  lastPathSegment,
  normalizeInstallPath,
  resolveMock,
} from '../desktop-utils';
import { createManualPreviewDetails } from '../fixtures';

export function mockInspectGameInstall(path: string): Promise<AddGameInspection> {
  return resolveMock(() => {
    const installPath = normalizeInstallPath(path);
    const installKey = createInstallPathKey(installPath);
    const existingId = mockState.manualGameIdByInstallPath.get(installKey);
    const exists = existingId !== undefined && findGameSummary(existingId) !== undefined;
    const contained = findContainedManualGames(installKey);
    // The in-memory mock has no filesystem topology evidence. It must not
    // invent an expansion from directory names; production returns one only
    // after InstallBoundaryAnalyzer proves the outer distribution root.
    if (!exists && contained.length > 0) {
      throw DesktopCommandError.fromDto({
        code: 'invalid_install_root',
        reasonCode: 'contains_proven_install',
      });
    }
    const containing = exists ? undefined : findContainingManualGame(installKey);
    const relationshipKind = exists ? 'exact_existing' : containing ? 'inside_existing' : 'new';
    const containingRoot = containing
      ? findGameSummary(containing.gameId)?.install_path
      : undefined;
    const relativeToContaining = containing
      ? installKey.slice(containing.installKey.length).replace(/^\/+/, '')
      : '';
    return {
      selectedRoot: installPath,
      inspectionFingerprint: `mock:${installKey}:${relationshipKind}`,
      catalogGeneration: 0,
      boundary: {
        kind: 'single_install',
        completeness: 'complete',
        candidateRoots: [installPath],
        evidence: ['root_executable'],
      },
      recommendation:
        containingRoot === undefined
          ? null
          : {
              root: containingRoot,
              source: 'existing_catalog',
              confidence: 'suggested',
              completeness: 'complete',
              evidence: [],
            },
      relationship: {
        kind: relationshipKind,
        gameIds: exists && existingId ? [existingId] : containing ? [containing.gameId] : [],
        provenInstallRoots: [],
      },
      executables: [
        {
          path: `${installPath}/Game.exe`,
          relativePath: 'Game.exe',
          sizeBytes: 1,
          rankScore: 100,
          validWindowsPe: true,
          rejectionKind: null,
          rejectionToken: null,
        },
      ],
      requiresExplicitExecutable: false,
      rootCorrection:
        containing !== undefined &&
        relativeToContaining.length > 0 &&
        !relativeToContaining.includes('/')
          ? {
              gameId: containing.gameId,
              status: 'ready',
              cleanupActions: [],
              blockers: [],
            }
          : null,
      decision:
        containing !== undefined &&
        relativeToContaining.length > 0 &&
        !relativeToContaining.includes('/')
          ? {
              kind: 'review',
              defaultOption: {
                rootChoice: 'selected',
                catalogAction: 'correct_existing_root',
              },
              options: [
                {
                  rootChoice: 'selected',
                  catalogAction: 'correct_existing_root',
                },
              ],
            }
          : {
              kind: 'automatic',
              option: {
                rootChoice: 'selected',
                catalogAction: exists ? 'rescan' : 'add',
              },
            },
      warnings: containing
        ? [
            {
              contractStatus: 'known',
              code: 'inside_existing_install',
              parameters: {},
            },
          ]
        : [],
    };
  });
}

export function mockAddGame(
  path: string,
  _rootChoice = 'selected',
  allowRootCorrection = false,
): Promise<AddGameResult> {
  return resolveMock(() => {
    const installPath = normalizeInstallPath(path);
    const installKey = createInstallPathKey(installPath);
    const correctionTarget = allowRootCorrection ? findContainingManualGame(installKey) : undefined;
    const gameId = correctionTarget?.gameId ?? getOrCreateManualGameId(installPath);
    const title = lastPathSegment(installPath) || 'Manual Game';

    const previousDetails = mockState.detailsByGameId.get(gameId);
    const previousSummary = findGameSummary(gameId);

    const details = createManualPreviewDetails(gameId, title, installPath);
    details.operations = previousDetails ? clone(previousDetails.operations) : [];

    mockState.detailsByGameId.set(gameId, details);
    if (correctionTarget !== undefined) {
      mockState.manualGameIdByInstallPath.delete(correctionTarget.installKey);
      mockState.manualGameIdByInstallPath.set(installKey, gameId);
    }

    upsertGameSummary({
      ...createGameSummaryFromDetails(details, {
        risk_level: 'medium',
        rollback_available: false,
        last_operation_status: getLatestOperationStatus(details),
      }),
      cover_updated_at_ms: previousSummary?.cover_updated_at_ms ?? null,
    });

    return {
      gameId,
      effectiveRoot: installPath,
      disposition: correctionTarget ? 'root_corrected' : previousDetails ? 'updated' : 'added',
      rootAuthority: 'user_confirmed',
      detectedLibraryCount: details.components.length,
      consolidatedGameIds: [],
      recoveryBundlePath: null,
      warnings: [],
    };
  });
}

function findContainedManualGames(
  selectedInstallKey: string,
): { installKey: string; gameId: string }[] {
  const prefix = `${selectedInstallKey.replace(/\/+$/, '')}/`;
  return [...mockState.manualGameIdByInstallPath.entries()]
    .filter(([installKey, gameId]) => {
      return installKey.startsWith(prefix) && findGameSummary(gameId) !== undefined;
    })
    .map(([installKey, gameId]) => ({ installKey, gameId }));
}

function findContainingManualGame(
  selectedInstallKey: string,
): { installKey: string; gameId: string } | undefined {
  return [...mockState.manualGameIdByInstallPath.entries()]
    .filter(([installKey, gameId]) => {
      return (
        selectedInstallKey.startsWith(`${installKey.replace(/\/+$/, '')}/`) &&
        findGameSummary(gameId) !== undefined
      );
    })
    .sort(([left], [right]) => right.length - left.length)
    .map(([installKey, gameId]) => ({ installKey, gameId }))[0];
}

export function mockScanAutoLibraries(): Promise<AutoScanResponse> {
  return resolveMock(() => {
    return {
      addedGameIds: [],
      updatedGameIds: [...mockState.autoGameIds],
      changedGameIds: [...mockState.autoGameIds],
      removedGameIds: [],
      partialFailureCount: 0,
    };
  });
}
