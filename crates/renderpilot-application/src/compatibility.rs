//! Pure compatibility rules shared by candidate listing and mutation boundaries.

mod d3d12;
mod microsoft;
mod openvr;
mod xiph;

use renderpilot_domain::{Architecture, LibraryArtifact, LibraryComponent, LibraryTechnology};

pub use d3d12::{
    D3d12ExecutableAction, D3d12ExecutableActionKind, D3d12ExecutableProfile,
    D3d12ExecutableSnapshot, SwapTargetProfile, d3d12_confirmation_token,
    replacement_executable_action,
};
pub use xiph::is_allowed_xiph_system_import;
pub(crate) use xiph::{
    ensure_candidate_compatible_without_alias_proof,
    ensure_transition_compatible_with_external_aliases,
};

#[cfg(test)]
use microsoft::{D3D12_PACKAGE_ID, DXC_PACKAGE_ID};
#[cfg(test)]
use renderpilot_domain::openvr::UPSTREAM_REPOSITORY as OPENVR_REPOSITORY;

/// Why a Microsoft runtime artifact cannot target the selected executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapCompatibilityError {
    /// Executable architecture could not be determined.
    MissingTargetArchitecture,
    /// Executable `D3D12SDKVersion` could not be determined.
    MissingD3d12SdkVersion,
    /// Candidate and executable architectures differ.
    ArchitectureMismatch {
        /// Architecture declared by the artifact.
        artifact: Architecture,
        /// Architecture observed in the executable.
        executable: Architecture,
    },
    /// Candidate metadata is absent or inconsistent with its technology.
    InvalidArtifactMetadata,
    /// DXC is not the strict compiler/validator pair.
    IncompleteDxcPackage,
    /// Agility SDK line differs from `D3D12SDKVersion`.
    D3d12SdkMismatch {
        /// SDK line declared by the artifact.
        artifact: u32,
        /// SDK line exported by the executable.
        executable: u32,
    },
    /// Candidate requests an SDK line older than the immutable original EXE.
    D3d12SdkDowngrade {
        /// SDK line declared by the artifact.
        artifact: u32,
        /// SDK line captured from the original executable.
        original: u32,
    },
    /// The executable changed outside the one managed SDK export field.
    D3d12ExecutableRepairRequired,
    /// Installed native-library member lacks freshly observed PE compatibility facts.
    MissingInstalledPeMetadata,
    /// Candidate native-library architecture differs from the installed DLL.
    InstalledArchitectureMismatch {
        /// Architecture declared by the candidate.
        artifact: Architecture,
        /// Architecture read from the installed DLL.
        installed: Architecture,
    },
    /// Candidate removes named exports required by the installed runtime ABI.
    ExportSurfaceMismatch,
    /// A required strict regular/delay import profile is absent.
    InvalidImportProfile,
    /// A Xiph member imports an unexpected DLL or changes its internal graph.
    UnexpectedDependency,
    /// Candidate and installed Xiph ABI aliases differ.
    NamingFamilyMismatch,
    /// A Xiph package or installed component has an invalid member shape.
    IncompleteXiphPackage,
    /// Vendor-suffixed Xiph layouts require a complete external importer proof.
    ExternalAliasProofRequired,
    /// An external importer proof contained an alias that is not one of the
    /// detected vendor-suffixed Xiph members.
    InvalidExternalAliasRequirement,
    /// An alias proof was supplied for a canonical Xiph deployment.
    UnexpectedExternalAliasRequirement,
    /// Vendor deployments may only use plain canonical catalog artifacts.
    VendorCandidateMustUsePlainNames,
    /// Preserving one vendor alias would leave a canonical DLL imported by the
    /// candidate unavailable at runtime.
    ConflictingExternalAliasRequirement,
    /// This runtime requires the installed component as compatibility context.
    ComponentContextRequired,
    /// Component and replacement artifact technologies differ.
    TechnologyMismatch,
}

