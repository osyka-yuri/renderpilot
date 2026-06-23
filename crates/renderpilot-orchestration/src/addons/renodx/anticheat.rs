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

use std::collections::VecDeque;
use std::fs;
use std::path::Path;

use serde::Serialize;

use super::types::{AnticheatEngine, AssessmentConfidence, OnlineKind, Risk, RiskSeverity};

/// i18n key used when a local scan escalates an otherwise-safe title because an
/// anti-cheat was found on disk.
const ANTICHEAT_DETECTED_KEY: &str = "renodx.risk.anticheat_detected";

/// Upper bound on filesystem entries walked while scanning for anti-cheat
/// markers, so a pathological game tree cannot stall the scan.
const MAX_SCANNED_ENTRIES: usize = 8192;

/// File and directory names (lowercased) that indicate Easy Anti-Cheat.
const EAC_MARKERS: &[&str] = &[
    "easyanticheat",
    "easyanticheat_eos",
    "easyanticheat_x64.dll",
    "easyanticheat_x86.dll",
    "eaclauncher.exe",
    "start_protected_game.exe",
];

/// File and directory names (lowercased) that indicate BattlEye.
const BATTLEYE_MARKERS: &[&str] = &[
    "battleye",
    "beservice.exe",
    "beservice_x64.dll",
    "beclient_x64.dll",
    "beclient_x86.dll",
];

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
    /// Optional provenance of the manifest assessment.
    pub source: Option<String>,
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
    combine(manifest_risk, detect_local_anticheat(game_dir))
}

/// Merges a manifest risk with an optionally-detected local anti-cheat engine.
///
/// Pure and independently testable; [`assess_risk`] supplies the detection.
fn combine(manifest: &Risk, detected: Option<AnticheatEngine>) -> RiskAssessment {
    let mut severity = manifest.severity;
    let mut engine = manifest.anticheat_engine;
    let mut message_key = manifest.message_key.clone();
    let mut confidence = manifest.confidence;
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
        source: manifest.source.clone(),
        detected_locally,
    }
}

/// Scans the game folder for known anti-cheat artifacts, returning the engine
/// found (Easy Anti-Cheat takes precedence when both are present).
///
/// Walks the directory tree breadth-first, bounded by [`MAX_SCANNED_ENTRIES`].
fn detect_local_anticheat(game_dir: &Path) -> Option<AnticheatEngine> {
    let mut queue = VecDeque::new();
    queue.push_back(game_dir.to_path_buf());
    let mut scanned = 0usize;

    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned > MAX_SCANNED_ENTRIES {
                return None;
            }

            // Avoid following directory junctions/symlinks out of the game tree.
            let is_symlink = entry
                .path()
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.is_symlink());
            if is_symlink {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if EAC_MARKERS.contains(&name.as_str()) {
                return Some(AnticheatEngine::Eac);
            }
            if BATTLEYE_MARKERS.contains(&name.as_str()) {
                return Some(AnticheatEngine::BattlEye);
            }
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                queue.push_back(entry.path());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::manifest_risk as risk;
    use tempfile::tempdir;

    #[test]
    fn safe_title_without_local_detection_needs_no_confirmation() {
        let assessment = combine(
            &risk(
                RiskSeverity::Info,
                AnticheatEngine::None,
                "renodx.risk.sp_safe",
            ),
            None,
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
            Some(AnticheatEngine::Eac),
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
            Some(AnticheatEngine::BattlEye),
        );
        assert!(assessment.requires_confirmation());
        assert_eq!(assessment.message_key, "renodx.risk.pvp_banrisk");
        assert_eq!(assessment.anticheat_engine, AnticheatEngine::BattlEye);
    }

    #[test]
    fn manifest_block_is_never_downgraded() {
        let assessment = combine(
            &risk(
                RiskSeverity::Block,
                AnticheatEngine::Eac,
                "renodx.risk.donotinstall",
            ),
            None,
        );
        assert!(assessment.is_blocked());
        assert!(!assessment.requires_confirmation());
    }

    #[test]
    fn detects_easy_anticheat_directory() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
        assert_eq!(
            detect_local_anticheat(dir.path()),
            Some(AnticheatEngine::Eac)
        );
    }

    #[test]
    fn detects_battleye_service_in_subfolder() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("bin");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("BEService_x64.dll"), b"stub").expect("write");
        assert_eq!(
            detect_local_anticheat(dir.path()),
            Some(AnticheatEngine::BattlEye)
        );
    }

    #[test]
    fn detects_nothing_in_clean_folder() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"stub").expect("write");
        assert_eq!(detect_local_anticheat(dir.path()), None);
    }

    #[test]
    fn assess_risk_combines_manifest_and_scan() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
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
