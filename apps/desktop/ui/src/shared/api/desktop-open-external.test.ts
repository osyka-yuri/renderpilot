/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const shellOpen = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>());
const previewState = vi.hoisted(() => ({ enabled: false }));

vi.mock('@tauri-apps/plugin-shell', () => ({ open: shellOpen }));
vi.mock('@shared/api-preview', () => ({
  isDesktopPreviewMode: () => previewState.enabled,
}));

import { openExternal } from './desktop-open-external';

describe('openExternal', () => {
  beforeEach(() => {
    previewState.enabled = false;
    shellOpen.mockReset();
    shellOpen.mockResolvedValue();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('opens the requested target through the desktop shell', async () => {
    await openExternal('ms-settings:developers', {
      previewUrl: 'https://example.test/developer-mode',
    });

    expect(shellOpen).toHaveBeenCalledWith('ms-settings:developers');
  });

  it('opens the browser-safe fallback in preview mode', async () => {
    previewState.enabled = true;
    const windowOpen = vi.spyOn(window, 'open').mockReturnValue({} as Window);

    await openExternal('ms-settings:developers', {
      previewUrl: 'https://example.test/developer-mode',
    });

    expect(windowOpen).toHaveBeenCalledWith(
      'https://example.test/developer-mode',
      '_blank',
      'noopener,noreferrer',
    );
    expect(shellOpen).not.toHaveBeenCalled();
  });

  it('treats a null noopener result as a successful browser handoff', async () => {
    previewState.enabled = true;
    vi.spyOn(window, 'open').mockReturnValue(null);

    await expect(openExternal('https://example.test')).resolves.toBeUndefined();
  });

  it('propagates an error thrown while opening the browser target', async () => {
    previewState.enabled = true;
    vi.spyOn(window, 'open').mockImplementation(() => {
      throw new Error('browser handoff failed');
    });

    await expect(openExternal('https://example.test')).rejects.toMatchObject({
      code: 'external_open_failed',
    });
  });
});
