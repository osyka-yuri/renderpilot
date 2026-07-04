/// Queries current RenoDX install state for a game.
use renderpilot_domain::{AddonKind, GameId, RenoDxInstallState};

use crate::{Context, ServiceError};

use crate::addons::records;
use crate::addons::renodx::tracking;

/// Returns the persisted RenoDX install state for `game_id`.
pub fn status(context: &Context, game_id: &GameId) -> Result<RenoDxInstallState, ServiceError> {
    Ok(
        records::record_of_kind(context, game_id, AddonKind::RenoDx)?
            .as_ref()
            .map(tracking::install_state_from_record)
            .unwrap_or(RenoDxInstallState::NotInstalled),
    )
}
