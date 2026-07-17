//! Deterministic, explainable resolver from an installed game to a Luma outcome.
//!
//! [`MatchFacts`] about a game are evaluated against every [`LumaTitle`]'s
//! ordered, tiered rules; the highest-specificity match wins by the shared
//! tie-break in [`crate::addons::matching::select_title`] (also used by RenoDX).
//! Luma has no engine-fallback (unlike RenoDX's generic titles): a game that
//! matches no curated title is simply [`LumaResolution::NoMatch`].
//!
//! Luma is narrower than RenoDX in one more way: RenoDX targets any DirectX
//! version, but Luma's add-on is DirectX-11-specific. Confirmed non-D3D11
//! renderers are normally incompatible. The one deliberate exception is a
//! Generic UE profile with a D3D12 import: that is an advisory signal that the
//! game may need the user's manual `-dx11` switch, not proof of its active RHI.

use renderpilot_domain::{Architecture, GraphicsApi};

use crate::addons::CatalogMessage;
use crate::addons::matching::{
    IncompatibilityReason, MatchConfidence, MatchFacts, SelectableTitle, confidence_for_match,
    select_title,
};
use crate::addons::reshade::proxy::{HostKind, host_decision, primary_api, resolve_proxy_dll};

use super::types::{
    LumaCategory, LumaExternalRequirement, LumaFeatures, LumaGuidance, LumaManifest, LumaProfile,
    LumaTitle, MatchRule,
};

/// A matched, compatible game resolved to everything a Luma install needs (owned).
#[derive(Debug, Clone)]
pub(crate) struct ResolvedLumaInstall {
    /// Release asset file name to fetch (see [`super::source::asset_url`]).
    pub(crate) asset: String,
    /// Exact root add-on file name the release asset must contain.
    pub(crate) addon_file: String,
    /// Architecture of the asset / executable.
    pub(crate) arch: Architecture,
    /// Proxy DLL file name to install ReShade as.
    pub(crate) proxy_dll_name: String,
    /// Confidence shown to the user.
    pub(crate) confidence: MatchConfidence,
    /// Required launch arguments, shown to the user as a copyable callout.
    pub(crate) launch_args: Vec<String>,
    /// Per-game HDR and DLSS/FSR availability from the upstream wiki.
    pub(crate) features: Option<LumaFeatures>,
    /// Reviewed user guidance from the curated profile.
    pub(crate) guidance: Vec<LumaGuidance>,
    /// Managed external dependency, if this title needs one.
    pub(crate) external_requirement: Option<LumaExternalRequirement>,
    /// Dedicated game profile vs shared engine payload (drives UI badge and
    /// the Generic UE D3D12 → manual `-dx11` launch-argument rule).
    pub(crate) profile: LumaProfile,
}

/// Outcome of resolving a game against the manifest.
#[derive(Debug, Clone)]
pub(crate) enum LumaResolution {
    /// A compatible game matched; the install plan is ready.
    Installable(Box<ResolvedLumaInstall>),
    /// A title matched but cannot be installed for this game.
    Incompatible {
        /// Why it cannot be installed.
        reason: IncompatibilityReason,
    },
    /// The game is blacklisted / known-broken (or needs an external prerequisite
    /// this installer doesn't automate).
    Blacklisted {
        /// Localizable explanation supplied by the catalogue.
        message: CatalogMessage,
    },
    /// Nothing matched the game.
    NoMatch,
}

impl SelectableTitle for LumaTitle {
    fn match_rules(&self) -> &[MatchRule] {
        &self.match_rules
    }

