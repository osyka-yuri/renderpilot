/// Queries current Luma install state for a game.
use renderpilot_domain::{AddonKind, GameId, LumaInstallState};

use crate::{Context, ServiceError};

use crate::addons::luma::game_context::resolve_launch_args;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::records;

/// Returns the persisted Luma install state for `game_id`. A record belonging
/// to a different addon kind (e.g. RenoDX) reads as `NotInstalled` — it is never
/// mistaken for a Luma install.
///
/// `manifest` is optional so a caller that already has an install result in
/// hand (or is offline with no cached manifest) can still get a state back:
/// `None` degrades `launch_args` to empty rather than requiring a manifest
/// fetch just to report a state the caller doesn't need re-resolved guidance
/// for.
pub fn status(
    context: &Context,
    manifest: Option<&LumaManifest>,
    game_id: &GameId,
) -> Result<LumaInstallState, ServiceError> {
    match records::record_of_kind(context, game_id, AddonKind::Luma)? {
        Some(record) => {
            let launch_args = match manifest {
                Some(manifest) => resolve_launch_args(context, manifest, game_id)?,
                None => Vec::new(),
            };
            Ok(tracking::install_state_from_record(&record, launch_args))
        }
        None => Ok(LumaInstallState::NotInstalled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::luma::test_support::manifest;
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{InstalledAddon, PathRef};
    use tempfile::tempdir;

    #[test]
    fn a_renodx_record_reads_as_not_installed_for_luma() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");
        let renodx_record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&renodx_record)
            .expect("seed renodx record");

        let state = status(&context, Some(&manifest(Vec::new())), &game_id).expect("status");
        assert_eq!(state, LumaInstallState::NotInstalled);
    }

    #[test]
    fn no_record_is_not_installed() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");

        let state = status(&context, Some(&manifest(Vec::new())), &game_id).expect("status");
        assert_eq!(state, LumaInstallState::NotInstalled);
    }

    #[test]
    fn without_a_manifest_an_installed_record_still_reports_a_state_with_empty_launch_args() {
        // C.9: a caller with no manifest in hand (offline, no cache) must still
        // get a state back for an existing install, rather than needing a
        // network round trip just to report it.
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed luma record");

        let state = status(&context, None, &game_id).expect("status");

        match state {
            LumaInstallState::Installed { launch_args, .. } => assert!(launch_args.is_empty()),
            other => panic!("expected Installed, got {other:?}"),
        }
    }
}
