use renderpilot_domain::{D3d12ExecutableBaseline, D3d12ExecutableIdentity, RuntimeCompatibility};
use renderpilot_platform_windows::DeveloperModeStatus;

use super::*;

fn d3d12_artifact_at(source: &Path, sdk_line: u32) -> LibraryArtifact {
    let source_hash = sha_of(source);
    let package_version = format!("1.{sdk_line}.1");
    LibraryArtifact::new(
        ArtifactId::for_bundle([&source_hash]),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            ComponentFile::new(path_as_ref(source))
                .with_sha256(source_hash)
                .with_version(Version::parse(&package_version).expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("D3D12 artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.D3D12",
                    &package_version,
                )
                .expect("package"),
            )
            .with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: sdk_line }),
            ),
    )
}

#[test]
fn d3d12_missing_executable_facts_are_blocked_at_plan_and_apply_boundaries() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let live = game_dir.join("D3D12Core.dll");
    let source = library_dir.join("D3D12Core.dll");
    write(&live, b"original-d3d12-core");
    write(&source, b"replacement-d3d12-core");

    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:d3d12-policy").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live))
            .with_sha256(sha_of(&live))
            .with_version(Version::parse("1.618.3").expect("version")),
    );

    let source_hash = sha_of(&source);
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle([&source_hash]),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            ComponentFile::new(path_as_ref(&source))
                .with_sha256(source_hash)
                .with_version(Version::parse("1.618.5").expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.D3D12",
                    "1.618.5",
                )
                .expect("package"),
            )
            .with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
            ),
    );

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    let plan_error =
        crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
            .err()
            .expect("plan must fail without an unambiguous executable");
    assert!(matches!(plan_error, crate::ServiceError::InvalidInput(_)));

    let apply_error =
        apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
            .expect_err("direct apply must enforce the same policy");
    assert!(matches!(apply_error, crate::ServiceError::InvalidInput(_)));
    assert_eq!(
        fs::read(&live).expect("live core"),
        b"original-d3d12-core",
        "policy rejection must happen before the first file mutation"
    );
    assert!(
        !bak_of(&live).exists(),
        "no recovery sidecar may be created"
    );
}

#[test]
fn patched_executable_backup_without_dll_backup_is_never_captured_as_a_mixed_baseline() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let executable_backup = bak_of(&executable);
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    let patched_executable = crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
    write(&executable, &patched_executable);
    write(&executable_backup, &original_executable);
    write(&live_dll, b"manually-installed-sdk-619-runtime");
    write(
        &source_dll,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(620),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-orphan-exe").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.619.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 620);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("repair plan");
    assert!(
        plan.plan
            .blockers()
            .iter()
            .any(|blocker| blocker.as_str() == "d3d12_executable_repair_required")
    );

    let error = apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
        .expect_err("mixed baseline must be blocked");
    assert!(matches!(error, crate::ServiceError::InvalidInput(_)));
    assert_eq!(fs::read(&executable).expect("live EXE"), patched_executable);
    assert_eq!(
        fs::read(&executable_backup).expect("EXE backup"),
        original_executable
    );
    assert_eq!(
        fs::read(&live_dll).expect("live DLL"),
        b"manually-installed-sdk-619-runtime"
    );
    assert!(
        !bak_of(&live_dll).exists(),
        "a rejected operation must not manufacture a DLL backup"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline lookup")
            .is_none(),
        "a rejected operation must not persist a mixed rollback aggregate"
    );
}

#[test]
fn confirmed_d3d12_apply_reports_token_mismatch_when_the_live_dll_changes() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    write(&executable, &original_executable);
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-stale-apply").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);
    let target =
        crate::catalog::runtime_compatibility::target_profile(&context, &game, Some(&component))
            .expect("target profile");
    let action = renderpilot_application::replacement_executable_action(&artifact, &target.profile)
        .expect("policy")
        .expect("action");
    let token = renderpilot_application::d3d12_confirmation_token(
        &component,
        &artifact,
        &target.profile,
        &action,
    )
    .expect("token");

    write(&live_dll, b"externally-changed-runtime");
    let error = apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
        None,
    )
    .expect_err("stale confirmation must fail");

    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&executable).expect("unchanged EXE"),
        original_executable
    );
    assert!(!bak_of(&executable).exists());
    assert!(!bak_of(&live_dll).exists());
}