    fn status(&self) -> crate::addons::matching::Status {
        self.status
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Resolves a game to a Luma outcome against a validated manifest.
#[must_use]
pub(crate) fn resolve(manifest: &LumaManifest, facts: &MatchFacts) -> LumaResolution {
    match select_title(&manifest.titles, facts) {
        Some((title, rule)) => resolve_title(title, rule, facts),
        None => LumaResolution::NoMatch,
    }
}

fn resolve_title(title: &LumaTitle, rule: &MatchRule, facts: &MatchFacts) -> LumaResolution {
    if let LumaCategory::Blacklist { message } = &title.category {
        return LumaResolution::Blacklisted {
            message: message.clone(),
        };
    }
    match build_install_plan(title, rule, facts) {
        Ok(plan) => LumaResolution::Installable(Box::new(plan)),
        Err(reason) => LumaResolution::Incompatible { reason },
    }
}

/// Gates a matched title against the game's detected renderer/architecture and
/// builds its install plan.
///
/// Detection is advisory for *which* proxy DLL to use (as for RenoDX), but — unlike
/// RenoDX — Luma hard-gates on the DirectX version itself: only a confirmed D3D11
/// renderer or an inconclusive read is accepted; a confirmed Vulkan/OpenGL/other-
/// DirectX-version renderer is incompatible. Luma also hard-gates on architecture:
/// a curated title always declares one exact CPU architecture (mirroring the
/// distinct `-x32` asset variant it maps to), so the detected executable
/// architecture, when known, must agree.
fn build_install_plan(
    title: &LumaTitle,
    rule: &MatchRule,
    facts: &MatchFacts,
) -> Result<ResolvedLumaInstall, IncompatibilityReason> {
    let raw_detected = primary_api(&facts.graphics);
    // Luma is a D3D11 add-on. If the executable also imports D3D11 (even if
    // D3D12 is higher in the list), prefer D3D11 for compatibility checks.
    // This lets games that support both (common in UE4/5) be treated as
    // Luma-compatible when they can run the D3D11 path.
    let detected = if facts.graphics.apis().contains(&GraphicsApi::D3D11) {
        GraphicsApi::D3D11
    } else {
        raw_detected
    };

    match host_decision(detected) {
        None => {
            return Err(IncompatibilityReason::ApiUnsupported {
                detected: raw_detected,
            });
        }
        Some(HostKind::Vulkan) => {
            return Err(IncompatibilityReason::ApiUnsupported {
                detected: raw_detected,
            });
        }
        Some(HostKind::Proxy) => {
            if detected != GraphicsApi::Unknown
                && detected != GraphicsApi::D3D11
                && !generic_ue_d3d12_can_be_switched(title, facts, detected)
                && !external_requirement_accepts(title.external_requirement.as_ref(), detected)
            {
                return Err(IncompatibilityReason::ApiNotAllowed {
                    detected: raw_detected,
                    required: allowed_apis_for_error(title.external_requirement.as_ref()),
                });
            }
        }
    }

    let Some(detected_arch) = facts.graphics.architecture() else {
        return Err(IncompatibilityReason::ArchUnknown);
    };
    if detected_arch != title.arch {
        return Err(IncompatibilityReason::ArchMismatch {
            detected: detected_arch,
            required: title.arch,
        });
    }

    Ok(ResolvedLumaInstall {
        asset: title.asset.clone(),
        addon_file: title.addon_file.clone(),
        arch: title.arch,
        confidence: confidence_for_match(title.status, rule.kind),
        launch_args: title.launch_args.clone(),
        features: title.features.clone(),
        guidance: title.guidance.clone(),
        external_requirement: title.external_requirement.clone(),
        profile: title.profile,
        proxy_dll_name: resolve_proxy_dll(
            title
                .external_requirement
                .as_ref()
                .map(LumaExternalRequirement::reshade_proxy_dll),
            &facts.graphics,
        ),
    })
}

fn generic_ue_d3d12_can_be_switched(
    title: &LumaTitle,
    facts: &MatchFacts,
    detected: GraphicsApi,
) -> bool {
    detected == GraphicsApi::D3D12
        && title.is_generic_unreal()
        && matches!(
            facts.engine,
            Some(crate::addons::matching::Engine::Unreal)
                | Some(crate::addons::matching::Engine::UnrealExtended)
        )
}

fn external_requirement_accepts(
    requirement: Option<&LumaExternalRequirement>,
    detected: GraphicsApi,
) -> bool {
    requirement
        .map(LumaExternalRequirement::accepted_detected_apis)
        .is_some_and(|apis| apis.contains(&detected))
}

fn allowed_apis_for_error(requirement: Option<&LumaExternalRequirement>) -> Vec<GraphicsApi> {
    let mut apis = vec![GraphicsApi::D3D11];
    if let Some(requirement) = requirement {
        for api in requirement.accepted_detected_apis() {
            if !apis.contains(api) {
                apis.push(*api);
            }
        }
    }
    apis
}

#[cfg(test)]
mod tests;
