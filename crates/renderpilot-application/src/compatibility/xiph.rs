//! Xiph Vorbis/Ogg graph compatibility.

use std::collections::BTreeMap;

use renderpilot_domain::{
    Architecture, ComponentFile, LibraryArtifact, LibraryComponent, PeCompatibilityProfile,
    xiph::{self, XiphLayout, XiphMember, XiphNameStyle, XiphRuntimeFileName},
};

use super::{SwapCompatibilityError, runtime_file_name};
use crate::ExternalAliasRequirements;

pub(super) fn validate_artifact(artifact: &LibraryArtifact) -> Result<(), SwapCompatibilityError> {
    let target = artifact
        .metadata()
        .runtime_target()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    if target.compatibility().is_some() {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }

    // Catalog names include the exact Unreal `_64` basenames, but no
    // arbitrary vendor suffixes.
    let classified = classify_canonical_files(
        artifact.files(),
        SwapCompatibilityError::InvalidArtifactMetadata,
    )?;
    validate_unreal_style_family(&classified, SwapCompatibilityError::InvalidArtifactMetadata)?;
    validate_layout(classified.values())?;
    for (member, classified_file) in &classified {
        if classified_file.profile.architecture() != target.architecture()
            || classified_file.file.sha256().is_none()
        {
            return Err(SwapCompatibilityError::InvalidArtifactMetadata);
        }
        if !classified_file
            .profile
            .named_exports()
            .names()
            .iter()
            .any(|export| xiph::is_public_api_export(*member, export))
        {
            return Err(SwapCompatibilityError::InvalidArtifactMetadata);
        }
        validate_system_imports(classified_file.profile)?;
    }
    Ok(())
}

