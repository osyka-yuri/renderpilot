import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  STEAMGRIDDB_SETTING_KEY,
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

/**
 * Parses a boolean-like setting where only false / 0 / no (any case) disables;
 * blank/missing values fall back to `defaultWhenAbsent`.
 */
export function parseCatalogBoolWithDefault(
  value: string | null,
  defaultWhenAbsent: boolean,
): boolean {
  const normalized = trimNullable(value);

  if (normalized.length === 0) {
    return defaultWhenAbsent;
  }

  return !BOOL_DEFAULT_TRUE_DISABLED_VALUES.has(normalized.toLowerCase());
}

/** Matches Rust `parse_setting_bool_default_true`: only false / 0 / no (any case) disables. */
export function parseCatalogBoolDefaultTrue(value: string | null): boolean {
  return parseCatalogBoolWithDefault(value, true);
}

export async function fetchCoverRemotePolicy(
  getCatalogSetting: CatalogSettingReader,
): Promise<CoverRemotePolicy> {
  const [steamCdn, gogCdn, steamgriddb, steamgriddbKey] = await Promise.all([
    getCatalogSetting(COVERS_STEAM_CDN_SETTING_KEY),
    getCatalogSetting(COVERS_GOG_CDN_SETTING_KEY),
    getCatalogSetting(COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY),
    getCatalogSetting(STEAMGRIDDB_SETTING_KEY),
  ]);

  // SteamGridDB does nothing without an API key — the Rust resolver returns
  // `CoverNotFound` when the key is missing regardless of this toggle. So when
  // the user has never set the toggle, default it to enabled only if a key is
  // already configured. This mirrors the backend's effective behavior and keeps
  // the toggle honest instead of showing "on" for a source that can't run.
  const hasSteamGridDbKey = catalogSettingHasSteamGridDbKey(steamgriddbKey.value);

  return {
    steamCdn: parseCatalogBoolDefaultTrue(steamCdn.value),
    gogCdn: parseCatalogBoolDefaultTrue(gogCdn.value),
    steamgriddb: parseCatalogBoolWithDefault(steamgriddb.value, hasSteamGridDbKey),
  };
}

export async function fetchSteamGridDbKeyConfigured(
  getCatalogSetting: CatalogSettingReader,
  settingKey: string,
): Promise<boolean> {
  const { value } = await getCatalogSetting(settingKey);

  return catalogSettingHasSteamGridDbKey(value);
}
