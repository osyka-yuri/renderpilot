/// Queries DLSS-Fix availability for an installed RenoDX game.
use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::GameId;

use crate::addons::renodx::dlss_fix::resolve_dlss_fix;
use crate::{Context, ServiceError};

/// Returns whether DLSS-Fix can be installed for this game.
pub fn availability(context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
    let Some(record) = context.storage().get_installed_addon(game_id)? else {
        return Ok(false);
    };
    if record.has_dlss_fix() {
        return Ok(false);
    }
    Ok(resolve_dlss_fix(context.storage(), game_id)?.is_some())
}
