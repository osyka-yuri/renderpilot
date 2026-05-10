import type { ThemeMode } from '@shared/theme/theme-mode';
import type { CoverRemotePolicy } from '@shared/covers/cover-sync';
import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  STEAMGRIDDB_SETTING_KEY,
} from '@shared/catalog/catalog-setting-keys';

export type SelectOption<Value extends string> = {
  value: Value;
  label: string;
};

export type LanguageMode = 'system' | 'en' | 'ru';
export type CoverSourcePolicyKey = keyof CoverRemotePolicy;

export const steamGridDbSettingKey = STEAMGRIDDB_SETTING_KEY;

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

export const themeOptions = [
  { value: 'system', label: 'System' },
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
] as const satisfies readonly SelectOption<ThemeMode>[];

export const languageOptions = [
  { value: 'system', label: 'Follow system' },
  { value: 'en', label: 'English' },
  { value: 'ru', label: 'Russian' },
] as const satisfies readonly SelectOption<LanguageMode>[];

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

export const defaultCoverSourcesState = {
  steamCdn: true,
  gogCdn: true,
  steamgriddb: true,
} as const satisfies CoverRemotePolicy;

export const catalogReadError = 'Could not read catalog settings.';
export const steamKeySaveError = 'Could not save API key.';
export const artworkSettingsReadError = 'Could not read automatic artwork settings.';
export const artworkSourceSaveError = 'Could not save artwork source setting.';

export function formatBooleanSetting(value: boolean): string {
  return String(value);
}

export function isOptionValue<Value extends string>(
  value: string,
  options: readonly SelectOption<Value>[],
): value is Value {
  return options.some((option) => option.value === value);
}
