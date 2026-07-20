import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { AutoScanResponse, ScanManualFolderResult } from '@entities/game';
import type { ManifestRefreshReport } from '../model/manifest-refresh';

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop('scan_auto_libraries');
}

export async function scanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return invokeDesktop<ScanManualFolderResult>('scan_manual_folder', {
    path: requireNonBlankString(path, 'path'),
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
