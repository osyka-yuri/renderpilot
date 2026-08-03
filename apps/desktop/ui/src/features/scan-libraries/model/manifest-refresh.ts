/** Per-kind status from a coordinated remote-manifest refresh (serde-aligned). */
export type ManifestKindStatus = { status: 'skipped' } | { status: 'ok' } | { status: 'error' };

/** High-level outcome of a coordinated remote-manifest refresh. */
export type ManifestRefreshOutcome =
  | { kind: 'passive_completed' }
  | { kind: 'forced_fetched' }
  | { kind: 'skipped_in_flight' }
  | { kind: 'skipped_cooldown'; retry_after_secs: number };

/** Report returned by `refresh_remote_manifests`. */
export type ManifestRefreshReport = {
  outcome: ManifestRefreshOutcome;
  kinds: {
    libraries: ManifestKindStatus;
    renodx: ManifestKindStatus;
    luma: ManifestKindStatus;
    reshade: ManifestKindStatus;
  };
};