#[test]
fn d3d12_preview_apply_rechecks_developer_mode_after_a_successful_plan() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    write(&executable, &original_executable);
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::catalog::test_support::synthetic_versioned_d3d12_runtime(619),
    );
    let original_runtime = fs::read(&live_dll).expect("runtime fixture");

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-developer-mode").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = crate::catalog::test_support::d3d12_preview_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");

    let developer_mode_enabled = Arc::new(AtomicBool::new(true));
    let status = Arc::clone(&developer_mode_enabled);
    let context = Context::from_storage(storage).with_developer_mode_status_provider(move || {
        if status.load(Ordering::SeqCst) {
            DeveloperModeStatus::Enabled
        } else {
            DeveloperModeStatus::Disabled
        }
    });

    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("enabled Developer Mode should allow planning");
    assert!(plan.plan.blockers().is_empty());
    let token = plan.plan.confirmation_token().to_owned();
    assert!(!token.is_empty());

    developer_mode_enabled.store(false, Ordering::SeqCst);
    let error = apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
        None,
    )
    .expect_err("apply must recheck Developer Mode");

    match error {
        crate::ServiceError::InvalidInput(message) => {
            assert!(message.contains("developer_mode_required"));
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(
        fs::read(&executable).expect("unchanged executable"),
        original_executable
    );
    assert_eq!(
        fs::read(&live_dll).expect("unchanged runtime"),
        original_runtime
    );
}

#[test]
fn confirmed_first_d3d12_apply_reports_token_mismatch_when_the_executable_changes_or_disappears() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    write(
        &executable,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606),
    );
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::addons::test_support::build_nvidia_dlss_pe([1, 619, 1, 0]),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-missing-after-plan").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);
    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("plan");
    let token = plan.plan.confirmation_token().to_owned();

    write(
        &executable,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(620),
    );
    let error = apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
        None,
    )
    .expect_err("an incompatible executable change must reject the confirmation");
    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&live_dll).expect("unchanged DLL"),
        b"scanned-sdk-606-runtime"
    );
    assert!(!bak_of(&live_dll).exists());
    assert!(!bak_of(&executable).exists());

    fs::remove_file(&executable).expect("remove executable after planning");
    let error = apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
        None,
    )
    .expect_err("stale executable state must reject the confirmation");

    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&live_dll).expect("unchanged DLL"),
        b"scanned-sdk-606-runtime"
    );
    assert!(!bak_of(&live_dll).exists());
}

#[test]
fn every_d3d12_apply_stage_rolls_back_dll_exe_sidecars_and_database_together() {
    use super::super::{D3d12ApplyFailurePoint, set_d3d12_apply_failure_point};

    let failure_points = [
        D3d12ApplyFailurePoint::AfterExecutableBackup,
        D3d12ApplyFailurePoint::AfterDllMutation,
        D3d12ApplyFailurePoint::AfterExecutableMutation,
        D3d12ApplyFailurePoint::BeforeDatabaseCommit,
    ];

    for (index, failure_point) in failure_points.into_iter().enumerate() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let library_dir = root.path().join("library");
        fs::create_dir_all(&game_dir).expect("game dir");
        fs::create_dir_all(&library_dir).expect("library dir");

        let executable = game_dir.join("game.exe");
        let live_dll = game_dir.join("D3D12Core.dll");
        let source_dll = library_dir.join("D3D12Core.dll");
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let original_dll = b"original-sdk-606-runtime";
        write(&executable, &original_executable);
        write(&live_dll, original_dll);
        write(
            &source_dll,
            &crate::addons::test_support::build_nvidia_dlss_pe([1, 619, 1, 0]),
        );

        let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
        let component_id =
            ComponentId::new(format!("component:d3d12-failure-{index}")).expect("component");
        let original_component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&live_dll))
                .with_version(Version::parse("1.606.1").expect("version")),
        );
        let artifact = d3d12_artifact_at(&source_dll, 619);
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&original_component))
            .expect("component");
        storage.upsert_artifact(&artifact).expect("artifact");
        let context = Context::from_storage(storage);
        let plan =
            crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
                .expect("plan");
        let token = plan.plan.confirmation_token().to_owned();

        let _failure = set_d3d12_apply_failure_point(failure_point);
        apply_swap_confirmed(
            &context,
            game.id(),
            &component_id,
            artifact.id(),
            Some(&token),
            None,
        )
        .expect_err("injected failure must abort the entire durable mutation");

        assert_eq!(
            fs::read(&live_dll).expect("live DLL"),
            original_dll,
            "DLL bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable).expect("live EXE"),
            original_executable,
            "EXE bytes were not restored after {failure_point:?}"
        );
        assert!(
            !bak_of(&live_dll).exists(),
            "DLL backup created by a failed first swap survived {failure_point:?}"
        );
        assert!(
            !bak_of(&executable).exists(),
            "EXE backup created by a failed first swap survived {failure_point:?}"
        );
        assert!(
            context
                .storage()
                .get_component_backup(&component_id)
                .expect("baseline lookup")
                .is_none(),
            "rollback aggregate was persisted after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(game.id())
                .expect("stored components"),
            vec![original_component],
            "component catalog changed after {failure_point:?}"
        );
    }
}

