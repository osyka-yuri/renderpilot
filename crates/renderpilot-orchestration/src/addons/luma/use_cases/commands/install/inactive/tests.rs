#[cfg(windows)]
mod windows {
    use std::path::Path;

    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, Architecture, GameId, GameIdentity, GameInstallation, GameRuntime,
        InstalledAddon, InstalledAddonHostKind, Launcher, PathRef, Platform,
    };
    use tempfile::tempdir;

    use super::super::{commit_prepared, plan};
    use crate::Context;
    use crate::addons::luma::fetch::types::LumaPayloadFile;
    use crate::addons::luma::install::PreparedInstall;
    use crate::addons::luma::test_support::{
        PE32_PLUS_MAGIC, build_pe_with_exports, manifest, rule, title,
    };
    use crate::addons::luma::types::Status;
    use crate::addons::matching::MatchKind;

    fn path_ref(path: &Path) -> PathRef {
        PathRef::new(path.to_string_lossy().into_owned()).expect("path")
    }

    fn seed_game(context: &Context, game_id: &GameId, root: &Path, exe: &Path) {
        let identity = GameIdentity::new(game_id.clone(), "Dishonored 2", Launcher::Steam)
            .expect("identity")
            .with_external_id("403640")
            .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            path_ref(root),
        )
        .with_executable_candidate(path_ref(exe));
        context.storage().upsert_game(&game).expect("game");
    }

    fn write_stub_exe(path: &Path) {
        std::fs::write(
            path,
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                PE32_PLUS_MAGIC,
                &[],
            ),
        )
        .expect("write executable");
    }

    fn set_new_addon_path(game_root: &Path) -> (std::path::PathBuf, Vec<u8>) {
        let addon_root = game_root.join("NewAddonPath");
        std::fs::create_dir_all(&addon_root).expect("new AddonPath");
        let ini = game_root.join("ReShade.ini");
        let bytes = format!("[ADDON]\r\nAddonPath={}\r\n", addon_root.display()).into_bytes();
        std::fs::write(&ini, &bytes).expect("ReShade.ini");
        (addon_root, bytes)
    }

    fn game_safety(context: &Context, game_id: &GameId) -> crate::GameSafetyPermit {
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(context, game_id)
            .expect("assessment");
        authority
            .game_permit(game_id.clone(), Some(&assessment.context_token))
            .expect("permit")
    }

    fn matching_manifest() -> crate::addons::luma::types::LumaManifest {
        manifest(vec![title(
            "dishonored-2",
            "Luma-Dishonored_2.zip",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "403640", 100)],
        )])
    }

    fn seed_owner(
        context: &Context,
        game_id: &GameId,
        external: &Path,
        kind: AddonKind,
    ) -> InstalledAddon {
        let addon = external.join(match kind {
            AddonKind::Luma => "Luma-Dishonored_2.addon",
            AddonKind::RenoDx => "renodx-dishonored.addon64",
            AddonKind::OptiScaler => panic!("OptiScaler is not an inactive Proxy owner"),
        });
        std::fs::write(&addon, b"old external payload").expect("old payload");
        let record = InstalledAddon::new(game_id.clone(), kind, path_ref(&addon))
            .with_host_kind(InstalledAddonHostKind::Proxy)
            .with_created_file(path_ref(&addon));
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("owner record");
        record
    }

    fn prepared_for(game_id: GameId, resolved: &plan::ResolvedInstallSnapshot) -> PreparedInstall {
        let reshade_dll_bytes = build_pe_with_exports(
            crate::addons::test_support::MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &[
                "ReShadeVersion",
                "ReShadeRegisterAddon",
                "ReShadeUnregisterAddon",
                "ReShadeRegisterEvent",
                "ReShadeUnregisterEvent",
            ],
        );
        let zip_digest = renderpilot_detection::sha256_bytes(b"luma fixture zip")
            .expect("zip digest")
            .to_string();
        let reshade_digest = renderpilot_detection::sha256_bytes(&reshade_dll_bytes)
            .expect("ReShade digest")
            .to_string();
        PreparedInstall {
            game_id,
            proxy_dll_name: resolved.plan.proxy_dll_name.clone(),
            payload: vec![LumaPayloadFile {
                relative_path: resolved.plan.addon_file.clone(),
                bytes: b"new Luma payload".to_vec(),
            }],
            main_addon_rel: resolved.plan.addon_file.clone(),
            asset_source_url: "https://example.invalid/Luma-Dishonored_2.zip".to_owned(),
            zip_digest,
            source_etag: None,
            source_last_modified: None,
            build_label: Some("Fixture".to_owned()),
            reshade_dll_bytes,
            reshade_source_url: "https://example.invalid/ReShade.zip".to_owned(),
            reshade_source_etag: None,
            reshade_last_modified: None,
            reshade_digest,
            dgvoodoo: None,
        }
    }

    async fn resolve_initial(
        context: &Context,
        manifest: &crate::addons::luma::types::LumaManifest,
        game_id: &GameId,
    ) -> plan::ResolvedInstallSnapshot {
        let _guard = crate::game_mutation_lock::lock(game_id).await;
        plan::resolve(context, manifest, game_id).expect("initial Luma resolution")
    }

    #[tokio::test]
    async fn inactive_install_replaces_same_kind_external_owner_as_one_durable_commit() {
        let db = tempdir().expect("db");
        let game = tempdir().expect("game");
        let external = tempdir().expect("old AddonPath");
        let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403640").expect("id");
        let exe = game.path().join("Dishonored2.exe");
        write_stub_exe(&exe);
        seed_game(&context, &game_id, game.path(), &exe);
        let (new_addon_path, ini_before) = set_new_addon_path(game.path());
        let old_owner = seed_owner(&context, &game_id, external.path(), AddonKind::Luma);
        let old_payload = Path::new(old_owner.addon_file().as_str()).to_path_buf();
        let manifest = matching_manifest();
        let initial = resolve_initial(&context, &manifest, &game_id).await;
        assert_eq!(
            initial
                .snapshot
                .external_owner
                .as_ref()
                .map(|owner| owner.record.kind()),
            Some(AddonKind::Luma)
        );
        let prepared = prepared_for(game_id.clone(), &initial);

        let installed = commit_prepared(
            &context,
            &manifest,
            &game_id,
            &game_safety(&context, &game_id),
            None,
            initial,
            prepared,
        )
        .await
        .expect("replace Luma owner");

        assert_eq!(installed.kind(), AddonKind::Luma);
        assert!(!old_payload.exists());
        assert_eq!(
            std::fs::read(new_addon_path.join("Luma-Dishonored_2.addon")).expect("new payload"),
            b"new Luma payload"
        );
        assert_eq!(
            std::fs::read(game.path().join("ReShade.ini")).expect("ReShade.ini remains"),
            ini_before,
            "new unowned INI remains byte-identical"
        );
        let persisted = context
            .storage()
            .get_installed_addon(&game_id)
            .expect("owner read")
            .expect("installed owner");
        assert!(persisted.eq_ignoring_persistence_timestamps(&installed));
    }

    #[tokio::test]
    async fn inactive_install_replaces_renodx_owner_after_addon_path_changes() {
        let db = tempdir().expect("db");
        let game = tempdir().expect("game");
        let external = tempdir().expect("old AddonPath");
        let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403640").expect("id");
        let exe = game.path().join("Dishonored2.exe");
        write_stub_exe(&exe);
        seed_game(&context, &game_id, game.path(), &exe);
        let (new_addon_path, ini_before) = set_new_addon_path(game.path());
        let old_owner = seed_owner(&context, &game_id, external.path(), AddonKind::RenoDx);
        let old_payload = Path::new(old_owner.addon_file().as_str()).to_path_buf();
        let manifest = matching_manifest();
        let initial = resolve_initial(&context, &manifest, &game_id).await;
        assert_eq!(
            initial
                .snapshot
                .external_owner
                .as_ref()
                .map(|owner| owner.record.kind()),
            Some(AddonKind::RenoDx)
        );
        let prepared = prepared_for(game_id.clone(), &initial);

        let installed = commit_prepared(
            &context,
            &manifest,
            &game_id,
            &game_safety(&context, &game_id),
            None,
            initial,
            prepared,
        )
        .await
        .expect("replace old RenoDX owner");

        assert_eq!(installed.kind(), AddonKind::Luma);
        assert!(!old_payload.exists());
        assert!(new_addon_path.join("Luma-Dishonored_2.addon").is_file());
        assert_eq!(
            std::fs::read(game.path().join("ReShade.ini")).expect("ReShade.ini remains"),
            ini_before,
            "changed AddonPath INI is unowned and remains byte-identical"
        );
        let persisted = context
            .storage()
            .get_installed_addon(&game_id)
            .expect("owner read")
            .expect("installed owner");
        assert!(persisted.eq_ignoring_persistence_timestamps(&installed));
    }

    #[tokio::test]
    async fn failed_replacement_catalog_commit_restores_old_files_and_owner() {
        let db = tempdir().expect("db");
        let game = tempdir().expect("game");
        let external = tempdir().expect("old AddonPath");
        let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403640").expect("id");
        let wrong_id = GameId::new("steam:403641").expect("wrong id");
        let exe = game.path().join("Dishonored2.exe");
        write_stub_exe(&exe);
        seed_game(&context, &game_id, game.path(), &exe);
        let (new_addon_path, _) = set_new_addon_path(game.path());
        let old_owner = seed_owner(&context, &game_id, external.path(), AddonKind::Luma);
        let old_payload = Path::new(old_owner.addon_file().as_str()).to_path_buf();
        let manifest = matching_manifest();
        let initial = resolve_initial(&context, &manifest, &game_id).await;
        let prepared = prepared_for(wrong_id, &initial);
        let safety = game_safety(&context, &game_id);

        let _error = commit_prepared(
            &context, &manifest, &game_id, &safety, None, initial, prepared,
        )
        .await
        .expect_err("wrong owner game must fail the catalog replacement");
        assert_eq!(
            std::fs::read(&old_payload).expect("old payload restored"),
            b"old external payload"
        );
        assert!(!new_addon_path.join("Luma-Dishonored_2.addon").exists());
        assert!(!game.path().join("dxgi.dll").exists());
        let persisted = context
            .storage()
            .get_installed_addon(&game_id)
            .expect("owner read")
            .expect("old owner remains");
        assert!(persisted.eq_ignoring_persistence_timestamps(&old_owner));
        assert!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending mutations")
                .is_empty()
        );
    }
}
