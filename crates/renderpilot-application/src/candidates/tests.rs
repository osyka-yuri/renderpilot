use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, CatalogPackageReceiptV1,
    CatalogReceiptSchemaV1, CatalogSignatureReceipt, CatalogTargetReceipt, ComponentFile,
    ComponentId, ComponentKind, D3d12ExecutableIdentity, GameId, LibraryArtifact, LibraryComponent,
    LibraryTechnology, PackageRelease, PackageVersion, PathRef, PeCompatibilityProfile,
    PeExportSet, ReleaseChannel, RuntimeCompatibility, RuntimeTarget, Sha256Hash, Swappability,
    UpstreamPackage, UpstreamPackageProvider, Version,
};

use crate::{
    D3d12ExecutableActionKind, D3d12ExecutableProfile, D3d12ExecutableSnapshot, SwapTargetProfile,
    dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME},
};

use super::dto::{
    ActiveCatalogPackage, CandidateComparison, ComponentReplacementCandidates,
    InstalledReleaseState, is_automatic_catalog_candidate,
};
use super::matcher::{CandidateContext, find_replacement_candidates};

#[test]
fn selects_only_same_technology_candidates() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );

    let sr_candidate = sample_artifact(
        "artifact:sr-3.7",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let fg_candidate = sample_artifact(
        "artifact:fg-3.7",
        LibraryTechnology::DlssFrameGeneration,
        Some("3.7.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameB/nvngx_dlssg.dll",
        Some("game:b"),
    );
    let rr_candidate = sample_artifact(
        "artifact:rr-3.7",
        LibraryTechnology::DlssRayReconstruction,
        Some("3.7.0"),
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "C:/Games/GameB/nvngx_dlssd.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(
        &[component],
        &[sr_candidate.clone(), fg_candidate, rr_candidate],
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
fn legacy_d3d12_candidates_require_the_exact_executable_sdk_line() {
    let component = sample_component(
        "component:game-a:d3d12",
        "game:a",
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
        Some("1.618.3"),
        &"a".repeat(64),
        "C:/Games/GameA/D3D12Core.dll",
    );
    let artifact = sample_artifact(
        "artifact:d3d12-618-5",
        LibraryTechnology::D3D12Agility,
        Some("1.618.5"),
        &"b".repeat(64),
        "C:/Library/D3D12Core.dll",
        None,
    )
    .with_metadata(
        ArtifactMetadata::default().with_runtime_target(
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
        ),
    );

    let blocked = find_replacement_candidates(
        std::slice::from_ref(&component),
        std::slice::from_ref(&artifact),
        &CandidateContext::empty()
            .with_target_profile(SwapTargetProfile::new(Some(Architecture::X64), Some(619))),
    );
    assert!(blocked.is_empty());

    let allowed = find_replacement_candidates(
        &[component],
        &[artifact],
        &CandidateContext::empty()
            .with_target_profile(SwapTargetProfile::new(Some(Architecture::X64), Some(618))),
    );
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed[0].candidates().len(), 1);
}

#[test]
fn managed_d3d12_candidates_allow_newer_lines_and_hide_original_downgrades() {
    let component = sample_component(
        "component:game-a:d3d12-managed",
        "game:a",
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
        Some("1.606.4"),
        &"a".repeat(64),
        "C:/Games/GameA/D3D12Core.dll",
    );
    let artifact = |id: &str, line: u32, hash: char| {
        sample_artifact(
            id,
            LibraryTechnology::D3D12Agility,
            Some(&format!("1.{line}.1")),
            &hash.to_string().repeat(64),
            &format!("C:/Library/{id}/D3D12Core.dll"),
            None,
        )
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: line }),
            ),
        )
    };
    let newer = artifact("artifact:d3d12-619", 619, 'b');
    let older = artifact("artifact:d3d12-605", 605, 'c');
    let original_hash = Sha256Hash::new("d".repeat(64)).expect("original hash");
    let current_hash = Sha256Hash::new("e".repeat(64)).expect("current hash");
    let profile = SwapTargetProfile::new(Some(Architecture::X64), Some(606))
        .with_d3d12_executable_snapshot(D3d12ExecutableSnapshot::new(
            PathRef::new("C:/Games/GameA/game.exe").expect("exe"),
            PathRef::new("C:/Games/GameA/game.exe.bak").expect("backup"),
            D3d12ExecutableIdentity::new(606, original_hash),
            D3d12ExecutableIdentity::new(606, current_hash),
            false,
            false,
        ));

    let groups = find_replacement_candidates(
        &[component],
        &[older, newer.clone()],
        &CandidateContext::empty().with_target_profile(profile),
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
    let candidate = &groups[0].candidates()[0];
    assert_eq!(candidate.artifact_id(), newer.id());
    let action = candidate
        .d3d12_executable_action()
        .expect("executable action");
    assert_eq!(action.kind(), D3d12ExecutableActionKind::Patch);
    assert_eq!(action.target_sdk_version(), 619);
    assert!(action.requires_confirmation());
}