impl std::fmt::Display for SwapCompatibilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MissingTargetArchitecture => {
                "selected executable architecture could not be determined"
            }
            Self::MissingD3d12SdkVersion => "selected executable does not export D3D12SDKVersion",
            Self::ArchitectureMismatch {
                artifact,
                executable,
            } => {
                return write!(
                    formatter,
                    "runtime architecture {} does not match executable architecture {}",
                    artifact.as_str(),
                    executable.as_str()
                );
            }
            Self::InvalidArtifactMetadata => "runtime artifact metadata is incomplete or invalid",
            Self::IncompleteDxcPackage => "DXC must contain matched dxcompiler.dll and dxil.dll",
            Self::D3d12SdkMismatch {
                artifact,
                executable,
            } => {
                return write!(
                    formatter,
                    "D3D12 Agility SDK line {artifact} does not match D3D12SDKVersion {executable}"
                );
            }
            Self::D3d12SdkDowngrade { artifact, original } => {
                return write!(
                    formatter,
                    "D3D12 Agility SDK line {artifact} is older than original line {original}"
                );
            }
            Self::D3d12ExecutableRepairRequired => {
                "D3D12 executable differs from its backup outside D3D12SDKVersion"
            }
            Self::MissingInstalledPeMetadata => {
                "installed library architecture, exports, or imports could not be determined"
            }
            Self::InstalledArchitectureMismatch {
                artifact,
                installed,
            } => {
                return write!(
                    formatter,
                    "runtime architecture {} does not match installed DLL architecture {}",
                    artifact.as_str(),
                    installed.as_str()
                );
            }
            Self::ExportSurfaceMismatch => {
                "candidate does not preserve the installed runtime's required named exports"
            }
            Self::InvalidImportProfile => "regular and delay-load PE imports could not be verified",
            Self::UnexpectedDependency => {
                "Xiph package has an unexpected dependency or incompatible import graph"
            }
            Self::NamingFamilyMismatch => {
                "Xiph candidate does not preserve the installed DLL aliases"
            }
            Self::IncompleteXiphPackage => "Xiph package does not cover the installed members",
            Self::ExternalAliasProofRequired => {
                "vendor-suffixed Xiph deployment requires a complete external alias proof"
            }
            Self::InvalidExternalAliasRequirement => {
                "external alias proof does not match an installed vendor Xiph DLL"
            }
            Self::UnexpectedExternalAliasRequirement => {
                "external alias proof was supplied for a canonical Xiph deployment"
            }
            Self::VendorCandidateMustUsePlainNames => {
                "vendor-suffixed Xiph deployment requires plain canonical candidate DLL names"
            }
            Self::ConflictingExternalAliasRequirement => {
                "required vendor alias conflicts with a canonical candidate dependency"
            }
            Self::ComponentContextRequired => {
                "runtime compatibility requires installed component context"
            }
            Self::TechnologyMismatch => {
                "component and replacement artifact technologies do not match"
            }
        })
    }
}

impl std::error::Error for SwapCompatibilityError {}

/// Validates the technology-specific contract carried by an artifact itself.
///
/// This does not inspect the selected executable. It is safe to call while a
/// catalog is being resolved, before any game has been selected.
pub fn validate_runtime_artifact(artifact: &LibraryArtifact) -> Result<(), SwapCompatibilityError> {
    match artifact.technology() {
        LibraryTechnology::MicrosoftDxc => microsoft::validate_dxc_artifact(artifact),
        LibraryTechnology::D3D12Agility => microsoft::validate_d3d12_artifact(artifact),
        LibraryTechnology::OpenVr => openvr::validate_artifact(artifact),
        LibraryTechnology::XiphVorbis => xiph::validate_artifact(artifact),
        _ => match artifact.metadata().runtime_target() {
            Some(target) if target.compatibility().is_some() => {
                Err(SwapCompatibilityError::InvalidArtifactMetadata)
            }
            _ => Ok(()),
        },
    }
}

