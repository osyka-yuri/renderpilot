#[derive(Clone, Copy)]
pub(super) enum DetectionMode {
    /// Full filesystem pass, but reuse cached hashes where possible.
    FullCached,
}

/// Controls how the scan derives game install roots from a scan target.
///
/// Shared by folder scan (all targets) and Windows auto-scan. Not
/// `#[cfg(windows)]`: user-initiated `scan_folder` uses this on every host.
#[derive(Clone, Copy)]
pub(super) enum InstallRootStrategy {
    /// The scan target is treated as a single game install.
    SingleInstall,
    /// The scan target may contain sibling game installs.
    SplitByFirstDiverge,
    /// Choose [`Self::SingleInstall`] for launcher-owned roots and
    /// [`Self::SplitByFirstDiverge`] for true manual folders.
    ///
    /// Used by user-initiated folder scan so Steam / GOG / Epic installs are
    /// not split into Manual sub-roots when DLL trees diverge.
    FromSelectedIdentity,
}

/// Resolves [`InstallRootStrategy::FromSelectedIdentity`] after discovery.
pub(super) fn resolve_install_root_strategy(
    strategy: InstallRootStrategy,
    selected_game: &renderpilot_domain::GameInstallation,
) -> InstallRootStrategy {
    match strategy {
        InstallRootStrategy::FromSelectedIdentity => {
            folder_scan_install_root_strategy(selected_game)
        }
        other => other,
    }
}

/// Folder-scan policy: split only when the selected root is still Manual.
pub(super) fn folder_scan_install_root_strategy(
    game: &renderpilot_domain::GameInstallation,
) -> InstallRootStrategy {
    if game.identity().launcher() == renderpilot_domain::Launcher::Manual {
        InstallRootStrategy::SplitByFirstDiverge
    } else {
        InstallRootStrategy::SingleInstall
    }
}