#[test]
fn every_d3d12_rollback_stage_restores_the_active_state_and_keeps_retry_sidecars() {
    use super::super::{D3d12RollbackFailurePoint, set_d3d12_rollback_failure_point};

    let failure_points = [
        D3d12RollbackFailurePoint::AfterDllRestore,
        D3d12RollbackFailurePoint::AfterExecutableRestore,
        D3d12RollbackFailurePoint::AfterDllSidecarRelease,
        D3d12RollbackFailurePoint::AfterExecutableSidecarRelease,
        D3d12RollbackFailurePoint::BeforeDatabaseCommit,
    ];

    for (index, failure_point) in failure_points.into_iter().enumerate() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        fs::create_dir_all(&game_dir).expect("game dir");
        let executable = game_dir.join("game.exe");
        let live_dll = game_dir.join("D3D12Core.dll");
        let executable_backup = bak_of(&executable);
        let dll_backup = bak_of(&live_dll);
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let active_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
        let original_dll = b"original-sdk-606-runtime";
        let active_dll = b"active-sdk-619-runtime";
        write(&executable, &active_executable);
        write(&executable_backup, &original_executable);
        write(&live_dll, active_dll);
        write(&dll_backup, original_dll);

        let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
        let component_id = ComponentId::new(format!("component:d3d12-rollback-failure-{index}"))
            .expect("component");
        let active_component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&live_dll))
                .with_version(Version::parse("1.619.1").expect("version")),
        );
        let rollback_baseline = ComponentRollbackBaseline::new(vec![
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&dll_backup))
                .with_version(Version::parse("1.606.1").expect("version")),
        ])
        .with_d3d12_executable(D3d12ExecutableBaseline::new(
            path_as_ref(&executable),
            D3d12ExecutableIdentity::new(
                606,
                renderpilot_detection::sha256_bytes(&original_executable)
                    .expect("original EXE hash"),
            ),
            D3d12ExecutableIdentity::new(
                619,
                renderpilot_detection::sha256_bytes(&active_executable).expect("active EXE hash"),
            ),
        ));
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&active_component))
            .expect("component");
        storage
            .recover_component_rollback_baseline(game.id(), &component_id, &rollback_baseline)
            .expect("rollback aggregate");
        let context = Context::from_storage(storage);
        let plan = crate::catalog::build_rollback_plan(&context, game.id(), &component_id)
            .expect("rollback plan");
        let action = plan.d3d12_executable_action().expect("EXE restore");
        assert!(!action.requires_confirmation());

        let _failure = set_d3d12_rollback_failure_point(failure_point);
        crate::catalog::rollback_component(&context, game.id(), &component_id)
            .expect_err("injected failure must abort the entire durable rollback");

        assert_eq!(
            fs::read(&live_dll).expect("live DLL"),
            active_dll,
            "active DLL bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable).expect("live EXE"),
            active_executable,
            "active EXE bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&dll_backup).expect("DLL backup"),
            original_dll,
            "retryable DLL sidecar was lost after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable_backup).expect("EXE backup"),
            original_executable,
            "retryable EXE sidecar was lost after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .get_component_backup(&component_id)
                .expect("baseline lookup"),
            Some(rollback_baseline),
            "rollback aggregate changed after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(game.id())
                .expect("stored components"),
            vec![active_component],
            "component catalog changed after {failure_point:?}"
        );
    }
}

#[test]
fn d3d12_rollback_revalidates_live_dll_without_user_confirmation() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    fs::create_dir_all(&game_dir).expect("game dir");
    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    let active_executable = crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
    write(&executable, &active_executable);
    write(&bak_of(&executable), &original_executable);
    write(&live_dll, b"active-sdk-619-runtime");
    write(&bak_of(&live_dll), b"original-sdk-606-runtime");

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-stale-rollback").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.619.1").expect("version")),
    );
    let rollback_baseline = ComponentRollbackBaseline::new(vec![
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&bak_of(&live_dll)))
            .with_version(Version::parse("1.606.1").expect("version")),
    ])
    .with_d3d12_executable(D3d12ExecutableBaseline::new(
        path_as_ref(&executable),
        D3d12ExecutableIdentity::new(
            606,
            renderpilot_detection::sha256_bytes(&original_executable).expect("original EXE hash"),
        ),
        D3d12ExecutableIdentity::new(
            619,
            renderpilot_detection::sha256_bytes(&active_executable).expect("active EXE hash"),
        ),
    ));
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage
        .recover_component_rollback_baseline(game.id(), &component_id, &rollback_baseline)
        .expect("rollback aggregate");
    let context = Context::from_storage(storage);
    let plan = crate::catalog::build_rollback_plan(&context, game.id(), &component_id)
        .expect("rollback plan");
    let affected = plan
        .affected_files()
        .iter()
        .map(PathRef::as_str)
        .collect::<Vec<_>>();
    assert!(
        affected
            .iter()
            .any(|path| crate::paths::same_path(Path::new(path), &bak_of(&live_dll))),
        "DLL sidecar is missing from {affected:?}"
    );
    assert!(
        affected
            .iter()
            .any(|path| crate::paths::same_path(Path::new(path), &bak_of(&executable))),
        "EXE sidecar is missing from {affected:?}"
    );
    let action = plan.d3d12_executable_action().expect("EXE restore");
    assert!(!action.requires_confirmation());

    write(&live_dll, b"externally-changed-after-plan");
    crate::catalog::rollback_component(&context, game.id(), &component_id)
        .expect_err("changed live DLL must fail fresh rollback validation");
    assert_eq!(
        fs::read(&executable).expect("unchanged active EXE"),
        active_executable
    );
    assert_eq!(
        fs::read(bak_of(&executable)).expect("unchanged EXE backup"),
        original_executable
    );
}
