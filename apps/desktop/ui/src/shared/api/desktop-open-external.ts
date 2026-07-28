import { open } from '@tauri-apps/plugin-shell';
import { isDesktopPreviewMode } from '@shared/api-preview';

type OpenExternalOptions = {
  /** Browser-safe target used when the desktop runtime is unavailable. */
  previewUrl?: string;
};

/**
 * Opens a URL or an allowlisted system URI in the user's default handler. In
 * preview mode (browser/dev without Tauri), opens the explicit browser-safe
 * fallback when supplied.
 */
export async function openExternal(url: string, options: OpenExternalOptions = {}): Promise<void> {
  if (isDesktopPreviewMode()) {
    openBrowserWindow(options.previewUrl ?? url);
    return;
  }

  await open(url);
}

function openBrowserWindow(url: string): void {
  // With `noopener`, browsers intentionally return null even when the target
  // opened successfully. Only synchronous exceptions are observable here.
  window.open(url, '_blank', 'noopener,noreferrer');
}
