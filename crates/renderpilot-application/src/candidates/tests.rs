use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
    Version,
};

use super::dto::{CandidateComparison, ComponentReplacementCandidates};
use super::matcher::{find_replacement_candidates, CandidateContext};

#[test]
fn selects_only_same_technology_candidates() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );

    let sr_candidate = sample_artifact(
        "artifact:sr-3.7",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let fg_candidate = sample_artifact(
        "artifact:fg-3.7",
        GraphicsTechnology::DlssFrameGeneration,
        Some("3.7.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameB/nvngx_dlssg.dll",
        Some("game:b"),
    );
    let rr_candidate = sample_artifact(
        "artifact:rr-3.7",
        GraphicsTechnology::DlssRayReconstruction,
        Some("3.7.0"),
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "C:/Games/GameB/nvngx_dlssd.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(
        &[component],
        &[
            sr_candidate.clone(),
            fg_candidate.clone(),
            rr_candidate.clone(),
        ],
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(groups[0].candidates()[0].artifact_id(), sr_candidate.id());
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::NewerVersion
    );
}

#[test]
fn dlss_v1_is_incompatible_with_v2_and_ignored() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("1.0.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let v2_artifact = sample_artifact(
        "artifact:dlss-v2",
        GraphicsTechnology::DlssSuperResolution,
        Some("2.0.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let v1_artifact = sample_artifact(
        "artifact:dlss-v1-other",
        GraphicsTechnology::DlssSuperResolution,
        Some("1.5.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );

    let groups = find_test_candidates(&[component], &[v2_artifact, v1_artifact]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(
        groups[0].candidates()[0].artifact_id().as_str(),
        "artifact:dlss-v1-other"
    );
}

#[test]
fn dlss_v2_is_compatible_with_v3_only() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("2.0.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let v3_artifact = sample_artifact(
        "artifact:dlss-v3",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.0.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let v1_artifact = sample_artifact(
        "artifact:dlss-v1",
        GraphicsTechnology::DlssSuperResolution,
        Some("1.0.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );

    let groups = find_test_candidates(&[component], &[v3_artifact, v1_artifact]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
}

#[test]
fn includes_all_known_versions_for_replacement() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.7.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let older = sample_artifact(
        "artifact:older",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let same = sample_artifact(
        "artifact:same",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );
    let newer = sample_artifact(
        "artifact:newer",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.8.0"),
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "C:/Games/GameD/nvngx_dlss.dll",
        Some("game:d"),
    );

    let groups = find_test_candidates(&[component], &[older, same, newer]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 3);
    // Version-descending order; the comparison verdict rides along per row.
    let ids: Vec<&str> = groups[0]
        .candidates()
        .iter()
        .map(|candidate| candidate.artifact_id().as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["artifact:newer", "artifact:same", "artifact:older"]
    );
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::NewerVersion
    );
    assert_eq!(
        groups[0].candidates()[1].comparison(),
        CandidateComparison::UnknownVersion
    );
    assert_eq!(
        groups[0].candidates()[2].comparison(),
        CandidateComparison::OlderVersion
    );
}

#[test]
fn order_is_version_descending_even_when_every_candidate_is_older() {
    // The installed version is newer than every candidate: the order must not
    // depend on comparison partitions — plain version-descending, always.
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("9.9.9"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let make = |id: &str, version: &str, sha: char| {
        sample_artifact(
            id,
            GraphicsTechnology::DlssSuperResolution,
            Some(version),
            &sha.to_string().repeat(64),
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        )
    };

    let groups = find_test_candidates(
        &[component],
        &[
            make("artifact:v35", "3.5.0", 'b'),
            make("artifact:v38", "3.8.0", 'c'),
            make("artifact:v37", "3.7.0", 'd'),
        ],
    );

    let ids: Vec<&str> = groups[0]
        .candidates()
        .iter()
        .map(|candidate| candidate.artifact_id().as_str())
        .collect();
    assert_eq!(ids, vec!["artifact:v38", "artifact:v37", "artifact:v35"]);
}

#[test]
fn unknown_version_candidates_sort_last() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.7.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let unknown = sample_artifact(
        "artifact:unknown",
        GraphicsTechnology::DlssSuperResolution,
        None,
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let versioned = sample_artifact(
        "artifact:v35",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.5.0"),
        &"c".repeat(64),
        "C:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );

    let groups = find_test_candidates(&[component], &[unknown, versioned]);

    let ids: Vec<&str> = groups[0]
        .candidates()
        .iter()
        .map(|candidate| candidate.artifact_id().as_str())
        .collect();
    assert_eq!(ids, vec!["artifact:v35", "artifact:unknown"]);
}

#[test]
fn download_state_does_not_reorder_distinct_versions() {
    // A completed download must not move a candidate: a downloaded older
    // version still sits below a non-downloaded newer one.
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let newer = sample_artifact(
        "artifact:v38",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.8.0"),
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let older_downloaded = sample_artifact(
        "artifact:v37",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"c".repeat(64),
        "C:/Library/nvngx_dlss.dll",
        None,
    );

    let context = CandidateContext::new(
        [older_downloaded.id().clone()].into_iter().collect(),
        std::collections::HashMap::new(),
        std::collections::HashSet::new(),
    );
    let groups = find_replacement_candidates(&[component], &[newer, older_downloaded], &context);

    let rows: Vec<(&str, bool)> = groups[0]
        .candidates()
        .iter()
        .map(|candidate| (candidate.artifact_id().as_str(), candidate.is_downloaded()))
        .collect();
    assert_eq!(rows, vec![("artifact:v38", false), ("artifact:v37", true)]);
}

#[test]
fn downloaded_twin_survives_deduplication() {
    // Two distinct artifacts share (file_name, version, build type) — e.g. a
    // downloaded library copy and its manifest twin. Exactly one row survives,
    // and it must be the downloaded one even when the sha tie-break alone
    // would have put the other first.
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let downloaded = sample_artifact(
        "artifact:downloaded",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"f".repeat(64), // sorts AFTER the twin's sha — only is_downloaded puts it first
        "C:/Library/nvngx_dlss.dll",
        None,
    );
    let manifest_twin = sample_artifact(
        "artifact:manifest",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );

    let context = CandidateContext::new(
        [downloaded.id().clone()].into_iter().collect(),
        std::collections::HashMap::new(),
        std::collections::HashSet::new(),
    );
    let groups = find_replacement_candidates(&[component], &[manifest_twin, downloaded], &context);

    assert_eq!(groups[0].candidates().len(), 1, "twins collapse to one row");
    assert_eq!(
        groups[0].candidates()[0].artifact_id().as_str(),
        "artifact:downloaded",
        "the downloaded twin survives deduplication"
    );
    assert!(groups[0].candidates()[0].is_downloaded());
}

#[test]
fn unknown_versions_are_manual_candidates() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let candidate = sample_artifact(
        "artifact:unknown",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(&[component], &[candidate]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::UnknownVersion
    );
}

#[test]
fn matches_same_family_artifacts_across_file_names() {
    // A Streamline component and a Streamline artifact with different file
    // names are a valid bundle swap now (the engine swaps the whole bundle),
    // so the candidate is offered instead of skipped on a name mismatch.
    let component = sample_component(
        "component:game-a:streamline",
        "game:a",
        GraphicsTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
        Some("2.4.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/sl.common.dll",
    );
    let artifact = sample_artifact(
        "artifact:streamline-interposer",
        GraphicsTechnology::NvidiaStreamline,
        Some("2.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/sl.interposer.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(&[component], &[artifact]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
}

#[test]
fn deduplicates_identical_candidates_observed_in_multiple_games() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    // Identical content has the same bundle id (`ArtifactId::for_bundle`), so
    // the same DLL observed in two different games is one artifact id.
    let duplicate_a = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let duplicate_b = sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "D:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );

    let groups = find_test_candidates(&[component], &[duplicate_a.clone(), duplicate_b]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(groups[0].candidates()[0].artifact_id(), duplicate_a.id());
}

#[test]
fn identical_sha256_is_returned_as_candidate() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let artifact = sample_artifact(
        "artifact:same-sha",
        GraphicsTechnology::DlssSuperResolution,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(&[component], &[artifact]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
}

fn find_test_candidates(
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
) -> Vec<ComponentReplacementCandidates> {
    find_replacement_candidates(components, artifacts, &CandidateContext::empty())
}

fn sample_component(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    version: Option<&str>,
    sha256: &str,
    path: &str,
) -> GraphicsComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
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
    version: Option<&str>,
    sha256: &str,
    path: &str,
    source_game_id: Option<&str>,
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

    let artifact = LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        file_name,
        vec![file],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source("scan-folder")
    .expect("source should be valid");

    match source_game_id {
        Some(source_game_id) => artifact.with_source_game_id(
            GameId::new(source_game_id).expect("source game id should be valid"),
        ),
        None => artifact,
    }
}

/// Builds a multi-file (split) FSR package artifact with virtual `manifest://`
/// member paths — like a composed FSR 4 release: upscaler (primary) + loader.
fn split_package_artifact(artifact_id: &str, version: &str) -> LibraryArtifact {
    let upscaler = ComponentFile::new(PathRef::new("manifest://upscaler").unwrap())
        .with_sha256(Sha256Hash::new("a".repeat(64)).unwrap())
        .with_version(Version::parse(version).unwrap());
    let loader = ComponentFile::new(PathRef::new("manifest://loader").unwrap())
        .with_sha256(Sha256Hash::new("b".repeat(64)).unwrap())
        .with_version(Version::parse("2.1.0").unwrap());

    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![upscaler, loader],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("split package artifact should be valid")
}

#[test]
fn split_fsr_component_is_not_offered_a_unified_single_file_downgrade() {
    let component = sample_component(
        "component:game-a:fsr",
        "game:a",
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
        Some("4.0.3"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
    );
    // The unified FSR 3.x backend is a single `amd_fidelityfx_dx12.dll`.
    let unified = sample_artifact(
        "artifact:fsr-3.1",
        GraphicsTechnology::AmdFsr,
        Some("3.1.0"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_dx12.dll",
        None,
    );

    let groups = find_test_candidates(&[component], &[unified]);
    assert!(
        groups.is_empty(),
        "a split FSR set must not be offered a unified single-file downgrade"
    );
}

#[test]
fn split_fsr_component_accepts_another_split_package() {
    let component = sample_component(
        "component:game-a:fsr",
        "game:a",
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
        Some("4.0.3"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
    );
    let newer = split_package_artifact("artifact:fsr-4.1", "4.1.0");

    let groups = find_test_candidates(&[component], &[newer]);
    assert_eq!(groups.len(), 1, "a newer split package is a valid update");
    assert_eq!(groups[0].candidates().len(), 1);
}

#[test]
fn unified_fsr_component_accepts_both_unified_and_split() {
    let component = sample_component(
        "component:game-a:fsr",
        "game:a",
        GraphicsTechnology::AmdFsr,
        Swappability::Swappable,
        Some("3.1.0"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_dx12.dll",
    );
    let unified = sample_artifact(
        "artifact:fsr-3.1.1",
        GraphicsTechnology::AmdFsr,
        Some("3.1.1"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_dx12.dll",
        None,
    );
    let split = split_package_artifact("artifact:fsr-4.0", "4.0.3");

    let groups = find_test_candidates(&[component], &[unified, split]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].candidates().len(),
        2,
        "a unified FSR 3.x set accepts both a unified swap and a split upgrade"
    );
}

#[test]
fn cohesive_fsr_candidate_group_uses_entry_point_as_display_path() {
    let component = fsr_component(&[
        "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll",
    ]);
    let split = split_package_artifact("artifact:fsr-4.0", "4.0.3");

    let groups = find_test_candidates(&[component], &[split]);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].file_path().as_str(),
        "C:/Game/amd_fidelityfx_dx12.dll"
    );
    assert_eq!(
        groups[0].current_version().map(|version| version.as_str()),
        Some("4.0.3")
    );
}

#[test]
fn mixed_fsr_component_reports_the_entry_points_version() {
    // A real unified FSR 3.1 entry point next to developer-left split files:
    // the builds do not match (no release cohesion), so the version the game
    // actually runs must win — regardless of the stored file order.
    let mut component = GraphicsComponent::new(
        ComponentId::new("component:game-a:fsr").expect("component id"),
        GameId::new("game:a").expect("game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
    );
    for (name, version, sha) in [
        (
            "amd_fidelityfx_upscaler_dx12.dll",
            "4.0.3.604",
            "a".repeat(64),
        ),
        ("amd_fidelityfx_dx12.dll", "1.0.1.41314", "b".repeat(64)),
        (
            "amd_fidelityfx_loader_dx12.dll",
            "2.1.0.604",
            "c".repeat(64),
        ),
    ] {
        component = component.with_file(
            ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("path"))
                .with_sha256(Sha256Hash::new(sha).expect("sha"))
                .with_version(Version::parse(version).expect("version")),
        );
    }
    let split = split_package_artifact("artifact:fsr-4.1", "4.1.0");

    let groups = find_test_candidates(&[component], &[split]);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].file_path().as_str(),
        "C:/Game/amd_fidelityfx_dx12.dll"
    );
    assert_eq!(
        groups[0].current_version().map(|version| version.as_str()),
        Some("1.0.1.41314"),
        "the leftover upscaler must not hijack the current version"
    );
}

#[test]
fn native_fsr_upscaler_component_only_matches_upscaler_singles() {
    let component = sample_component(
        "component:game-a:fsr-upscaler",
        "game:a",
        GraphicsTechnology::AmdFsrUpscaler,
        Swappability::Swappable,
        Some("4.0.3"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
    );
    let upscaler = sample_artifact(
        "artifact:fsr-upscaler-4.1",
        GraphicsTechnology::AmdFsrUpscaler,
        Some("4.1.0"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_upscaler_dx12.dll",
        None,
    );
    let framegen = sample_artifact(
        "artifact:fsr-framegen-4.1",
        GraphicsTechnology::AmdFsrFrameGeneration,
        Some("4.1.0"),
        &"d".repeat(64),
        "C:/Lib/amd_fidelityfx_framegeneration_dx12.dll",
        None,
    );
    let package = split_package_artifact("artifact:fsr-4.1", "4.1.0");

    let groups = find_test_candidates(&[component], &[upscaler.clone(), framegen, package]);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(groups[0].candidates()[0].artifact_id(), upscaler.id());
}

/// Builds a multi-file FSR component with the given file basenames (FSR family,
/// the first file is the primary). Used to model dx12-lineage vs native FSR 4.
fn fsr_component(file_names: &[&str]) -> GraphicsComponent {
    let mut component = GraphicsComponent::new(
        ComponentId::new("component:game-a:fsr").expect("component id"),
        GameId::new("game:a").expect("game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
    );
    for (index, name) in file_names.iter().enumerate() {
        let sha = char::from(b'a' + index as u8).to_string().repeat(64);
        component = component.with_file(
            ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("path"))
                .with_sha256(Sha256Hash::new(sha).expect("sha"))
                .with_version(Version::parse("4.0.3").expect("version")),
        );
    }
    component
}

#[test]
fn dx12_lineage_fsr4_is_offered_a_unified_fsr3_downgrade() {
    // A game we upgraded to FSR 4 still loads `amd_fidelityfx_dx12.dll` (the loader
    // is installed under that name), so it can return to FSR 3.1.
    let upgraded = fsr_component(&[
        "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll",
    ]);
    let unified = sample_artifact(
        "artifact:fsr-3.1.4",
        GraphicsTechnology::AmdFsr,
        Some("3.1.4"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_dx12.dll",
        None,
    );

    let groups = find_test_candidates(&[upgraded], &[unified]);
    assert_eq!(
        groups.len(),
        1,
        "a dx12-lineage FSR 4 set can pick a unified FSR 3.1 again"
    );
    assert_eq!(groups[0].candidates().len(), 1);
}

#[test]
fn native_fsr4_is_not_offered_a_unified_fsr3_downgrade() {
    // A native FSR 4 game loads its own loader and has no dx12 entry point — there
    // is no FSR 3 to return to.
    let native = fsr_component(&[
        "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_loader_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll",
    ]);
    let unified = sample_artifact(
        "artifact:fsr-3.1.4",
        GraphicsTechnology::AmdFsr,
        Some("3.1.4"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_dx12.dll",
        None,
    );

    let groups = find_test_candidates(&[native], &[unified]);
    assert!(
        groups.is_empty(),
        "a native FSR 4 set must not be offered a unified FSR 3 downgrade"
    );
}
