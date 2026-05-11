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
    title: 'Steam CDN',
    description: 'Public Steam library artwork when the catalog has a Steam app id.',
  },
  {
    settingKey: COVERS_GOG_CDN_SETTING_KEY,
    ariaLabel: 'Use GOG CDN for artwork',
    eyebrow: 'GOG',
    title: 'GOG CDN',
    description: 'GOG vertical covers when the catalog has a numeric GOG product id.',
  },
  {
    settingKey: COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
    ariaLabel: 'Use SteamGridDB for artwork search',
    eyebrow: 'SteamGridDB',
    title: 'Remote search',
    description:
      'Slug lookups, autocomplete, and grid images via the SteamGridDB API (requires a key).',
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
