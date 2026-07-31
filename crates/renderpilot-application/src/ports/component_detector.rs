use renderpilot_domain::{GameInstallation, LibraryComponent};

use crate::AppResult;

/// Port implemented by library component detectors.
pub trait ComponentDetector: Send + Sync {
    /// Returns a stable detector name for logs and diagnostics.
    #[must_use]
    fn name(&self) -> &str;

    /// Detects library components for a single game installation.
    fn detect_components(&self, game: &GameInstallation) -> AppResult<Vec<LibraryComponent>>;
}
