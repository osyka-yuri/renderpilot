use renderpilot_detection::LibraryPatternComponentDetector;
use renderpilot_domain::{GameId, RootAuthority};
use renderpilot_platform_windows::{ManualFolderGameSource, game_libraries::DiscoveredInstall};

use crate::ServiceError;
use crate::catalog::ScanFolderCatalogResult;

use super::scan_source_impl;

/// Per-install auto-scan using a shared open catalog, detector, and full
/// catalog index.
pub(crate) fn scan_auto_in_shared_batch(
    context: &crate::Context,
    detector: &LibraryPatternComponentDetector,
    catalog_index: &super::reconcile::CatalogInstallIndex,
    install: &DiscoveredInstall,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let path = &install.install_path;
    let game_id = catalog_index
        .game_id_for_install_path(path)
        .cloned()
        .unwrap_or_else(GameId::generate);
    let source = ManualFolderGameSource::new(path)
        .with_game_id(game_id)
        .with_known_identity(install.identity.clone())
        .with_root_authority(RootAuthority::LauncherManifest);

    scan_source_impl(
        super::ScanInputs { context, detector },
        &source,
        Some(catalog_index),
        super::ExplicitRootChange::Unchanged,
        &[],
    )
    .map(|result| vec![result])
}
