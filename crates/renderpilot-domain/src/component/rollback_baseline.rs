//! Immutable rollback identity for a graphics component and its D3D12 executable.

use serde::{Deserialize, Serialize};

use crate::{ComponentFile, PathRef, Sha256Hash};

/// Exact identity of a D3D12 executable at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct D3d12ExecutableIdentity {
    sdk_version: u32,
    sha256: Sha256Hash,
}

impl D3d12ExecutableIdentity {
    /// Creates an identity from the exported SDK line and the complete file hash.
    #[must_use]
    pub const fn new(sdk_version: u32, sha256: Sha256Hash) -> Self {
        Self {
            sdk_version,
            sha256,
        }
    }

    /// Returns the exported `D3D12SDKVersion`.
    #[must_use]
    pub const fn sdk_version(&self) -> u32 {
        self.sdk_version
    }

    /// Returns the SHA-256 hash of the complete executable.
    #[must_use]
    pub const fn sha256(&self) -> &Sha256Hash {
        &self.sha256
    }
}

/// Original component files plus the optional executable managed with their rollback.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentRollbackBaseline {
    files: Vec<ComponentFile>,
    /// Last component-file identities committed by RenderPilot.
    ///
    /// Older records legitimately omit this field. Such records can still be
    /// rolled back while their component exists (the component row supplies
    /// the active identity), but an orphaned baseline must fail closed rather
    /// than overwrite unproven live bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    expected_active_files: Vec<ComponentFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    d3d12_executable: Option<D3d12ExecutableBaseline>,
}

impl ComponentRollbackBaseline {
    /// Creates a baseline from the exact component files observed before the first overlay.
    #[must_use]
    pub fn new(files: Vec<ComponentFile>) -> Self {
        Self {
            files,
            expected_active_files: Vec::new(),
            d3d12_executable: None,
        }
    }

    /// Creates the complete aggregate from its explicit domain parts.
    #[must_use]
    pub fn from_parts(
        files: Vec<ComponentFile>,
        d3d12_executable: Option<D3d12ExecutableBaseline>,
    ) -> Self {
        Self {
            files,
            expected_active_files: Vec::new(),
            d3d12_executable,
        }
    }

    /// Returns the immutable original component-file identities.
    #[must_use]
    pub fn files(&self) -> &[ComponentFile] {
        &self.files
    }

    /// Returns the last active component identity committed with this baseline.
    #[must_use]
    pub fn expected_active_files(&self) -> &[ComponentFile] {
        &self.expected_active_files
    }

    /// Records the active component identity produced by the latest committed
    /// replacement without changing the immutable original baseline.
    #[must_use]
    pub fn with_expected_active_files(mut self, files: Vec<ComponentFile>) -> Self {
        self.expected_active_files = files;
        self
    }

    /// Attaches the executable baseline while the aggregate is first captured.
    #[must_use]
    pub fn with_d3d12_executable(mut self, baseline: D3d12ExecutableBaseline) -> Self {
        self.d3d12_executable = Some(baseline);
        self
    }

    /// Returns the D3D12 executable baseline, when this component owns one.
    #[must_use]
    pub const fn d3d12_executable(&self) -> Option<&D3d12ExecutableBaseline> {
        self.d3d12_executable.as_ref()
    }

    /// Updates only the expected active D3D12 executable identity.
    ///
    /// The executable path and original identity are deliberately retained from
    /// the captured record so subsequent swaps cannot replace the rollback source.
    #[must_use]
    pub fn with_expected_d3d12_identity(
        mut self,
        expected_active: D3d12ExecutableIdentity,
    ) -> Option<Self> {
        let baseline = self.d3d12_executable.take()?;
        self.d3d12_executable = Some(baseline.with_expected_active(expected_active));
        Some(self)
    }
}

/// Immutable original and mutable expected-active identity of a D3D12 executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct D3d12ExecutableBaseline {
    executable_path: PathRef,
    original: D3d12ExecutableIdentity,
    expected_active: D3d12ExecutableIdentity,
}

impl D3d12ExecutableBaseline {
    /// Captures the original executable identity and its initial active state.
    #[must_use]
    pub const fn new(
        executable_path: PathRef,
        original: D3d12ExecutableIdentity,
        expected_active: D3d12ExecutableIdentity,
    ) -> Self {
        Self {
            executable_path,
            original,
            expected_active,
        }
    }

    /// Returns the executable path permanently bound to the rollback aggregate.
    #[must_use]
    pub const fn executable_path(&self) -> &PathRef {
        &self.executable_path
    }

    /// Returns the one-time original executable identity.
    #[must_use]
    pub const fn original(&self) -> &D3d12ExecutableIdentity {
        &self.original
    }

    /// Returns the executable identity expected after the latest committed operation.
    #[must_use]
    pub const fn expected_active(&self) -> &D3d12ExecutableIdentity {
        &self.expected_active
    }

    /// Returns a copy with only the expected active identity changed.
    #[must_use]
    pub fn with_expected_active(mut self, expected_active: D3d12ExecutableIdentity) -> Self {
        self.expected_active = expected_active;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: char) -> Sha256Hash {
        Sha256Hash::new(byte.to_string().repeat(64)).expect("hash")
    }

    #[test]
    fn active_state_update_preserves_original_executable_identity() {
        let original = D3d12ExecutableIdentity::new(606, hash('a'));
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                PathRef::new(r"C:\Game\game.exe").expect("path"),
                original.clone(),
                original.clone(),
            ),
        );

        let updated = baseline
            .with_expected_d3d12_identity(D3d12ExecutableIdentity::new(619, hash('b')))
            .expect("d3d12 baseline");
        let executable = updated.d3d12_executable().expect("executable");
        assert_eq!(executable.original(), &original);
        assert_eq!(executable.expected_active().sdk_version(), 619);
    }

    #[test]
    fn aggregate_represents_at_most_one_d3d12_executable() {
        let baseline = ComponentRollbackBaseline::from_parts(
            Vec::new(),
            Some(D3d12ExecutableBaseline::new(
                PathRef::new(r"C:\Game\game.exe").expect("path"),
                D3d12ExecutableIdentity::new(606, hash('a')),
                D3d12ExecutableIdentity::new(619, hash('b')),
            )),
        );

        assert!(baseline.d3d12_executable().is_some());
    }
}
