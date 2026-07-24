//! D3D12 Agility executable policy and confirmation fingerprinting.

use renderpilot_domain::{
    Architecture, ComponentFile, D3d12ExecutableIdentity, GraphicsComponent, GraphicsTechnology,
    LibraryArtifact, PathRef, RuntimeCompatibility, normalized_path_key,
};
use sha2::{Digest, Sha256};

use super::SwapCompatibilityError;

/// Fresh facts read from the executable selected by a game installation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwapTargetProfile {
    architecture: Option<Architecture>,
    d3d12_sdk_version: Option<u32>,
    d3d12_executable: Option<D3d12ExecutableProfile>,
    d3d12_snapshot: Option<D3d12ExecutableSnapshot>,
}

impl SwapTargetProfile {
    /// Creates a profile from independently observable executable facts.
    #[must_use]
    pub const fn new(architecture: Option<Architecture>, d3d12_sdk_version: Option<u32>) -> Self {
        Self {
            architecture,
            d3d12_sdk_version,
            d3d12_executable: None,
            d3d12_snapshot: None,
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

    /// Attaches the lightweight D3D12 read model used for candidate presentation.
    #[must_use]
    pub fn with_d3d12_executable_profile(mut self, profile: D3d12ExecutableProfile) -> Self {
        self.d3d12_sdk_version = Some(profile.current_sdk_version);
        self.d3d12_executable = Some(profile);
        self.d3d12_snapshot = None;
        self
    }

    /// Attaches an authoritative snapshot used by planning and execution.
    #[must_use]
    pub fn with_d3d12_executable_snapshot(mut self, snapshot: D3d12ExecutableSnapshot) -> Self {
        self.d3d12_sdk_version = Some(snapshot.profile.current_sdk_version);
        self.d3d12_executable = Some(snapshot.profile.clone());
        self.d3d12_snapshot = Some(snapshot);
        self
    }

    /// Returns managed D3D12 presentation facts, when orchestration resolved them.
    #[must_use]
    pub const fn d3d12_executable(&self) -> Option<&D3d12ExecutableProfile> {
        self.d3d12_executable.as_ref()
    }

    /// Returns authoritative D3D12 identities, when a mutation preflight resolved them.
    #[must_use]
    pub const fn d3d12_snapshot(&self) -> Option<&D3d12ExecutableSnapshot> {
        self.d3d12_snapshot.as_ref()
    }
}

/// Lightweight D3D12 executable read model without whole-file hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D3d12ExecutableProfile {
    executable_path: PathRef,
    backup_path: PathRef,
    original_sdk_version: u32,
    current_sdk_version: u32,
    backup_exists: bool,
    repair_required: bool,
}

impl D3d12ExecutableProfile {
    /// Creates presentation and policy facts.
    #[must_use]
    pub const fn new(
        executable_path: PathRef,
        backup_path: PathRef,
        original_sdk_version: u32,
        current_sdk_version: u32,
        backup_exists: bool,
        repair_required: bool,
    ) -> Self {
        Self {
            executable_path,
            backup_path,
            original_sdk_version,
            current_sdk_version,
            backup_exists,
            repair_required,
        }
    }

    /// Returns the active executable path.
    pub const fn executable_path(&self) -> &PathRef {
        &self.executable_path
    }

    /// Returns the immutable sidecar path.
    pub const fn backup_path(&self) -> &PathRef {
        &self.backup_path
    }

    /// Returns the SDK line exported by the original executable.
    pub const fn original_sdk_version(&self) -> u32 {
        self.original_sdk_version
    }

    /// Returns the SDK line exported by the live executable.
    pub const fn current_sdk_version(&self) -> u32 {
        self.current_sdk_version
    }

    /// Whether the executable pair cannot be handled as a managed four-byte patch.
    pub const fn repair_required(&self) -> bool {
        self.repair_required
    }

    /// Whether the immutable original sidecar exists.
    pub const fn backup_exists(&self) -> bool {
        self.backup_exists
    }
}

/// Authoritative D3D12 state with complete byte identities for plan/apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D3d12ExecutableSnapshot {
    profile: D3d12ExecutableProfile,
    original: D3d12ExecutableIdentity,
    current: D3d12ExecutableIdentity,
}

