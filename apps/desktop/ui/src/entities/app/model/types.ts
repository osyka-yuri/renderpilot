// Wire-level DTO mirrors apps/desktop/src-tauri/src/lib.rs::AppInitializationState
// with `rename_all = "camelCase"` serde attribute.

export type AppInitializationState = {
  /** Process is running with administrator rights. */
  isElevated: boolean;
  /** `false` on non-Windows platforms — UI hides elevation UI. */
  elevationSupported: boolean;
  /** User cancelled UAC dialog or OS policy blocked elevation this session. */
  elevationUserDeclined: boolean;
  /** A UAC prompt was already shown once this session (success or fail). */
  elevationAttempted: boolean;
};
