use renderpilot_domain::GameInstallation;

use crate::AppResult;

/// Port implemented by launcher, platform or manual game discovery adapters.
pub trait GameSourceProvider: Send + Sync {
    /// Returns a stable provider name for logs and diagnostics.
    #[must_use]
    fn name(&self) -> &str;

    /// Discovers game installations available from this provider.
    fn discover_games(&self) -> AppResult<Vec<GameInstallation>>;
}
