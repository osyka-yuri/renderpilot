import { openExternal } from '@shared/api';

const DEVELOPER_MODE_SETTINGS_URI = 'ms-settings:developers';
const DEVELOPER_MODE_DOCUMENTATION_URL =
  'https://learn.microsoft.com/en-us/windows/advanced-settings/developer-mode';

/** Opens Windows Developer Settings or official guidance in browser preview. */
export function openDeveloperModeSettings(): Promise<void> {
  return openExternal(DEVELOPER_MODE_SETTINGS_URI, {
    previewUrl: DEVELOPER_MODE_DOCUMENTATION_URL,
  });
}
