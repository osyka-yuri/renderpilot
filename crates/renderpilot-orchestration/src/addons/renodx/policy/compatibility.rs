use crate::addons::matching::{IncompatibilityReason, MatchFacts};
use crate::addons::renodx::types::Title;
use crate::addons::reshade::proxy::{
    HostKind, api_supports_directx_addon, host_decision, primary_api, resolve_proxy_dll,
};

/// Validates a matched curated title and returns its [`HostKind`] and (for a proxy
/// host) the proxy DLL name.
///
/// A curated title is RenoDX-supported by definition (it is in the catalogue) and
/// carries its own architecture, so unreliable runtime API/architecture detection
/// — which comes back empty whenever a game loads Direct3D dynamically — must
/// never veto it. Detection is advisory here: it only picks the host and refines the
/// proxy DLL.
///
/// The two hard gates are physical: a **confirmed** OpenGL renderer has no host
/// RenoDX can drive, so the title is declined even though it is curated (a confirmed
/// Vulkan renderer is now hosted by the shared Vulkan layer, and an
/// `Unknown`/inconclusive read still defaults to a proxy); and an explicit
/// `required_api`, enforced only when detection identified a supported API to check
/// against.
pub fn check_title_compatibility(
    title: &Title,
    facts: &MatchFacts,
) -> Result<(HostKind, String), IncompatibilityReason> {
    let detected = primary_api(&facts.graphics);
    let Some(host_kind) = host_decision(detected) else {
        return Err(IncompatibilityReason::ApiUnsupported { detected });
    };
    if api_supports_directx_addon(detected)
        && !title.compatibility.required_api.is_empty()
        && !title.compatibility.required_api.contains(&detected)
    {
        return Err(IncompatibilityReason::ApiNotAllowed {
            detected,
            required: title.compatibility.required_api.clone(),
        });
    }
    let proxy_dll_name = match host_kind {
        HostKind::Proxy => resolve_proxy_dll(title.proxy_dll_override.as_deref(), &facts.graphics),
        HostKind::Vulkan => String::new(),
    };
    Ok((host_kind, proxy_dll_name))
}