#[test]
fn presentation_d3d12_candidates_explain_actions_without_minting_tokens() {
    let component = sample_component(
        "component:game-a:d3d12-presentation",
        "game:a",
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
        Some("1.606.1"),
        &"a".repeat(64),
        "C:/Games/GameA/D3D12Core.dll",
    );
    let artifact = sample_artifact(
        "artifact:d3d12-presentation-619",
        LibraryTechnology::D3D12Agility,
        Some("1.619.1"),
        &"b".repeat(64),
        "catalog://microsoft/D3D12Core.dll",
        None,
    )
    .with_metadata(
        ArtifactMetadata::default().with_runtime_target(
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 619 }),
        ),
    );
    let profile = SwapTargetProfile::new(Some(Architecture::X64), Some(606))
        .with_d3d12_executable_profile(D3d12ExecutableProfile::new(
            PathRef::new("C:/Games/GameA/game.exe").expect("exe"),
            PathRef::new("C:/Games/GameA/game.exe.bak").expect("backup"),
            606,
            606,
            false,
            false,
        ));

    let groups = find_replacement_candidates(
        &[component],
        &[artifact],
        &CandidateContext::empty().with_target_profile(profile),
    );
    let action = groups[0].candidates()[0]
        .d3d12_executable_action()
        .expect("presentation action");

    assert_eq!(action.kind(), D3d12ExecutableActionKind::Patch);
    assert!(action.requires_confirmation());
}

#[test]
fn dxc_candidates_allow_a_standalone_installed_compiler() {
    let component = sample_component(
        "component:game-a:dxc",
        "game:a",
        LibraryTechnology::MicrosoftDxc,
        Swappability::Swappable,
        Some("1.5.0"),
        &"a".repeat(64),
        &format!("C:/Games/GameA/{COMPILER_FILE_NAME}"),
    );
    let artifact = dxc_package_artifact();

    let groups = find_replacement_candidates(
        &[component],
        &[artifact],
        &CandidateContext::empty()
            .with_target_profile(SwapTargetProfile::new(Some(Architecture::X64), None)),
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 1);
}

