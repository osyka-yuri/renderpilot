import { openFolderPicker } from '@shared/api';

/** Opens the system folder picker for a manual scan. Returns the chosen path or null. */
export async function selectManualScanFolder(): Promise<string | null> {
  return openFolderPicker({ title: 'Select a folder to scan for games' });
}
