export type CoverRemotePolicy = {
  steamCdn: boolean;
  gogCdn: boolean;
  steamgriddb: boolean;
};

export type CatalogSettingPayload = {
  value: string | null;
};

/** Tone of a settings status message, used to style success vs. error feedback. */
export type SettingsMessageKind = 'success' | 'error';
