import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/utils';
import type { AutoScanResponse, ScanManualFolderResult } from '@entities/game';

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop('scan_auto_libraries');
}

export async function scanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return invokeDesktop<ScanManualFolderResult>('scan_manual_folder', {
    path: requireNonBlankString(path, 'path'),
  });
}
