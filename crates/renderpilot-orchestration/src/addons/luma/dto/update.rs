/// Update report DTOs for Luma.
use serde::Serialize;

use crate::addons::update::{UpdateStatus, combine};

/// A per-source update report for Luma's release payload, ReShade host, and
/// owned external dependency.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LumaUpdateReport {
    /// Update verdict for the release asset (the whole ZIP payload). `None` if
    /// nothing is installed.
    pub addon: Option<UpdateStatus>,
    /// Update verdict for the ReShade host. `None` when no safe host verdict can
    /// be derived for this install.
    pub host: Option<UpdateStatus>,
    /// Update verdict for an owned dgVoodoo dependency. `None` for profiles
    /// without one, or when the compatible runtime remains user-reused.
    pub dgvoodoo: Option<UpdateStatus>,
    /// The combined verdict: available if any tracked source changed, current if
    /// all applicable sources are current.
    pub overall: UpdateStatus,
}

impl LumaUpdateReport {
    /// Creates a new update report, automatically combining the verdicts into
    /// [`LumaUpdateReport::overall`].
    ///
    /// A missing addon verdict (`None`, nothing installed) contributes
    /// [`UpdateStatus::Unknown`]; missing host/dgVoodoo verdicts (`None`, not
    /// applicable or no resolvable automatic target) contribute
    /// [`UpdateStatus::Current`] so they never force the addon verdict to unknown.
    #[must_use]
    pub fn new(
        addon: Option<UpdateStatus>,
        host: Option<UpdateStatus>,
        dgvoodoo: Option<UpdateStatus>,
    ) -> Self {
        let overall = combine(
            combine(
                addon.unwrap_or(UpdateStatus::Unknown),
                host.unwrap_or(UpdateStatus::Current),
            ),
            dgvoodoo.unwrap_or(UpdateStatus::Current),
        );
        Self {
            addon,
            host,
            dgvoodoo,
            overall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::update::UpdateStatus::{Available, Current, Unknown};

    #[test]
    fn report_assembly_recorded_host_entry() {
        let report = LumaUpdateReport::new(Some(Current), Some(Available), None);
        assert_eq!(report.overall, Available);

        let report = LumaUpdateReport::new(Some(Current), Some(Current), None);
        assert_eq!(report.overall, Current);
    }

    #[test]
    fn report_assembly_without_a_host_entry() {
        let report = LumaUpdateReport::new(Some(Current), None, None);
        assert_eq!(report.overall, Current);

        let report = LumaUpdateReport::new(Some(Available), None, None);
        assert_eq!(report.overall, Available);
    }

    #[test]
    fn report_assembly_without_anything_installed() {
        let report = LumaUpdateReport::new(None, Some(Current), None);
        assert_eq!(report.overall, Unknown);
    }

    #[test]
    fn report_assembly_with_dgvoodoo_entry() {
        let report = LumaUpdateReport::new(Some(Current), Some(Current), Some(Available));
        assert_eq!(report.overall, Available);
    }
}
