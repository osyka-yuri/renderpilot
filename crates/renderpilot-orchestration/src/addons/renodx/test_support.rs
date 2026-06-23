//! Shared fixtures for RenoDX unit tests.

use renderpilot_domain::Architecture;

use super::types::{
    manifest_defaults, AnticheatEngine, AssessmentConfidence, Category, Channel, Compatibility,
    MatchKind, MatchRule, OnlineKind, RenoDxManifest, ReshadeConfig, ReshadeNightly, Risk,
    RiskSeverity, Status, Title,
};

/// Builds a single match rule.
pub(crate) fn rule(kind: MatchKind, value: &str, tier: u32) -> MatchRule {
    MatchRule {
        kind,
        value: value.to_owned(),
        tier,
    }
}

/// Builds a single-line risk, used by both the matcher and anti-cheat fixtures.
pub(crate) fn manifest_risk(
    severity: RiskSeverity,
    anticheat_engine: AnticheatEngine,
    message_key: &str,
) -> Risk {
    Risk {
        anticheat_engine,
        online: OnlineKind::Singleplayer,
        severity,
        message_key: message_key.to_owned(),
        confidence: AssessmentConfidence::Medium,
        source: None,
    }
}

/// Builds a standard title with the given match rules.
pub(crate) fn title(
    id: &str,
    slug: &str,
    arch: Architecture,
    status: Status,
    match_rules: Vec<MatchRule>,
) -> Title {
    Title {
        id: id.to_owned(),
        name: format!("Game {id}"),
        category: Category::default(),
        slug: slug.to_owned(),
        arch,
        status,
        channel: Channel::default(),
        min_app_version: "1.0.0".to_owned(),
        match_rules,
        compatibility: Compatibility::default(),
        risk: manifest_risk(
            RiskSeverity::Info,
            AnticheatEngine::None,
            "renodx.risk.sp_safe",
        ),
        proxy_dll_override: None,
        notes_keys: Vec::new(),
        download_url: None,
    }
}

/// Builds a manifest over the given titles with a default ReShade config.
pub(crate) fn manifest(titles: Vec<Title>) -> RenoDxManifest {
    RenoDxManifest {
        schema_version: 3,
        generated_at: "2026-06-15T00:00:00Z".to_owned(),
        reshade: ReshadeConfig {
            nightly: ReshadeNightly {
                url64: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(64-bit).zip".to_owned(),
                url32: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(32-bit).zip".to_owned(),
            },
        },
        generics: Vec::new(),
        defaults: manifest_defaults(),
        titles,
    }
}
