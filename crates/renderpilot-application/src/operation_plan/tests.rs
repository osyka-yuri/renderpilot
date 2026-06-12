use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
    Version,
};

use super::{
    build_swap_operation_plan, OperationPlanBlocker, OperationPlanFileAction,
    OperationPlanRiskLevel, OperationPlanWarning,
};

#[test]
fn builds_valid_swap_plan_for_swappable_component() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert_eq!(plan.game_id(), component.game_id());
    assert_eq!(plan.operation_type().as_str(), "replace_component");
    assert_eq!(plan.target_path().as_str(), "C:/Games/GameA/nvngx_dlss.dll");
    assert_eq!(
        plan.replacement_path().as_str(),
        "D:/Library/nvngx_dlss.dll"
    );
    assert_eq!(plan.original_version().map(|v| v.as_str()), Some("3.5.0"));
    assert_eq!(
        plan.replacement_version().map(|v| v.as_str()),
        Some("3.7.0")
    );
    assert_eq!(
        plan.original_sha256().map(|h| h.as_str()),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        plan.replacement_sha256().map(|h| h.as_str()),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );

    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Low);
    assert!(!plan.requires_elevation());
    assert!(plan.blockers().is_empty());
    assert!(plan.warnings().is_empty());
}

#[test]
fn operation_id_is_generated() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan_1 = build_swap_operation_plan(&component, &artifact).expect("plan should build");
    let plan_2 = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    let id_1 = plan_1.operation_id().as_str();
    let id_2 = plan_2.operation_id().as_str();

    assert!(id_1.starts_with("operation:replace_component:"));
    assert!(id_2.starts_with("operation:replace_component:"));

    assert_ne!(
        id_1, id_2,
        "operation ids should incorporate timestamp/nonce to ensure uniqueness"
    );
}

#[test]
fn confirmation_token_is_64_char_hex_string() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    let token = plan.confirmation_token();

    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn blocks_technology_mismatch() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:fsr-3.1",
        GraphicsTechnology::AmdFsr,
        "D:/Library/amd_fssr3.dll",
        Some("3.1.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Blocked);
    assert!(plan
        .blockers()
        .contains(&OperationPlanBlocker::TechnologyMismatch));
}

#[test]
fn blocks_invalid_artifact_with_same_hash() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.5",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss_original.dll",
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Blocked);
    assert!(plan
        .blockers()
        .contains(&OperationPlanBlocker::ArtifactMatchesCurrentFile));
}

