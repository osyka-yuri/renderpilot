import { beforeEach, describe, expect, it, vi } from 'vitest';
import { openFilePicker } from '@shared/api';
import { publishCoverPickerPreviewModeNotification } from './notifications';
import { selectCoverFilePath } from './cover-ops';

vi.mock('@shared/api', () => ({
  openFilePicker: vi.fn(),
}));

vi.mock('./notifications', () => ({
  publishCoverPickerPreviewModeNotification: vi.fn(),
}));

describe('selectCoverFilePath', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('publishes the preview-mode notification and restores focus once', async () => {
    vi.mocked(openFilePicker).mockImplementation((options) => {
      if (options.onPreviewMode) {
        options.onPreviewMode();
      }

      return Promise.resolve(null);
    });

    const focusMenuTrigger = vi.fn();

    const result = await selectCoverFilePath('game-1', { focusMenuTrigger });

    expect(result).toBeNull();
    expect(publishCoverPickerPreviewModeNotification).toHaveBeenCalledTimes(1);
    expect(focusMenuTrigger).toHaveBeenCalledTimes(1);
    expect(focusMenuTrigger).toHaveBeenCalledWith('game-1');
  });

  it('restores focus when the picker is cancelled without preview mode', async () => {
    vi.mocked(openFilePicker).mockImplementation(() => Promise.resolve(null));

    const focusMenuTrigger = vi.fn();

    const result = await selectCoverFilePath('game-1', { focusMenuTrigger });

    expect(result).toBeNull();
    expect(publishCoverPickerPreviewModeNotification).not.toHaveBeenCalled();
    expect(focusMenuTrigger).toHaveBeenCalledTimes(1);
    expect(focusMenuTrigger).toHaveBeenCalledWith('game-1');
  });
});
