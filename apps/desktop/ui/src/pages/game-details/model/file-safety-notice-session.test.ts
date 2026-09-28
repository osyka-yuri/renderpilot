import { beforeEach, describe, expect, it, vi } from 'vitest';

const { getCatalogSettingMock, setCatalogSettingMock } = vi.hoisted(() => ({
  getCatalogSettingMock: vi.fn(),
  setCatalogSettingMock: vi.fn(),
}));

vi.mock('@entities/settings', () => ({
  CATALOG_SETTING_KEYS: { GAME_FILE_SAFETY_WARNING_V1: 'game_file_safety_warning_v1' },
  getCatalogSetting: getCatalogSettingMock,
  setCatalogSetting: setCatalogSettingMock,
}));

describe('file safety notice session persistence', () => {
  beforeEach(() => {
    vi.resetModules();
    getCatalogSettingMock.mockReset().mockResolvedValue({ value: null });
    setCatalogSettingMock.mockReset().mockResolvedValue({ saved: true });
  });

  it('persists the explicit general warning opt-out under its versioned catalog key', async () => {
    const { persistGeneralFileSafetyWarningOptOut, shouldShowGeneralFileSafetyWarning } =
      await import('./file-safety-notice-session');

    await expect(shouldShowGeneralFileSafetyWarning()).resolves.toBe(true);
    await expect(persistGeneralFileSafetyWarningOptOut()).resolves.toBe(true);
    expect(setCatalogSettingMock).toHaveBeenCalledOnce();
    expect(setCatalogSettingMock).toHaveBeenCalledWith('game_file_safety_warning_v1', 'true');
    await expect(shouldShowGeneralFileSafetyWarning()).resolves.toBe(false);
  });

  it('keeps the general warning enabled when saving the explicit opt-out fails', async () => {
    setCatalogSettingMock.mockResolvedValueOnce({ saved: false });
    const { persistGeneralFileSafetyWarningOptOut, shouldShowGeneralFileSafetyWarning } =
      await import('./file-safety-notice-session');

    await expect(shouldShowGeneralFileSafetyWarning()).resolves.toBe(true);
    await expect(persistGeneralFileSafetyWarningOptOut()).resolves.toBe(false);
    await expect(shouldShowGeneralFileSafetyWarning()).resolves.toBe(true);

    vi.resetModules();
    const nextSession = await import('./file-safety-notice-session');
    await expect(nextSession.shouldShowGeneralFileSafetyWarning()).resolves.toBe(true);
  });
});
