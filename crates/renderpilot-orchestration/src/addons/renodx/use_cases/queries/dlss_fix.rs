/// Queries DLSS-Fix availability for an installed RenoDX game.
use renderpilot_domain::{AddonKind, GameId};

use crate::addons::records;
use crate::addons::renodx::dlss_fix::resolve_dlss_fix;
use crate::{Context, ServiceError};

/// Returns whether DLSS-Fix can be installed for this game.
///
/// Reads the record through `records::record_of_kind` rather than the raw
/// repository, agreeing with the install flow's kind-scoped read before acting.
pub fn availability(context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
    let Some(record) = records::record_of_kind(context, game_id, AddonKind::RenoDx)? else {
        return Ok(false);
    };
    if record.has_dlss_fix() {
        return Ok(false);
    }
    Ok(resolve_dlss_fix(context.storage(), game_id)?.is_some())
}
