use std::path::PathBuf;

use renderpilot_domain::{TrackedSource, TrackedSourceRole};

use crate::addons::engine::IniSection;

use super::super::types::LumaExternalRequirement;

/// One verified dgVoodoo file to lay down into the game directory.
#[derive(Debug, Clone)]
pub(crate) struct PreparedDgVoodooFile {
    pub(crate) dest: String,
    pub(crate) bytes: Vec<u8>,
}

/// A downloaded and integrity-verified managed dgVoodoo dependency.
#[derive(Debug, Clone)]
pub(crate) struct PreparedDgVoodoo {
    pub(crate) version: String,
    pub(crate) files: Vec<PreparedDgVoodooFile>,
    pub(crate) config_file: String,
    /// Minimal managed config used when the destination config is absent.
    /// Existing user configs are read from disk and receive only the required
    /// key overrides.
    pub(crate) config_default: String,
    pub(crate) config_sections: Vec<IniSection>,
    pub(crate) source_url: String,
    pub(crate) source_etag: Option<String>,
    pub(crate) source_last_modified: Option<String>,
    pub(crate) archive_digest: String,
}

/// A compatible dgVoodoo runtime that was already present before Luma's
/// install. Only the Luma-owned configuration keys are merged into it; DLLs
/// and the resulting config remain user-owned on later uninstall.
#[derive(Debug, Clone)]
pub(crate) struct ReusedDgVoodoo {
    pub(crate) config_file: String,
    pub(crate) config_default: String,
    pub(crate) config_sections: Vec<IniSection>,
}

/// A compatible dgVoodoo runtime whose DLLs and configuration are provably an
/// abandoned Luma-shaped stack during a live install (empty-host path). Owned
/// for cleanup; install does not invent wrapper provenance. DB-loss recovery
/// separately attaches an advisory `DgVoodooWrapper` source so freshness and
/// manage gates work after the install DB is wiped.
#[derive(Debug, Clone)]
pub(crate) struct AdoptedDgVoodoo {
    pub(crate) config: ReusedDgVoodoo,
    /// Existing DLLs and, when present, the safe configuration file.
    pub(crate) existing_paths: Vec<PathBuf>,
}

/// Lifecycle decision for a profile's optional dgVoodoo dependency.
#[derive(Debug, Clone)]
pub(crate) enum DgVoodooInstall {
    /// RenderPilot must download and own the declared dependency.
    Managed(PreparedDgVoodoo),
    /// A compatible user runtime exists; merge configuration only.
    Reused(ReusedDgVoodoo),
    /// A compatible, manifest-only orphan runtime is retained and owned.
    Adopted(AdoptedDgVoodoo),
}

/// Input to the Luma fetch layer after the read-only lifecycle assessment.
/// Keeping the decision explicit prevents a compatible existing runtime from
/// being downloaded and turned into a managed one later in the install path.
#[derive(Debug, Clone)]
pub(crate) enum DgVoodooPreparation<'a> {
    Managed(&'a LumaExternalRequirement),
    Reused(ReusedDgVoodoo),
    Adopted(AdoptedDgVoodoo),
}

/// Read-only assessment of a profile's existing dgVoodoo runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExistingDgVoodoo {
    /// None of the declared runtime files exist, so a normal managed install is safe.
    Absent,
    /// All required files identify a sufficiently new dgVoodoo runtime, but
    /// its configuration contains user-owned or unreadable content.
    CompatibleReusable,
    /// All required files identify a sufficiently new runtime and its config
    /// is absent or contains only exact manifest-owned assignments.
    CompatibleAdoptable,
    /// A partial, unrecognised, or too-old runtime exists and must not be modified.
    Conflict(String),
}

/// Freshness of a dgVoodoo runtime Luma already owns. This deliberately does
/// not describe whether an untracked user runtime is installable: callers use
/// it only after proving every current `install_map` destination is owned by
/// the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnedDgVoodooStatus {
    /// The D3D9 identity is dgVoodoo and its normalized version satisfies the
    /// current manifest requirement (a newer version is also current).
    Current,
    /// A recognizable dgVoodoo runtime is older than the manifest requirement.
    Outdated,
    /// One or more owned `install_map` destinations are missing (or not a
    /// regular file). Actionable: Update/Repair should re-place the stack.
    Incomplete,
    /// The local runtime files are present but unreadable, not safely
    /// identifiable as dgVoodoo, or the required version cannot be parsed.
    /// Conservative: normal Update skips; Repair may still reconverge.
    Unknown,
}

impl PreparedDgVoodoo {
    /// Source provenance recorded on the install so update checks can compare the
    /// pinned manifest dependency against what was actually installed.
    pub(crate) fn tracked_source(&self) -> TrackedSource {
        TrackedSource::new(
            TrackedSourceRole::DgVoodooWrapper,
            self.source_url.clone(),
            self.source_etag.clone(),
            self.archive_digest.clone(),
        )
        .with_last_modified(self.source_last_modified.clone())
        .with_channel(format!("dgvoodoo2@{}", self.version))
    }
}
