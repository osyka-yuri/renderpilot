use std::collections::BTreeSet;

use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, ComponentId,
    ComponentKind, GameId, LibraryArtifact, LibraryComponent, LibraryTechnology, PathRef,
    PeCompatibilityProfile, PeExportSet, PeImportProfile, PeImportSet, RuntimeTarget, Sha256Hash,
    Swappability,
    xiph::{self, XiphMember},
};

use crate::dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME};

use super::*;

fn file(path: &str, hash: char) -> ComponentFile {
    ComponentFile::new(PathRef::new(path).expect("path")).with_sha256(
        Sha256Hash::new(std::iter::repeat_n(hash, 64).collect::<String>()).expect("hash"),
    )
}

fn streamline_component(names: &[&str]) -> LibraryComponent {
    names.iter().fold(
        LibraryComponent::new(
            ComponentId::new("component:streamline-transition").expect("component"),
            GameId::new("game:streamline-transition").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
        ),
        |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
    )
}

fn streamline_artifact(names: &[&str]) -> LibraryArtifact {
    let files = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            file(
                &format!("C:/Library/{name}"),
                char::from(b'a' + u8::try_from(index).unwrap_or(0)),
            )
        })
        .collect();
    LibraryArtifact::new(
        ArtifactId::new("artifact:streamline-transition").expect("artifact"),
        LibraryTechnology::NvidiaStreamline,
        names[0],
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
}

fn dxc_component(names: &[&str]) -> LibraryComponent {
    names.iter().fold(
        LibraryComponent::new(
            ComponentId::new("component:dxc-transition").expect("component"),
            GameId::new("game:dxc-transition").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::MicrosoftDxc,
            if names.len() > 1 {
                Swappability::BundleOnly
            } else {
                Swappability::Swappable
            },
        ),
        |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
    )
}

