use renderpilot_domain::{AddonKind, GameId, InstalledAddon};

use crate::AppResult;

/// Repository port for storing and loading installed add-on records.
///
/// One record per game captures everything needed to reverse an install (the
/// files RenderPilot created and the pre-existing files it backed up). The
/// record intentionally has no foreign key to `games` and survives catalog
/// pruning/rescans, so it remains the source of truth for uninstall.
pub trait InstalledAddonRepository: Send + Sync {
    /// Inserts or replaces the add-on install record for a game.
    fn upsert_installed_addon(&self, addon: &InstalledAddon) -> AppResult<()>;

    /// Returns the add-on install record for a game, if one is recorded.
    fn get_installed_addon(&self, game_id: &GameId) -> AppResult<Option<InstalledAddon>>;

    /// Lists every recorded add-on install.
    fn list_installed_addons(&self) -> AppResult<Vec<InstalledAddon>>;

    /// Deletes the add-on install record for a game when it matches `kind`.
    /// Missing rows and kind mismatches are a no-op (defense-in-depth under the
    /// one-row-per-game exclusivity model).
    fn delete_installed_addon(&self, game_id: &GameId, kind: AddonKind) -> AppResult<()>;
}
