/// Queries current RenoDX install state for a game.
use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{GameId, RenoDxInstallState};

use crate::{Context, ServiceError};

use crate::addons::renodx::tracking;

/// Returns the persisted RenoDX install state for `game_id`.
pub fn status(context: &Context, game_id: &GameId) -> Result<RenoDxInstallState, ServiceError> {
    Ok(context
        .storage()
        .get_installed_addon(game_id)?
        .as_ref()
        .map(tracking::install_state_from_record)
        .unwrap_or(RenoDxInstallState::NotInstalled))
}