pub(super) fn ensure_transition_compatible(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> Result<(), SwapCompatibilityError> {
    ensure_transition_compatible_with_external_aliases(
        component,
        artifact,
        &ExternalAliasRequirements::NotRequired,
    )
}

/// Validates a Xiph deployment using orchestration's external-import proof.
/// Vendor aliases require a complete game-root scan of regular/delay imports.
/// An empty proven set records zero bindings; dynamic loading is not observed.
pub(crate) fn ensure_transition_compatible_with_external_aliases(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    aliases: &ExternalAliasRequirements,
) -> Result<(), SwapCompatibilityError> {
    ensure_transition_compatible_inner(component, artifact, Some(aliases))
}

/// Checks the package ABI for candidate presentation. Vendor transitions must
/// use [`ensure_transition_compatible_with_external_aliases`] before planning.
pub(crate) fn ensure_candidate_compatible_without_alias_proof(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> Result<(), SwapCompatibilityError> {
    validate_artifact(artifact)?;
    ensure_transition_compatible_inner(component, artifact, None)
}

fn ensure_transition_compatible_inner(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    aliases: Option<&ExternalAliasRequirements>,
) -> Result<(), SwapCompatibilityError> {
    let installed = classify_runtime_files(
        component.files(),
        SwapCompatibilityError::MissingInstalledPeMetadata,
    )?;
    let installed_layout = validate_layout(installed.values())?;
    let candidates = classify_canonical_files(
        artifact.files(),
        SwapCompatibilityError::InvalidArtifactMetadata,
    )?;
    validate_unreal_style_family(&candidates, SwapCompatibilityError::NamingFamilyMismatch)?;

    let vendor_layout = installed.values().any(|file| file.runtime.is_vendor());

    if vendor_layout {
        if let Some(aliases) = aliases {
            validate_vendor_alias_requirements(&installed, aliases)?;
        }
        if candidates.values().any(|candidate| {
            !matches!(
                candidate.name_style,
                XiphNameStyle::Plain | XiphNameStyle::Unreal
            )
        }) {
            return Err(SwapCompatibilityError::VendorCandidateMustUsePlainNames);
        }
    } else if aliases
        .is_some_and(|aliases| !matches!(aliases, ExternalAliasRequirements::NotRequired))
    {
        return Err(SwapCompatibilityError::UnexpectedExternalAliasRequirement);
    }

    let mut projected = BTreeMap::new();
    for (member, installed_file) in &installed {
        let candidate_file = candidates
            .get(member)
            .ok_or(SwapCompatibilityError::IncompleteXiphPackage)?;
        if !installed_file
            .runtime
            .accepts_candidate_file_name(candidate_file.runtime.normalized_name())
        {
            return Err(SwapCompatibilityError::NamingFamilyMismatch);
        }
        if installed_file.profile.architecture() != candidate_file.profile.architecture() {
            return Err(SwapCompatibilityError::InstalledArchitectureMismatch {
                artifact: candidate_file.profile.architecture(),
                installed: installed_file.profile.architecture(),
            });
        }
        if !preserves_public_api(*member, installed_file.profile, candidate_file.profile) {
            return Err(SwapCompatibilityError::ExportSurfaceMismatch);
        }
        projected.insert(*member, candidate_file);
    }

    let candidate_layout = validate_layout(projected.values().copied())?;
    if installed_layout.topology() != candidate_layout.topology() {
        return Err(SwapCompatibilityError::UnexpectedDependency);
    }
    Ok(())
}

fn preserves_public_api(
    member: XiphMember,
    installed: &PeCompatibilityProfile,
    candidate: &PeCompatibilityProfile,
) -> bool {
    let mut has_public_api = false;
    for required in installed
        .named_exports()
        .names()
        .iter()
        .filter(|export| xiph::is_public_api_export(member, export))
    {
        has_public_api = true;
        if candidate
            .named_exports()
            .names()
            .binary_search(required)
            .is_err()
        {
            return false;
        }
    }
    has_public_api
}

#[derive(Clone)]
struct ClassifiedFile<'a> {
    file: &'a ComponentFile,
    profile: &'a PeCompatibilityProfile,
    runtime: XiphRuntimeFileName,
    name_style: XiphNameStyle,
}

fn classify_runtime_files<'a>(
    files: &'a [ComponentFile],
    missing_profile: SwapCompatibilityError,
) -> Result<BTreeMap<XiphMember, ClassifiedFile<'a>>, SwapCompatibilityError> {
    let mut classified = BTreeMap::new();
    for file in files {
        let name =
            runtime_file_name(file).ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
        let runtime = xiph::parse_runtime_file_name(name)
            .ok()
            .flatten()
            .ok_or(SwapCompatibilityError::NamingFamilyMismatch)?;
        let profile = file.pe_compatibility().ok_or(missing_profile)?;
        if profile.imports().is_none()
            || classified
                .insert(
                    runtime.member(),
                    ClassifiedFile {
                        file,
                        profile,
                        name_style: runtime.base_style(),
                        runtime,
                    },
                )
                .is_some()
        {
            return Err(SwapCompatibilityError::InvalidImportProfile);
        }
    }
    if classified.is_empty() {
        return Err(SwapCompatibilityError::IncompleteXiphPackage);
    }
    Ok(classified)
}

fn classify_canonical_files<'a>(
    files: &'a [ComponentFile],
    missing_profile: SwapCompatibilityError,
) -> Result<BTreeMap<XiphMember, ClassifiedFile<'a>>, SwapCompatibilityError> {
    let mut classified = BTreeMap::new();
    for file in files {
        let name =
            runtime_file_name(file).ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
        let (member, name_style) = xiph::classify_canonical_file_name(name)
            .ok_or(SwapCompatibilityError::NamingFamilyMismatch)?;
        let runtime = xiph::parse_runtime_file_name(name)
            .ok()
            .flatten()
            .ok_or(SwapCompatibilityError::NamingFamilyMismatch)?;
        let profile = file.pe_compatibility().ok_or(missing_profile)?;
        if name_style == XiphNameStyle::Unreal && profile.architecture() != Architecture::X64 {
            return Err(SwapCompatibilityError::InvalidArtifactMetadata);
        }
        if profile.imports().is_none()
            || classified
                .insert(
                    member,
                    ClassifiedFile {
                        file,
                        profile,
                        name_style,
                        runtime,
                    },
                )
                .is_some()
        {
            return Err(SwapCompatibilityError::InvalidImportProfile);
        }
    }
    if classified.is_empty() {
        return Err(SwapCompatibilityError::IncompleteXiphPackage);
    }
    Ok(classified)
}

