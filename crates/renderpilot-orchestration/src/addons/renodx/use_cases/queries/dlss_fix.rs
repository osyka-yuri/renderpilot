/// Queries DLSS-Fix availability for an installed RenoDX game.
use renderpilot_domain::{AddonKind, GameId};

use crate::addons::records;
use crate::addons::renodx::dlss_fix::resolve_dlss_fix;
use crate::{Context, ServiceError};

/// Returns whether DLSS-Fix can be installed for this game.
///
/// Reads the active record rather than the raw repository. A record for another
/// add-on kind, or a stale RenoDX row whose primary payload was removed, is
/// treated as absent.
pub fn availability(context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
    let Some(record) = records::active_record_of_kind(context, game_id, AddonKind::RenoDx)? else {
        return Ok(false);
    };
    if record.has_dlss_fix() {
        return Ok(false);
    }
    Ok(resolve_dlss_fix(context.storage(), game_id)?.is_some())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{InstalledAddon, PathRef};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn availability_is_false_when_the_installed_record_belongs_to_luma() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\games\x\addon.dll").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed luma record");

        assert_eq!(availability(&context, &game_id), Ok(false));
    }
}
