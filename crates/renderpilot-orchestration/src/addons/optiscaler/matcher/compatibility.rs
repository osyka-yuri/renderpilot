//! Compatibility knowledge and detected-input evidence.

use std::path::Path;

use renderpilot_domain::OptiScalerPrerequisiteBinding;

use super::super::compatibility_catalog::{
    CatalogGuidance, CompatibilityPrerequisite, CompatibilityStatus, DeclaredInput,
    LaunchRequirement, ResolvedCompatibility, ResolvedVariant,
};
use super::super::types::{
    OptiScalerCompatibilityBlockCode, OptiScalerCompatibilityGuidance,
    OptiScalerCompatibilityStatus, OptiScalerDeclaredInput, OptiScalerEvidence, OptiScalerLaunch,
    OptiScalerLaunchRequirement,
};
use super::{GraphicsComponent, GraphicsTechnology};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvaluationMode {
    ManagedTarget,
    Candidate,
}

pub(super) struct CompatibilityEvaluation<'a> {
    pub(super) evidence: Vec<OptiScalerEvidence>,
    pub(super) status: OptiScalerCompatibilityStatus,
    pub(super) guidance: Vec<OptiScalerCompatibilityGuidance>,
    pub(super) declared_inputs: Vec<OptiScalerDeclaredInput>,
    pub(super) launch: Option<OptiScalerLaunch>,
    pub(super) policy: Option<&'a ResolvedVariant>,
    pub(super) accepted_prerequisite_binding: OptiScalerPrerequisiteBinding,
    pub(super) block_code: Option<OptiScalerCompatibilityBlockCode>,
    pub(super) blocked_reason: Option<String>,
}

pub(super) fn evaluate<'a>(
    components: &[GraphicsComponent],
    resolved: ResolvedCompatibility<'a>,
    release_available: bool,
    mode: EvaluationMode,
) -> CompatibilityEvaluation<'a> {
    let evidence = evidence_from_components(components);
    let (
        status,
        guidance,
        declared_inputs,
        launch,
        policy,
        accepted_prerequisite_binding,
        identity_conflict,
    ) = match resolved {
        ResolvedCompatibility::NoMatch => (
            OptiScalerCompatibilityStatus::Untested,
            Vec::new(),
            Vec::new(),
            None,
            None,
            OptiScalerPrerequisiteBinding::None,
            false,
        ),
        ResolvedCompatibility::Conflict => (
            OptiScalerCompatibilityStatus::Untested,
            Vec::new(),
            Vec::new(),
            None,
            None,
            OptiScalerPrerequisiteBinding::None,
            true,
        ),
        ResolvedCompatibility::Match { entry, variant } => (
            match entry.status {
                CompatibilityStatus::Working => OptiScalerCompatibilityStatus::Working,
                CompatibilityStatus::Conditional => OptiScalerCompatibilityStatus::Conditional,
                CompatibilityStatus::Unsupported => OptiScalerCompatibilityStatus::Unsupported,
            },
            guidance_from(&entry.guidance),
            entry
                .declared_inputs
                .iter()
                .copied()
                .map(declared_input_from)
                .collect(),
            variant.launch.as_ref().map(launch_from),
            Some(variant),
            match variant.prerequisite {
                CompatibilityPrerequisite::None => OptiScalerPrerequisiteBinding::None,
                CompatibilityPrerequisite::Luma => OptiScalerPrerequisiteBinding::Luma,
            },
            false,
        ),
    };
    let candidate_has_evidence = !evidence.is_empty();
    let (block_code, blocked_reason) = if !release_available {
        (
            Some(OptiScalerCompatibilityBlockCode::ReleaseUnavailable),
            Some("the stable release catalogue has no current release".to_owned()),
        )
    } else if mode == EvaluationMode::Candidate && identity_conflict {
        (
            Some(OptiScalerCompatibilityBlockCode::CatalogIdentityConflict),
            Some("the compatibility catalogue has conflicting exact identities".to_owned()),
        )
    } else if mode == EvaluationMode::Candidate
        && status == OptiScalerCompatibilityStatus::Unsupported
    {
        (
            Some(OptiScalerCompatibilityBlockCode::CatalogUnsupported),
            Some("the compatibility catalogue marks this game unsupported".to_owned()),
        )
    } else if mode == EvaluationMode::Candidate
        && status == OptiScalerCompatibilityStatus::Untested
        && !candidate_has_evidence
    {
        (
            Some(OptiScalerCompatibilityBlockCode::InputNotDetected),
            Some("no DLSS2+, FSR2+, or XeSS input was detected".to_owned()),
        )
    } else {
        (None, None)
    };
    CompatibilityEvaluation {
        evidence,
        status,
        guidance,
        declared_inputs,
        launch,
        policy,
        accepted_prerequisite_binding,
        block_code,
        blocked_reason,
    }
}