impl D3d12ExecutableSnapshot {
    /// Creates a mutation-grade snapshot.
    #[must_use]
    pub fn new(
        executable_path: PathRef,
        backup_path: PathRef,
        original: D3d12ExecutableIdentity,
        current: D3d12ExecutableIdentity,
        backup_exists: bool,
        repair_required: bool,
    ) -> Self {
        let profile = D3d12ExecutableProfile::new(
            executable_path,
            backup_path,
            original.sdk_version(),
            current.sdk_version(),
            backup_exists,
            repair_required,
        );
        Self {
            profile,
            original,
            current,
        }
    }

    /// Returns the lightweight policy facts.
    pub const fn profile(&self) -> &D3d12ExecutableProfile {
        &self.profile
    }

    /// Returns the authoritative original identity.
    pub const fn original(&self) -> &D3d12ExecutableIdentity {
        &self.original
    }

    /// Returns the authoritative active identity.
    pub const fn current(&self) -> &D3d12ExecutableIdentity {
        &self.current
    }
}

/// Planned action for the main executable that selects a D3D12 Agility SDK line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D3d12ExecutableAction {
    kind: D3d12ExecutableActionKind,
    executable_path: PathRef,
    backup_path: PathRef,
    original_sdk_version: u32,
    current_sdk_version: u32,
    target_sdk_version: u32,
    backup_exists: bool,
    confirmation_required: bool,
}

impl D3d12ExecutableAction {
    /// Assesses a user-selected swap against current executable facts.
    pub fn for_swap(
        profile: &D3d12ExecutableProfile,
        target_sdk_version: u32,
    ) -> Result<Self, SwapCompatibilityError> {
        Self::assess(profile, target_sdk_version, true)
    }

    /// Creates an original-baseline action for a managed rollback.
    pub fn for_managed_rollback(
        profile: &D3d12ExecutableProfile,
    ) -> Result<Self, SwapCompatibilityError> {
        Self::assess(profile, profile.original_sdk_version, false)
    }

    fn assess(
        profile: &D3d12ExecutableProfile,
        target_sdk_version: u32,
        user_selected: bool,
    ) -> Result<Self, SwapCompatibilityError> {
        if target_sdk_version < profile.original_sdk_version {
            return Err(SwapCompatibilityError::D3d12SdkDowngrade {
                artifact: target_sdk_version,
                original: profile.original_sdk_version,
            });
        }
        let kind = if profile.repair_required {
            D3d12ExecutableActionKind::RepairRequired
        } else if target_sdk_version == profile.current_sdk_version {
            D3d12ExecutableActionKind::None
        } else if target_sdk_version == profile.original_sdk_version {
            D3d12ExecutableActionKind::Restore
        } else {
            D3d12ExecutableActionKind::Patch
        };
        Ok(Self {
            kind,
            executable_path: profile.executable_path.clone(),
            backup_path: profile.backup_path.clone(),
            original_sdk_version: profile.original_sdk_version,
            current_sdk_version: profile.current_sdk_version,
            target_sdk_version,
            backup_exists: profile.backup_exists,
            confirmation_required: user_selected
                && (matches!(kind, D3d12ExecutableActionKind::Restore)
                    || matches!(kind, D3d12ExecutableActionKind::Patch)
                        && profile.current_sdk_version == profile.original_sdk_version),
        })
    }

    /// Returns the action kind.
    pub const fn kind(&self) -> D3d12ExecutableActionKind {
        self.kind
    }

    /// Returns the active executable path.
    pub const fn executable_path(&self) -> &PathRef {
        &self.executable_path
    }

    /// Returns the immutable backup path.
    pub const fn backup_path(&self) -> &PathRef {
        &self.backup_path
    }

    /// Returns the original SDK line.
    pub const fn original_sdk_version(&self) -> u32 {
        self.original_sdk_version
    }

    /// Returns the active SDK line before the action.
    pub const fn current_sdk_version(&self) -> u32 {
        self.current_sdk_version
    }

    /// Returns the requested SDK line.
    pub const fn target_sdk_version(&self) -> u32 {
        self.target_sdk_version
    }

    /// Whether the immutable backup already exists.
    pub const fn backup_exists(&self) -> bool {
        self.backup_exists
    }

    /// Whether a fresh swap-plan token must accompany apply.
    pub const fn requires_confirmation(&self) -> bool {
        self.confirmation_required
    }

