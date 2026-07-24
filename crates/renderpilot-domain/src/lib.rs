//! Pure domain model for RenderPilot.
//!
//! This crate must stay independent from UI frameworks, persistence adapters,
//! operating-system APIs, and detection implementation details.

mod addon;
mod component;
pub mod dlss;
mod exe_graphics;
pub mod fsr;
mod game;
mod ids;
mod model;
pub mod mutation_features;
pub mod openvr;
mod path;
mod text;
mod version;

pub use addon::{
    InstalledAddon, InstalledAddonHostKind, InstalledAddonInvariantError, InstalledAddonParts,
    LumaInstallState, ManagedAddonFile, ManagedFileBaseline, ManagedFileMode, RenoDxHostKind,
    RenoDxInstallState, SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord,
    SharedArtifactSource, TrackedSource, TrackedSourceRole,
};
pub use component::{
    ArtifactMetadata, ArtifactTrustLevel, ComponentError, ComponentFile, ComponentRollbackBaseline,
    ComponentVersionReport, D3d12ExecutableBaseline, D3d12ExecutableIdentity, GraphicsComponent,
    LibraryArtifact, PeCompatibilityProfile, PeExportSet, PeExportSetError, ReleaseMetadata,
    RuntimeCompatibility, RuntimeTarget, Sha256Digest, Sha256Hash, UpstreamPackage,
    UpstreamPackageProvider, component_version_report,
};
pub use exe_graphics::ExeGraphicsInfo;
pub use game::{GameIdentity, GameInstallation, GameModelError};
pub use ids::{ArtifactId, ComponentId, GameId, IdentifierError, OperationId};
pub use model::{
    AddonKind, Architecture, ComponentKind, GameRuntime, GraphicsApi, GraphicsTechnology, Launcher,
    Platform, Swappability,
};
pub use path::{PathRef, PathRefError, normalized_path_key};
pub use version::{Version, VersionParseError};

/// Human-readable product name used across user-facing entry points.
pub const APP_NAME: &str = "RenderPilot";
