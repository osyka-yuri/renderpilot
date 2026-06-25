import { open } from '@tauri-apps/plugin-shell';
import { isDesktopPreviewMode } from '@shared/api-preview';

/**
 * Opens `url` in the user's default browser. In preview mode (browser/dev
 * without Tauri) falls back to `window.open` so the link still works.
 */
export async function openExternal(url: string): Promise<void> {
  if (isDesktopPreviewMode()) {
    window.open(url, '_blank', 'noopener,noreferrer');
    return;
  }

  await open(url);
}