/// Enforces the complete technology-specific transition contract.
///
/// Microsoft runtimes are checked against the selected executable. OpenVR and
/// Xiph are checked against freshly inspected facts from their installed DLLs.
pub fn ensure_replacement_compatible(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
) -> Result<(), SwapCompatibilityError> {
    if component.technology() != artifact.technology() {
        return Err(SwapCompatibilityError::TechnologyMismatch);
    }
    validate_runtime_artifact(artifact)?;
    match artifact.technology() {
        LibraryTechnology::OpenVr => openvr::ensure_transition_compatible(component, artifact),
        LibraryTechnology::XiphVorbis => xiph::ensure_transition_compatible(component, artifact),
        _ => {
            microsoft::ensure_executable_compatible(artifact, profile)?;
            if replacement_executable_action(artifact, profile)?
                .is_some_and(|action| action.kind() == D3d12ExecutableActionKind::RepairRequired)
            {
                return Err(SwapCompatibilityError::D3d12ExecutableRepairRequired);
            }
            Ok(())
        }
    }
}

/// Enforces an executable-context runtime contract.
///
/// OpenVR is rejected because its compatibility depends on freshly observed
/// facts from the installed component. Use [`ensure_replacement_compatible`]
/// for a complete transition decision.
pub fn ensure_swap_compatible(
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
) -> Result<(), SwapCompatibilityError> {
    if matches!(
        artifact.technology(),
        LibraryTechnology::OpenVr | LibraryTechnology::XiphVorbis
    ) {
        return Err(SwapCompatibilityError::ComponentContextRequired);
    }
    validate_runtime_artifact(artifact)?;
    microsoft::ensure_executable_compatible(artifact, profile)
}

