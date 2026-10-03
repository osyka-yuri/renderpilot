//! Resolved transition types and their read-only accessors.

use std::collections::BTreeSet;

use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, PathRef,
    xiph::{XiphMember, XiphTopology},
};

use super::paths::materialize_at;

/// External-import proof consumed by vendor-suffixed Xiph resolution.
///
/// Orchestration supplies a complete game-root scan; resolution performs no I/O.
/// Dynamic `LoadLibrary` consumers are outside this proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalAliasRequirements {
    /// The runtime has no vendor-suffixed alias to preserve.
    NotRequired,
    /// Exact required aliases in lowercase ASCII. An empty set means the
    /// completed scan found no external regular/delay bindings.
    Proven(BTreeSet<String>),
    /// The importer walk was absent, incomplete, or unstable.
    Unproven,
}

/// Sidecar state required before an original baseline member can be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveMode {
    /// The original live baseline file must be archived during this mutation.
    Create,
    /// The live file is already absent and a same-mutation/persisted owned
    /// archive must be proved by the durable mutation layer.
    RequireOwnedArchive,
}

/// A candidate artifact member written to one resolved target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWrite {
    pub(super) target: PathRef,
    pub(super) source: ComponentFile,
    pub(super) current: Option<ComponentFile>,
    pub(super) baseline: Option<ComponentFile>,
    pub(super) member: Option<XiphMember>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented resolved-write contract"
)]
impl ResolvedWrite {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn source(&self) -> &ComponentFile {
        &self.source
    }

    #[must_use]
    pub const fn current(&self) -> Option<&ComponentFile> {
        self.current.as_ref()
    }

    #[must_use]
    pub const fn baseline(&self) -> Option<&ComponentFile> {
        self.baseline.as_ref()
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// An immutable baseline member that must remain sidecar-preserved while no
/// longer live after the transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArchiveAndRemove {
    pub(super) target: PathRef,
    pub(super) baseline: ComponentFile,
    pub(super) current: Option<ComponentFile>,
    pub(super) mode: ArchiveMode,
    pub(super) member: Option<XiphMember>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented archive contract"
)]
impl ResolvedArchiveAndRemove {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn baseline(&self) -> &ComponentFile {
        &self.baseline
    }

    #[must_use]
    pub const fn current(&self) -> Option<&ComponentFile> {
        self.current.as_ref()
    }

    #[must_use]
    pub const fn mode(&self) -> ArchiveMode {
        self.mode
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// A current unowned addition that is deliberately removed without creating a
/// new immutable rollback sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRemove {
    pub(super) target: PathRef,
    pub(super) current: ComponentFile,
    pub(super) member: Option<XiphMember>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented removal contract"
)]
impl ResolvedRemove {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentFile {
        &self.current
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// A baseline member intentionally restored and left live outside this
/// package's writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedUntouchedBaseline {
    pub(super) target: PathRef,
    pub(super) baseline: ComponentFile,
    pub(super) current: ComponentFile,
    pub(super) member: Option<XiphMember>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented untouched-baseline contract"
)]
impl ResolvedUntouchedBaseline {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn baseline(&self) -> &ComponentFile {
        &self.baseline
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentFile {
        &self.current
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// One exhaustive path disposition in a resolved transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPathDisposition {
    /// Copy a verified artifact member to the target, replacing or adding it.
    Write(ResolvedWrite),
    /// Sidecar-preserve an immutable original, then make the live target absent.
    ArchiveAndRemove(ResolvedArchiveAndRemove),
    /// Remove a current addition that has no immutable baseline identity.
    Remove(ResolvedRemove),
    /// Keep an immutable baseline member live without writing it.
    UntouchedBaseline(ResolvedUntouchedBaseline),
}

impl ResolvedPathDisposition {
    /// Returns the unique normalized target represented by this disposition.
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        match self {
            Self::Write(value) => value.target(),
            Self::ArchiveAndRemove(value) => value.target(),
            Self::Remove(value) => value.target(),
            Self::UntouchedBaseline(value) => value.target(),
        }
    }
}

/// Semantic Xiph facts kept with a resolved transition instead of being
/// recomputed by preview, apply, rollback, or journal consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedXiphTransition {
    pub(super) topology: XiphTopology,
    pub(super) vendor_suffix: Option<String>,
    pub(super) external_aliases: BTreeSet<String>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented Xiph transition contract"
)]
impl ResolvedXiphTransition {
    #[must_use]
    pub const fn topology(&self) -> &XiphTopology {
        &self.topology
    }

    #[must_use]
    pub fn vendor_suffix(&self) -> Option<&str> {
        self.vendor_suffix.as_deref()
    }

    #[must_use]
    pub const fn external_aliases(&self) -> &BTreeSet<String> {
        &self.external_aliases
    }
}

/// The sole pure transition contract for preview, filesystem planning, apply,
/// rollback, active-state rebuilding, and journaling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTransition {
    pub(super) component_id: ComponentId,
    pub(super) artifact_id: ArtifactId,
    pub(super) target_directory: String,
    pub(super) primary_target: PathRef,
    pub(super) paths: Vec<ResolvedPathDisposition>,
    pub(super) xiph: Option<ResolvedXiphTransition>,
}

#[expect(
    missing_docs,
    reason = "accessors repeat the documented resolved-transition contract"
)]
impl ResolvedTransition {
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    #[must_use]
    pub const fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    #[must_use]
    pub fn target_directory(&self) -> &str {
        &self.target_directory
    }

    #[must_use]
    pub const fn primary_target(&self) -> &PathRef {
        &self.primary_target
    }

    #[must_use]
    pub fn paths(&self) -> &[ResolvedPathDisposition] {
        &self.paths
    }

    #[must_use]
    pub const fn xiph(&self) -> Option<&ResolvedXiphTransition> {
        self.xiph.as_ref()
    }

    /// Expected live component files after successful application. This is
    /// derived only from the owned path partition. An untouched baseline
    /// disposition resolves to its immutable original, because application
    /// restores that sidecar before writing any new overlay members.
    #[must_use]
    pub fn expected_active(&self) -> Vec<ComponentFile> {
        self.paths
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::Write(write) => {
                    Some(materialize_at(&write.source, write.target.clone()))
                }
                ResolvedPathDisposition::UntouchedBaseline(untouched) => {
                    Some(untouched.baseline.clone())
                }
                ResolvedPathDisposition::ArchiveAndRemove(_)
                | ResolvedPathDisposition::Remove(_) => None,
            })
            .collect()
    }

    /// Immutable originals whose live paths must remain reserved/absent after
    /// application. This is derived only from `ArchiveAndRemove` actions.
    #[must_use]
    pub fn reserved(&self) -> Vec<ComponentFile> {
        self.paths
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                    Some(archive.baseline.clone())
                }
                ResolvedPathDisposition::Write(_)
                | ResolvedPathDisposition::Remove(_)
                | ResolvedPathDisposition::UntouchedBaseline(_) => None,
            })
            .collect()
    }
}
