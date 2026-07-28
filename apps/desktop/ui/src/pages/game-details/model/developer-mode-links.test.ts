import { describe, expect, it, vi } from 'vitest';

const openExternal = vi.hoisted(() => vi.fn<() => Promise<void>>());

vi.mock('@shared/api', () => ({ openExternal }));

import { openDeveloperModeSettings } from './developer-mode-links';

describe('openDeveloperModeSettings', () => {
  it('keeps the system URI and browser fallback inside the game-details feature', async () => {
    openExternal.mockResolvedValue();

    await openDeveloperModeSettings();

    expect(openExternal).toHaveBeenCalledWith('ms-settings:developers', {
      previewUrl: 'https://learn.microsoft.com/en-us/windows/advanced-settings/developer-mode',
    });
  });
});
