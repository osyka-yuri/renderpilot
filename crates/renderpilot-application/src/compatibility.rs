//! Pure compatibility rules shared by candidate listing and mutation boundaries.

mod microsoft;
mod openvr;

use renderpilot_domain::{Architecture, GraphicsComponent, GraphicsTechnology, LibraryArtifact};

#[cfg(test)]
use microsoft::{D3D12_PACKAGE_ID, DXC_PACKAGE_ID};
#[cfg(test)]
use renderpilot_domain::openvr::UPSTREAM_REPOSITORY as OPENVR_REPOSITORY;

/// Fresh facts read from the selected game executable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwapTargetProfile {
    architecture: Option<Architecture>,
    d3d12_sdk_version: Option<u32>,
}

impl SwapTargetProfile {
    /// Creates a profile from independently observable executable facts.
    #[must_use]
    pub const fn new(architecture: Option<Architecture>, d3d12_sdk_version: Option<u32>) -> Self {
        Self {
            architecture,
            d3d12_sdk_version,
        }
    }

    /// Returns the executable architecture.
    pub const fn architecture(&self) -> Option<Architecture> {
        self.architecture
    }

    /// Returns the exact Agility SDK line requested by the executable.
    pub const fn d3d12_sdk_version(&self) -> Option<u32> {
        self.d3d12_sdk_version
    }
}

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
    /// Installed OpenVR DLL lacks freshly observed PE compatibility facts.
    MissingInstalledPeMetadata,
    /// Candidate OpenVR DLL architecture differs from the installed DLL.
    InstalledArchitectureMismatch {
        /// Architecture declared by the candidate.
        artifact: Architecture,
        /// Architecture read from the installed DLL.
        installed: Architecture,
    },
    /// Candidate removes named exports required by the installed OpenVR DLL.
    ExportSurfaceMismatch,
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
            Self::MissingInstalledPeMetadata => {
                "installed OpenVR DLL architecture or named exports could not be determined"
            }
            Self::InstalledArchitectureMismatch {
                artifact,
                installed,
            } => {
                return write!(
                    formatter,
                    "OpenVR runtime architecture {} does not match installed DLL architecture {}",
                    artifact.as_str(),
                    installed.as_str()
                );
            }
            Self::ExportSurfaceMismatch => {
                "OpenVR candidate does not preserve the installed DLL's named export surface"
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
        GraphicsTechnology::MicrosoftDxc => microsoft::validate_dxc_artifact(artifact),
        GraphicsTechnology::D3D12Agility => microsoft::validate_d3d12_artifact(artifact),
        GraphicsTechnology::OpenVr => openvr::validate_artifact(artifact),
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
/// Microsoft runtimes are checked against the selected executable. OpenVR is
/// checked against freshly inspected facts from the installed `openvr_api.dll`.
pub fn ensure_replacement_compatible(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
) -> Result<(), SwapCompatibilityError> {
    if component.technology() != artifact.technology() {
        return Err(SwapCompatibilityError::TechnologyMismatch);
    }
    validate_runtime_artifact(artifact)?;
    match artifact.technology() {
        GraphicsTechnology::OpenVr => openvr::ensure_transition_compatible(component, artifact),
        _ => microsoft::ensure_executable_compatible(artifact, profile),
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
    if artifact.technology() == GraphicsTechnology::OpenVr {
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
        ComponentKind, GameId, GraphicsComponent, PathRef, PeCompatibilityProfile, PeExportSet,
        RuntimeCompatibility, RuntimeTarget, Sha256Hash, Swappability, UpstreamPackage,
        UpstreamPackageProvider,
    };

    use super::*;

    fn file(name: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(format!("C:/runtime/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn artifact(
        technology: GraphicsTechnology,
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
            if technology == GraphicsTechnology::MicrosoftDxc {
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
            GraphicsTechnology::OpenVr,
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

    fn openvr_component(architecture: Architecture, exports: &[&str]) -> GraphicsComponent {
        GraphicsComponent::new(
            ComponentId::new("component:openvr").expect("id"),
            GameId::new("game:openvr").expect("id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::OpenVr,
            Swappability::Swappable,
        )
        .with_file(openvr_file(architecture, exports, '7'))
    }

    fn complete_dxc_artifact(architecture: Architecture) -> LibraryArtifact {
        artifact(
            GraphicsTechnology::MicrosoftDxc,
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
            GraphicsTechnology::MicrosoftDxc,
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
        let standalone_target = GraphicsComponent::new(
            ComponentId::new("component:dxc-standalone").expect("component id"),
            GameId::new("game:dxc-standalone").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::MicrosoftDxc,
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
            GraphicsTechnology::MicrosoftDxc,
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
            GraphicsTechnology::D3D12Agility,
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
            GraphicsTechnology::D3D12Agility,
            vec![file("D3D12SDKLayers.dll", '1')],
            RuntimeTarget::new(Architecture::X64)
                .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
        );
        assert_eq!(
            ensure_swap_compatible(&wrong_file, &profile),
            Err(SwapCompatibilityError::InvalidArtifactMetadata)
        );

        let core = artifact(
            GraphicsTechnology::D3D12Agility,
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
            GraphicsTechnology::DirectStorage,
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
            GraphicsTechnology::OpenVr,
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
}
