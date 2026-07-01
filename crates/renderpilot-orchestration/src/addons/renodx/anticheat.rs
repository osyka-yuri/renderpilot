//! Anti-cheat risk assessment for a RenoDX install.
//!
//! The ReShade add-on build RenoDX depends on is frequently flagged by
//! multiplayer anti-cheat and can get players banned, so installing is gated on
//! an explicit warning the user must accept. This module combines the manifest's
//! curated [`Risk`] with a local heuristic — scanning the game folder for known
//! Easy Anti-Cheat / BattlEye artifacts — into a single [`RiskAssessment`].
//!
//! Per the agreed policy, *detecting* anti-cheat never hard-blocks: it escalates
//! the assessment to require explicit confirmation. Only a manifest author can
//! mark a title [`RiskSeverity::Block`] (a deliberate do-not-install decision),
//! which the install flow refuses.

use std::path::Path;

use renderpilot_detection::{
    AntiCheatEngine as DetectedAntiCheatEngine, AntiCheatScanReport, scan_anticheat,
};
use serde::Serialize;

use super::types::{AnticheatEngine, AssessmentConfidence, OnlineKind, Risk, RiskSeverity};

/// i18n key used when a local scan escalates an otherwise-safe title because an
/// anti-cheat was found on disk.
const ANTICHEAT_DETECTED_KEY: &str = "renodx.risk.anticheat_detected";

/// The effective ban/stability risk of installing RenoDX into a game, merging
/// the manifest's curated assessment with a local anti-cheat scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RiskAssessment {
    /// Effective severity (the stricter of manifest and local signals).
    pub severity: RiskSeverity,
    /// Effective anti-cheat engine (a locally-detected engine overrides a
    /// manifest value of `none`/`unknown`).
    pub anticheat_engine: AnticheatEngine,
    /// Online/multiplayer classification from the manifest.
    pub online: OnlineKind,
    /// i18n message key describing the risk to the user.
    pub message_key: String,
    /// Confidence in the assessment (high when corroborated by a local scan).
    pub confidence: AssessmentConfidence,
    /// Optional reference URL for the manifest assessment.
    pub reference_url: Option<String>,
    /// Whether the local scan found an anti-cheat in the game folder.
    pub detected_locally: bool,
}

impl RiskAssessment {
    /// Returns whether the install must be refused outright (a manifest
    /// `block`).
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.severity == RiskSeverity::Block
    }

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

/// Assesses install risk for a game, combining its manifest [`Risk`] with a scan
/// of `game_dir` for anti-cheat artifacts.
#[must_use]
pub fn assess_risk(manifest_risk: &Risk, game_dir: &Path) -> RiskAssessment {
    combine(manifest_risk, &scan_anticheat(game_dir))
}

/// Merges a manifest risk with a generic anti-cheat scan report.
///
/// Pure and independently testable; [`assess_risk`] supplies the detection.
fn combine(manifest: &Risk, scan: &AntiCheatScanReport) -> RiskAssessment {
    let mut severity = manifest.severity;
    let mut engine = manifest.anticheat_engine;
    let mut message_key = manifest.message_key.clone();
    let mut confidence = manifest.confidence;
    let detected = detected_engine(scan);
    let detected_locally = detected.is_some();

    if let Some(found) = detected {
        // A real anti-cheat on disk is a high-confidence signal: escalate an
        // otherwise-safe title to require confirmation, but never past the
        // manifest author's own stronger verdict.
        if severity == RiskSeverity::Info {
            severity = RiskSeverity::Warn;
            message_key = ANTICHEAT_DETECTED_KEY.to_owned();
        }
        if matches!(engine, AnticheatEngine::None | AnticheatEngine::Unknown) {
            engine = found;
        }
        confidence = AssessmentConfidence::High;
    }

    RiskAssessment {
        severity,
        anticheat_engine: engine,
        online: manifest.online,
        message_key,
        confidence,
        reference_url: manifest.source.clone(),
        detected_locally,
    }
}

fn detected_engine(scan: &AntiCheatScanReport) -> Option<AnticheatEngine> {
    if scan
        .engines
        .contains(&DetectedAntiCheatEngine::EasyAntiCheat)
    {
        return Some(AnticheatEngine::Eac);
    }
    if scan.engines.contains(&DetectedAntiCheatEngine::BattlEye) {
        return Some(AnticheatEngine::BattlEye);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::manifest_risk as risk;
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
            &risk(
                RiskSeverity::Info,
                AnticheatEngine::None,
                "renodx.risk.sp_safe",
            ),
            &report(Vec::new()),
        );
        assert!(assessment.is_safe());
        assert!(!assessment.requires_confirmation());
        assert!(!assessment.detected_locally);
        assert_eq!(assessment.message_key, "renodx.risk.sp_safe");
    }

    #[test]
    fn local_detection_escalates_safe_title_to_confirmation() {
        let assessment = combine(
            &risk(
                RiskSeverity::Info,
                AnticheatEngine::None,
                "renodx.risk.sp_safe",
            ),
            &report(vec![DetectedAntiCheatEngine::EasyAntiCheat]),
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.anticheat_engine, AnticheatEngine::Eac);
        assert_eq!(assessment.confidence, AssessmentConfidence::High);
        assert!(assessment.detected_locally);
        assert_eq!(assessment.message_key, ANTICHEAT_DETECTED_KEY);
    }

    #[test]
    fn local_detection_keeps_manifest_warning_message() {
        let assessment = combine(
            &risk(
                RiskSeverity::Warn,
                AnticheatEngine::BattlEye,
                "renodx.risk.pvp_banrisk",
            ),
            &report(vec![DetectedAntiCheatEngine::BattlEye]),
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.message_key, "renodx.risk.pvp_banrisk");
        assert_eq!(assessment.anticheat_engine, AnticheatEngine::BattlEye);
    }

    #[test]
    fn local_detection_prefers_eac_when_multiple_engines_are_found() {
        let assessment = combine(
            &risk(
                RiskSeverity::Info,
                AnticheatEngine::None,
                "renodx.risk.sp_safe",
            ),
            &report(vec![
                DetectedAntiCheatEngine::BattlEye,
                DetectedAntiCheatEngine::EasyAntiCheat,
            ]),
        );

        assert_eq!(assessment.anticheat_engine, AnticheatEngine::Eac);
        assert!(assessment.requires_confirmation());
    }

    #[test]
    fn manifest_block_is_never_downgraded() {
        let assessment = combine(
            &risk(
                RiskSeverity::Block,
                AnticheatEngine::Eac,
                "renodx.risk.donotinstall",
            ),
            &report(Vec::new()),
        );
        assert!(assessment.is_blocked());
        assert!(!assessment.requires_confirmation());
    }

    #[test]
    fn assess_risk_combines_manifest_and_scan() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
        let assessment = assess_risk(
            &risk(
                RiskSeverity::Info,
                AnticheatEngine::None,
                "renodx.risk.sp_safe",
            ),
            dir.path(),
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.anticheat_engine, AnticheatEngine::Eac);
    }
}