const fn declared_input_from(input: DeclaredInput) -> OptiScalerDeclaredInput {
    match input {
        DeclaredInput::Dlss2Plus => OptiScalerDeclaredInput::Dlss2Plus,
        DeclaredInput::Fsr2Plus => OptiScalerDeclaredInput::Fsr2Plus,
        DeclaredInput::Xess => OptiScalerDeclaredInput::Xess,
    }
}

fn launch_from(policy: &super::super::compatibility_catalog::WireLaunchPolicy) -> OptiScalerLaunch {
    OptiScalerLaunch {
        arguments: policy.arguments.clone(),
        requirement: match policy.requirement {
            LaunchRequirement::Required => OptiScalerLaunchRequirement::Required,
            LaunchRequirement::Recommended => OptiScalerLaunchRequirement::Recommended,
        },
    }
}

fn guidance_from(values: &[CatalogGuidance]) -> Vec<OptiScalerCompatibilityGuidance> {
    values
        .iter()
        .map(|guidance| OptiScalerCompatibilityGuidance {
            kind: guidance.kind.as_str().to_owned(),
            message: guidance.message.clone(),
        })
        .collect()
}

pub(super) fn evidence_from_components(
    components: &[renderpilot_domain::LibraryComponent],
) -> Vec<OptiScalerEvidence> {
    components
        .iter()
        .filter(|component| is_input_technology(component.technology()))
        .map(|component| OptiScalerEvidence {
            technology: component.technology(),
            paths: component
                .files()
                .iter()
                .map(|file| file.path().as_str().to_owned())
                .collect(),
        })
        .collect()
}

pub(super) const fn is_input_technology(technology: GraphicsTechnology) -> bool {
    matches!(
        technology,
        GraphicsTechnology::DlssSuperResolution
            | GraphicsTechnology::IntelXeSs
            | GraphicsTechnology::AmdFsr
            | GraphicsTechnology::AmdFsrUpscaler
    )
}

