import { invokeDesktop } from '@shared/api';
import { ClientError, reportClientError } from '@shared/errors';
import { isPlainObject, requireNonBlankString } from '@shared/validation';
import type { AutoScanResponse } from '@entities/game';
import type { AddGameInspection, AddGameRequest, AddGameResult } from '../model/add-game';
import { normalizeAddGameWarnings } from '../model/add-game-warning';
import type { ManifestRefreshReport } from '../model/manifest-refresh';

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop('scan_auto_libraries');
}

export async function inspectGameInstall(path: string): Promise<AddGameInspection> {
  return invokeWithNormalizedWarnings<AddGameInspection>('inspect_game_install', {
    path: requireNonBlankString(path, 'path'),
  });
}

export async function addGame(request: AddGameRequest): Promise<AddGameResult> {
  return invokeWithNormalizedWarnings<AddGameResult>('add_game', {
    selectedRoot: requireNonBlankString(request.selectedRoot, 'selectedRoot'),
    rootChoice: request.rootChoice,
    allowRootCorrection: request.allowRootCorrection,
    chosenExecutable: request.chosenExecutable,
    inspectionFingerprint: requireNonBlankString(
      request.inspectionFingerprint,
      'inspectionFingerprint',
    ),
  });
}

async function invokeWithNormalizedWarnings<
  Result extends { warnings: ReturnType<typeof normalizeAddGameWarnings> },
>(
  operation: 'inspect_game_install' | 'add_game',
  payload: Record<string, unknown>,
): Promise<Result> {
  const response = await invokeDesktop<unknown>(operation, payload);
  if (!isPlainObject(response) || !Array.isArray(response.warnings)) {
    const error = new ClientError('desktop_transport_failed', response);
    reportClientError(operation, error);
    throw error;
  }

  return {
    ...response,
    warnings: normalizeAddGameWarnings(response.warnings, operation),
  } as Result;
}

/**
 * Force-refreshes all remote CDN manifests (libraries, RenoDX, Luma, ReShade).
 * Backend applies cooldown / single-flight; partial failures are encoded in the
 * report and do not throw for the overall command — shell Refresh always
 * proceeds to disk scan. Hard invoke failures (task crash, IPC) may still reject.
 */
export async function refreshRemoteManifests(): Promise<ManifestRefreshReport> {
  return invokeDesktop<ManifestRefreshReport>('refresh_remote_manifests');
}

/** Rebuilds the durable capability projection after scan/manifest changes. */
export async function refreshCatalogCapabilities(): Promise<{ refreshed: boolean }> {
  return invokeDesktop<{ refreshed: boolean }>('refresh_catalog_capabilities');
}
