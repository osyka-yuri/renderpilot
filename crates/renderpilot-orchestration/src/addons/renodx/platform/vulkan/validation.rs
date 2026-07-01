//! Invariants for shared ReShade Vulkan layer update validation.
//!
//! Enforces the priority: actual file digest > SQLite advisory record.

use super::report::conflict_is_standard_mutable;
use crate::addons::renodx::dto::vulkan::{
    LayerDiagnosticReason, VulkanLayerDetection, VulkanLayerReport,
};
use crate::addons::update::UpdateStatus;

/// Whether a mutating Vulkan layer operation (install/update) may proceed,
/// given the current detection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerMutationGate {
    /// The detection state permits the operation to proceed.
    Proceed,
    /// A non-standard layer is visible read-only; the caller must not modify it.
    ///
    /// Deliberately left caller-owned rather than folded into a shared
    /// `errors::` constructor like the other two rejections: the install and
    /// update call sites already used two independently-worded sentences for
    /// this state before this gate existed ("a non-standard ReShade Vulkan
    /// layer is already registered…" vs. "the visible Vulkan ReShade layer is
    /// external…"), and neither wording is operation-specific the way
    /// [`UnresolvedConflict`](Self::UnresolvedConflict)'s is — unifying them
    /// would mean picking one and changing the other's user-facing text for
    /// no functional reason. See `errors::vulkan_layer_conflict` for the
    /// sibling case that *was* just a parametrizable verb and got unified.
    ExternalReadOnly,
    /// The conflict isn't the standard, safely-resolvable shape. Callers
    /// report this via `errors::vulkan_layer_conflict(operation)`.
    UnresolvedConflict,
    /// Vulkan layer management isn't supported in this environment. Callers
    /// report this via `errors::vulkan_unsupported_platform()`.
    Unsupported,
}

/// Classifies whether `report`'s detection state permits a mutating install/update
/// operation to proceed. `NotInstalled`, `Installed`, `InstalledDisabled`, and a
/// standard/mutable `Conflict` all proceed; `ExternalReadOnly`, an unresolved
/// `Conflict`, and `Unsupported` do not — see each variant's docs for how its
/// rejection is reported.
pub(crate) fn layer_mutation_gate(report: &VulkanLayerReport) -> LayerMutationGate {
    match report.detection() {
        VulkanLayerDetection::ExternalReadOnly => LayerMutationGate::ExternalReadOnly,
        VulkanLayerDetection::Conflict if !conflict_is_standard_mutable(report) => {
            LayerMutationGate::UnresolvedConflict
        }
        VulkanLayerDetection::Unsupported => LayerMutationGate::Unsupported,
        _ => LayerMutationGate::Proceed,
    }
}

/// Update verdict for the shared ReShade Vulkan layer.
pub(crate) struct LayerUpdateVerdict {
    /// Whether the layer is current, needs an update, or needs stronger validation.
    pub(crate) status: UpdateStatus,
    /// Closed diagnostics explaining non-current verdicts.
    pub(crate) diagnostics: Vec<LayerDiagnosticReason>,
}

