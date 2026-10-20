use renderpilot_application::ResolvedTransition;

use super::super::fs_ops::{perform_transition_apply_fs, revert_to_baseline_fs};
use super::*;

fn component_with_files(
    technology: LibraryTechnology,
    files: impl IntoIterator<Item = ComponentFile>,
) -> LibraryComponent {
    files.into_iter().fold(
        LibraryComponent::new(
            ComponentId::new("component:filesystem-transition-test").expect("component id"),
            GameId::new("manual:filesystem-transition-test").expect("game id"),
            ComponentKind::NativeLibrary,
            technology,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    )
}

fn artifact_member(source: &Path, install_as: &str) -> ComponentFile {
    let member = comp_file(source).with_install_as(install_as);
    if source.is_file() {
        member.with_sha256(sha_of(source))
    } else {
        member.with_sha256(Sha256Hash::new(HEX64).expect("synthetic artifact hash"))
    }
}

fn filesystem_transition(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    baseline: &[ComponentFile],
) -> ResolvedTransition {
    resolve_transition(
        component,
        artifact,
        baseline,
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("resolve filesystem transition")
}

#[test]
fn overlay_backs_up_existing_target_and_installs_durably() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("nvngx_dlss.dll");
    let source = dir.path().join("source.dll");
    write(&target, b"original");
    write(&source, b"new-version");

    let baseline = vec![comp_file(&target).with_sha256(sha_of(&target))];
    let component = component_with_files(
        LibraryTechnology::DlssSuperResolution,
        baseline.iter().cloned(),
    );
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:filesystem-overlay").expect("artifact id"),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![artifact_member(&source, "nvngx_dlss.dll")],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    let transition = filesystem_transition(&component, &artifact, &baseline);
    let changes = perform_transition_apply_fs(&transition, None).expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"new-version");
    assert_eq!(
        fs::read(bak_of(&target)).expect("bak readable"),
        b"original"
    );
    assert_eq!(changes.copied, vec![target.clone()]);
    assert_eq!(
        changes.created_sidecars,
        vec![(target.clone(), bak_of(&target))]
    );
}

#[test]
fn overlay_adds_new_file_without_creating_backup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("nvngx_dlss.dll");
    let existing = dir.path().join("nvngx_dlssg.dll");
    let source = dir.path().join("source.dll");
    write(&existing, b"installed-baseline");
    write(&source, b"fresh");

    let baseline = [comp_file(&existing).with_sha256(sha_of(&existing))];
    let component = component_with_files(
        LibraryTechnology::DlssSuperResolution,
        baseline.iter().cloned(),
    );
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:filesystem-new-file").expect("artifact id"),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![artifact_member(&source, "nvngx_dlss.dll")],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    let transition = filesystem_transition(&component, &artifact, &baseline);
    let changes = perform_transition_apply_fs(&transition, None).expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"fresh");
    assert_eq!(
        fs::read(&existing).expect("existing component file remains"),
        b"installed-baseline"
    );
    assert!(
        !bak_of(&target).exists(),
        "no backup for a newly added file"
    );
    assert!(changes.created_sidecars.is_empty());
}

#[test]
fn removed_member_is_backed_up_then_deleted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let entry = dir.path().join("amd_fidelityfx_dx12.dll");
    let member = dir.path().join("amd_fidelityfx_framegeneration_dx12.dll");
    let source = dir.path().join("new-amd_fidelityfx_dx12.dll");
    write(&entry, b"original-entry");
    write(&member, b"fsr4-member");
    write(&source, b"replacement-entry");

    let baseline = vec![
        comp_file(&entry).with_sha256(sha_of(&entry)),
        comp_file(&member).with_sha256(sha_of(&member)),
    ];
    let component = component_with_files(LibraryTechnology::AmdFsr, baseline.iter().cloned());
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:filesystem-fsr-removal").expect("artifact id"),
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![artifact_member(&source, "amd_fidelityfx_dx12.dll")],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    let transition = filesystem_transition(&component, &artifact, &baseline);
    let changes = perform_transition_apply_fs(&transition, None).expect("apply should succeed");

    assert!(!member.exists(), "removed member should be gone");
    assert_eq!(
        fs::read(bak_of(&member)).expect("bak readable"),
        b"fsr4-member",
        "removed member must be preserved as a .bak for rollback"
    );
    assert_eq!(changes.created_sidecars.len(), 2);
    assert!(
        changes
            .created_sidecars
            .iter()
            .any(|(path, _)| path.ends_with("amd_fidelityfx_dx12.dll"))
    );
    assert!(
        changes
            .created_sidecars
            .iter()
            .any(|(path, _)| path.ends_with("amd_fidelityfx_framegeneration_dx12.dll"))
    );
    assert_eq!(
        fs::read(bak_of(&entry)).expect("entry baseline sidecar"),
        b"original-entry"
    );
}

