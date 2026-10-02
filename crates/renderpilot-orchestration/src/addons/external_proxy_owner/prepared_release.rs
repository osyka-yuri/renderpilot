use std::path::{Path, PathBuf};

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, LibraryComponent};

use super::InactiveExternalProxyOwner;
use crate::{Context, ServiceError};

/// Concrete receipt-only release prepared by the owning tool's established
/// uninstall planner.
pub(crate) enum PreparedExternalOwnerRelease {
    RenoDx(crate::addons::renodx::install::PreparedRenoDxUninstall),
    Luma(Box<crate::addons::luma::use_cases::commands::uninstall::PreparedExternalOwnerRelease>),
}

impl PreparedExternalOwnerRelease {
    pub(crate) fn prepare(
        context: &Context,
        game_id: &GameId,
        owner: Option<&InactiveExternalProxyOwner>,
    ) -> Result<Option<Self>, ServiceError> {
        let Some(owner) = owner else {
            return Ok(None);
        };
        match owner.record.kind() {
            AddonKind::RenoDx => Ok(Some(Self::RenoDx(
                crate::addons::renodx::install::PreparedRenoDxUninstall::prepare_receipt_only(
                    &owner.record,
                )?,
            ))),
            AddonKind::Luma => Ok(Some(Self::Luma(Box::new(
                crate::addons::luma::use_cases::commands::uninstall::prepare_external_owner_release(
                    context,
                    game_id,
                    &owner.record,
                )?,
            )))),
            AddonKind::OptiScaler => Err(ServiceError::invalid_input(
                "external owner replacement is supported only for inactive Proxy RenoDX or Luma",
            )),
        }
    }

    pub(crate) fn affected_paths(&self) -> Vec<PathBuf> {
        match self {
            Self::RenoDx(release) => release.affected_paths(),
            Self::Luma(release) => release.targets().paths.clone(),
        }
    }

    /// Roots for the SVAM file participant. RenoDX's exact receipt closure is
    /// rooted at the external payload and affected path parents; Luma reuses
    /// the roots captured by its prepared plan.
    pub(crate) fn roots(&self, owner: &InstalledAddon) -> Vec<PathBuf> {
        match self {
            Self::RenoDx(release) => {
                let mut roots = vec![
                    Path::new(owner.addon_file().as_str())
                        .parent()
                        .unwrap_or_else(|| Path::new(owner.addon_file().as_str()))
                        .to_path_buf(),
                ];
                roots.extend(
                    release
                        .affected_paths()
                        .iter()
                        .filter_map(|path| path.parent().map(Path::to_path_buf)),
                );
                roots
            }
            Self::Luma(release) => release.targets().roots.clone(),
        }
    }

    pub(crate) fn apply_filesystem_only(&self) -> Result<(), ServiceError> {
        match self {
            Self::RenoDx(release) => release.apply(),
            Self::Luma(release) => release.apply_filesystem_only(),
        }
    }

    pub(crate) fn ensure_native_catalog_effects_empty(&self) -> Result<(), ServiceError> {
        match self {
            Self::RenoDx(_) => Ok(()),
            Self::Luma(release) => release.ensure_no_catalog_effects(),
        }
    }

    pub(crate) fn file_intents(
        &mut self,
    ) -> Result<Vec<crate::addons::shared_vulkan_mutation::FileIntent>, ServiceError> {
        match self {
            Self::RenoDx(release) => release.take_file_intents(),
            Self::Luma(release) => release.file_intents_for_shared_vulkan(),
        }
    }

    pub(crate) fn file_only_catalog_projection(
        &self,
    ) -> (
        Option<&[LibraryComponent]>,
        Vec<renderpilot_storage_sqlite::ComponentBaselineMutation<'_>>,
    ) {
        match self {
            Self::RenoDx(_) => (None, Vec::new()),
            Self::Luma(release) => (
                Some(release.next_components()),
                release.baseline_mutations(),
            ),
        }
    }

    pub(crate) fn journal_after_commit(&self, context: &Context, game_id: &GameId) {
        if let Self::Luma(release) = self {
            release.journal_after_commit(context, game_id);
        }
    }
}
