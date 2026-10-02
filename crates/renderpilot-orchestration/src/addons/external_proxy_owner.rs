use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, ManagedFileBaseline,
    NormalizedPathRelation, normalized_path_relation,
};

use crate::{Context, ServiceError};

pub(crate) mod prepared_release;

#[cfg(test)]
mod tests;

/// A raw Proxy owner whose only file receipts are outside the current game
/// installation root. This is the narrow explicit-install replacement case;
/// normal ownership and unmanaged guards remain authoritative otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InactiveExternalProxyOwner {
    pub(crate) record: InstalledAddon,
    /// Exact existing unmanaged-signature paths backed by this owner receipt.
    pub(crate) unmanaged_paths: Vec<PathBuf>,
}

impl InactiveExternalProxyOwner {
    /// Old split-payload root. The target union retains it even when all old
    /// receipt operations are idempotently absent.
    #[must_use]
    pub(crate) fn external_addon_root(&self) -> PathBuf {
        Path::new(self.record.addon_file().as_str())
            .parent()
            .unwrap_or_else(|| Path::new(self.record.addon_file().as_str()))
            .to_path_buf()
    }
}

/// Adds the old external receipt root and exact prepared release paths to the
/// new install's durable target set.
pub(crate) fn augment_external_owner_targets(
    targets: &mut crate::addons::mutation_targets::MutationTargets,
    owner: &InactiveExternalProxyOwner,
    release_paths: impl IntoIterator<Item = PathBuf>,
) {
    targets.roots.push(owner.external_addon_root());
    for path in release_paths {
        if let Some(parent) = path.parent() {
            targets.roots.push(parent.to_path_buf());
        }
        targets.paths.push(path);
    }
    let mut seen = std::collections::HashSet::new();
    targets
        .paths
        .retain(|path| seen.insert(crate::paths::normalized_key(path)));
    let mut seen = std::collections::HashSet::new();
    targets
        .roots
        .retain(|path| seen.insert(crate::paths::normalized_key(path)));
}

/// Resolves a retained external Proxy owner without changing its raw ownership
/// row. Callers hold the game's mutation boundary while capturing/revalidating.
pub(crate) fn resolve_inactive_external_proxy_owner(
    context: &Context,
    game_id: &GameId,
    game_root: &Path,
) -> Result<Option<InactiveExternalProxyOwner>, ServiceError> {
    use renderpilot_application::InstalledAddonRepository;

    let Some(record) = context.storage().get_installed_addon(game_id)? else {
        return Ok(None);
    };
    if record.game_id() != game_id
        || !matches!(record.kind(), AddonKind::Luma | AddonKind::RenoDx)
        // Missing host metadata is the legacy Proxy form; only the explicit
        // Shared Vulkan tag removes a row from this receipt-only path.
        || !matches!(
            record.host_kind(),
            None | Some(InstalledAddonHostKind::Proxy)
        )
        || crate::addons::tool::record_is_active_for_game(context, &record)?
    {
        return Ok(None);
    }

    let game_root = game_root.to_string_lossy();
    let addon_file = PathBuf::from(record.addon_file().as_str());
    if !addon_file.is_absolute() {
        return Err(ServiceError::invalid_input(
            "inactive Proxy owner has a malformed add-on receipt path",
        ));
    }
    // The projected external-owner shape must remain outside the current game
    // root. A local missing-payload record remains on the ordinary guarded
    // path and cannot be treated as permission to replace an inactive install.
    if !matches!(
        normalized_path_relation(&game_root, &addon_file.to_string_lossy()),
        NormalizedPathRelation::Disjoint
    ) {
        return Ok(None);
    }

    let mut receipt_paths = vec![addon_file.clone()];
    receipt_paths.extend(
        record
            .created_files()
            .iter()
            .map(|path| PathBuf::from(path.as_str())),
    );
    for path in record.backed_up_files() {
        let path = PathBuf::from(path.as_str());
        receipt_paths.push(path.clone());
        receipt_paths.push(crate::fs::backup_path(&path).map_err(|error| {
            ServiceError::invalid_input(format!(
                "inactive Proxy owner has an invalid backup receipt: {error}"
            ))
        })?);
    }
    for managed in record.managed_files() {
        let path = PathBuf::from(managed.path().as_str());
        receipt_paths.push(path.clone());
        if matches!(managed.baseline(), ManagedFileBaseline::Present { .. }) {
            // Managed baselines use the conventional sidecar derived from the
            // exact receipt path. Install planners independently authorize the
            // concrete operation paths they execute.
            receipt_paths.push(crate::fs::backup_path(&path).map_err(|error| {
                ServiceError::invalid_input(format!(
                    "inactive Proxy owner has an invalid managed backup receipt: {error}"
                ))
            })?);
        }
    }
    if let Some(receipt) = record.renodx_config_receipt() {
        receipt_paths.push(PathBuf::from(receipt.ini_path.as_str()));
    }

    for path in &receipt_paths {
        if !path.is_absolute()
            || !matches!(
                normalized_path_relation(&game_root, &path.to_string_lossy()),
                NormalizedPathRelation::Disjoint
            )
        {
            return Err(ServiceError::invalid_input(
                "inactive Proxy owner contains a malformed or game-local external receipt",
            ));
        }
    }

    let mut unmanaged_paths = vec![addon_file];
    unmanaged_paths.extend(
        record
            .created_files()
            .iter()
            .map(|path| PathBuf::from(path.as_str())),
    );
    unmanaged_paths.extend(
        record
            .managed_files()
            .iter()
            .filter(|managed| managed.mode() == renderpilot_domain::ManagedFileMode::Owned)
            .map(|managed| PathBuf::from(managed.path().as_str())),
    );
    unmanaged_paths.sort_by_cached_key(|path| crate::paths::normalized_key(path));
    unmanaged_paths.dedup_by(|left, right| crate::paths::same_path(left, right));

    Ok(Some(InactiveExternalProxyOwner {
        record,
        unmanaged_paths,
    }))
}
