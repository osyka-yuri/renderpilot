import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '@entities/settings';
import type { MessageKey } from '@shared/i18n';

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
  ariaLabelKey: MessageKey;
  eyebrow: string;
  titleKey: MessageKey;
  descriptionKey: MessageKey;
};

type CoverSourceToggleRowDefinition = Omit<CoverSourceToggleRow, 'policyKey'>;

const coverSourceToggleRowDefinitions = [
  {
    settingKey: COVERS_STEAM_CDN_SETTING_KEY,
    ariaLabelKey: 'settings.catalog.source.steam.aria',
    eyebrow: 'Steam',
    titleKey: 'settings.catalog.source.steam.title',
    descriptionKey: 'settings.catalog.source.steam.description',
  },
  {
    settingKey: COVERS_GOG_CDN_SETTING_KEY,
    ariaLabelKey: 'settings.catalog.source.gog.aria',
    eyebrow: 'GOG',
    titleKey: 'settings.catalog.source.gog.title',
    descriptionKey: 'settings.catalog.source.gog.description',
  },
  {
    settingKey: COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
    ariaLabelKey: 'settings.catalog.source.steamgriddb.aria',
    eyebrow: 'SteamGridDB',
    titleKey: 'settings.catalog.source.steamgriddb.title',
    descriptionKey: 'settings.catalog.source.steamgriddb.description',
  },
] as const satisfies readonly CoverSourceToggleRowDefinition[];

export const coverSourceToggleRows = coverSourceToggleRowDefinitions.map((row) => ({
  ...row,
  policyKey: coverSourcePolicyBySetting[row.settingKey],
})) satisfies readonly CoverSourceToggleRow[];

export function formatBooleanSetting(value: boolean): string {
  return String(value);
}
