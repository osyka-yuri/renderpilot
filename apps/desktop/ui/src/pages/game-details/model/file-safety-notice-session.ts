import { CATALOG_SETTING_KEYS, getCatalogSetting, setCatalogSetting } from '@entities/settings';

let generalWarningPreference: Promise<boolean> | null = null;

/** Reads the opt-out once per app session; failures deliberately mean "show it". */
export async function shouldShowGeneralFileSafetyWarning(): Promise<boolean> {
  generalWarningPreference ??= getCatalogSetting(CATALOG_SETTING_KEYS.GAME_FILE_SAFETY_WARNING_V1)
    .then(({ value }) => value === 'true')
    .catch(() => {
      generalWarningPreference = null;
      return false;
    });
  return !(await generalWarningPreference);
}

/** Saves an explicit opt-out from future general file safety warnings. */
export async function persistGeneralFileSafetyWarningOptOut(): Promise<boolean> {
  try {
    const result = await setCatalogSetting(
      CATALOG_SETTING_KEYS.GAME_FILE_SAFETY_WARNING_V1,
      'true',
    );
    if (!result.saved) {
      generalWarningPreference = Promise.resolve(false);
      return false;
    }
    generalWarningPreference = Promise.resolve(true);
    return true;
  } catch {
    generalWarningPreference = Promise.resolve(false);
    return false;
  }
}
