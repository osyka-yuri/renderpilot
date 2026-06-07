use std::path::PathBuf;

use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::GameInstallation;

use super::scan_plan::InstallRootStrategy;

pub(super) fn derive_install_roots(
    selected_game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
    strategy: InstallRootStrategy,
) -> Vec<PathBuf> {
    let scan_root = super::normalized_install_path_buf(selected_game);

    match strategy {
        InstallRootStrategy::SingleInstall => vec![scan_root],
        InstallRootStrategy::SplitByFirstDiverge => {
            super::detect_game_install_roots(&scan_root, libraries)
        }
    }
}
