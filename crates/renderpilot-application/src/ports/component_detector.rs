use renderpilot_domain::{GameInstallation, GraphicsComponent};

use crate::AppResult;

/// Port implemented by graphics component detectors.
pub trait ComponentDetector: Send + Sync {
    /// Returns a stable detector name for logs and diagnostics.
    #[must_use]
    fn name(&self) -> &str;

    /// Detects graphics components for a single game installation.
    fn detect_components(&self, game: &GameInstallation) -> AppResult<Vec<GraphicsComponent>>;
}