#[test]
fn non_swappable_component_requires_confirmation_or_blocks() {
    let bundle_only_component = sample_component(
        "component:game-a:streamline",
        "game:a",
        GraphicsTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
        "C:/Games/GameA/sl.interposer.dll",
        Some("2.4.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let unsafe_component = sample_component(
        "component:game-a:unsafe",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Unsafe,
        "C:/Games/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
    );
    let streamline_artifact = sample_artifact(
        "artifact:streamline-2.5",
        GraphicsTechnology::NvidiaStreamline,
        "D:/Library/sl.interposer.dll",
        Some("2.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    let dlss_artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    );

    let bundle_only_plan = build_swap_operation_plan(&bundle_only_component, &streamline_artifact)
        .expect("plan should build");
    let unsafe_plan =
        build_swap_operation_plan(&unsafe_component, &dlss_artifact).expect("plan should build");

    assert_eq!(bundle_only_plan.risk_level(), OperationPlanRiskLevel::High);
    assert!(bundle_only_plan
        .warnings()
        .contains(&OperationPlanWarning::ConfirmationRequiredForSwappability));
    assert_eq!(unsafe_plan.risk_level(), OperationPlanRiskLevel::Blocked);
    assert!(unsafe_plan
        .blockers()
        .contains(&OperationPlanBlocker::ComponentUnsafe));
}

#[test]
fn streamline_bundle_requires_confirmation_warning() {
    // Streamline is BundleOnly, so it must still surface a confirmation warning
    // (and HIGH risk) now that the dedicated StreamlinePartialSwap warning is gone.
    let component = sample_component(
        "component:game-a:streamline",
        "game:a",
        GraphicsTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
        "C:/Games/GameA/sl.interposer.dll",
        Some("2.4.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:streamline-2.5",
        GraphicsTechnology::NvidiaStreamline,
        "D:/Library/sl.interposer.dll",
        Some("2.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(plan
        .warnings()
        .contains(&OperationPlanWarning::ConfirmationRequiredForSwappability));
    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::High);
}

#[test]
fn missing_versions_require_manual_review_and_medium_risk() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Games/GameA/nvngx_dlss.dll",
        None,
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Medium);
    assert!(plan
        .warnings()
        .contains(&OperationPlanWarning::ManualVersionComparisonRequired));
}

#[test]
fn native_split_fsr_loader_targets_existing_loader_file_in_plan() {
    let component = sample_bundle_component(
        "component:game-a:fsr",
        "game:a",
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
        &[
            (
                "C:/Games/GameA/amd_fidelityfx_loader_dx12.dll",
                Some("2.0.0"),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                None,
            ),
            (
                "C:/Games/GameA/amd_fidelityfx_upscaler_dx12.dll",
                Some("4.0.2"),
                Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                None,
            ),
            (
                "C:/Games/GameA/amd_fidelityfx_framegeneration_dx12.dll",
                Some("3.1.5"),
                Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
                None,
            ),
        ],
    );
    let artifact = sample_bundle_artifact(
        "artifact:fsr-4.1",
        GraphicsTechnology::AmdFsr,
        &[
            (
                "D:/Library/amd_fidelityfx_upscaler_dx12.dll",
                Some("4.1.0"),
                "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                None,
            ),
            (
                "D:/Library/amd_fidelityfx_loader_dx12.dll",
                Some("2.1.0"),
                "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                Some("amd_fidelityfx_dx12.dll"),
            ),
            (
                "D:/Library/amd_fidelityfx_framegeneration_dx12.dll",
                Some("4.1.0"),
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                None,
            ),
        ],
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    let loader_file = plan
        .files()
        .iter()
        .find(|file| {
            file.replacement_path().map(|path| path.as_str())
                == Some("D:/Library/amd_fidelityfx_loader_dx12.dll")
        })
        .expect("loader file should be present in plan");

    assert_eq!(loader_file.action(), OperationPlanFileAction::Replace);
    assert_eq!(
        loader_file.target_path().as_str(),
        "C:/Games/GameA/amd_fidelityfx_loader_dx12.dll"
    );
    assert!(
        plan.files()
            .iter()
            .all(|file| file.action() == OperationPlanFileAction::Replace),
        "native split update should replace every existing member in place"
    );
}

#[test]
fn entry_point_component_with_separate_loader_stack_targets_entry_point_in_plan() {
    // Mixed lineage — a real unified FSR 3.1 entry point next to a
    // loader+denoiser Ray Regeneration stack. The package's loader must replace
    // the entry point the game loads for upscaling; the RR stack's loader and
    // denoiser must not appear in the plan at all.
    let component = sample_bundle_component(
        "component:game-a:fsr",
        "game:a",
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
        &[
            (
                "C:/Games/GameA/amd_fidelityfx_dx12.dll",
                Some("1.0.1.41314"),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                None,
            ),
            (
                "C:/Games/GameA/amd_fidelityfx_loader_dx12.dll",
                Some("2.1.0.604"),
                Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                None,
            ),
            (
                "C:/Games/GameA/amd_fidelityfx_denoiser_dx12.dll",
                Some("1.0.0.604"),
                Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
                None,
            ),
        ],
    );
    let artifact = sample_bundle_artifact(
        "artifact:fsr-4.1",
        GraphicsTechnology::AmdFsr,
        &[
            (
                "D:/Library/amd_fidelityfx_upscaler_dx12.dll",
                Some("4.1.0"),
                "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                None,
            ),
            (
                "D:/Library/amd_fidelityfx_loader_dx12.dll",
                Some("2.2.0"),
                "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                Some("amd_fidelityfx_dx12.dll"),
            ),
            (
                "D:/Library/amd_fidelityfx_framegeneration_dx12.dll",
                Some("4.1.0"),
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                None,
            ),
        ],
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    let loader_file = plan
        .files()
        .iter()
        .find(|file| {
            file.replacement_path().map(|path| path.as_str())
                == Some("D:/Library/amd_fidelityfx_loader_dx12.dll")
        })
        .expect("loader file should be present in plan");

    assert_eq!(loader_file.action(), OperationPlanFileAction::Replace);
    assert_eq!(
        loader_file.target_path().as_str(),
        "C:/Games/GameA/amd_fidelityfx_dx12.dll",
        "the package loader must replace the entry point, not the RR stack's loader"
    );
    assert!(
        plan.files().iter().all(|file| {
            let target = file.target_path().as_str();
            target != "C:/Games/GameA/amd_fidelityfx_loader_dx12.dll"
                && target != "C:/Games/GameA/amd_fidelityfx_denoiser_dx12.dll"
        }),
        "the game's own loader+denoiser stack must stay untouched"
    );
    assert!(
        plan.files().iter().any(|file| {
            file.action() == OperationPlanFileAction::Add
                && file.target_path().as_str() == "C:/Games/GameA/amd_fidelityfx_upscaler_dx12.dll"
        }),
        "the upscaler is added under its own name"
    );
}

#[test]
fn protected_windows_paths_require_elevation() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Program Files/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(plan.requires_elevation());
    assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Medium);
}

#[test]
fn protected_windows_paths_are_case_insensitive() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "D:/proGRAM fileS (x86)/GameA/nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(plan.requires_elevation());
}

#[test]
fn protected_windows_backslash_paths_require_elevation() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:\\Windows\\System32\\nvngx_dlss.dll",
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:\\Library\\nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(plan.requires_elevation());
}

