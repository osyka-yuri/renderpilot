/// Queries current RenoDX install state for a game.
use renderpilot_domain::{AddonKind, GameId, RenoDxInstallState};

use crate::{Context, ServiceError};

use crate::addons::records;
use crate::addons::renodx::tracking;

/// Returns the persisted RenoDX install state for `game_id`. A record belonging
/// to a different addon kind (e.g. Luma) reads as `NotInstalled` — it is never
/// mistaken for a RenoDX install.
pub fn status(context: &Context, game_id: &GameId) -> Result<RenoDxInstallState, ServiceError> {
    Ok(
        records::record_of_kind(context, game_id, AddonKind::RenoDx)?
            .as_ref()
            .map(tracking::install_state_from_record)
            .unwrap_or(RenoDxInstallState::NotInstalled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{InstalledAddon, PathRef};
    use tempfile::tempdir;

    #[test]
    fn a_luma_record_reads_as_not_installed_for_renodx() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");
        let luma_record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&luma_record)
            .expect("seed luma record");

        let state = status(&context, &game_id).expect("status");
        assert_eq!(state, RenoDxInstallState::NotInstalled);
    }
}