#[test]
fn dlss_v1_is_incompatible_with_v2_and_ignored() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("1.0.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let v2_artifact = sample_artifact(
        "artifact:dlss-v2",
        LibraryTechnology::DlssSuperResolution,
        Some("2.0.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let v1_artifact = sample_artifact(
        "artifact:dlss-v1-other",
        LibraryTechnology::DlssSuperResolution,
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
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("2.0.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let v3_artifact = sample_artifact(
        "artifact:dlss-v3",
        LibraryTechnology::DlssSuperResolution,
        Some("3.0.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let v1_artifact = sample_artifact(
        "artifact:dlss-v1",
        LibraryTechnology::DlssSuperResolution,
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
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.7.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let older = sample_artifact(
        "artifact:older",
        LibraryTechnology::DlssSuperResolution,
        Some("3.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let same = sample_artifact(
        "artifact:same",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "C:/Games/GameC/nvngx_dlss.dll",
        Some("game:c"),
    );
    let newer = sample_artifact(
        "artifact:newer",
        LibraryTechnology::DlssSuperResolution,
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
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("9.9.9"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let make = |id: &str, version: &str, sha: char| {
        sample_artifact(
            id,
            LibraryTechnology::DlssSuperResolution,
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
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.7.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let unknown = sample_artifact(
        "artifact:unknown",
        LibraryTechnology::DlssSuperResolution,
        None,
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let versioned = sample_artifact(
        "artifact:v35",
        LibraryTechnology::DlssSuperResolution,
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
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let newer = sample_artifact(
        "artifact:v38",
        LibraryTechnology::DlssSuperResolution,
        Some("3.8.0"),
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let older_downloaded = sample_artifact(
        "artifact:v37",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"c".repeat(64),
        "C:/Library/nvngx_dlss.dll",
        None,
    );

    let context = CandidateContext::new(
        [older_downloaded.id().clone()].into_iter().collect(),
        std::collections::HashMap::new(),
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
fn distinct_payloads_with_the_same_version_are_not_deduplicated() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let downloaded = sample_artifact(
        "artifact:downloaded",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"f".repeat(64), // sorts AFTER the twin's sha — only is_downloaded puts it first
        "C:/Library/nvngx_dlss.dll",
        None,
    );
    let manifest_twin = sample_artifact(
        "artifact:manifest",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"b".repeat(64),
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );

    let context = CandidateContext::new(
        [downloaded.id().clone()].into_iter().collect(),
        std::collections::HashMap::new(),
    );
    let groups = find_replacement_candidates(&[component], &[manifest_twin, downloaded], &context);

    assert_eq!(groups[0].candidates().len(), 2);
    assert!(
        groups[0]
            .candidates()
            .iter()
            .any(|candidate| candidate.is_downloaded())
    );
}

#[test]
fn unknown_versions_are_manual_candidates() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let candidate = sample_artifact(
        "artifact:unknown",
        LibraryTechnology::DlssSuperResolution,
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
fn mixed_streamline_reports_range_and_comparisons_are_unknown() {
    // Matcher baseline and DTO report share the domain state: mixed plugins
    // have no comparison baseline, so every candidate remains manual-review.
    let component = streamline_component(&[
        ("sl.common.dll", Some("2.9.0")),
        ("sl.interposer.dll", Some("2.4.0")),
    ]);
    let package = streamline_package_artifact("artifact:sl-2.9", "2.9.0", 3);

    let groups = find_test_candidates(&[component], &[package]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].installed_release(),
        &InstalledReleaseState::Mixed {
            min_technical_version: Version::parse("2.4.0").expect("version"),
            max_technical_version: Version::parse("2.9.0").expect("version"),
        }
    );
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::UnknownVersion,
        "with no baseline, same-release package is still offered as unknown"
    );
}

#[test]
fn uniform_streamline_compares_against_shared_version() {
    let component = streamline_component(&[
        ("sl.common.dll", Some("2.4.0")),
        ("sl.interposer.dll", Some("2.4.0")),
    ]);
    let newer = streamline_package_artifact("artifact:sl-2.9", "2.9.0", 2);

    let groups = find_test_candidates(&[component], &[newer]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .installed_release()
            .known_version()
            .map(Version::as_str),
        Some("2.4.0")
    );
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::NewerVersion
    );
}

#[test]
fn pe_trailing_zeros_match_manifest_label_for_streamline_baseline() {
    // PE often reports 4-part versions; CDN packages may label 3-part. Equality
    // must not invent Newer/Older solely from trailing zeros.
    let component = streamline_component(&[
        ("sl.common.dll", Some("2.9.0.0")),
        ("sl.interposer.dll", Some("2.9.0.0")),
    ]);
    let same_release = streamline_package_artifact("artifact:sl-2.9", "2.9.0", 2);

    let groups = find_test_candidates(&[component], &[same_release]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .installed_release()
            .known_version()
            .map(Version::as_str),
        Some("2.9.0.0")
    );
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::UnknownVersion,
        "2.9.0.0 and 2.9.0 are the same release (not newer/older)"
    );
}

#[test]
fn distinct_streamline_payloads_with_the_same_version_remain_visible() {
    let component = streamline_component(&[
        ("sl.common.dll", Some("2.4.0")),
        ("sl.interposer.dll", Some("2.4.0")),
    ]);
    let alternate = streamline_artifact_with_members(
        "artifact:sl-alternate-payload",
        "2.9.0",
        &[("sl.common.dll", 'e'), ("sl.interposer.dll", 'f')],
    );
    let package = streamline_package_artifact("artifact:sl-pkg-2.9", "2.9.0", 2);

    let groups = find_test_candidates(&[component], &[alternate, package.clone()]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 2);
    assert!(
        groups[0]
            .candidates()
            .iter()
            .any(|candidate| candidate.artifact_id() == package.id()),
        "distinct complete payloads remain independently selectable"
    );
}

#[test]
fn catalog_and_local_payloads_with_the_same_version_remain_distinct() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        &"a".repeat(64),
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let local = sample_artifact(
        "artifact:local",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        &"b".repeat(64),
        "C:/Other/nvngx_dlss.dll",
        Some("game:b"),
    );
    let manifest = LibraryArtifact::new(
        ArtifactId::new("artifact:manifest").expect("id"),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![
            ComponentFile::new(PathRef::new("C:/cache/nvngx_dlss.dll").expect("path"))
                .with_sha256(Sha256Hash::new("c".repeat(64)).expect("sha"))
                .with_version(Version::parse("3.7.0").expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("manifest artifact");

    let groups = find_test_candidates(&[component], &[local, manifest.clone()]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].candidates().len(), 2);
    assert_eq!(
        groups[0].candidates()[0].artifact_id(),
        manifest.id(),
        "CatalogDownloaded still sorts before LocalObserved"
    );
}

#[test]
fn streamline_candidate_without_an_installed_target_is_rejected() {
    // A package that cannot write any installed plugin is not a transition,
    // even though its technology matches the component.
    let component = sample_component(
        "component:game-a:streamline",
        "game:a",
        LibraryTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
        Some("2.4.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/sl.common.dll",
    );
    let artifact = sample_artifact(
        "artifact:streamline-interposer",
        LibraryTechnology::NvidiaStreamline,
        Some("2.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/sl.interposer.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(&[component], &[artifact]);

    assert!(groups.is_empty());
}

#[test]
fn deduplicates_identical_candidates_observed_in_multiple_games() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        Some("3.5.0"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    // Identical content has the same bundle id (`ArtifactId::for_bundle`), so
    // the same DLL observed in two different games is one artifact id.
    let duplicate_a = sample_artifact(
        "artifact:dlss-3.7",
        LibraryTechnology::DlssSuperResolution,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );
    let duplicate_b = sample_artifact(
        "artifact:dlss-3.7",
        LibraryTechnology::DlssSuperResolution,
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
fn content_identical_to_the_installed_component_is_not_a_candidate() {
    let component = sample_component(
        "component:game-a:dlss",
        "game:a",
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameA/nvngx_dlss.dll",
    );
    let artifact = sample_artifact(
        "artifact:same-sha",
        LibraryTechnology::DlssSuperResolution,
        None,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("game:b"),
    );

    let groups = find_test_candidates(&[component], &[artifact]);

    assert!(groups.is_empty());
}

#[test]
fn package_backed_candidate_keeps_technical_version_separate_from_package_release() {
    let component = sample_component(
        "component:game-a:xess",
        "game:a",
        LibraryTechnology::IntelXeSs,
        Swappability::Swappable,
        Some("3.5.0"),
        &"a".repeat(64),
        "C:/Games/GameA/libxess.dll",
    );
    let artifact = sample_artifact(
        "artifact:package-version",
        LibraryTechnology::IntelXeSs,
        Some("101.7.2207.20"),
        &"b".repeat(64),
        "C:/Library/libxess.dll",
        None,
    )
    .with_metadata(
        ArtifactMetadata::default().with_upstream_package(
            UpstreamPackage::new(
                UpstreamPackageProvider::NuGet,
                "Microsoft.Direct3D.DXC",
                "1.7.2207.7",
            )
            .expect("package"),
        ),
    );

    let groups = find_test_candidates(&[component], &[artifact]);

    assert_eq!(
        groups[0].candidates()[0]
            .technical_version()
            .map(Version::as_str),
        Some("101.7.2207.20")
    );
    assert_eq!(
        groups[0].candidates()[0].comparison(),
        CandidateComparison::NewerVersion,
        "comparison must use the technical PE version rather than NuGet identity"
    );
}

#[test]
fn catalog_candidates_sort_by_full_package_release_before_pe_file_version() {
    let component = sample_component(
        "component:package-order",
        "game:a",
        LibraryTechnology::IntelXeSs,
        Swappability::Swappable,
        Some("0.5.0"),
        &"a".repeat(64),
        "C:/Games/GameA/libxess.dll",
    );
    let catalog_artifact = |id: &str, package_version: &str, pe_version: &str, hash: char| {
        let mut receipt = test_catalog_receipt(id, "intel_xess", package_version, None);
        receipt.release.channel = ReleaseChannel::Preview;
        sample_artifact(
            id,
            LibraryTechnology::IntelXeSs,
            Some(pe_version),
            &hash.to_string().repeat(64),
            &format!("C:/Library/{id}.dll"),
            None,
        )
        .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(receipt))
    };
    let package_newer = catalog_artifact("artifact:package-newer", "2.0.0-preview", "1.0.0", 'b');
    let pe_newer = catalog_artifact("artifact:pe-newer", "1.0.0-preview", "9.0.0", 'c');

    let groups = find_test_candidates(&[component], &[pe_newer, package_newer.clone()]);

    assert_eq!(
        groups[0].candidates()[0].artifact_id(),
        package_newer.id(),
        "candidate presentation order must follow the displayed package release"
    );
}

#[test]
fn automatic_policy_accepts_only_active_stable_catalog_candidates() {
    let component = sample_component(
        "component:automatic-policy",
        "game:a",
        LibraryTechnology::IntelXeSs,
        Swappability::Swappable,
        Some("1.0.0"),
        &"a".repeat(64),
        "C:/Games/GameA/libxess.dll",
    );
    let catalog_artifact = |id: &str, hash: char, channel| {
        let release = if channel == ReleaseChannel::Preview {
            "9.0.0-preview"
        } else {
            "9.0.0"
        };
        let mut receipt = test_catalog_receipt(id, "intel_xess", release, None);
        receipt.release.channel = channel;
        sample_artifact(
            id,
            LibraryTechnology::IntelXeSs,
            Some("9.0.0"),
            &hash.to_string().repeat(64),
            &format!("C:/Library/{id}.dll"),
            None,
        )
        .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(receipt))
    };
    let stable = catalog_artifact("artifact:stable", 'b', ReleaseChannel::Stable);
    let preview = catalog_artifact("artifact:preview", 'c', ReleaseChannel::Preview);
    let active_catalog = active_catalog_for(&[stable.clone(), preview.clone()]);
    let active = find_replacement_candidates(
        std::slice::from_ref(&component),
        &[stable.clone(), preview],
        &CandidateContext::new(std::collections::HashSet::new(), active_catalog),
    );
    let stable_candidate = active[0]
        .candidates()
        .iter()
        .find(|candidate| candidate.artifact_id() == stable.id())
        .expect("stable candidate");
    assert!(is_automatic_catalog_candidate(stable_candidate));
    assert!(
        active[0]
            .candidates()
            .iter()
            .filter(|candidate| candidate.artifact_id() != stable.id())
            .all(|candidate| !is_automatic_catalog_candidate(candidate))
    );

    let local_only =
        find_replacement_candidates(&[component], &[stable], &CandidateContext::empty());
    assert!(!is_automatic_catalog_candidate(
        &local_only[0].candidates()[0]
    ));
}

#[test]
fn active_descriptor_enriches_a_legacy_download_without_a_receipt() {
    let component = sample_component(
        "component:legacy-catalog",
        "game:a",
        LibraryTechnology::IntelXeSs,
        Swappability::Swappable,
        Some("1.0.0"),
        &"a".repeat(64),
        "C:/Games/GameA/libxess.dll",
    );
    let active = sample_artifact(
        "artifact:legacy-catalog",
        LibraryTechnology::IntelXeSs,
        Some("2.0.0"),
        &"b".repeat(64),
        "catalog://intel/libxess.dll",
        None,
    )
    .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(
        test_catalog_receipt("intel-xess-2.0.0", "intel_xess", "2.0.0", None),
    ));
    let active_catalog = active_catalog_for(std::slice::from_ref(&active));
    let legacy_download = active.clone().with_metadata(ArtifactMetadata::default());
    let groups = find_replacement_candidates(
        &[component],
        &[legacy_download],
        &CandidateContext::new([active.id().clone()].into_iter().collect(), active_catalog),
    );

    let candidate = &groups[0].candidates()[0];
    let package = candidate.catalog_package().expect("catalog package");
    assert_eq!(package.package_id(), "intel-xess-2.0.0");
    assert_eq!(
        package.availability(),
        renderpilot_domain::CatalogPackageAvailability::Available
    );
    assert!(is_automatic_catalog_candidate(candidate));
}

#[test]
fn openvr_installed_release_is_resolved_by_full_catalog_content() {
    let installed_file = ComponentFile::new(PathRef::new("C:/Game/openvr_api.dll").unwrap())
        .with_sha256(Sha256Hash::new("a".repeat(64)).unwrap())
        .with_pe_compatibility(PeCompatibilityProfile::new(
            Architecture::X64,
            PeExportSet::from_canonical_names(vec!["A".into()]).unwrap(),
        ));
    let component = LibraryComponent::new(
        ComponentId::new("component:openvr").unwrap(),
        GameId::new("game:openvr").unwrap(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::OpenVr,
        Swappability::Swappable,
    )
    .with_file(installed_file);
    let old = openvr_catalog_artifact("artifact:openvr-old", "1.0.0", "a", None);
    let canonical = openvr_catalog_artifact(
        "artifact:openvr-canonical",
        "1.1.0",
        "a",
        Some("revision b"),
    );
    let candidate = openvr_catalog_artifact("artifact:openvr-new", "2.0.0", "b", None);
    let active_catalog = active_catalog_for(&[old.clone(), canonical.clone(), candidate.clone()]);
    let context = CandidateContext::new(std::collections::HashSet::new(), active_catalog);

    let groups = find_replacement_candidates(&[component], &[old, canonical, candidate], &context);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .installed_release()
            .known_version()
            .map(Version::as_str),
        None
    );
    assert_eq!(
        groups[0].installed_release(),
        &InstalledReleaseState::Known {
            technical_version: None,
            release_label: Some("revision b".to_owned()),
            catalog_release: Some(PackageRelease {
                version: PackageVersion::parse("1.1.0").expect("package version"),
                channel: ReleaseChannel::Stable,
                label: Some("revision b".to_owned()),
            }),
        }
    );
    assert_eq!(groups[0].candidates().len(), 1);
    assert_eq!(
        groups[0].candidates()[0]
            .technical_version()
            .map(Version::as_str),
        None,
        "a package release must not be presented as a technical PE FileVersion"
    );
    assert_eq!(
        groups[0].candidates()[0]
            .catalog_package()
            .map(|package| package.release().version.as_str()),
        Some("2.0.0")
    );
}

#[test]
fn installed_catalog_release_resolution_is_not_openvr_specific() {
    let component = sample_component(
        "component:fsr-catalog-release",
        "game:a",
        LibraryTechnology::AmdFsr,
        Swappability::Swappable,
        Some("1.0.1.41314"),
        &"a".repeat(64),
        "C:/Game/amd_fidelityfx_dx12.dll",
    )
    .with_file(
        ComponentFile::new(PathRef::new("C:/Game/amd_fidelityfx_denoiser_dx12.dll").expect("path"))
            .with_sha256(Sha256Hash::new("c".repeat(64)).expect("hash"))
            .with_version(Version::parse("1.1.0").expect("version")),
    );
    let installed = sample_artifact(
        "artifact:fsr-installed",
        LibraryTechnology::AmdFsr,
        Some("1.0.1.41314"),
        &"a".repeat(64),
        "catalog://amd/amd_fidelityfx_dx12.dll",
        None,
    )
    .with_metadata(
        ArtifactMetadata::default()
            .with_release(
                Version::parse("4.1.1.2740").expect("release"),
                Some("FSR 3.1.4".to_owned()),
            )
            .expect("metadata")
            .with_catalog_package_receipt(test_catalog_receipt(
                "fsr-installed-package",
                "amd_fsr",
                "4.1.1.2740",
                Some("FSR 3.1.4"),
            )),
    );
    let installed_preview = sample_artifact(
        "artifact:fsr-installed-preview",
        LibraryTechnology::AmdFsr,
        Some("1.0.1.41314"),
        &"a".repeat(64),
        "catalog://amd/amd_fidelityfx_dx12.dll",
        None,
    )
    .with_metadata(
        ArtifactMetadata::default()
            .with_release(
                Version::parse("4.1.1.2740").expect("release"),
                Some("FSR preview".to_owned()),
            )
            .expect("metadata")
            .with_catalog_package_receipt({
                let mut receipt = test_catalog_receipt(
                    "fsr-installed-preview-package",
                    "amd_fsr",
                    "4.1.1.2740-preview",
                    Some("FSR preview"),
                );
                receipt.release.channel = ReleaseChannel::Preview;
                receipt
            }),
    );
    let candidate = sample_artifact(
        "artifact:fsr-candidate",
        LibraryTechnology::AmdFsr,
        Some("1.0.2.0"),
        &"b".repeat(64),
        "catalog://amd/amd_fidelityfx_dx12.dll",
        None,
    );
    let context = CandidateContext::new(
        std::collections::HashSet::new(),
        active_catalog_for(&[installed.clone(), installed_preview.clone()]),
    );

    let groups = find_replacement_candidates(
        &[component],
        &[installed, installed_preview, candidate],
        &context,
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].installed_release(),
        &InstalledReleaseState::Known {
            technical_version: Some(Version::parse("1.0.1.41314").expect("technical version")),
            release_label: Some("FSR 3.1.4".to_owned()),
            catalog_release: Some(PackageRelease {
                version: PackageVersion::parse("4.1.1.2740").expect("package version"),
                channel: ReleaseChannel::Stable,
                label: Some("FSR 3.1.4".to_owned()),
            }),
        },
        "stable package identity must win over a preview with the same technical version"
    );
}

#[test]
fn streamline_identity_uses_only_members_written_by_the_transition() {
    let component = streamline_component(&[
        ("sl.common.dll", Some("2.4.0")),
        ("sl.interposer.dll", Some("2.4.0")),
    ]);
    let installed_with_ignored_extra = streamline_artifact_with_members(
        "artifact:streamline-installed-with-extra",
        "2.9.0",
        &[
            ("sl.common.dll", 'a'),
            ("sl.interposer.dll", 'b'),
            ("sl.dlss.dll", 'c'),
        ],
    );
    assert!(
        find_test_candidates(
            std::slice::from_ref(&component),
            &[installed_with_ignored_extra]
        )
        .is_empty(),
        "a package whose written members already match the component is a no-op"
    );

    let first = streamline_artifact_with_members(
        "artifact:streamline-transition-a",
        "2.9.0",
        &[
            ("sl.common.dll", 'c'),
            ("sl.interposer.dll", 'd'),
            ("sl.dlss.dll", 'e'),
        ],
    );
    let second = streamline_artifact_with_members(
        "artifact:streamline-transition-b",
        "2.9.0",
        &[
            ("sl.common.dll", 'c'),
            ("sl.interposer.dll", 'd'),
            ("sl.reflex.dll", 'f'),
        ],
    );
    let groups = find_test_candidates(&[component], &[first, second]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].candidates().len(),
        1,
        "packages that write the same targets and hashes are one transition"
    );
}

fn openvr_catalog_artifact(
    id: &str,
    release: &str,
    hash: &str,
    label: Option<&str>,
) -> LibraryArtifact {
    let file = ComponentFile::new(PathRef::new("catalog://valve/openvr_api.dll").unwrap())
        .with_sha256(Sha256Hash::new(hash.repeat(64)).unwrap())
        .with_pe_compatibility(PeCompatibilityProfile::new(
            Architecture::X64,
            PeExportSet::from_canonical_names(vec!["A".into(), "B".into()]).unwrap(),
        ));
    let metadata = ArtifactMetadata::default()
        .with_release(Version::parse(release).unwrap(), label.map(str::to_owned))
        .unwrap()
        .with_runtime_target(RuntimeTarget::new(Architecture::X64))
        .with_upstream_package(
            UpstreamPackage::new(
                UpstreamPackageProvider::GitHub,
                "ValveSoftware/openvr",
                release,
            )
            .unwrap(),
        )
        .with_catalog_package_receipt(test_catalog_receipt(
            &format!("openvr-x64-{release}"),
            "openvr",
            release,
            label,
        ));
    LibraryArtifact::new(
        ArtifactId::new(id).unwrap(),
        LibraryTechnology::OpenVr,
        "openvr_api.dll",
        vec![file],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .unwrap()
    .with_metadata(metadata)
}

fn test_catalog_receipt(
    package_id: &str,
    technology: &str,
    release: &str,
    label: Option<&str>,
) -> CatalogPackageReceiptV1 {
    CatalogPackageReceiptV1 {
        schema_version: CatalogReceiptSchemaV1,
        package_id: package_id.to_owned(),
        vendor: "test-vendor".to_owned(),
        technology: technology.to_owned(),
        variant: "runtime".to_owned(),
        display_name: package_id.to_owned(),
        release: PackageRelease {
            version: PackageVersion::parse(release).expect("package version"),
            channel: ReleaseChannel::Stable,
            label: label.map(str::to_owned),
        },
        target: CatalogTargetReceipt {
            os: "windows".to_owned(),
            architecture: Architecture::X64,
            compatibility: None,
        },
        provenance: None,
        revision_sha256: Sha256Hash::new("e".repeat(64)).expect("hash"),
        primary_file_name: "runtime.dll".to_owned(),
        primary_sha256: Sha256Hash::new("a".repeat(64)).expect("hash"),
        primary_signature: CatalogSignatureReceipt::Unsigned,
        legal_documents: Vec::new(),
        size_bytes: 1,
    }
}

fn find_test_candidates(
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
) -> Vec<ComponentReplacementCandidates> {
    find_replacement_candidates(components, artifacts, &CandidateContext::empty())
}

fn active_catalog_for(
    artifacts: &[LibraryArtifact],
) -> std::collections::HashMap<ArtifactId, ActiveCatalogPackage> {
    artifacts
        .iter()
        .filter_map(|artifact| {
            let receipt = artifact.metadata().catalog_package_receipt()?;
            Some((
                artifact.id().clone(),
                ActiveCatalogPackage::new(
                    receipt.package_id.clone(),
                    receipt.release.clone(),
                    receipt.release.channel == ReleaseChannel::Stable,
                ),
            ))
        })
        .collect()
}

fn sample_component(
    component_id: &str,
    game_id: &str,
    technology: LibraryTechnology,
    swappability: Swappability,
    version: Option<&str>,
    sha256: &str,
    path: &str,
) -> LibraryComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    LibraryComponent::new(
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
    technology: LibraryTechnology,
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

fn dxc_package_artifact() -> LibraryArtifact {
    const VERSION: &str = "1.8.2505.28";

    let compiler = ComponentFile::new(
        PathRef::new(format!("C:/Library/{COMPILER_FILE_NAME}")).expect("compiler path"),
    )
    .with_sha256(Sha256Hash::new("b".repeat(64)).expect("compiler hash"))
    .with_version(Version::parse(VERSION).expect("compiler version"));
    let validator = ComponentFile::new(
        PathRef::new(format!("C:/Library/{VALIDATOR_FILE_NAME}")).expect("validator path"),
    )
    .with_sha256(Sha256Hash::new("c".repeat(64)).expect("validator hash"))
    .with_version(Version::parse(VERSION).expect("validator version"));

    LibraryArtifact::new(
        ArtifactId::for_bundle(
            [&compiler, &validator]
                .into_iter()
                .filter_map(ComponentFile::sha256),
        ),
        LibraryTechnology::MicrosoftDxc,
        COMPILER_FILE_NAME,
        vec![compiler, validator],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("DXC artifact")
    .with_metadata(
        ArtifactMetadata::default()
            .with_release(Version::parse(VERSION).expect("release version"), None)
            .expect("release metadata")
            .with_runtime_target(RuntimeTarget::new(Architecture::X64))
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.DXC",
                    VERSION,
                )
                .expect("upstream package"),
            ),
    )
}

/// Multi-file Streamline component with optional per-file PE versions.
fn streamline_component(files: &[(&str, Option<&str>)]) -> LibraryComponent {
    let mut component = LibraryComponent::new(
        ComponentId::new("component:game-a:streamline").expect("component id"),
        GameId::new("game:a").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
    );
    for (index, (name, version)) in files.iter().enumerate() {
        let sha = char::from(b'a' + index as u8).to_string().repeat(64);
        let mut file = ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(sha).expect("sha"));
        if let Some(version) = version {
            file = file.with_version(Version::parse(*version).expect("version"));
        }
        component = component.with_file(file);
    }
    component
}

/// Multi-file Streamline package (name-min primary = first member after sort).
fn streamline_package_artifact(
    artifact_id: &str,
    version: &str,
    member_count: usize,
) -> LibraryArtifact {
    let names = ["sl.common.dll", "sl.interposer.dll", "sl.dlss.dll"];
    let files: Vec<_> = (0..member_count)
        .map(|index| {
            let name = names[index % names.len()];
            let sha = char::from(b'c' + index as u8).to_string().repeat(64);
            ComponentFile::new(PathRef::new(format!("manifest://{name}")).expect("path"))
                .with_sha256(Sha256Hash::new(sha).expect("sha"))
                .with_version(Version::parse(version).expect("version"))
        })
        .collect();
    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id"),
        LibraryTechnology::NvidiaStreamline,
        "sl.common.dll",
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("streamline package")
}

fn streamline_artifact_with_members(
    artifact_id: &str,
    version: &str,
    members: &[(&str, char)],
) -> LibraryArtifact {
    let files = members
        .iter()
        .map(|(name, hash)| {
            ComponentFile::new(PathRef::new(format!("manifest://{name}")).expect("path"))
                .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("member hash"))
                .with_version(Version::parse(version).expect("version"))
        })
        .collect();
    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id"),
        LibraryTechnology::NvidiaStreamline,
        members[0].0,
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("streamline artifact")
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
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![upscaler, loader],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("split package artifact should be valid")
}

#[test]
fn split_fsr_component_is_not_offered_a_unified_single_file_downgrade() {
    let component = sample_component(
        "component:game-a:fsr",
        "game:a",
        LibraryTechnology::AmdFsr,
        Swappability::BundleOnly,
        Some("4.0.3"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
    );
    // The unified FSR 3.x backend is a single `amd_fidelityfx_dx12.dll`.
    let unified = sample_artifact(
        "artifact:fsr-3.1",
        LibraryTechnology::AmdFsr,
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
        LibraryTechnology::AmdFsr,
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
        LibraryTechnology::AmdFsr,
        Swappability::Swappable,
        Some("3.1.0"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_dx12.dll",
    );
    let unified = sample_artifact(
        "artifact:fsr-3.1.1",
        LibraryTechnology::AmdFsr,
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
        groups[0]
            .installed_release()
            .known_version()
            .map(|version| version.as_str()),
        Some("4.0.3")
    );
}

#[test]
fn mixed_fsr_component_reports_the_entry_points_version() {
    // A real unified FSR 3.1 entry point next to developer-left split files:
    // the builds do not match (no release cohesion), so the version the game
    // actually runs must win — regardless of the stored file order.
    let mut component = LibraryComponent::new(
        ComponentId::new("component:game-a:fsr").expect("component id"),
        GameId::new("game:a").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::AmdFsr,
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
        groups[0]
            .installed_release()
            .known_version()
            .map(|version| version.as_str()),
        Some("1.0.1.41314"),
        "the leftover upscaler must not hijack the current version"
    );
}

#[test]
fn native_fsr_upscaler_component_only_matches_upscaler_singles() {
    let component = sample_component(
        "component:game-a:fsr-upscaler",
        "game:a",
        LibraryTechnology::AmdFsrUpscaler,
        Swappability::Swappable,
        Some("4.0.3"),
        &"f".repeat(64),
        "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
    );
    let upscaler = sample_artifact(
        "artifact:fsr-upscaler-4.1",
        LibraryTechnology::AmdFsrUpscaler,
        Some("4.1.0"),
        &"e".repeat(64),
        "C:/Lib/amd_fidelityfx_upscaler_dx12.dll",
        None,
    );
    let framegen = sample_artifact(
        "artifact:fsr-framegen-4.1",
        LibraryTechnology::AmdFsrFrameGeneration,
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
fn fsr_component(file_names: &[&str]) -> LibraryComponent {
    let mut component = LibraryComponent::new(
        ComponentId::new("component:game-a:fsr").expect("component id"),
        GameId::new("game:a").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::AmdFsr,
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
        LibraryTechnology::AmdFsr,
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
fn unified_fsr_cleanup_remains_visible_when_the_entry_point_hash_already_matches() {
    let upgraded = fsr_component(&[
        "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll",
    ]);
    let cleanup = sample_artifact(
        "artifact:fsr-cleanup",
        LibraryTechnology::AmdFsr,
        Some("3.1.4"),
        &"b".repeat(64),
        "C:/Lib/amd_fidelityfx_dx12.dll",
        None,
    );

    let groups = find_test_candidates(&[upgraded], &[cleanup]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].candidates().len(),
        1,
        "stale split members make an otherwise byte-identical unified transition actionable"
    );
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
        LibraryTechnology::AmdFsr,
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