#[test]
fn similar_but_unprotected_windows_roots_do_not_require_elevation() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "C:/Program Files Games/GameA/nvngx_dlss.dll", // prefix match, but not identical directory
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(!plan.requires_elevation());
}

#[test]
fn non_windows_paths_do_not_require_elevation() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        "/usr/local/games/GameA/nvngx_dlss.dll", // Unix path
        Some("3.5.0"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let artifact = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

    assert!(!plan.requires_elevation());
}

fn sample_component(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    path: &str,
    version: Option<&str>,
    sha256: Option<&str>,
) -> GraphicsComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    if let Some(sha256) = sha256 {
        file = file.with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));
    }

    GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    )
    .with_file(file)
}

fn sample_artifact(
    artifact_id: &str,
    technology: GraphicsTechnology,
    path: &str,
    version: Option<&str>,
    sha256: &str,
) -> LibraryArtifact {
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("artifact path should contain a file name");
    let mut file = ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        file_name,
        vec![file],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source("scan-folder")
    .expect("source should be valid")
}

/// `(path, version, sha256, install_as)` spec for one file in a test bundle.
type ComponentFileSpec<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

fn sample_bundle_component(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    files: &[ComponentFileSpec<'_>],
) -> GraphicsComponent {
    let mut component = GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    );

    for (path, version, sha256, install_as) in files {
        let mut file =
            ComponentFile::new(PathRef::new(*path).expect("component path should be valid"));

        if let Some(version) = version {
            file = file.with_version(Version::parse(*version).expect("version should be valid"));
        }
        if let Some(sha256) = sha256 {
            file = file.with_sha256(Sha256Hash::new(*sha256).expect("sha256 should be valid"));
        }
        if let Some(install_as) = install_as {
            file = file.with_install_as(*install_as);
        }

        component = component.with_file(file);
    }

    component
}

fn sample_bundle_artifact(
    artifact_id: &str,
    technology: GraphicsTechnology,
    files: &[(&str, Option<&str>, &str, Option<&str>)],
) -> LibraryArtifact {
    let component_files = files
        .iter()
        .map(|(path, version, sha256, install_as)| {
            let mut file =
                ComponentFile::new(PathRef::new(*path).expect("artifact path should be valid"))
                    .with_sha256(Sha256Hash::new(*sha256).expect("sha256 should be valid"));

            if let Some(version) = version {
                file =
                    file.with_version(Version::parse(*version).expect("version should be valid"));
            }
            if let Some(install_as) = install_as {
                file = file.with_install_as(*install_as);
            }

            file
        })
        .collect();

    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        std::path::Path::new(files[0].0)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("artifact path should contain a file name"),
        component_files,
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source("scan-folder")
    .expect("source should be valid")
}
