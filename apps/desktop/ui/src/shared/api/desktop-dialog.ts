import { open, type DialogFilter } from '@tauri-apps/plugin-dialog';
import { isDesktopPreviewMode } from '@shared/api-preview';

export type { DialogFilter };

export type FilePickerOptions = {
  filters?: DialogFilter[];
};

export type FolderPickerOptions = {
  title?: string;
};

/**
 * Opens the system file picker.
 * Returns the chosen file path, or null if cancelled or in preview mode.
 * In preview mode calls `onPreviewMode` instead of opening the dialog.
 */
export async function openFilePicker(
  options: FilePickerOptions & { onPreviewMode?: () => void },
): Promise<string | null> {
  if (isDesktopPreviewMode()) {
    options.onPreviewMode?.();
    return null;
  }

  const selected = await open({
    multiple: false,
    filters: options.filters,
  });

  return typeof selected === 'string' ? selected : null;
}

/**
 * Opens the system folder picker.
 * Returns the chosen folder path, or null if cancelled or in preview mode.
 */
export async function openFolderPicker(options: FolderPickerOptions = {}): Promise<string | null> {
  if (isDesktopPreviewMode()) {
    return null;
  }

  const selected = await open({
    directory: true,
    multiple: false,
    title: options.title,
  });

  return typeof selected === 'string' ? selected : null;
}
