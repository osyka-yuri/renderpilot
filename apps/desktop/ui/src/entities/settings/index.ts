export {
  STEAMGRIDDB_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  GAMES_FILTERS_CATALOG_SETTING_KEY,
} from './model/catalog-setting-keys';

export {
  type CoverRemotePolicy,
  type CatalogSettingPayload,
  type LanguageMode,
} from './model/view-model';

export { getCatalogSetting, setCatalogSetting } from './api/desktop';

export {
  catalogSettingHasSteamGridDbKey,
  parseCatalogBoolDefaultTrue,
  fetchCoverRemotePolicy,
  fetchSteamGridDbKeyConfigured,
} from './api/cover-policy';
