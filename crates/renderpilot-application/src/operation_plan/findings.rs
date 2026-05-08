/// Risk level assigned to a swap operation plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanRiskLevel {
    /// No special risk beyond the standard backup flow.
    Low,
    /// User should review the plan before execution.
    Medium,
    /// User confirmation is strongly required.
    High,
    /// The plan is blocked and should not be executed.
    Blocked,
}

impl OperationPlanRiskLevel {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Blocked => "blocked",
        }
    }

    pub(crate) fn from_findings(
        blockers: &[OperationPlanBlocker],
        warnings: &[OperationPlanWarning],
        requires_elevation: bool,
    ) -> Self {
        if !blockers.is_empty() {
            return Self::Blocked;
        }

        if warnings
            .iter()
            .copied()
            .any(OperationPlanWarning::raises_risk_to_high)
        {
            return Self::High;
        }

        if requires_elevation || !warnings.is_empty() {
            return Self::Medium;
        }

        Self::Low
    }
}

/// Condition that blocks a planned swap operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanBlocker {
    /// Component and artifact technologies do not match.
    TechnologyMismatch,
    /// Component is marked read-only.
    ComponentReadOnly,
    /// Component is integrated into the engine and should not be swapped directly.
    ComponentIntegratedIntoEngine,
    /// Component is explicitly marked unsafe.
    ComponentUnsafe,
    /// Selected artifact matches the currently installed file hash.
    ArtifactMatchesCurrentFile,
}

impl OperationPlanBlocker {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TechnologyMismatch => "technology_mismatch",
            Self::ComponentReadOnly => "component_read_only",
            Self::ComponentIntegratedIntoEngine => "component_integrated_into_engine",
            Self::ComponentUnsafe => "component_unsafe",
            Self::ArtifactMatchesCurrentFile => "artifact_matches_current_file",
        }
    }
}

/// Warning that should be surfaced before executing a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanWarning {
    /// Component swap requires explicit confirmation because it is not independently swappable.
    ConfirmationRequiredForSwappability,
    /// Streamline single-file replacement is a partial swap and needs an explicit warning.
    StreamlinePartialSwap,
    /// One or both versions are unknown and the user must compare manually.
    ManualVersionComparisonRequired,
}

impl OperationPlanWarning {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmationRequiredForSwappability => "confirmation_required_for_swappability",
            Self::StreamlinePartialSwap => "streamline_partial_swap",
            Self::ManualVersionComparisonRequired => "manual_version_comparison_required",
        }
    }

    pub(crate) const fn raises_risk_to_high(self) -> bool {
        matches!(
            self,
            Self::ConfirmationRequiredForSwappability | Self::StreamlinePartialSwap
        )
    }
}
