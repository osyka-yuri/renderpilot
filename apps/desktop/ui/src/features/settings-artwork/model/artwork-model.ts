import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '@entities/settings';

export type CoverSourcePolicyKey = 'steamCdn' | 'gogCdn' | 'steamgriddb';

const coverSourcePolicyBySetting = {
  [COVERS_STEAM_CDN_SETTING_KEY]: 'steamCdn',
  [COVERS_GOG_CDN_SETTING_KEY]: 'gogCdn',
  [COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY]: 'steamgriddb',
} as const satisfies Record<string, CoverSourcePolicyKey>;

export type CoverSourceSettingKey = keyof typeof coverSourcePolicyBySetting;

export type CoverSourceToggleRow = {
  settingKey: CoverSourceSettingKey;
  policyKey: CoverSourcePolicyKey;
  ariaLabel: string;
  eyebrow: string;
  title: string;
  description: string;
};

type CoverSourceToggleRowDefinition = Omit<CoverSourceToggleRow, 'policyKey'>;

const coverSourceToggleRowDefinitions = [
  {
    settingKey: COVERS_STEAM_CDN_SETTING_KEY,
    ariaLabel: 'Use Steam CDN for artwork',
    eyebrow: 'Steam',
    title: 'Steam artwork',
    description: 'Uses public Steam library artwork when a Steam app ID is available.',
  },
  {
    settingKey: COVERS_GOG_CDN_SETTING_KEY,
    ariaLabel: 'Use GOG CDN for artwork',
    eyebrow: 'GOG',
    title: 'GOG artwork',
    description: 'Uses official GOG cover artwork when a numeric GOG product ID is available.',
  },
  {
    settingKey: COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
    ariaLabel: 'Use SteamGridDB as an artwork source',
    eyebrow: 'SteamGridDB',
    title: 'SteamGridDB artwork',
    description:
      'Uses SteamGridDB as an additional source for missing artwork and non-Steam titles. Requires an API key.',
  },
] as const satisfies readonly CoverSourceToggleRowDefinition[];

export const coverSourceToggleRows = coverSourceToggleRowDefinitions.map((row) => ({
  ...row,
  policyKey: coverSourcePolicyBySetting[row.settingKey],
})) satisfies readonly CoverSourceToggleRow[];

export const artworkSettingsReadError = 'Could not read automatic artwork settings.';
export const artworkSourceSaveError = 'Could not save artwork source setting.';

export function formatBooleanSetting(value: boolean): string {
  return String(value);
}
