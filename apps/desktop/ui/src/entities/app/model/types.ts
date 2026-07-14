// Wire-level DTO mirrors apps/desktop/src-tauri/src/lib.rs::AppInitializationState
// with `rename_all = "camelCase"` serde attribute.

export type AppInitializationState = {
  /** Process is running with administrator rights. */
  isElevated: boolean;
  /** `false` on non-Windows platforms — UI hides elevation UI. */
  elevationSupported: boolean;
  /**
   * Degraded unelevated mode after UAC cancel, policy block, or live handoff
   * skip. IPC name kept for compatibility — not only a literal user cancel.
   * Banner visibility still keys off `!isElevated && elevationSupported`.
   */
  elevationUserDeclined: boolean;
  /**
   * Startup auto-elevation was attempted or skipped due to a live handoff
   * marker. Does not mean the manual banner relaunch was shown.
   */
  elevationAttempted: boolean;
};
