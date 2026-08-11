/** Exact DTO returned by the Rust `app_update_check` command. */
export type AppUpdateCheckDto = {
  sessionId: string;
  metadata: AppUpdateMetadataDto;
};

export type AppUpdateMetadataDto = {
  currentVersion: string;
  version: string;
  date: string | null;
  body: string;
};

/** Exact event payload emitted through Rust's `app_update_download` channel. */
export type AppUpdateDownloadEventDto =
  | {
      type: 'started';
      contentLength: number | null;
    }
  | {
      type: 'progress';
      chunkLength: number;
    }
  | {
      type: 'finished';
    };

/** Exact discriminated DTO returned by the Rust `app_update_apply` command. */
export type AppUpdateApplyDto = { type: 'installed' } | { type: 'native-exit' };