fn validate_unreal_style_family(
    files: &BTreeMap<XiphMember, ClassifiedFile<'_>>,
    error: SwapCompatibilityError,
) -> Result<(), SwapCompatibilityError> {
    let mut is_unreal_family = None;
    for file in files.values() {
        let is_unreal = file.name_style == XiphNameStyle::Unreal;
        if is_unreal_family.is_some_and(|expected| expected != is_unreal) {
            return Err(error);
        }
        is_unreal_family = Some(is_unreal);
    }
    Ok(())
}

fn validate_layout<'a>(
    files: impl IntoIterator<Item = &'a ClassifiedFile<'a>>,
) -> Result<XiphLayout, SwapCompatibilityError> {
    xiph::detect_layout_with_file_names(
        files
            .into_iter()
            .map(|classified| (classified.runtime.normalized_name(), classified.file)),
    )
    .ok_or(SwapCompatibilityError::UnexpectedDependency)
}

fn validate_vendor_alias_requirements(
    installed: &BTreeMap<XiphMember, ClassifiedFile<'_>>,
    aliases: &ExternalAliasRequirements,
) -> Result<(), SwapCompatibilityError> {
    let ExternalAliasRequirements::Proven(aliases) = aliases else {
        return Err(SwapCompatibilityError::ExternalAliasProofRequired);
    };
    for alias in aliases {
        if !alias.is_ascii() || alias.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(SwapCompatibilityError::InvalidExternalAliasRequirement);
        }
        let matches_installed_vendor = installed
            .values()
            .any(|file| file.runtime.is_vendor() && file.runtime.normalized_name() == alias);
        if !matches_installed_vendor {
            return Err(SwapCompatibilityError::InvalidExternalAliasRequirement);
        }
    }
    Ok(())
}

fn validate_system_imports(profile: &PeCompatibilityProfile) -> Result<(), SwapCompatibilityError> {
    let imports = profile
        .imports()
        .ok_or(SwapCompatibilityError::InvalidImportProfile)?;
    for name in imports.regular.names().iter().chain(imports.delay.names()) {
        if xiph::parse_runtime_file_name(name).ok().flatten().is_none()
            && !is_allowed_xiph_system_import(name)
        {
            return Err(SwapCompatibilityError::UnexpectedDependency);
        }
    }
    Ok(())
}

