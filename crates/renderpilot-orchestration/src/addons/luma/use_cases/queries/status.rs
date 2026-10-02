/// Queries current Luma install state for a game.
use renderpilot_domain::{AddonKind, GameId, LumaInstallState};

use crate::{Context, ServiceError};

use crate::addons::luma::game_context::resolve_launch_args;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::records;

/// Returns the current Luma install state for `game_id`. A durable Luma receipt
/// is presented as installed only while its payload remains bound to a
/// compatible host in the current game's loading chain. A record belonging to
/// a different addon kind (e.g. RenoDX) reads as `NotInstalled` — it is never
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
    match records::active_record_of_kind(context, game_id, AddonKind::Luma)? {
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
    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher, PathRef, Platform,
    };
    use renderpilot_storage_sqlite::AuthorityCas;
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
        let game_id = GameId::new("manual:luma-offline-status").expect("game id");
        let game_dir = tempdir().expect("game dir");
        let addon_path = game_dir.path().join("Luma-Test.addon");
        let exe_path = game_dir.path().join("LumaTest.exe");
        std::fs::write(&addon_path, b"luma payload").expect("write Luma payload");
        std::fs::write(
            &exe_path,
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[],
            ),
        )
        .expect("write game executable");
        std::fs::write(
            game_dir.path().join("dxgi.dll"),
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[
                    "ReShadeVersion",
                    "ReShadeRegisterAddon",
                    "ReShadeUnregisterAddon",
                    "ReShadeRegisterEvent",
                ],
            ),
        )
        .expect("write compatible ReShade host");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Luma offline status", Launcher::Manual)
                .expect("game identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy()).expect("game root"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy()).expect("game executable"),
        );
        context
            .storage()
            .upsert_game(&game)
            .expect("seed active game");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(addon_path.to_string_lossy()).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed luma record");

        let state = status(&context, None, &game_id).expect("status");

        match state {
            LumaInstallState::Installed { launch_args, .. } => assert!(launch_args.is_empty()),
            LumaInstallState::NotInstalled => panic!("expected Installed, got NotInstalled"),
        }
    }

    #[test]
    fn luma_status_hides_a_retained_record_until_the_current_host_binds_its_payload() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:luma-current-binding").expect("game id");
        let game_dir = tempdir().expect("game dir");
        let external_dir = tempdir().expect("external add-on dir");
        let addon_path = external_dir.path().join("Luma-Test.addon");
        let exe_path = game_dir.path().join("LumaTest.exe");
        std::fs::write(&addon_path, b"luma payload").expect("write Luma payload");
        std::fs::write(
            &exe_path,
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[],
            ),
        )
        .expect("write game executable");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Luma current binding", Launcher::Manual)
                .expect("game identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy()).expect("game root"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy()).expect("game executable"),
        );
        context.storage().upsert_game(&game).expect("seed game");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(addon_path.to_string_lossy()).expect("payload path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed retained Luma receipt");

        assert_eq!(
            status(&context, None, &game_id).expect("status without host"),
            LumaInstallState::NotInstalled,
            "a retained owner receipt does not make a fresh root installed"
        );
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("raw owner query")
                .is_some(),
            "read-side inactivity preserves cleanup ownership"
        );

        std::fs::write(
            game_dir.path().join("dxgi.dll"),
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[
                    "ReShadeVersion",
                    "ReShadeRegisterAddon",
                    "ReShadeUnregisterAddon",
                    "ReShadeRegisterEvent",
                ],
            ),
        )
        .expect("write compatible ReShade host");
        let addon_path_text = external_dir.path().to_string_lossy().replace('\\', "/");
        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            format!("[ADDON]\nAddonPath={addon_path_text}\n"),
        )
        .expect("write current external AddonPath");
        assert!(matches!(
            status(&context, None, &game_id).expect("status with current binding"),
            LumaInstallState::Installed { launch_args, .. } if launch_args.is_empty()
        ));

        let changed_dir = tempdir().expect("changed add-on dir");
        let changed_path_text = changed_dir.path().to_string_lossy().replace('\\', "/");
        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            format!("[ADDON]\nAddonPath={changed_path_text}\n"),
        )
        .expect("write changed AddonPath");
        assert_eq!(
            status(&context, None, &game_id).expect("status with changed binding"),
            LumaInstallState::NotInstalled,
            "a retained record is hidden when the current AddonPath no longer binds its payload"
        );

        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            format!("[ADDON]\nAddonPath={addon_path_text}\n"),
        )
        .expect("restore current AddonPath");
        std::fs::remove_file(&addon_path).expect("remove current payload");
        assert_eq!(
            status(&context, None, &game_id).expect("status without payload"),
            LumaInstallState::NotInstalled,
            "a missing payload is hidden from current install status"
        );
        std::fs::write(&addon_path, b"luma payload").expect("restore payload");

        let readiness = context
            .storage()
            .catalog_readiness(&game_id)
            .expect("catalog readiness");
        context
            .storage()
            .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
            .expect("mark current game absent");
        assert_eq!(
            status(&context, None, &game_id).expect("status for absent registration"),
            LumaInstallState::NotInstalled,
            "a retained Luma receipt is hidden when its game registration is absent"
        );
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("raw owner after absence")
                .is_some(),
            "an inactive status read preserves the cleanup receipt"
        );
    }
}
