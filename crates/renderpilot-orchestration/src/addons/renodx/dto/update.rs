/// Update report DTOs for RenoDX.
use serde::Serialize;

use crate::addons::update::{UpdateStatus, combine};

use super::vulkan::LayerDiagnosticReason;

/// A per-source update report for RenoDX, its ReShade host, and the optional
/// DLSS-Fix companion add-on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenoDxUpdateReport {
    /// Update verdict for the add-on payload. `None` if the source is not tracked (e.g., file install).
    pub addon: Option<UpdateStatus>,
    /// Update verdict for the ReShade host. `None` when no safe host verdict can
    /// be derived for this install.
    pub host: Option<UpdateStatus>,
    /// Update verdict for the DLSS-Fix companion add-on. `None` if not installed.
    pub dlss_fix: Option<UpdateStatus>,
    /// The generic RenoDX verdict. It combines only the main add-on and ReShade
    /// host; DLSS-Fix is independently actionable and never drives this button.
    pub overall: UpdateStatus,
    /// Vulkan-layer digest-mismatch diagnostics, populated only for shared-Vulkan
    /// installs. Empty for proxy installs or when no digest mismatch is detected.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub vulkan_diagnostics: Vec<LayerDiagnosticReason>,
}

impl RenoDxUpdateReport {
    /// Creates a new update report, automatically combining the verdicts into
    /// [`RenoDxUpdateReport::overall`].
    ///
    /// The combine rule is asymmetric by design:
    /// * A missing add-on source (`None`, e.g. a file install) contributes
    ///   [`UpdateStatus::Unknown`] — there is nothing upstream to compare, so
    ///   "is there an update?" is genuinely unknown.
    /// * A missing host verdict (`None`) contributes [`UpdateStatus::Current`] —
    ///   some installs have no resolvable automatic ReShade target, and that must
    ///   not force the add-on verdict to unknown.
    /// * A missing DLSS-Fix source (`None`, e.g. not installed) contributes
    ///   [`UpdateStatus::Current`] — an absent companion must not force the
    ///   overall verdict to unknown.
    #[must_use]
    pub fn new(
        addon: Option<UpdateStatus>,
        host: Option<UpdateStatus>,
        dlss_fix: Option<UpdateStatus>,
    ) -> Self {
        let overall = combine(
            addon.unwrap_or(UpdateStatus::Unknown),
            host.unwrap_or(UpdateStatus::Current),
        );
        Self {
            addon,
            host,
            dlss_fix,
            overall,
            vulkan_diagnostics: Vec::new(),
        }
    }

    /// Creates a new update report with Vulkan-layer digest-mismatch diagnostics.
    /// Used by the shared-Vulkan path to thread `HashMismatch` / `DbOnlyFallback`
    /// reasons into the public report so the UI can show a precise cause instead
    /// of a bare update-available.
    #[must_use]
    pub fn with_vulkan_diagnostics(
        addon: Option<UpdateStatus>,
        host: Option<UpdateStatus>,
        dlss_fix: Option<UpdateStatus>,
        vulkan_diagnostics: Vec<LayerDiagnosticReason>,
    ) -> Self {
        let mut report = Self::new(addon, host, dlss_fix);
        report.vulkan_diagnostics = vulkan_diagnostics;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::update::UpdateStatus::{Available, Current, Unknown};

    #[test]
    fn report_assembly_recorded_host_entry() {
        let report = RenoDxUpdateReport::new(Some(Current), Some(Available), None);
        assert_eq!(report.overall, Available);

        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), None);
        assert_eq!(report.overall, Current);
    }

    #[test]
    fn report_assembly_without_host_entry() {
        let report = RenoDxUpdateReport::new(Some(Current), None, None);
        assert_eq!(report.overall, Current);

        let report = RenoDxUpdateReport::new(Some(Available), None, None);
        assert_eq!(report.overall, Available);
    }

    #[test]
    fn report_assembly_file_install() {
        let report = RenoDxUpdateReport::new(None, Some(Current), None);
        assert_eq!(report.overall, Unknown);
    }

    #[test]
    fn report_assembly_keeps_dlss_fix_out_of_generic_overall() {
        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), Some(Available));
        assert_eq!(report.overall, Current);
    }
}
