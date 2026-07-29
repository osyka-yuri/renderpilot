import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { AutoScanResponse } from '@entities/game';
import type { AddGameInspection, AddGameRequest, AddGameResult } from '../model/add-game';
import type { ManifestRefreshReport } from '../model/manifest-refresh';

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop('scan_auto_libraries');
}

export async function inspectGameInstall(path: string): Promise<AddGameInspection> {
  return invokeDesktop<AddGameInspection>('inspect_game_install', {
    path: requireNonBlankString(path, 'path'),
  });
}

export async function addGame(request: AddGameRequest): Promise<AddGameResult> {
  return invokeDesktop<AddGameResult>('add_game', {
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
