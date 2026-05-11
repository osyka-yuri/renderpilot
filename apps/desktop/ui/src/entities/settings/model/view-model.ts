export type CoverRemotePolicy = {
  steamCdn: boolean;
  gogCdn: boolean;
  steamgriddb: boolean;
};

export type CatalogSettingPayload = {
  value: string | null;
};

export type LanguageMode = 'system' | 'en' | 'ru';
