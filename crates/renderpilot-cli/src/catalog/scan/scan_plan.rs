#[derive(Clone, Copy)]
pub(super) enum DetectionMode {
    /// Full filesystem pass, but reuse cached hashes where possible.
    FullCached,

    /// Prefer fast cached detection, but fall back to a full cached pass when
    /// the fast path cannot produce a useful result.
    FastCachedWithFullFallback,
}

/// Controls how the scan derives game install roots from a scan target.
#[derive(Clone, Copy)]
pub(super) enum InstallRootStrategy {
    /// The scan target is treated as a single game install.
    SingleInstall,
    /// The scan target may contain sibling game installs.
    SplitByFirstDiverge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FastScanFallbackReason {
    EmptyFastResult,
    IncompleteFastResult,
    DegradedComparedToCatalog,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FastScanDecision {
    pub(super) fallback_reason: Option<FastScanFallbackReason>,
}

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
