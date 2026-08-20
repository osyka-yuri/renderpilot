import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  type CatalogBooleanSettingKey,
} from '@entities/settings';
import type { MessageKeyWithoutParams } from '@shared/i18n';

export type CoverSourcePolicyKey = 'steamCdn' | 'gogCdn' | 'steamgriddb';

const coverSourcePolicyBySetting = {
  [COVERS_STEAM_CDN_SETTING_KEY]: 'steamCdn',
  [COVERS_GOG_CDN_SETTING_KEY]: 'gogCdn',
  [COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY]: 'steamgriddb',
} as const satisfies Record<CatalogBooleanSettingKey, CoverSourcePolicyKey>;

export type CoverSourceSettingKey = CatalogBooleanSettingKey;

export type CoverSourceToggleRow = {
  settingKey: CoverSourceSettingKey;
  policyKey: CoverSourcePolicyKey;
  actionLabelKey: MessageKeyWithoutParams;
  eyebrow: string;
  titleKey: MessageKeyWithoutParams;
  descriptionKey: MessageKeyWithoutParams;
};

type CoverSourceToggleRowDefinition = Omit<CoverSourceToggleRow, 'policyKey'>;

const coverSourceToggleRowDefinitions = [
  {
    settingKey: COVERS_STEAM_CDN_SETTING_KEY,
    actionLabelKey: 'settings.catalog.source.steam.actionLabel',
    eyebrow: 'Steam',
    titleKey: 'settings.catalog.source.steam.title',
    descriptionKey: 'settings.catalog.source.steam.description',
  },
  {
    settingKey: COVERS_GOG_CDN_SETTING_KEY,
    actionLabelKey: 'settings.catalog.source.gog.actionLabel',
    eyebrow: 'GOG',
    titleKey: 'settings.catalog.source.gog.title',
    descriptionKey: 'settings.catalog.source.gog.description',
  },
  {
    settingKey: COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
    actionLabelKey: 'settings.catalog.source.steamgriddb.actionLabel',
    eyebrow: 'SteamGridDB',
    titleKey: 'settings.catalog.source.steamgriddb.title',
    descriptionKey: 'settings.catalog.source.steamgriddb.description',
  },
] as const satisfies readonly CoverSourceToggleRowDefinition[];

export const coverSourceToggleRows = coverSourceToggleRowDefinitions.map((row) => ({
  ...row,
  policyKey: coverSourcePolicyBySetting[row.settingKey],
})) satisfies readonly CoverSourceToggleRow[];