pub(super) fn proxy_conflict_message(path: &Path, slot_name: &str) -> String {
    let identity = renderpilot_detection::inspect_pe(path)
        .map(|inspection| inspection.identity)
        .map(|identity| {
            [identity.product_name, identity.file_description]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    let implementation = if identity.contains("special k") || identity.contains("specialk") {
        "Special K"
    } else if identity.contains("ultimate asi") {
        "Ultimate ASI Loader"
    } else if identity.contains("dlss enabler") {
        "DLSS Enabler"
    } else {
        "unknown external proxy"
    };
    format!(
        "{implementation} occupies {slot_name}; no safe automatic OptiScaler topology is available"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::matching::MatchFacts;
    use crate::addons::optiscaler::compatibility_catalog::OptiScalerCompatibilityCatalog;
    use crate::addons::optiscaler::types::OptiScalerCompatibilityStatus;
    use renderpilot_domain::{
        Architecture, ComponentId, ComponentKind, ExeGraphicsInfo, GameId, GraphicsApi, Launcher,
        LibraryComponent, LibraryTechnology, Swappability,
    };

    fn valid_test_guidance() -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "kind": "warning",
            "message": {
                "id": "optiscaler-test-warning",
                "fallback_text": "Test warning fallback text"
            }
        })]
    }

    fn unsupported_catalog() -> OptiScalerCompatibilityCatalog {
        let value = serde_json::json!({
            "schema_version": 1,
            "revision": "2026-01-01.1",
            "upstream": {
                "source": "test-fixture",
                "snapshot_revision": "2026-01-01",
                "snapshot_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            },
            "entries": [{
                "id": "f2-unsupported",
                "status": "unsupported",
                "identities": [{ "kind": "steam_appid", "value": "999999991" }],
                "declared_inputs": [],
                "guidance": valid_test_guidance(),
                "variants": [{
                    "proxy": { "kind": "automatic" },
                    "ini_overrides": [],
                    "restricted_modules": [],
                    "optipatcher": "unspecified",
                    "prerequisite": "none"
                }]
            }]
        });
        let bytes = serde_json::to_vec(&value).expect("serialize catalog");
        crate::addons::optiscaler::compatibility_catalog::parse_catalog(&bytes)
            .expect("unsupported catalog remains valid")
    }

    fn conflicting_catalog() -> OptiScalerCompatibilityCatalog {
        let entry = serde_json::json!({
            "id": "conflict-entry-a",
            "status": "working",
            "identities": [{ "kind": "steam_appid", "value": "999999991" }],
            "declared_inputs": [],
            "guidance": valid_test_guidance(),
            "variants": [{
                "proxy": { "kind": "automatic" },
                "ini_overrides": [],
                "restricted_modules": [],
                "optipatcher": "unspecified",
                "prerequisite": "none"
            }]
        });
        let value = serde_json::json!({
            "schema_version": 1,
            "revision": "2026-01-01.1",
            "upstream": {
                "source": "test-fixture",
                "snapshot_revision": "2026-01-01",
                "snapshot_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            },
            "entries": [
                entry,
                {
                    "id": "conflict-entry-b",
                    "status": "working",
                    "identities": [{ "kind": "exe_name", "value": "Game.exe" }],
                    "declared_inputs": [],
                    "guidance": [],
                    "variants": [{
                        "proxy": { "kind": "automatic" },
                        "ini_overrides": [],
                        "restricted_modules": [],
                        "optipatcher": "unspecified",
                        "prerequisite": "none"
                    }]
                }
            ]
        });
        let bytes = serde_json::to_vec(&value).expect("serialize conflict catalog");
        crate::addons::optiscaler::compatibility_catalog::parse_catalog(&bytes)
            .expect("conflict catalog remains valid")
    }

    fn facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("999999991".to_owned()),
            exe_file_name: Some("Game.exe".to_owned()),
            engine: None,
            unreal_version: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64)),
            unreal_detection: None,
            target_platform: None,
        }
    }

    fn input(technology: LibraryTechnology, suffix: &str) -> LibraryComponent {
        LibraryComponent::new(
            ComponentId::new(format!("component:optiscaler-f2:{suffix}")).expect("component id"),
            GameId::new("steam:999999991").expect("game id"),
            ComponentKind::NativeLibrary,
            technology,
            Swappability::Swappable,
        )
    }

    fn bundled_catalog() -> OptiScalerCompatibilityCatalog {
        crate::addons::optiscaler::compatibility_catalog::parse_catalog(include_bytes!(
            "../../../../assets/optiscaler-compatibility-fallback.json"
        ))
        .expect("bundled catalog")
    }

    fn bundled_facts(app_id: &str) -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some(app_id.to_owned()),
            exe_file_name: None,
            engine: None,
            unreal_version: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64)),
            unreal_detection: None,
            target_platform: None,
        }
    }

    #[test]
    fn selected_launch_policy_exposes_arguments_and_requirement_without_guidance_mixing() {
        let catalog = bundled_catalog();
        let facts = bundled_facts("1139900");
        let evaluation = evaluate(
            &[],
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );

        assert!(evaluation.guidance.is_empty());
        let launch = evaluation.launch.expect("Ghostrunner launch policy");
        assert_eq!(launch.arguments, ["-dx12"]);
        assert_eq!(
            launch.requirement,
            crate::addons::optiscaler::types::OptiScalerLaunchRequirement::Required
        );
    }

    #[test]
    fn declared_inputs_are_exact_ordered_catalog_data_and_never_leak_from_no_match_or_conflict() {
        let catalog = bundled_catalog();
        let facts = bundled_facts("1091500");
        let matched = evaluate(
            &[],
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );
        assert_eq!(
            matched.declared_inputs,
            vec![
                OptiScalerDeclaredInput::Dlss2Plus,
                OptiScalerDeclaredInput::Fsr2Plus,
                OptiScalerDeclaredInput::Xess,
            ]
        );

        for resolution in [
            ResolvedCompatibility::NoMatch,
            ResolvedCompatibility::Conflict,
        ] {
            let evaluation = evaluate(&[], resolution, true, EvaluationMode::Candidate);
            assert!(evaluation.declared_inputs.is_empty());
            assert_eq!(
                evaluation.accepted_prerequisite_binding,
                OptiScalerPrerequisiteBinding::None
            );
        }
    }

    #[test]
    fn exact_luma_prerequisite_is_mapped_to_the_persisted_binding() {
        let catalog = bundled_catalog();
        let facts = MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("1549970".to_owned()),
            exe_file_name: Some("Endeavor.exe".to_owned()),
            engine: None,
            unreal_version: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64)),
            unreal_detection: None,
            target_platform: None,
        };
        let evaluation = evaluate(
            &[],
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );
        assert_eq!(
            evaluation.accepted_prerequisite_binding,
            OptiScalerPrerequisiteBinding::Luma
        );
    }

    #[test]
    fn bundled_launch_policy_preserves_recommendation_and_condition_only_guidance() {
        let catalog = bundled_catalog();
        let facts = bundled_facts("1164940");
        let evaluation = evaluate(
            &[],
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );
        assert_eq!(
            evaluation
                .launch
                .expect("Trepang2 launch policy")
                .requirement,
            crate::addons::optiscaler::types::OptiScalerLaunchRequirement::Recommended
        );
        assert_eq!(evaluation.guidance.len(), 1);

        let condition_only = bundled_facts("2795230");
        let resolved =
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &condition_only);
        let crate::addons::optiscaler::compatibility_catalog::ResolvedCompatibility::Match {
            entry,
            variant,
        } = resolved
        else {
            panic!("ODDRoom must resolve exactly");
        };
        assert_eq!(entry.guidance.len(), 1);
        assert!(variant.launch.is_none());
    }

    #[test]
    fn catalog_rejects_empty_duplicate_and_unsafe_launch_arguments() {
        for arguments in [
            serde_json::json!([]),
            serde_json::json!(["-dx12", "-dx12"]),
            serde_json::json!(["-dx12;unsafe"]),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
                "../../../../assets/optiscaler-compatibility-fallback.json"
            ))
            .expect("catalog JSON");
            let entry = value["entries"]
                .as_array_mut()
                .expect("entries")
                .iter_mut()
                .find(|entry| entry["id"] == "ghostrunner")
                .expect("Ghostrunner entry");
            entry["variants"][0]["launch"]["arguments"] = arguments;
            let bytes = serde_json::to_vec(&value).expect("serialize invalid catalog");
            assert!(
                crate::addons::optiscaler::compatibility_catalog::parse_catalog(&bytes).is_err(),
                "unsafe launch policy must fail closed"
            );
        }
    }

    #[test]
    fn unsupported_exact_rule_stays_visible_but_blocks_install() {
        let catalog = unsupported_catalog();
        let facts = facts();

        for (technology, suffix) in [
            (LibraryTechnology::DlssSuperResolution, "dlss"),
            (LibraryTechnology::AmdFsr, "fsr"),
            (LibraryTechnology::IntelXeSs, "xess"),
        ] {
            let component = input(technology, suffix);
            assert!(crate::addons::optiscaler::matcher::capability_available(
                &catalog,
                &facts,
                std::slice::from_ref(&component)
            ));

            let resolved =
                crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts);
            let evaluation = evaluate(
                std::slice::from_ref(&component),
                resolved,
                true,
                EvaluationMode::Candidate,
            );
            assert_eq!(
                evaluation.status,
                OptiScalerCompatibilityStatus::Unsupported
            );
            assert_eq!(
                evaluation.block_code,
                Some(crate::addons::optiscaler::types::OptiScalerCompatibilityBlockCode::CatalogUnsupported)
            );
            assert!(evaluation.blocked_reason.is_some());
        }

        assert!(crate::addons::optiscaler::matcher::capability_available(
            &catalog,
            &facts,
            &[]
        ));
    }

    #[test]
    fn conflicting_catalog_is_visible_but_hard_blocks_candidate_install() {
        let catalog = conflicting_catalog();
        let facts = facts();
        let resolved = crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts);

        assert!(matches!(resolved, ResolvedCompatibility::Conflict));
        assert!(crate::addons::optiscaler::matcher::capability_available(
            &catalog,
            &facts,
            &[]
        ));

        let evaluation = evaluate(&[], resolved, true, EvaluationMode::Candidate);
        assert_eq!(
            evaluation.block_code,
            Some(OptiScalerCompatibilityBlockCode::CatalogIdentityConflict)
        );
        assert!(evaluation.blocked_reason.is_some());
    }

    #[test]
    fn unknown_input_is_installable_when_supported_input_is_detected() {
        let catalog = bundled_catalog();
        let mut facts = facts();
        facts.external_id = Some("unknown-game".to_owned());
        let component = input(LibraryTechnology::AmdFsr, "fsr");
        let evaluation = evaluate(
            std::slice::from_ref(&component),
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );

        assert_eq!(evaluation.status, OptiScalerCompatibilityStatus::Untested);
        assert_eq!(evaluation.block_code, None);
        assert_eq!(evaluation.blocked_reason, None);
    }

    #[test]
    fn unknown_without_input_remains_hard_blocked() {
        let catalog = bundled_catalog();
        let mut facts = facts();
        facts.external_id = Some("unknown-game".to_owned());
        facts.graphics = ExeGraphicsInfo::new(vec![], None);
        let evaluation = evaluate(
            &[],
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &facts),
            true,
            EvaluationMode::Candidate,
        );

        assert_eq!(evaluation.status, OptiScalerCompatibilityStatus::Untested);
        assert!(!crate::addons::optiscaler::matcher::capability_available(
            &catalog,
            &facts,
            &[]
        ));
        assert_eq!(
            evaluation.block_code,
            Some(OptiScalerCompatibilityBlockCode::InputNotDetected)
        );
        assert!(evaluation.blocked_reason.is_some());
    }

    #[test]
    fn visibility_uses_catalog_or_input_and_ignores_pe_facts() {
        let catalog = bundled_catalog();
        let mut unknown = facts();
        unknown.external_id = Some("unknown-game".to_owned());
        unknown.exe_file_name = Some("Cyberpunk2077.exe".to_owned());
        unknown.graphics = ExeGraphicsInfo::new(vec![], Some(Architecture::X64));
        let component = input(LibraryTechnology::AmdFsr, "fsr");
        assert!(crate::addons::optiscaler::matcher::capability_available(
            &catalog,
            &unknown,
            std::slice::from_ref(&component)
        ));
        let evaluation = evaluate(
            std::slice::from_ref(&component),
            crate::addons::optiscaler::compatibility_catalog::resolve(&catalog, &unknown),
            true,
            EvaluationMode::Candidate,
        );
        assert_eq!(evaluation.status, OptiScalerCompatibilityStatus::Untested);
        assert_eq!(evaluation.block_code, None);
        assert_eq!(evaluation.blocked_reason, None);

        let mut catalog_only = bundled_facts("1091500");
        catalog_only.graphics = ExeGraphicsInfo::new(vec![], None);
        assert!(crate::addons::optiscaler::matcher::capability_available(
            &catalog,
            &catalog_only,
            &[]
        ));
    }
}