    /// Whether the action mutates the active executable bytes.
    pub const fn changes_executable(&self) -> bool {
        matches!(
            self.kind,
            D3d12ExecutableActionKind::Patch | D3d12ExecutableActionKind::Restore
        )
    }
}

/// Stable wire-level action kinds for D3D12 executable handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum D3d12ExecutableActionKind {
    /// The active executable already selects the requested SDK.
    None,
    /// Patch the SDK export from the immutable original.
    Patch,
    /// Restore the immutable original executable.
    Restore,
    /// Refuse mutation because the executable pair is not safely managed.
    RepairRequired,
}

impl D3d12ExecutableActionKind {
    /// Returns the stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Patch => "patch",
            Self::Restore => "restore",
            Self::RepairRequired => "repair_required",
        }
    }
}

/// Computes the D3D12 executable action for a replacement artifact.
pub fn replacement_executable_action(
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
) -> Result<Option<D3d12ExecutableAction>, SwapCompatibilityError> {
    if artifact.technology() != GraphicsTechnology::D3D12Agility {
        return Ok(None);
    }
    let Some(executable) = profile.d3d12_executable() else {
        return Ok(None);
    };
    let target = artifact
        .metadata()
        .runtime_target()
        .and_then(|target| target.compatibility())
        .and_then(RuntimeCompatibility::as_d3d12_sdk_version)
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    D3d12ExecutableAction::for_swap(executable, target).map(Some)
}

/// Computes the canonical, state-bound preflight fingerprint.
#[must_use]
pub fn d3d12_confirmation_token(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
    action: &D3d12ExecutableAction,
) -> Option<String> {
    profile
        .d3d12_snapshot()
        .map(|snapshot| d3d12_confirmation_fingerprint(component, snapshot, action, artifact))
}

fn d3d12_confirmation_fingerprint(
    component: &GraphicsComponent,
    snapshot: &D3d12ExecutableSnapshot,
    action: &D3d12ExecutableAction,
    artifact: &LibraryArtifact,
) -> String {
    let executable = snapshot.profile();
    let mut hasher = Sha256::new();
    hash_text(&mut hasher, "renderpilot:d3d12-confirmation:v4");
    for value in [
        component.game_id().as_str(),
        component.id().as_str(),
        action.kind().as_str(),
        &normalized_path_key(executable.executable_path().as_str()),
        &normalized_path_key(executable.backup_path().as_str()),
        snapshot.original().sha256().as_str(),
        snapshot.current().sha256().as_str(),
    ] {
        hash_text(&mut hasher, value);
    }
    hash_component_files(&mut hasher, b"active", component.files());
    hash_text(&mut hasher, "swap");
    hash_text(&mut hasher, artifact.id().as_str());
    let mut files = artifact.files().iter().collect::<Vec<_>>();
    files.sort_by_key(|file| {
        file.install_as()
            .or_else(|| file.path().file_name())
            .unwrap_or("")
            .to_ascii_lowercase()
    });
    hash_count(&mut hasher, files.len());
    for file in files {
        hash_text(
            &mut hasher,
            file.install_as()
                .or_else(|| file.path().file_name())
                .unwrap_or(""),
        );
        hash_text(&mut hasher, file.sha256().map_or("", |hash| hash.as_str()));
    }
    hasher.update([
        u8::from(executable.backup_exists()),
        u8::from(executable.repair_required()),
    ]);
    hasher.update(executable.original_sdk_version().to_le_bytes());
    hasher.update(executable.current_sdk_version().to_le_bytes());
    hasher.update(action.target_sdk_version().to_le_bytes());
    hex::encode(hasher.finalize())
}

fn hash_component_files(hasher: &mut Sha256, label: &[u8], files: &[ComponentFile]) {
    hash_bytes(hasher, label);
    let mut files = files.iter().collect::<Vec<_>>();
    files.sort_by_key(|file| normalized_path_key(file.path().as_str()));
    hash_count(hasher, files.len());
    for file in files {
        hash_text(hasher, &normalized_path_key(file.path().as_str()));
        hash_text(hasher, file.sha256().map_or("", |hash| hash.as_str()));
        hash_text(
            hasher,
            file.version().map_or("", |version| version.as_str()),
        );
    }
}

fn hash_count(hasher: &mut Sha256, count: usize) {
    hash_text(hasher, &count.to_string());
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_bytes(hasher, value.as_bytes());
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(value.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(value);
}
