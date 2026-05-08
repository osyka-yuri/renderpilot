/**
 * Keys for rows in the catalog `settings` table.
 *
 * Must stay in sync with Rust:
 * - `crates/renderpilot-cli/src/catalog/covers/mod.rs` (`STEAMGRIDDB_API_KEY_SETTING`)
 * - `crates/renderpilot-cli/src/catalog/covers/policy.rs` (`COVERS_*`)
 */

export const CATALOG_SETTING_KEYS = {
  STEAMGRIDDB_API_KEY: 'steamgriddb_api_key',

  COVERS_STEAM_CDN_ENABLED: 'covers_steam_cdn_enabled',
  COVERS_GOG_CDN_ENABLED: 'covers_gog_cdn_enabled',

  /**
   * When false / `0` / `no`, skips SteamGridDB remote steps.
   *
   * This is independent of the SteamGridDB API key setting.
   */
  COVERS_STEAMGRIDDB_REMOTE_ENABLED: 'covers_steamgriddb_enabled',
} as const;

export type CatalogSettingKey = (typeof CATALOG_SETTING_KEYS)[keyof typeof CATALOG_SETTING_KEYS];

export const STEAMGRIDDB_SETTING_KEY = CATALOG_SETTING_KEYS.STEAMGRIDDB_API_KEY;

export const COVERS_STEAM_CDN_SETTING_KEY = CATALOG_SETTING_KEYS.COVERS_STEAM_CDN_ENABLED;

export const COVERS_GOG_CDN_SETTING_KEY = CATALOG_SETTING_KEYS.COVERS_GOG_CDN_ENABLED;

export const COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY =
  CATALOG_SETTING_KEYS.COVERS_STEAMGRIDDB_REMOTE_ENABLED;
