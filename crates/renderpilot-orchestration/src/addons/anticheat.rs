//! Anti-cheat risk assessment for an add-on install.
//!
//! A ReShade add-on hooks into the game process and can be flagged by multiplayer
//! anti-cheat, so installing is gated on an explicit warning the user must accept.
//! This module combines a tool's default risk severity with a local heuristic —
//! scanning the game folder for known Easy Anti-Cheat / BattlEye artifacts — into
//! a single [`RiskAssessment`]. The risk copy is addon-agnostic (every tool
//! surfaces the same `addon.risk.*` message), so tools only ever supply their
//! default [`RiskSeverity`].
//!
//! Detecting anti-cheat never hard-blocks: it escalates the assessment to require
//! explicit confirmation.

use std::path::Path;

use renderpilot_detection::AntiCheatScanReport;
use renderpilot_detection::scan_anticheat;
use serde::Serialize;

/// i18n key for the default single-player-safe risk message.
const DEFAULT_MESSAGE_KEY: &str = "addon.risk.sp_safe";

/// i18n key used when a local scan escalates an otherwise-safe title because an
/// anti-cheat was found on disk.
const ANTICHEAT_DETECTED_KEY: &str = "addon.risk.anticheat_detected";

/// How the installer should act on an add-on risk assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    /// Safe; informational only.
    Info,
    /// Risky; require explicit user confirmation before installing.
    Warn,
}

/// The effective ban/stability risk of installing an add-on into a game, merging
/// the tool default with a local anti-cheat scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RiskAssessment {
    /// Effective severity.
    pub severity: RiskSeverity,
    /// i18n message key describing the risk to the user.
    pub message_key: String,
}

impl RiskAssessment {
    /// Returns whether the install requires explicit user confirmation before
    /// proceeding (a `warn`).
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        self.severity == RiskSeverity::Warn
    }

    /// Returns whether the install is safe to proceed without a prompt.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        self.severity == RiskSeverity::Info
    }
}

/// Assesses install risk for a game: combines the tool's default risk severity
/// with a scan of `game_dir` for anti-cheat artifacts. The risk copy is
/// addon-agnostic, so every tool gets the same message keys.
#[must_use]
pub fn assess_risk(game_dir: &Path, default_severity: RiskSeverity) -> RiskAssessment {
    combine(
        &scan_anticheat(game_dir),
        default_severity,
        DEFAULT_MESSAGE_KEY,
        ANTICHEAT_DETECTED_KEY,
    )
}

/// Merges a default risk with a generic anti-cheat scan report.
///
/// Pure and independently testable; [`assess_risk`] supplies the detection and
/// the addon-agnostic message keys.
fn combine(
    scan: &AntiCheatScanReport,
    default_severity: RiskSeverity,
    default_message_key: &str,
    anticheat_detected_key: &str,
) -> RiskAssessment {
    let mut severity = default_severity;
    let mut message_key = default_message_key.to_owned();

    if scan_found_anticheat(scan) {
        // A real anti-cheat on disk escalates an otherwise-safe title to require
        // confirmation, but never changes an existing warning's editorial copy.
        if severity == RiskSeverity::Info {
            severity = RiskSeverity::Warn;
            message_key = anticheat_detected_key.to_owned();
        }
    }

    RiskAssessment {
        severity,
        message_key,
    }
}

/// Outcome of the risk gate for an install command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallGate {
    Proceed,
    NeedsConfirmation,
}

/// Decides whether to proceed or require confirmation based on the risk assessment
/// and whether the user has confirmed.
#[must_use]
pub fn decide_gate(risk: &RiskAssessment, confirmed: bool) -> InstallGate {
    if risk.requires_confirmation() && !confirmed {
        InstallGate::NeedsConfirmation
    } else {
        InstallGate::Proceed
    }
}

/// Enforces the gate: returns Ok only if we can proceed (either safe or confirmed).
pub fn enforce_gate(risk: &RiskAssessment, confirmed: bool) -> Result<(), crate::ServiceError> {
    match decide_gate(risk, confirmed) {
        InstallGate::Proceed => Ok(()),
        InstallGate::NeedsConfirmation => Err(crate::addons::errors::invalid(
            "install requires explicit confirmation of the anti-cheat ban risk".to_owned(),
        )),
    }
}

fn scan_found_anticheat(scan: &AntiCheatScanReport) -> bool {
    !scan.engines.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_detection::AntiCheatEngine as DetectedAntiCheatEngine;
    use renderpilot_detection::AntiCheatEvidence;
    use renderpilot_detection::AntiCheatEvidenceKind;
    use tempfile::tempdir;

    fn report(engines: Vec<DetectedAntiCheatEngine>) -> AntiCheatScanReport {
        AntiCheatScanReport {
            evidence: engines
                .iter()
                .map(|engine| AntiCheatEvidence {
                    engine: *engine,
                    matched_marker: "marker".to_owned(),
                    path: std::path::PathBuf::from("C:\\Games\\Game\\marker"),
                    kind: AntiCheatEvidenceKind::File,
                })
                .collect(),
            engines,
            scanned_entry_count: 1,
            truncated: false,
        }
    }

    #[test]
    fn safe_title_without_local_detection_needs_no_confirmation() {
        let assessment = combine(
            &report(Vec::new()),
            RiskSeverity::Info,
            DEFAULT_MESSAGE_KEY,
            ANTICHEAT_DETECTED_KEY,
        );
        assert!(assessment.is_safe());
        assert!(!assessment.requires_confirmation());
        assert_eq!(assessment.message_key, DEFAULT_MESSAGE_KEY);
    }

    #[test]
    fn local_detection_escalates_safe_title_to_confirmation() {
        let assessment = combine(
            &report(vec![DetectedAntiCheatEngine::EasyAntiCheat]),
            RiskSeverity::Info,
            DEFAULT_MESSAGE_KEY,
            ANTICHEAT_DETECTED_KEY,
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.message_key, ANTICHEAT_DETECTED_KEY);
    }

    #[test]
    fn local_detection_keeps_existing_warning_message() {
        let assessment = combine(
            &report(vec![DetectedAntiCheatEngine::BattlEye]),
            RiskSeverity::Warn,
            "addon.risk.warn",
            ANTICHEAT_DETECTED_KEY,
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.message_key, "addon.risk.warn");
    }

    #[test]
    fn assess_risk_combines_default_and_scan() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
        let assessment = assess_risk(dir.path(), RiskSeverity::Info);
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.message_key, ANTICHEAT_DETECTED_KEY);
    }
}
