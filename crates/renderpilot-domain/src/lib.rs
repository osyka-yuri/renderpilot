//! Pure domain model for RenderPilot.
//!
//! This crate must stay independent from UI frameworks, persistence adapters,
//! operating-system APIs, and detection implementation details.

mod component;
mod game;
mod ids;
mod model;
mod path;
mod text;
mod version;

pub use component::{
    ArtifactTrustLevel, ComponentError, ComponentFile, GraphicsComponent, LibraryArtifact,
    Sha256Digest, Sha256Hash,
};
pub use game::{GameIdentity, GameInstallation, GameModelError};
pub use ids::{ArtifactId, ComponentId, GameId, IdentifierError, OperationId};
pub use model::{ComponentKind, GameRuntime, GraphicsTechnology, Launcher, Platform, Swappability};
pub use path::{PathRef, PathRefError};
pub use version::{Version, VersionParseError};

/// Human-readable product name used across user-facing entry points.
pub const APP_NAME: &str = "RenderPilot";