#[test]
fn dxc_pair_failure_midway_rolls_back_both_members() {
    let dir = tempfile::tempdir().expect("temp dir");
    let compiler = dir.path().join("dxcompiler.dll");
    let validator = dir.path().join("dxil.dll");
    let good_source = dir.path().join("new-dxcompiler.dll");
    let missing_source = dir.path().join("missing-dxil.dll");
    write(&compiler, b"original-compiler");
    write(&validator, b"original-validator");
    write(&good_source, b"new-compiler");

    let baseline = vec![
        comp_file(&compiler).with_sha256(sha_of(&compiler)),
        comp_file(&validator).with_sha256(sha_of(&validator)),
    ];
    let component = LibraryComponent::new(
        ComponentId::new("component:dxc-atomicity").expect("component id"),
        GameId::new("manual:dxc-atomicity").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::MicrosoftDxc,
        Swappability::BundleOnly,
    )
    .with_file(baseline[0].clone())
    .with_file(baseline[1].clone());
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:dxc-atomicity").expect("artifact id"),
        LibraryTechnology::MicrosoftDxc,
        "dxcompiler.dll",
        vec![
            artifact_member(&good_source, "dxcompiler.dll"),
            artifact_member(&missing_source, "dxil.dll"),
        ],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    let transition = filesystem_transition(&component, &artifact, &baseline);
    let context = crate::Context::from_storage(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        GameId::new(format!("manual:apply-failure-{}", ulid::Ulid::generate())).expect("game id");
    let guard = crate::game_mutation_lock::blocking_lock(&game_id);
    let mutation = crate::file_mutation::DurableFileTransaction::prepare(
        &context,
        &guard,
        &crate::file_mutation::MutationScope::single(dir.path()).expect("scope"),
        "test_apply_failure",
        None,
        [
            compiler.clone(),
            bak_of(&compiler),
            validator.clone(),
            bak_of(&validator),
        ],
    )
    .expect("durable transaction");
    let result = perform_transition_apply_fs(&transition, None);

    assert!(result.is_err(), "missing source must fail the apply");
    mutation
        .rollback(context.storage())
        .expect("durable rollback");
    assert_eq!(
        fs::read(&compiler).expect("compiler readable"),
        b"original-compiler",
        "the compiler must be restored to its original bytes"
    );
    assert_eq!(
        fs::read(&validator).expect("validator readable"),
        b"original-validator",
        "the validator must be restored to its original bytes"
    );
    assert!(
        !bak_of(&compiler).exists() && !bak_of(&validator).exists(),
        "both recovery sidecars must be consumed by the restore"
    );
}

#[test]
fn xiph_transition_failure_during_late_copy_or_before_commit_rolls_back_all_members() {
    for failure_point in ["during_late_member_copy", "before_database_commit"] {
        let dir = tempfile::tempdir().expect("temp dir");
        let game_dir = dir.path().join("game");
        let source_dir = dir.path().join("source");
        fs::create_dir_all(&game_dir).expect("game dir");
        fs::create_dir_all(&source_dir).expect("source dir");

        let vorbis_dir = game_dir.join("Engine/Binaries/ThirdParty/Vorbis/Win64");
        let ogg_dir = game_dir.join("Engine/Binaries/ThirdParty/Ogg/Win64");
        fs::create_dir_all(&vorbis_dir).expect("Vorbis dir");
        fs::create_dir_all(&ogg_dir).expect("Ogg dir");
        let wrapper = vorbis_dir.join("vorbisfile.dll");
        let vorbis = vorbis_dir.join("vorbis.dll");
        let ogg = ogg_dir.join("ogg.dll");
        let source_wrapper = source_dir.join("vorbisfile.dll");
        let source_vorbis = source_dir.join("vorbis.dll");
        let source_ogg = source_dir.join("ogg.dll");
        write(
            &wrapper,
            &synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]),
        );
        write(
            &vorbis,
            &synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]),
        );
        write(&ogg, &synthetic_xiph_pe("ogg_sync_init", &[]));
        write(
            &source_wrapper,
            &synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]),
        );
        write(
            &source_vorbis,
            &synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]),
        );
        write(&source_ogg, &synthetic_xiph_pe("ogg_sync_init", &[]));

        let game = sample_game_at(&game_dir);
        let baseline = vec![
            observed_xiph_file(&wrapper),
            observed_xiph_file(&vorbis),
            observed_xiph_file(&ogg),
        ];
        let component_id =
            ComponentId::new(format!("component:xiph:{failure_point}")).expect("component id");
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        )
        .with_file(baseline[0].clone())
        .with_file(baseline[1].clone())
        .with_file(baseline[2].clone());
        let artifact = LibraryArtifact::new(
            ArtifactId::new(format!("artifact:xiph:{failure_point}")).expect("artifact id"),
            LibraryTechnology::XiphVorbis,
            "vorbisfile.dll",
            vec![
                observed_xiph_file(&source_wrapper),
                observed_xiph_file(&source_vorbis),
                observed_xiph_file(&source_ogg),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        );
        let transition = filesystem_transition(&component, &artifact, &baseline);

        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&component))
            .expect("component");
        let context = crate::Context::from_storage(storage);
        let guard = crate::game_mutation_lock::blocking_lock(game.id());
        let mutation = crate::file_mutation::DurableFileTransaction::prepare(
            &context,
            &guard,
            &crate::file_mutation::MutationScope::single(&game_dir).expect("scope"),
            "test_xiph_apply_failure",
            Some(component_id.as_str()),
            [
                wrapper.clone(),
                bak_of(&wrapper),
                vorbis.clone(),
                bak_of(&vorbis),
                ogg.clone(),
                bak_of(&ogg),
            ],
        )
        .expect("durable transaction");

        if failure_point == "during_late_member_copy" {
            fs::remove_file(&source_wrapper).expect("remove final source to inject copy failure");
        }
        let result: renderpilot_application::AppResult<()> =
            perform_transition_apply_fs(&transition, None).and_then(|_| {
                if failure_point == "before_database_commit" {
                    Err(renderpilot_application::AppError::provider_failed(
                        "injected failure before database commit",
                    ))
                } else {
                    Ok(())
                }
            });
        assert!(result.is_err(), "{failure_point} must abort the apply");
        mutation
            .rollback(context.storage())
            .expect("durable rollback");

        assert_eq!(
            fs::read(&wrapper).expect("wrapper readable"),
            synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]),
            "{failure_point}: wrapper must be restored"
        );
        assert_eq!(
            fs::read(&vorbis).expect("vorbis readable"),
            synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]),
            "{failure_point}: Vorbis must be restored"
        );
        assert_eq!(
            fs::read(&ogg).expect("ogg readable"),
            synthetic_xiph_pe("ogg_sync_init", &[]),
            "{failure_point}: Ogg must be restored"
        );
        assert!(
            !bak_of(&wrapper).exists() && !bak_of(&vorbis).exists() && !bak_of(&ogg).exists(),
            "{failure_point}: recovery sidecars must be consumed"
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(game.id())
                .expect("stored components"),
            vec![component],
            "{failure_point}: database state must remain unchanged"
        );
    }
}

#[test]
fn revert_to_baseline_restores_backup_and_deletes_added_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let replaced = dir.path().join("nvngx_dlss.dll");
    let added = dir.path().join("nvngx_dlssg.dll");
    write(&replaced, b"overlay");
    write(&bak_of(&replaced), b"original");
    write(&added, b"added-by-swap");

    let current = vec![comp_file(&replaced), comp_file(&added)];
    let baseline = vec![comp_file(&replaced).with_sha256(sha_of(&bak_of(&replaced)))];
    revert_to_baseline_fs(&current, &baseline).expect("revert should succeed");

    assert_eq!(fs::read(&replaced).expect("readable"), b"original");
    assert!(!bak_of(&replaced).exists(), "backup consumed by restore");
    assert!(!added.exists(), "overlay-added file removed on revert");
}