fn runtime_file_name(file: &renderpilot_domain::ComponentFile) -> Option<&str> {
    file.install_as().or_else(|| file.path().file_name())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, ComponentId,
        ComponentKind, GameId, LibraryComponent, PathRef, PeCompatibilityProfile, PeExportSet,
        RuntimeCompatibility, RuntimeTarget, Sha256Hash, Swappability, UpstreamPackage,
        UpstreamPackageProvider, Version,
    };

    use super::*;

    fn file(name: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(format!("C:/runtime/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn artifact(
        technology: LibraryTechnology,
        files: Vec<ComponentFile>,
        target: RuntimeTarget,
    ) -> LibraryArtifact {
        let file_name = files[0].path().file_name().expect("name").to_owned();
        LibraryArtifact::new(
            ArtifactId::for_bundle(files.iter().filter_map(ComponentFile::sha256)),
            technology,
            file_name,
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata({
            let mut metadata = ArtifactMetadata::default().with_runtime_target(target);
            if technology == LibraryTechnology::MicrosoftDxc {
                metadata = metadata.with_upstream_package(
                    UpstreamPackage::new(
                        UpstreamPackageProvider::NuGet,
                        DXC_PACKAGE_ID,
                        "1.9.2602.24",
                    )
                    .expect("package"),
                );
            }
            metadata
        })
    }

    fn openvr_file(architecture: Architecture, exports: &[&str], hash: char) -> ComponentFile {
        file(renderpilot_domain::openvr::DLL_NAME, hash).with_pe_compatibility(
            PeCompatibilityProfile::new(
                architecture,
                PeExportSet::from_canonical_names(
                    exports.iter().map(|name| (*name).to_owned()).collect(),
                )
                .expect("exports"),
            ),
        )
    }

    fn openvr_artifact(architecture: Architecture, exports: &[&str]) -> LibraryArtifact {
        artifact(
            LibraryTechnology::OpenVr,
            vec![openvr_file(architecture, exports, '8')],
            RuntimeTarget::new(architecture),
        )
        .with_metadata(
            ArtifactMetadata::default()
                .with_runtime_target(RuntimeTarget::new(architecture))
                .with_upstream_package(
                    UpstreamPackage::new(
                        UpstreamPackageProvider::GitHub,
                        OPENVR_REPOSITORY,
                        "2.15.6",
                    )
                    .expect("package"),
                ),
        )
    }

    fn openvr_component(architecture: Architecture, exports: &[&str]) -> LibraryComponent {
        LibraryComponent::new(
            ComponentId::new("component:openvr").expect("id"),
            GameId::new("game:openvr").expect("id"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::OpenVr,
            Swappability::Swappable,
        )
        .with_file(openvr_file(architecture, exports, '7'))
    }

    fn complete_dxc_artifact(architecture: Architecture) -> LibraryArtifact {
        artifact(
            LibraryTechnology::MicrosoftDxc,
            vec![
                file(crate::dxc::COMPILER_FILE_NAME, 'a'),
                file(crate::dxc::VALIDATOR_FILE_NAME, 'b'),
            ],
            RuntimeTarget::new(architecture),
        )
    }

    #[test]
    fn dxc_requires_architecture_and_complete_pair() {
        let complete = complete_dxc_artifact(Architecture::X64);
        let x64 = SwapTargetProfile::new(Some(Architecture::X64), None);
        let x86 = SwapTargetProfile::new(Some(Architecture::X86), None);
        assert!(ensure_swap_compatible(&complete, &x64).is_ok());
        assert_eq!(
            ensure_swap_compatible(&complete, &x86),
            Err(SwapCompatibilityError::ArchitectureMismatch {
                artifact: Architecture::X64,
                executable: Architecture::X86,
            })
        );

        let partial = artifact(
            LibraryTechnology::MicrosoftDxc,
            vec![file(crate::dxc::COMPILER_FILE_NAME, 'c')],
            RuntimeTarget::new(Architecture::X64),
        );
        assert_eq!(
            ensure_swap_compatible(&partial, &x64),
            Err(SwapCompatibilityError::IncompleteDxcPackage)
        );
    }

    #[test]
    fn dxc_complete_pair_is_compatible_with_a_standalone_compiler() {
        let standalone_target = LibraryComponent::new(
            ComponentId::new("component:dxc-standalone").expect("component id"),
            GameId::new("game:dxc-standalone").expect("game id"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::MicrosoftDxc,
            Swappability::Swappable,
        )
        .with_file(file(crate::dxc::COMPILER_FILE_NAME, 'd'));

        assert!(
            ensure_replacement_compatible(
                &standalone_target,
                &complete_dxc_artifact(Architecture::X64),
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            )
            .is_ok()
        );
    }

    #[test]
    fn dxc_requires_proven_nuget_package_identity() {
        let files = vec![file("dxcompiler.dll", 'e'), file("dxil.dll", 'f')];
        let without_package = LibraryArtifact::new(
            ArtifactId::for_bundle(files.iter().filter_map(ComponentFile::sha256)),
            LibraryTechnology::MicrosoftDxc,
            "dxcompiler.dll",
            files,
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        );

        assert_eq!(
            ensure_swap_compatible(
                &without_package,
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            ),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );
    }

    #[test]
    fn d3d12_requires_exact_exported_sdk_line() {
        let core = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12Core.dll", 'd')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
        );
        assert!(
            ensure_swap_compatible(
                &core,
                &SwapTargetProfile::new(Some(Architecture::X64), Some(618)),
            )
            .is_ok()
        );
        assert_eq!(
            ensure_swap_compatible(
                &core,
                &SwapTargetProfile::new(Some(Architecture::X64), Some(619)),
            ),
            Err(SwapCompatibilityError::D3d12SdkMismatch {
                artifact: 618,
                executable: 619,
            })
        );
        assert_eq!(
            ensure_swap_compatible(
                &core,
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            ),
            Err(SwapCompatibilityError::MissingD3d12SdkVersion)
        );
    }

    #[test]
    fn d3d12_requires_the_core_install_unit_and_consistent_package_line() {
        let profile = SwapTargetProfile::new(Some(Architecture::X64), Some(618));
        let wrong_file = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12SDKLayers.dll", '1')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
        );
        assert_eq!(
            ensure_swap_compatible(&wrong_file, &profile),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );

        let core = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12Core.dll", '2')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
        )
        .with_metadata(
            ArtifactMetadata::default()
                .with_runtime_target(
                    RuntimeTarget::new(Architecture::X64)
                        .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
                )
                .with_upstream_package(
                    UpstreamPackage::new(
                        UpstreamPackageProvider::NuGet,
                        D3D12_PACKAGE_ID,
                        "1.619.4",
                    )
                    .expect("package"),
                ),
        );
        assert_eq!(
            ensure_swap_compatible(&core, &profile),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );
    }

    #[test]
    fn every_declared_runtime_target_requires_the_executable_architecture() {
        let runtime = artifact(
            LibraryTechnology::DirectStorage,
            vec![file("dstorage.dll", '3')],
            RuntimeTarget::new(Architecture::X86),
        );

        assert!(
            ensure_swap_compatible(
                &runtime,
                &SwapTargetProfile::new(Some(Architecture::X86), None),
            )
            .is_ok()
        );
        assert_eq!(
            ensure_swap_compatible(
                &runtime,
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            ),
            Err(SwapCompatibilityError::ArchitectureMismatch {
                artifact: Architecture::X86,
                executable: Architecture::X64,
            })
        );
    }

    #[test]
    fn openvr_uses_installed_dll_architecture_instead_of_executable() {
        let component = openvr_component(Architecture::X86, &["A", "B"]);
        let candidate = openvr_artifact(Architecture::X86, &["A", "B", "C"]);
        assert!(
            ensure_replacement_compatible(
                &component,
                &candidate,
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            )
            .is_ok()
        );

        let wrong = openvr_artifact(Architecture::X64, &["A", "B", "C"]);
        assert!(matches!(
            ensure_replacement_compatible(&component, &wrong, &SwapTargetProfile::default()),
            Err(SwapCompatibilityError::InstalledArchitectureMismatch { .. })
        ));
    }

    #[test]
    fn installed_architecture_error_is_technology_neutral() {
        assert_eq!(
            SwapCompatibilityError::InstalledArchitectureMismatch {
                artifact: Architecture::X64,
                installed: Architecture::X86,
            }
            .to_string(),
            "runtime architecture X64 does not match installed DLL architecture X86"
        );
    }

    #[test]
    fn openvr_export_surface_guard_is_fail_closed() {
        let component = openvr_component(Architecture::X64, &["A", "C"]);
        let equal = openvr_artifact(Architecture::X64, &["A", "C"]);
        assert!(
            ensure_replacement_compatible(&component, &equal, &SwapTargetProfile::default())
                .is_ok()
        );

        let subset = openvr_artifact(Architecture::X64, &["A", "B"]);
        assert_eq!(
            ensure_replacement_compatible(&component, &subset, &SwapTargetProfile::default()),
            Err(SwapCompatibilityError::ExportSurfaceMismatch)
        );

        let missing_metadata = component.rebuild_with_files(vec![file("openvr_api.dll", '7')]);
        assert_eq!(
            ensure_replacement_compatible(
                &missing_metadata,
                &openvr_artifact(Architecture::X64, &["A", "C"]),
                &SwapTargetProfile::default(),
            ),
            Err(SwapCompatibilityError::MissingInstalledPeMetadata)
        );

        let wrong_name = component.rebuild_with_files(vec![
            openvr_file(Architecture::X64, &["A", "C"], '7').with_install_as("not_openvr.dll"),
        ]);
        assert_eq!(
            ensure_replacement_compatible(
                &wrong_name,
                &openvr_artifact(Architecture::X64, &["A", "C"]),
                &SwapTargetProfile::default(),
            ),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );
    }

    #[test]
    fn legacy_executable_context_api_cannot_bypass_openvr_policy() {
        assert_eq!(
            ensure_swap_compatible(
                &openvr_artifact(Architecture::X64, &["A"]),
                &SwapTargetProfile::new(Some(Architecture::X64), None),
            ),
            Err(SwapCompatibilityError::ComponentContextRequired)
        );

        let incomplete = LibraryArtifact::new(
            ArtifactId::new("artifact:openvr-incomplete").expect("artifact"),
            LibraryTechnology::OpenVr,
            renderpilot_domain::openvr::DLL_NAME,
            vec![file(renderpilot_domain::openvr::DLL_NAME, '9')],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");
        assert_eq!(
            ensure_swap_compatible(&incomplete, &SwapTargetProfile::default()),
            Err(SwapCompatibilityError::ComponentContextRequired),
            "legacy validation must never expose a context-free OpenVR path"
        );
    }

    fn d3d12_context(original: u32, current: u32, repair_required: bool) -> D3d12ExecutableProfile {
        D3d12ExecutableProfile::new(
            PathRef::new("C:/Game/game.exe").expect("path"),
            PathRef::new("C:/Game/game.exe.bak").expect("path"),
            original,
            current,
            current != original,
            repair_required,
        )
    }

    #[test]
    fn d3d12_executable_policy_covers_none_patch_restore_and_repair() {
        let original = d3d12_context(606, 606, false);
        assert_eq!(
            D3d12ExecutableAction::for_swap(&original, 606)
                .expect("none")
                .kind(),
            D3d12ExecutableActionKind::None
        );
        let first_patch = D3d12ExecutableAction::for_swap(&original, 619).expect("patch");
        assert_eq!(first_patch.kind(), D3d12ExecutableActionKind::Patch);
        assert!(!first_patch.backup_exists());
        assert!(first_patch.requires_confirmation());

        let patched = d3d12_context(606, 619, false);
        let repatch = D3d12ExecutableAction::for_swap(&patched, 618).expect("repatch");
        assert!(repatch.backup_exists());
        assert!(!repatch.requires_confirmation());
        let restore = D3d12ExecutableAction::for_swap(&patched, 606).expect("restore");
        assert_eq!(restore.kind(), D3d12ExecutableActionKind::Restore);
        assert!(restore.requires_confirmation());
        assert_eq!(
            D3d12ExecutableAction::for_swap(&d3d12_context(606, 619, true), 619)
                .expect("repair state")
                .kind(),
            D3d12ExecutableActionKind::RepairRequired
        );
    }

    #[test]
    fn d3d12_executable_policy_rejects_sdk_lines_below_original() {
        assert_eq!(
            D3d12ExecutableAction::for_swap(&d3d12_context(606, 619, false), 605),
            Err(SwapCompatibilityError::D3d12SdkDowngrade {
                artifact: 605,
                original: 606,
            })
        );
    }

    #[test]
    fn d3d12_confirmation_fingerprint_changes_with_active_executable_hash() {
        let component = LibraryComponent::new(
            ComponentId::new("component:d3d12-token").expect("component"),
            GameId::new("game:d3d12-token").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(file("D3D12Core.dll", '7'));
        let artifact = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12Core.dll", '8')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 619 }),
        );
        let profile = |active_hash: char| {
            SwapTargetProfile::new(Some(Architecture::X64), Some(606))
                .with_d3d12_executable_snapshot(D3d12ExecutableSnapshot::new(
                    PathRef::new("C:/Game/game.exe").expect("exe"),
                    PathRef::new("C:/Game/game.exe.bak").expect("backup"),
                    renderpilot_domain::D3d12ExecutableIdentity::new(
                        606,
                        Sha256Hash::new("a".repeat(64)).expect("original hash"),
                    ),
                    renderpilot_domain::D3d12ExecutableIdentity::new(
                        606,
                        Sha256Hash::new(active_hash.to_string().repeat(64)).expect("active hash"),
                    ),
                    false,
                    false,
                ))
        };
        let first_profile = profile('b');
        let first_action = replacement_executable_action(&artifact, &first_profile)
            .expect("policy")
            .expect("action");
        let first = d3d12_confirmation_token(&component, &artifact, &first_profile, &first_action)
            .expect("token");
        let changed_profile = profile('c');
        let changed_action = replacement_executable_action(&artifact, &changed_profile)
            .expect("policy")
            .expect("action");
        let changed =
            d3d12_confirmation_token(&component, &artifact, &changed_profile, &changed_action)
                .expect("token");

        assert_ne!(first, changed);
        assert_eq!(first.len(), 64);
        assert_eq!(changed.len(), 64);
    }

    #[test]
    fn d3d12_confirmation_fingerprint_uses_unambiguous_file_fields() {
        let component = |path: &str, version: &str| {
            LibraryComponent::new(
                ComponentId::new("component:d3d12-token-fields").expect("component"),
                GameId::new("game:d3d12-token-fields").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::D3D12Agility,
                Swappability::Swappable,
            )
            .with_file(
                ComponentFile::new(PathRef::new(path).expect("path"))
                    .with_version(Version::parse(version).expect("version")),
            )
        };
        let artifact = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12Core.dll", '8')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 619 }),
        );
        let profile = SwapTargetProfile::new(Some(Architecture::X64), Some(606))
            .with_d3d12_executable_snapshot(D3d12ExecutableSnapshot::new(
                PathRef::new("C:/Game/game.exe").expect("exe"),
                PathRef::new("C:/Game/game.exe.bak").expect("backup"),
                renderpilot_domain::D3d12ExecutableIdentity::new(
                    606,
                    Sha256Hash::new("a".repeat(64)).expect("original hash"),
                ),
                renderpilot_domain::D3d12ExecutableIdentity::new(
                    606,
                    Sha256Hash::new("b".repeat(64)).expect("active hash"),
                ),
                false,
                false,
            ));
        let action = replacement_executable_action(&artifact, &profile)
            .expect("policy")
            .expect("action");

        // The old concatenation encoded both records as `c:/runtime/a12`.
        let first = d3d12_confirmation_token(
            &component("C:/runtime/a", "12"),
            &artifact,
            &profile,
            &action,
        )
        .expect("first token");
        let second = d3d12_confirmation_token(
            &component("C:/runtime/a1", "2"),
            &artifact,
            &profile,
            &action,
        )
        .expect("second token");

        assert_ne!(first, second);
    }

    #[test]
    fn d3d12_confirmation_fingerprint_changes_with_backup_presence() {
        let component = LibraryComponent::new(
            ComponentId::new("component:d3d12-token-backup").expect("component"),
            GameId::new("game:d3d12-token-backup").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(file("D3D12Core.dll", '7'));
        let artifact = artifact(
            LibraryTechnology::D3D12Agility,
            vec![file("D3D12Core.dll", '8')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 619 }),
        );
        let profile = |backup_exists| {
            SwapTargetProfile::new(Some(Architecture::X64), Some(606))
                .with_d3d12_executable_snapshot(D3d12ExecutableSnapshot::new(
                    PathRef::new("C:/Game/game.exe").expect("exe"),
                    PathRef::new("C:/Game/game.exe.bak").expect("backup"),
                    renderpilot_domain::D3d12ExecutableIdentity::new(
                        606,
                        Sha256Hash::new("a".repeat(64)).expect("original hash"),
                    ),
                    renderpilot_domain::D3d12ExecutableIdentity::new(
                        606,
                        Sha256Hash::new("b".repeat(64)).expect("active hash"),
                    ),
                    backup_exists,
                    false,
                ))
        };
        let without_backup = profile(false);
        let without_backup_action = replacement_executable_action(&artifact, &without_backup)
            .expect("policy")
            .expect("action");
        let with_backup = profile(true);
        let with_backup_action = replacement_executable_action(&artifact, &with_backup)
            .expect("policy")
            .expect("action");

        assert_ne!(
            d3d12_confirmation_token(
                &component,
                &artifact,
                &without_backup,
                &without_backup_action,
            ),
            d3d12_confirmation_token(&component, &artifact, &with_backup, &with_backup_action),
        );
    }

    #[test]
    fn managed_rollback_restore_does_not_require_confirmation() {
        let context = d3d12_context(606, 619, false);
        let action = D3d12ExecutableAction::for_swap(&context, 606).expect("restore action");

        assert!(action.requires_confirmation());
        let rollback =
            D3d12ExecutableAction::for_managed_rollback(&context).expect("rollback action");
        assert_eq!(rollback.kind(), D3d12ExecutableActionKind::Restore);
        assert!(!rollback.requires_confirmation());
    }
}
