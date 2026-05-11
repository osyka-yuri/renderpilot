import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '../model/catalog-setting-keys';
import type { CoverRemotePolicy } from '../model/view-model';

type CatalogSettingReader = (key: string) => Promise<{ value: string | null }>;

const BOOL_DEFAULT_TRUE_DISABLED_VALUES = new Set(['false', '0', 'no']);

function trimNullable(value: string | null): string {
  return value?.trim() ?? '';
}

/** True when the catalog setting row holds a non-blank SteamGridDB bearer token. */
export function catalogSettingHasSteamGridDbKey(value: string | null): boolean {
  return trimNullable(value).length > 0;
}

/** Matches Rust `parse_setting_bool_default_true`: only false / 0 / no (any case) disables. */
export function parseCatalogBoolDefaultTrue(value: string | null): boolean {
  const normalized = trimNullable(value);

  if (normalized.length === 0) {
    return true;
  }

  return !BOOL_DEFAULT_TRUE_DISABLED_VALUES.has(normalized.toLowerCase());
}

async function readBoolSettingDefaultTrue(
  getCatalogSetting: CatalogSettingReader,
  key: string,
): Promise<boolean> {
  const { value } = await getCatalogSetting(key);

  return parseCatalogBoolDefaultTrue(value);
}

export async function fetchCoverRemotePolicy(
  getCatalogSetting: CatalogSettingReader,
): Promise<CoverRemotePolicy> {
  const [steamCdn, gogCdn, steamgriddb] = await Promise.all([
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_STEAM_CDN_SETTING_KEY),
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_GOG_CDN_SETTING_KEY),
    readBoolSettingDefaultTrue(getCatalogSetting, COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY),
  ]);

  return { steamCdn, gogCdn, steamgriddb };
}

export async function fetchSteamGridDbKeyConfigured(
  getCatalogSetting: CatalogSettingReader,
  settingKey: string,
): Promise<boolean> {
  const { value } = await getCatalogSetting(settingKey);

  return catalogSettingHasSteamGridDbKey(value);
}