fn dxc_package() -> LibraryArtifact {
    LibraryArtifact::new(
        ArtifactId::new("artifact:dxc-transition").expect("artifact"),
        LibraryTechnology::MicrosoftDxc,
        COMPILER_FILE_NAME,
        vec![
            file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a'),
            file(&format!("C:/Library/{VALIDATOR_FILE_NAME}"), 'b'),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
}

fn xiph_component(names: &[&str]) -> LibraryComponent {
    names.iter().fold(
        LibraryComponent::new(
            ComponentId::new("component:xiph-transition").expect("component"),
            GameId::new("game:xiph-transition").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
    )
}

fn xiph_artifact(names: &[&str]) -> LibraryArtifact {
    let files = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            file(
                &format!("C:/Library/{name}"),
                char::from(b'a' + u8::try_from(index).unwrap_or(0)),
            )
        })
        .collect();
    LibraryArtifact::new(
        ArtifactId::new("artifact:xiph-transition").expect("artifact"),
        LibraryTechnology::XiphVorbis,
        names[0],
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
}

fn dide_member(name: &str, imports: &[&str], hash: char, root: &str) -> ComponentFile {
    let member = xiph::parse_runtime_file_name(name)
        .expect("runtime name")
        .expect("Xiph member")
        .member();
    let export = match member {
        XiphMember::VorbisFile => "ov_open",
        XiphMember::VorbisEnc => "vorbis_encode_init",
        XiphMember::Vorbis => "vorbis_info_init",
        XiphMember::Ogg => "ogg_sync_init",
    };
    ComponentFile::new(PathRef::new(format!("{root}/{name}")).expect("path"))
        .with_sha256(
            Sha256Hash::new(std::iter::repeat_n(hash, 64).collect::<String>()).expect("hash"),
        )
        .with_pe_compatibility(
            PeCompatibilityProfile::new(
                Architecture::X64,
                PeExportSet::from_observed_names(vec![export.to_owned()]).expect("exports"),
            )
            .with_imports(PeImportProfile {
                regular: PeImportSet::from_observed_names(
                    imports.iter().map(|name| (*name).to_owned()).collect(),
                )
                .expect("imports"),
                delay: PeImportSet::default(),
            }),
        )
}

fn dide_component() -> LibraryComponent {
    dide_component_with_roots(["C:/Game", "C:/Game", "C:/Game"])
}

fn dide_component_with_roots(roots: [&str; 3]) -> LibraryComponent {
    [
        dide_member(
            "vorbisfile_vs2010_x64_rwdi.dll",
            &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
            '1',
            roots[0],
        ),
        dide_member(
            "vorbis_vs2010_x64_rwdi.dll",
            &["ogg_vs2010_x64_rwdi.dll"],
            '2',
            roots[1],
        ),
        dide_member("ogg_vs2010_x64_rwdi.dll", &[], '3', roots[2]),
    ]
    .into_iter()
    .fold(
        LibraryComponent::new(
            ComponentId::new("component:dide").expect("component"),
            GameId::new("game:dide").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    )
}

fn dide_artifact(files: Vec<ComponentFile>) -> LibraryArtifact {
    let primary = files[0].path().file_name().expect("primary").to_owned();
    LibraryArtifact::new(
        ArtifactId::new("artifact:xiph:dide").expect("artifact"),
        LibraryTechnology::XiphVorbis,
        primary,
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
    .with_metadata(
        ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
    )
}

fn canonical_dide_artifact() -> LibraryArtifact {
    dide_artifact(vec![
        dide_member(
            "vorbisfile.dll",
            &["vorbis.dll", "ogg.dll"],
            'a',
            "C:/Library",
        ),
        dide_member("vorbis.dll", &["ogg.dll"], 'b', "C:/Library"),
        dide_member("ogg.dll", &[], 'c', "C:/Library"),
    ])
}

fn unreal_component() -> LibraryComponent {
    [
        dide_member(
            "libvorbisfile_64.dll",
            &["libvorbis_64.dll", "libogg_64.dll"],
            '1',
            "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64",
        ),
        dide_member(
            "libvorbis_64.dll",
            &["libogg_64.dll"],
            '2',
            "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64",
        ),
        dide_member(
            "libogg_64.dll",
            &[],
            '3',
            "C:/Game/Engine/Binaries/ThirdParty/Ogg/Win64",
        ),
    ]
    .into_iter()
    .fold(
        LibraryComponent::new(
            ComponentId::new("component:unreal-xiph").expect("component"),
            GameId::new("game:unreal-xiph").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    )
}

fn unreal_shared_artifact() -> LibraryArtifact {
    dide_artifact(vec![
        dide_member(
            "libvorbisfile_64.dll",
            &["libvorbis_64.dll", "libogg_64.dll"],
            'a',
            "C:/Library",
        ),
        dide_member(
            "libvorbisenc_64.dll",
            &["libvorbis_64.dll"],
            'd',
            "C:/Library",
        ),
        dide_member("libvorbis_64.dll", &["libogg_64.dll"], 'b', "C:/Library"),
        dide_member("libogg_64.dll", &[], 'c', "C:/Library"),
    ])
}

#[test]
fn xiph_transition_writes_only_members_present_in_the_game() {
    let component = xiph_component(&["libvorbisfile.dll", "libvorbis.dll", "libogg.dll"]);
    let package = xiph_artifact(&[
        "libvorbis.dll",
        "libvorbisfile.dll",
        "libvorbisenc.dll",
        "libogg.dll",
    ]);

    let members = resolve_transition_members(&component, &package).expect("transition");
    let targets = members
        .iter()
        .map(|member| resolve_transition_install_target(&component, member))
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        ["libvorbis.dll", "libvorbisfile.dll", "libogg.dll"],
        "the optional encoder must not expand a three-member game integration"
    );
}

#[test]
fn split_xiph_transition_writes_each_member_into_its_own_directory() {
    let component = [
        dide_member(
            "vorbisfile.dll",
            &["vorbis.dll", "ogg.dll"],
            '1',
            "C:/Game/Plugin",
        ),
        dide_member("vorbis.dll", &["ogg.dll"], '2', "C:/Game/Codec"),
        dide_member("ogg.dll", &[], '3', "C:/Game/Container"),
    ]
    .into_iter()
    .fold(
        LibraryComponent::new(
            ComponentId::new("component:split-xiph-transition").expect("component"),
            GameId::new("game:split-xiph-transition").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    );
    let resolved = resolve_transition(
        &component,
        &canonical_dide_artifact(),
        component.files(),
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("split Xiph transition");

    assert_eq!(resolved.target_directory(), "C:/Game/Plugin");
    assert_eq!(
        resolved.primary_target().as_str(),
        "C:/Game/Plugin/vorbisfile.dll"
    );
    assert_eq!(
        resolved
            .paths()
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::Write(write) => Some(write.target().as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "C:/Game/Codec/vorbis.dll",
            "C:/Game/Container/ogg.dll",
            "C:/Game/Plugin/vorbisfile.dll",
        ])
    );
}

#[test]
fn streamline_transition_uses_only_installed_targets_and_requires_coverage() {
    let component = streamline_component(&["sl.common.dll", "sl.interposer.dll"]);
    let complete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);
    let members = resolve_transition_members(&component, &complete).expect("transition");
    assert_eq!(
        members
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<Vec<_>>(),
        ["sl.common.dll", "sl.interposer.dll"]
    );

    let incomplete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll"]);
    assert!(
        resolve_transition_members(&component, &incomplete)
            .expect_err("coverage")
            .message()
            .contains("sl.interposer.dll")
    );
}

#[test]
fn streamline_transition_rejects_an_empty_intersection() {
    let error = resolve_transition_members(
        &streamline_component(&["sl.common.dll"]),
        &streamline_artifact(&["sl.dlss.dll"]),
    )
    .expect_err("empty transition");
    assert!(error.message().contains("no installable files"));
}

#[test]
fn dxc_transition_keeps_a_standalone_compiler_standalone() {
    let component = dxc_component(&[COMPILER_FILE_NAME]);
    let package = dxc_package();

    let members = resolve_transition_members(&component, &package).expect("transition");
    assert_eq!(members.len(), 1);
    assert_eq!(
        members[0].path().file_name(),
        Some(COMPILER_FILE_NAME),
        "a standalone game integration must remain standalone"
    );
}

#[test]
fn dxc_transition_keeps_an_installed_pair_complete() {
    let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
    let package = dxc_package();

    let members = resolve_transition_members(&component, &package).expect("transition");
    assert_eq!(
        members
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<Vec<_>>(),
        [COMPILER_FILE_NAME, VALIDATOR_FILE_NAME],
        "an installed pair must remain a two-file integration"
    );
}

#[test]
fn dxc_transition_rejects_a_validator_without_a_compiler() {
    let component = dxc_component(&[VALIDATOR_FILE_NAME]);

    let error = resolve_transition_members(&component, &dxc_package())
        .expect_err("dxil.dll alone is not a valid DXC integration");
    assert!(error.message().contains(COMPILER_FILE_NAME));
}

#[test]
fn dxc_transition_requires_the_package_to_cover_an_installed_pair() {
    let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
    let incomplete = LibraryArtifact::new(
        ArtifactId::new("artifact:dxc-incomplete").expect("artifact"),
        LibraryTechnology::MicrosoftDxc,
        COMPILER_FILE_NAME,
        vec![file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a')],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");

    let error = resolve_transition_members(&component, &incomplete)
        .expect_err("the installed validator must be covered");
    assert!(error.message().contains(VALIDATOR_FILE_NAME));
}

#[test]
fn transition_rejects_a_technology_mismatch() {
    let component = streamline_component(&["sl.common.dll"]);
    let mismatched = LibraryArtifact::new(
        ArtifactId::new("artifact:mismatched-transition").expect("artifact"),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![file("C:/Library/nvngx_dlss.dll", 'a')],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");
    assert!(
        resolve_transition_members(&component, &mismatched)
            .expect_err("technology mismatch")
            .message()
            .contains("technologies do not match")
    );
}

#[test]
fn transition_rejects_duplicate_resolved_targets() {
    let component = streamline_component(&["sl.common.dll"]);
    let duplicate = streamline_artifact(&["sl.common.dll", "SL.COMMON.DLL"]);
    assert!(
        resolve_transition_members(&component, &duplicate)
            .expect_err("duplicate target")
            .message()
            .contains("duplicate install target")
    );
}

#[test]
fn fsr_reswap_reserves_missing_split_baseline_member_from_immutable_baseline() {
    // A previous unified FSR deployment owns only the entry point live;
    // the original split upscaler survives only in its immutable sidecar.
    // Re-applying a unified target must keep that split original reserved
    // instead of silently dropping it from the transition partition.
    let baseline = vec![
        file("C:/Game/amd_fidelityfx_dx12.dll", 'a'),
        file("C:/Game/amd_fidelityfx_upscaler_dx12.dll", 'b'),
    ];
    let component = LibraryComponent::new(
        ComponentId::new("component:fsr-reswap").expect("component"),
        GameId::new("game:fsr-reswap").expect("game"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::AmdFsr,
        Swappability::BundleOnly,
    )
    .with_file(file("C:/Game/amd_fidelityfx_dx12.dll", 'c'));
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr-unified").expect("artifact"),
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![file("C:/Library/amd_fidelityfx_dx12.dll", 'd')],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");

    let transition = resolve_transition(
        &component,
        &artifact,
        &baseline,
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("FSR reswap transition");

    assert!(transition.paths().iter().any(|path| {
        matches!(path, ResolvedPathDisposition::ArchiveAndRemove(archive)
            if archive.target().file_name() == Some("amd_fidelityfx_upscaler_dx12.dll")
                && archive.baseline() == &baseline[1]
                && archive.current().is_none()
                && archive.mode() == ArchiveMode::RequireOwnedArchive)
    }));
    assert_eq!(
        transition
            .reserved()
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["amd_fidelityfx_upscaler_dx12.dll"])
    );
}

#[test]
fn non_xiph_transition_preserves_xiph_named_baseline_without_member_metadata() {
    let component = streamline_component(&["sl.common.dll"]);
    let artifact = streamline_artifact(&["sl.common.dll"]);
    let baseline = vec![
        file("C:/Game/sl.common.dll", 'a'),
        file("C:/Game/libogg.dll", 'b'),
    ];

    let transition = resolve_transition(
        &component,
        &artifact,
        &baseline,
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("non-Xiph transition preserves its baseline partition");

    let xiph_named_baseline = transition
        .paths()
        .iter()
        .find(|path| path.target() == baseline[1].path())
        .expect("Xiph-named baseline path remains in the partition");
    assert!(matches!(
        xiph_named_baseline,
        ResolvedPathDisposition::ArchiveAndRemove(archive)
            if archive.target() == baseline[1].path()
                && archive.baseline() == &baseline[1]
                && archive.current().is_none()
                && archive.mode() == ArchiveMode::RequireOwnedArchive
                && archive.member().is_none()
    ));
    assert_eq!(transition.reserved(), vec![baseline[1].clone()]);
    assert!(transition.xiph().is_none());
}

#[test]
fn dide_transition_preserves_only_proven_wrapper_and_archives_vendor_core() {
    let component = dide_component();
    let baseline = component.files().to_vec();
    let resolved = resolve_transition(
        &component,
        &canonical_dide_artifact(),
        &baseline,
        &ExternalAliasRequirements::Proven(BTreeSet::from([
            "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
        ])),
    )
    .expect("DIDE transition resolves");

    assert_eq!(resolved.paths().len(), 5);
    assert_eq!(
        resolved
            .paths()
            .iter()
            .filter(|path| matches!(path, ResolvedPathDisposition::Write(_)))
            .count(),
        3
    );
    assert_eq!(
        resolved
            .paths()
            .iter()
            .filter(|path| matches!(path, ResolvedPathDisposition::ArchiveAndRemove(_)))
            .count(),
        2
    );
    assert!(resolved.paths().iter().any(|path| {
        matches!(path, ResolvedPathDisposition::Write(write)
            if write.target().file_name() == Some("vorbisfile_vs2010_x64_rwdi.dll"))
    }));
    assert!(resolved.paths().iter().any(|path| {
        matches!(path, ResolvedPathDisposition::Write(write)
            if write.target().file_name() == Some("vorbis.dll"))
    }));
    assert!(resolved.paths().iter().any(|path| {
        matches!(path, ResolvedPathDisposition::Write(write)
            if write.target().file_name() == Some("ogg.dll"))
    }));
    assert_eq!(
        resolved
            .reserved()
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["ogg_vs2010_x64_rwdi.dll", "vorbis_vs2010_x64_rwdi.dll",])
    );
    assert_eq!(
        resolved
            .expected_active()
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["ogg.dll", "vorbis.dll", "vorbisfile_vs2010_x64_rwdi.dll"])
    );
}

#[test]
fn vendor_xiph_requires_completed_exact_external_alias_proof() {
    let component = dide_component_with_roots([
        "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64",
        "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64",
        "C:/Game/Engine/Binaries/ThirdParty/Ogg/Win64",
    ]);
    let baseline = component.files().to_vec();
    assert!(
        resolve_transition(
            &component,
            &canonical_dide_artifact(),
            &baseline,
            &ExternalAliasRequirements::NotRequired,
        )
        .is_err(),
        "vendor transitions must not treat NotRequired as an empty completed proof"
    );
    assert!(
        resolve_transition(
            &component,
            &canonical_dide_artifact(),
            &baseline,
            &ExternalAliasRequirements::Unproven,
        )
        .is_err()
    );
    assert!(
        resolve_transition(
            &component,
            &canonical_dide_artifact(),
            &baseline,
            &ExternalAliasRequirements::Proven(BTreeSet::from(["not-xiph.dll".to_owned()])),
        )
        .is_err()
    );

    let resolved = resolve_transition(
        &component,
        &canonical_dide_artifact(),
        &baseline,
        &ExternalAliasRequirements::Proven(BTreeSet::new()),
    )
    .expect("a completed zero-binding proof permits the manual transition");
    assert_eq!(
        resolved
            .paths()
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::Write(write) => Some(write.target().as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "C:/Game/Engine/Binaries/ThirdParty/Ogg/Win64/ogg.dll",
            "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64/vorbis.dll",
            "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64/vorbisfile.dll",
        ])
    );
    assert_eq!(
        resolved
            .paths()
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                    archive.target().file_name()
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "ogg_vs2010_x64_rwdi.dll",
            "vorbis_vs2010_x64_rwdi.dll",
            "vorbisfile_vs2010_x64_rwdi.dll",
        ])
    );
}

#[test]
fn vendor_xiph_rejects_alias_that_strands_a_canonical_candidate_import() {
    let component = dide_component();
    let error = resolve_transition(
        &component,
        &canonical_dide_artifact(),
        component.files(),
        &ExternalAliasRequirements::Proven(BTreeSet::from([
            "vorbisfile_vs2010_x64_rwdi.dll".to_owned(),
            "vorbis_vs2010_x64_rwdi.dll".to_owned(),
        ])),
    )
    .expect_err("canonical Vorbis import must remain live");
    assert!(
        error
            .message()
            .contains("conflicts with canonical candidate dependency")
    );
}

#[test]
fn unreal_xiph_transition_keeps_exact_runtime_names_with_empty_or_nonempty_proof() {
    let component = unreal_component();
    let artifact = unreal_shared_artifact();
    let expected_targets = BTreeSet::from([
        "C:/Game/Engine/Binaries/ThirdParty/Ogg/Win64/libogg_64.dll",
        "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64/libvorbis_64.dll",
        "C:/Game/Engine/Binaries/ThirdParty/Vorbis/Win64/libvorbisfile_64.dll",
    ]);
    let proofs = [
        ExternalAliasRequirements::Proven(BTreeSet::new()),
        ExternalAliasRequirements::Proven(BTreeSet::from(["libvorbisfile_64.dll".to_owned()])),
        ExternalAliasRequirements::Proven(BTreeSet::from([
            "libvorbisfile_64.dll".to_owned(),
            "libvorbis_64.dll".to_owned(),
        ])),
        ExternalAliasRequirements::Proven(BTreeSet::from([
            "libogg_64.dll".to_owned(),
            "libvorbis_64.dll".to_owned(),
            "libvorbisfile_64.dll".to_owned(),
        ])),
    ];

    for proof in proofs {
        let transition = resolve_transition(&component, &artifact, component.files(), &proof)
            .expect("reviewed Unreal transition resolves");
        let ExternalAliasRequirements::Proven(expected_aliases) = &proof else {
            panic!("test cases use completed alias proofs");
        };
        assert_eq!(
            transition
                .xiph()
                .expect("Xiph transition details")
                .external_aliases(),
            expected_aliases
        );
        let write_targets = transition
            .paths()
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::Write(write) => Some(write.target().as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(write_targets, expected_targets);
        assert_eq!(
            transition
                .paths()
                .iter()
                .filter(|path| matches!(path, ResolvedPathDisposition::Write(_)))
                .count(),
            3
        );
        assert!(
            transition
                .paths()
                .iter()
                .all(|path| { !matches!(path, ResolvedPathDisposition::ArchiveAndRemove(_)) })
        );
    }
}

#[test]
fn legacy_xiph_install_target_preserves_only_canonical_runtime_aliases() {
    let vendor = unreal_component();
    let plain = canonical_dide_artifact();
    let exact = unreal_shared_artifact();

    for file in plain.files() {
        assert_eq!(
            resolve_transition_install_target(&vendor, file),
            file.path().file_name().expect("plain candidate name")
        );
    }
    for file in exact.files() {
        assert_eq!(
            resolve_transition_install_target(&vendor, file),
            file.path()
                .file_name()
                .expect("exact Unreal candidate name")
        );
    }
    assert!(resolve_transition_members(&vendor, &exact).is_err());

    let canonical = xiph_component(&["libvorbisfile.dll", "libvorbis.dll", "libogg.dll"]);
    for file in plain.files() {
        let member = xiph::classify_canonical_file_name(
            file.path().file_name().expect("plain candidate name"),
        )
        .expect("canonical candidate member")
        .0;
        assert_eq!(
            resolve_transition_install_target(&canonical, file),
            xiph::file_name(member, xiph::XiphNameStyle::Lib)
        );
    }
}

#[test]
fn plain_xiph_candidate_still_rejects_a_proven_core_alias_it_would_strand() {
    let component = unreal_component();
    let error = resolve_transition(
        &component,
        &canonical_dide_artifact(),
        component.files(),
        &ExternalAliasRequirements::Proven(BTreeSet::from([
            "libvorbisfile_64.dll".to_owned(),
            "libvorbis_64.dll".to_owned(),
        ])),
    )
    .expect_err("plain imports cannot resolve after the core keeps its Unreal alias");
    assert!(
        error
            .message()
            .contains("conflicts with canonical candidate dependency")
    );
}

#[test]
fn vendor_xiph_rejects_nonplain_catalog_candidate() {
    let component = dide_component();
    let artifact = dide_artifact(vec![
        dide_member(
            "libvorbisfile.dll",
            &["libvorbis.dll", "libogg.dll"],
            'a',
            "C:/Library",
        ),
        dide_member("libvorbis.dll", &["libogg.dll"], 'b', "C:/Library"),
        dide_member("libogg.dll", &[], 'c', "C:/Library"),
    ]);
    let error = resolve_transition(
        &component,
        &artifact,
        component.files(),
        &ExternalAliasRequirements::Proven(BTreeSet::from([
            "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
        ])),
    )
    .expect_err("vendor target accepts only plain canonical candidates");
    assert!(error.message().contains("plain canonical candidate"));
}