/// Pure digest-comparison logic. The actual on-disk DLL digest is authoritative;
/// the DB digest is advisory fallback only and never produces a strong
/// [`UpdateStatus::Current`].
///
/// * `actual_digest` - SHA-256 of the on-disk `ReShade64.dll`, or `None` if
///   missing/unreadable.
/// * `db_digest` - advisory provenance digest from SQLite, consulted only when
///   the DLL is missing/unreadable.
/// * `expected_digest` - the upstream artifact digest.
pub(crate) fn resolve_digest_verdict(
    actual_digest: Option<&str>,
    db_digest: Option<&str>,
    expected_digest: &str,
) -> LayerUpdateVerdict {
    if let Some(actual) = actual_digest {
        if actual == expected_digest {
            return LayerUpdateVerdict {
                status: UpdateStatus::Current,
                diagnostics: Vec::new(),
            };
        }
        log::info!("Vulkan layer hash mismatch: actual={actual}, expected={expected_digest}");
        return LayerUpdateVerdict {
            status: UpdateStatus::Available,
            diagnostics: vec![LayerDiagnosticReason::HashMismatch],
        };
    }

    if let Some(db) = db_digest {
        if db == expected_digest {
            log::info!(
                "Vulkan layer DB-only fallback: advisory digest matches upstream \
                 but DLL is missing/unreadable; degrading to needs-validation"
            );
            return LayerUpdateVerdict {
                status: UpdateStatus::UnknownNeedsValidation,
                diagnostics: vec![LayerDiagnosticReason::DbOnlyFallback],
            };
        }
        log::info!("Vulkan layer DB-only fallback: db={db}, expected={expected_digest}");
        return LayerUpdateVerdict {
            status: UpdateStatus::Available,
            diagnostics: vec![LayerDiagnosticReason::DbOnlyFallback],
        };
    }

    LayerUpdateVerdict {
        status: UpdateStatus::Available,
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::dto::vulkan::{
        VulkanLayerArchitecture, VulkanLayerFacts, VulkanLoaderVisibility,
    };

    fn report_with(
        detection: VulkanLayerDetection,
        diagnostic_reasons: Vec<LayerDiagnosticReason>,
    ) -> VulkanLayerReport {
        VulkanLayerReport {
            layer_detection: detection,
            layer_facts: VulkanLayerFacts {
                manifest_path: None,
                dll_path: None,
                version: None,
                architecture: VulkanLayerArchitecture::Unknown,
                loader_visibility: VulkanLoaderVisibility::Normal,
            },
            diagnostic_reasons,
            actions: crate::addons::renodx::dto::vulkan::VulkanLayerActions {
                install: None,
                update: None,
                switch_channel: None,
                remove: None,
                resolve_conflict: None,
            },
        }
    }

    #[test]
    fn gate_proceeds_for_not_installed_installed_and_installed_disabled() {
        for detection in [
            VulkanLayerDetection::NotInstalled,
            VulkanLayerDetection::Installed,
            VulkanLayerDetection::InstalledDisabled,
        ] {
            assert_eq!(
                layer_mutation_gate(&report_with(detection, Vec::new())),
                LayerMutationGate::Proceed
            );
        }
    }

    #[test]
    fn gate_proceeds_for_standard_mutable_conflict() {
        let report = report_with(
            VulkanLayerDetection::Conflict,
            vec![LayerDiagnosticReason::RegistryDisabled],
        );
        assert_eq!(layer_mutation_gate(&report), LayerMutationGate::Proceed);
    }

    #[test]
    fn gate_rejects_unresolved_conflict() {
        let report = report_with(
            VulkanLayerDetection::Conflict,
            vec![LayerDiagnosticReason::DuplicateLayerManifest],
        );
        assert_eq!(
            layer_mutation_gate(&report),
            LayerMutationGate::UnresolvedConflict
        );
    }

    #[test]
    fn gate_rejects_external_read_only_and_unsupported() {
        assert_eq!(
            layer_mutation_gate(&report_with(
                VulkanLayerDetection::ExternalReadOnly,
                Vec::new()
            )),
            LayerMutationGate::ExternalReadOnly
        );
        assert_eq!(
            layer_mutation_gate(&report_with(VulkanLayerDetection::Unsupported, Vec::new())),
            LayerMutationGate::Unsupported
        );
    }

    #[test]
    fn actual_dll_digest_wins_over_stale_db_digest() {
        let expected = "abc123";
        let verdict =
            resolve_digest_verdict(Some(expected), Some("stale-digest-from-db"), expected);
        assert_eq!(verdict.status, UpdateStatus::Current);
        assert!(verdict.diagnostics.is_empty());
    }

    #[test]
    fn db_only_fallback_does_not_return_strong_current() {
        let expected = "abc123";
        let verdict = resolve_digest_verdict(None, Some(expected), expected);
        assert_eq!(verdict.status, UpdateStatus::UnknownNeedsValidation);
        assert_eq!(
            verdict.diagnostics,
            vec![LayerDiagnosticReason::DbOnlyFallback]
        );
    }

    #[test]
    fn hash_mismatch_returns_update_available() {
        let verdict =
            resolve_digest_verdict(Some("actual-digest"), Some("db-digest"), "expected-digest");
        assert_eq!(verdict.status, UpdateStatus::Available);
        assert_eq!(
            verdict.diagnostics,
            vec![LayerDiagnosticReason::HashMismatch]
        );
    }

    #[test]
    fn db_only_mismatch_returns_available() {
        let verdict = resolve_digest_verdict(None, Some("old-db-digest"), "expected-digest");
        assert_eq!(verdict.status, UpdateStatus::Available);
        assert_eq!(
            verdict.diagnostics,
            vec![LayerDiagnosticReason::DbOnlyFallback]
        );
    }

    #[test]
    fn no_dll_no_db_returns_available() {
        let verdict = resolve_digest_verdict(None, None, "expected-digest");
        assert_eq!(verdict.status, UpdateStatus::Available);
        assert!(verdict.diagnostics.is_empty());
    }
}