/// Returns whether a non-Xiph import belongs to the reviewed Windows runtime set.
///
/// Kept in the application compatibility layer because this is deployment
/// policy rather than Xiph domain identity. Catalog validation reuses the same
/// function to prevent producer/runtime policy drift.
#[must_use]
pub fn is_allowed_xiph_system_import(name: &str) -> bool {
    (name.starts_with("api-ms-win-") && !name.starts_with("api-ms-win-crt-"))
        || name.starts_with("ext-ms-win-")
        || matches!(
            name,
            "kernel32.dll"
                | "ntdll.dll"
                | "advapi32.dll"
                | "bcrypt.dll"
                | "ole32.dll"
                | "oleaut32.dll"
                | "shell32.dll"
                | "user32.dll"
                | "ws2_32.dll"
        )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentId, ComponentKind,
        GameId, LibraryTechnology, PathRef, PeExportSet, PeImportProfile, PeImportSet,
        RuntimeTarget, Sha256Hash, Swappability,
    };

    use super::*;

    #[test]
    fn full_shared_package_projects_to_dmc_three_member_graph() {
        let artifact = artifact(
            Architecture::X86,
            &[
                ("libvorbisfile.dll", &["libogg.dll", "libvorbis.dll"]),
                ("libvorbisenc.dll", &["libvorbis.dll"]),
                ("libvorbis.dll", &["libogg.dll"]),
                ("libogg.dll", &[]),
            ],
        );
        let installed = component(&[
            ("libvorbisfile.dll", &["libogg.dll", "libvorbis.dll"]),
            ("libvorbis.dll", &["libogg.dll"]),
            ("libogg.dll", &[]),
        ]);
        assert_eq!(validate_artifact(&artifact), Ok(()));
        assert_eq!(ensure_transition_compatible(&installed, &artifact), Ok(()));
    }

    #[test]
    fn embedded_package_projects_to_fallout_pair() {
        let artifact = artifact(
            Architecture::X86,
            &[
                ("libvorbisfile.dll", &["libvorbis.dll"]),
                ("libvorbisenc.dll", &["libvorbis.dll"]),
                ("libvorbis.dll", &[]),
            ],
        );
        let installed = component(&[
            ("libvorbisfile.dll", &["libvorbis.dll"]),
            ("libvorbis.dll", &[]),
        ]);
        assert_eq!(validate_artifact(&artifact), Ok(()));
        assert_eq!(ensure_transition_compatible(&installed, &artifact), Ok(()));
    }

    #[test]
    fn rejects_alias_or_topology_change_and_dynamic_crt() {
        let wrong_alias = artifact(
            Architecture::X86,
            &[
                ("libvorbisfile.dll", &["libvorbis.dll"]),
                ("libvorbis.dll", &[]),
            ],
        );
        let abi_installed = component(&[
            ("libvorbisfile-3.dll", &["libvorbis-0.dll"]),
            ("libvorbis-0.dll", &[]),
        ]);
        assert_eq!(
            ensure_transition_compatible(&abi_installed, &wrong_alias),
            Err(SwapCompatibilityError::NamingFamilyMismatch)
        );

        let dynamic_crt = artifact(
            Architecture::X86,
            &[
                ("libvorbis.dll", &["libogg.dll", "vcruntime140.dll"]),
                ("libogg.dll", &[]),
            ],
        );
        assert_eq!(
            validate_artifact(&dynamic_crt),
            Err(SwapCompatibilityError::UnexpectedDependency)
        );
    }

    #[test]
    fn coordinated_transition_allows_removed_private_diagnostic_export() {
        let installed = component_from_files(vec![
            member_with_exports(
                "libvorbisfile.dll",
                Architecture::X86,
                &["libvorbis.dll"],
                &["ov_open"],
                16,
            ),
            member_with_exports(
                "libvorbis.dll",
                Architecture::X86,
                &[],
                &["vorbis_info_init", "_analysis_output_always"],
                17,
            ),
        ]);
        let compatible = artifact_from_files(
            Architecture::X86,
            vec![
                member_with_exports(
                    "libvorbisfile.dll",
                    Architecture::X86,
                    &["libvorbis.dll"],
                    &["ov_open"],
                    1,
                ),
                member_with_exports(
                    "libvorbis.dll",
                    Architecture::X86,
                    &[],
                    &["vorbis_info_init", "_analysis_output_always"],
                    2,
                ),
            ],
        );
        let without_private_diagnostic = artifact_from_files(
            Architecture::X86,
            vec![
                member_with_exports(
                    "libvorbisfile.dll",
                    Architecture::X86,
                    &["libvorbis.dll"],
                    &["ov_open"],
                    3,
                ),
                member_with_exports(
                    "libvorbis.dll",
                    Architecture::X86,
                    &[],
                    &["vorbis_info_init"],
                    4,
                ),
            ],
        );

        assert_eq!(
            ensure_transition_compatible(&installed, &compatible),
            Ok(())
        );
        assert_eq!(
            ensure_transition_compatible(&installed, &without_private_diagnostic),
            Ok(())
        );
    }

    #[test]
    fn rejects_public_xiph_export_regressions() {
        let installed = component_from_files(vec![
            member_with_exports(
                "libvorbisfile.dll",
                Architecture::X86,
                &["libvorbis.dll"],
                &["ov_open"],
                16,
            ),
            member_with_exports(
                "libvorbis.dll",
                Architecture::X86,
                &[],
                &["vorbis_info_clear", "vorbis_info_init"],
                17,
            ),
        ]);
        let regression = artifact_from_files(
            Architecture::X86,
            vec![
                member_with_exports(
                    "libvorbisfile.dll",
                    Architecture::X86,
                    &["libvorbis.dll"],
                    &["ov_open"],
                    1,
                ),
                member_with_exports(
                    "libvorbis.dll",
                    Architecture::X86,
                    &[],
                    &["vorbis_info_init"],
                    2,
                ),
            ],
        );

        assert_eq!(
            ensure_transition_compatible(&installed, &regression),
            Err(SwapCompatibilityError::ExportSurfaceMismatch)
        );
    }

    #[test]
    fn rejects_ogg_bitpacking_api_regressions() {
        let installed = component_from_files(vec![member_with_exports(
            "libogg.dll",
            Architecture::X86,
            &[],
            &["ogg_sync_init", "oggpack_read", "oggpackB_write"],
            16,
        )]);
        let regression = artifact_from_files(
            Architecture::X86,
            vec![member_with_exports(
                "libogg.dll",
                Architecture::X86,
                &[],
                &["ogg_sync_init"],
                1,
            )],
        );

        assert_eq!(
            ensure_transition_compatible(&installed, &regression),
            Err(SwapCompatibilityError::ExportSurfaceMismatch)
        );
    }

    #[test]
    fn rejects_xiph_artifact_member_without_a_public_api_surface() {
        let artifact = artifact_from_files(
            Architecture::X86,
            vec![member_with_exports(
                "libvorbis.dll",
                Architecture::X86,
                &[],
                &["_analysis_output_always"],
                1,
            )],
        );

        assert_eq!(
            validate_artifact(&artifact),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );
    }

    #[test]
    fn vendor_candidate_listing_is_semantic_only_and_transition_requires_completed_proof() {
        let installed = component_from_files(vec![
            member_with_exports(
                "vorbisfile_vs2010_x64_rwdi.dll",
                Architecture::X64,
                &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
                &["ov_open"],
                16,
            ),
            member_with_exports(
                "vorbis_vs2010_x64_rwdi.dll",
                Architecture::X64,
                &["ogg_vs2010_x64_rwdi.dll"],
                &["vorbis_info_init"],
                17,
            ),
            member_with_exports(
                "ogg_vs2010_x64_rwdi.dll",
                Architecture::X64,
                &[],
                &["ogg_sync_init"],
                18,
            ),
        ]);
        let candidate = artifact_from_files(
            Architecture::X64,
            vec![
                member_with_exports(
                    "vorbisfile.dll",
                    Architecture::X64,
                    &["vorbis.dll", "ogg.dll"],
                    &["ov_open"],
                    1,
                ),
                member_with_exports(
                    "vorbis.dll",
                    Architecture::X64,
                    &["ogg.dll"],
                    &["vorbis_info_init"],
                    2,
                ),
                member_with_exports("ogg.dll", Architecture::X64, &[], &["ogg_sync_init"], 3),
            ],
        );

        assert_eq!(
            ensure_transition_compatible(&installed, &candidate),
            Err(SwapCompatibilityError::ExternalAliasProofRequired),
            "the legacy compatibility path must not authorize a vendor transition"
        );
        assert_eq!(
            ensure_candidate_compatible_without_alias_proof(&installed, &candidate),
            Ok(()),
            "candidate display may report semantic compatibility but cannot construct a transition"
        );
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &candidate,
                &ExternalAliasRequirements::Proven(BTreeSet::new()),
            ),
            Ok(()),
            "a complete strict scan with zero external bindings is a valid proof state"
        );
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &candidate,
                &ExternalAliasRequirements::Unproven,
            ),
            Err(SwapCompatibilityError::ExternalAliasProofRequired),
        );
    }

    #[test]
    fn unreal_catalog_artifacts_are_x64_and_reject_mixed_or_arbitrary_names() {
        let valid = artifact(
            Architecture::X64,
            &[
                (
                    "libvorbisfile_64.dll",
                    &["libvorbis_64.dll", "libogg_64.dll"],
                ),
                ("libvorbisenc_64.dll", &["libvorbis_64.dll"]),
                ("libvorbis_64.dll", &["libogg_64.dll"]),
                ("libogg_64.dll", &[]),
            ],
        );
        assert_eq!(validate_artifact(&valid), Ok(()));

        let x86 = artifact(
            Architecture::X86,
            &[
                ("libvorbis_64.dll", &["libogg_64.dll"]),
                ("libogg_64.dll", &[]),
            ],
        );
        assert_eq!(
            validate_artifact(&x86),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );

        let mixed = artifact(
            Architecture::X64,
            &[
                ("libvorbisfile_64.dll", &["libvorbis.dll"]),
                ("libvorbis.dll", &[]),
            ],
        );
        assert_eq!(
            validate_artifact(&mixed),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );

        let arbitrary = artifact(Architecture::X64, &[("libvorbis_64_more.dll", &[])]);
        assert!(validate_artifact(&arbitrary).is_err());
    }

    #[test]
    fn unreal_runtime_aliases_require_exact_names_and_complete_external_proof() {
        let installed = unreal_component("_64");
        let candidate = artifact(
            Architecture::X64,
            &[
                (
                    "libvorbisfile_64.dll",
                    &["libvorbis_64.dll", "libogg_64.dll"],
                ),
                ("libvorbis_64.dll", &["libogg_64.dll"]),
                ("libogg_64.dll", &[]),
            ],
        );
        let required_aliases = ExternalAliasRequirements::Proven(BTreeSet::from([
            "libvorbisfile_64.dll".to_owned(),
            "libvorbis_64.dll".to_owned(),
        ]));

        assert_eq!(validate_artifact(&candidate), Ok(()));
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &candidate,
                &required_aliases,
            ),
            Ok(())
        );
        for proof in [
            ExternalAliasRequirements::NotRequired,
            ExternalAliasRequirements::Unproven,
        ] {
            assert_eq!(
                ensure_transition_compatible_with_external_aliases(&installed, &candidate, &proof,),
                Err(SwapCompatibilityError::ExternalAliasProofRequired)
            );
        }
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &candidate,
                &ExternalAliasRequirements::Proven(BTreeSet::from([
                    "libvorbisfile_64_more.dll".to_owned(),
                ])),
            ),
            Err(SwapCompatibilityError::InvalidExternalAliasRequirement)
        );

        let wrong_family = artifact(
            Architecture::X64,
            &[
                ("libvorbisfile.dll", &["libvorbis.dll", "libogg.dll"]),
                ("libvorbis.dll", &["libogg.dll"]),
                ("libogg.dll", &[]),
            ],
        );
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &wrong_family,
                &ExternalAliasRequirements::Proven(BTreeSet::new()),
            ),
            Err(SwapCompatibilityError::VendorCandidateMustUsePlainNames)
        );

        let wrong_abi = artifact(
            Architecture::X64,
            &[
                ("libvorbisfile-3.dll", &["libvorbis-0.dll", "libogg-0.dll"]),
                ("libvorbis-0.dll", &["libogg-0.dll"]),
                ("libogg-0.dll", &[]),
            ],
        );
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &wrong_abi,
                &ExternalAliasRequirements::Proven(BTreeSet::new()),
            ),
            Err(SwapCompatibilityError::VendorCandidateMustUsePlainNames)
        );

        let mismatched_runtime = unreal_component("_64_more");
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &mismatched_runtime,
                &candidate,
                &ExternalAliasRequirements::Proven(BTreeSet::new()),
            ),
            Err(SwapCompatibilityError::NamingFamilyMismatch)
        );

        let changed_topology = artifact(
            Architecture::X64,
            &[
                ("libvorbisfile_64.dll", &["libvorbis_64.dll"]),
                ("libvorbis_64.dll", &["libogg_64.dll"]),
                ("libogg_64.dll", &[]),
            ],
        );
        assert_eq!(
            ensure_transition_compatible_with_external_aliases(
                &installed,
                &changed_topology,
                &ExternalAliasRequirements::Proven(BTreeSet::new()),
            ),
            Err(SwapCompatibilityError::UnexpectedDependency)
        );
    }

    fn unreal_component(suffix: &str) -> LibraryComponent {
        component_from_files(vec![
            member_with_exports(
                &format!("libvorbisfile{suffix}.dll"),
                Architecture::X64,
                &[
                    &format!("libvorbis{suffix}.dll"),
                    &format!("libogg{suffix}.dll"),
                ],
                &["ov_open"],
                16,
            ),
            member_with_exports(
                &format!("libvorbis{suffix}.dll"),
                Architecture::X64,
                &[&format!("libogg{suffix}.dll")],
                &["vorbis_info_init"],
                17,
            ),
            member_with_exports(
                &format!("libogg{suffix}.dll"),
                Architecture::X64,
                &[],
                &["ogg_sync_init"],
                18,
            ),
        ])
    }

    fn artifact(architecture: Architecture, files: &[(&str, &[&str])]) -> LibraryArtifact {
        let files = files
            .iter()
            .enumerate()
            .map(|(index, (name, imports))| member(name, architecture, imports, index as u8 + 1))
            .collect::<Vec<_>>();
        artifact_from_files(architecture, files)
    }

    fn artifact_from_files(
        architecture: Architecture,
        files: Vec<ComponentFile>,
    ) -> LibraryArtifact {
        let primary_name = files[0].path().file_name().expect("name").to_owned();
        LibraryArtifact::new(
            ArtifactId::new("artifact:xiph:test").expect("id"),
            LibraryTechnology::XiphVorbis,
            &primary_name,
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(architecture)),
        )
    }

    fn component(files: &[(&str, &[&str])]) -> LibraryComponent {
        component_from_files(
            files
                .iter()
                .enumerate()
                .map(|(index, (name, imports))| {
                    member(name, Architecture::X86, imports, index as u8 + 16)
                })
                .collect(),
        )
    }

    fn component_from_files(files: Vec<ComponentFile>) -> LibraryComponent {
        files.into_iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:xiph").expect("id"),
                GameId::new("game:xiph").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            LibraryComponent::with_file,
        )
    }

    fn member(name: &str, architecture: Architecture, imports: &[&str], hash: u8) -> ComponentFile {
        let export = match xiph::classify_file_name(name).map(|value| value.0) {
            Some(XiphMember::Ogg) => "ogg_sync_init",
            Some(XiphMember::Vorbis) => "vorbis_info_init",
            Some(XiphMember::VorbisFile) => "ov_open",
            Some(XiphMember::VorbisEnc) => "vorbis_encode_init",
            None => "unknown",
        };
        member_with_exports(name, architecture, imports, &[export], hash)
    }

    fn member_with_exports(
        name: &str,
        architecture: Architecture,
        imports: &[&str],
        exports: &[&str],
        hash: u8,
    ) -> ComponentFile {
        ComponentFile::new(PathRef::new(format!("C:/runtime/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(format!("{hash:02x}").repeat(32)).expect("hash"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    architecture,
                    PeExportSet::from_observed_names(
                        exports.iter().map(|export| (*export).to_owned()).collect(),
                    )
                    .expect("exports"),
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
}
