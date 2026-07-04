#[derive(Clone, Copy)]
pub(super) enum DetectionMode {
    /// Full filesystem pass, but reuse cached hashes where possible.
    FullCached,

    /// Prefer fast cached detection, but fall back to a full cached pass when
    /// the fast path cannot produce a useful result.
    ///
    /// Windows auto-scan only (`scan/auto.rs`). Folder scan on every host uses
    /// [`Self::FullCached`].
    #[cfg(windows)]
    FastCachedWithFullFallback,
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

/// Fast-path fallback decision for Windows auto-scan. Compiled on non-Windows
/// only under `cfg(test)` so the pure decision table stays unit-tested everywhere.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FastScanFallbackReason {
    EmptyFastResult,
    IncompleteFastResult,
    DegradedComparedToCatalog,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FastScanDecision {
    pub(super) fallback_reason: Option<FastScanFallbackReason>,
}

#[cfg(any(windows, test))]
impl FastScanDecision {
    fn with_reason(reason: FastScanFallbackReason) -> Self {
        Self {
            fallback_reason: Some(reason),
        }
    }

    fn keep_fast_result() -> Self {
        Self {
            fallback_reason: None,
        }
    }

    pub(super) fn should_fallback(self) -> bool {
        self.fallback_reason.is_some()
    }
}

#[cfg(any(windows, test))]
pub(super) fn decide_fast_scan_fallback(
    fast_count: usize,
    expected_detectable_count: usize,
    existing_component_count: usize,
) -> FastScanDecision {
    if fast_count == 0 {
        return FastScanDecision::with_reason(FastScanFallbackReason::EmptyFastResult);
    }

    if fast_count < expected_detectable_count {
        return FastScanDecision::with_reason(FastScanFallbackReason::IncompleteFastResult);
    }

    if existing_component_count > 0 && fast_count < existing_component_count {
        return FastScanDecision::with_reason(FastScanFallbackReason::DegradedComparedToCatalog);
    }

    FastScanDecision::keep_fast_result()
}
