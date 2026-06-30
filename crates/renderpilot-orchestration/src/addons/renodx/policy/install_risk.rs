use crate::addons::renodx::types::{
    AnticheatEngine, AssessmentConfidence, OnlineKind, Risk, RiskSeverity,
};

const RISK_GENERIC_KEY: &str = "renodx.risk.generic";

/// A conservative generic risk assessment for unknown titles.
pub fn generic_risk() -> Risk {
    Risk {
        anticheat_engine: AnticheatEngine::None,
        online: OnlineKind::Singleplayer,
        severity: RiskSeverity::Info,
        message_key: RISK_GENERIC_KEY.to_owned(),
        confidence: AssessmentConfidence::Low,
        source: None,
    }
}
