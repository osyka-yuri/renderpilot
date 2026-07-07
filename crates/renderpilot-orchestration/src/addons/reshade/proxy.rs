//! How a DirectX ReShade add-on hooks itself into a game, decided by the renderer.
//!
//! The detection layer ([`renderpilot_detection::analyze_executable`]) reports the
//! set of graphics APIs a binary imports as a plain fact, without ranking. This
//! module owns the tool-agnostic policy that picks the primary API and the ReShade
//! proxy DLL, keeping such knowledge out of the generic domain and detection
//! layers. Both RenoDX and Luma target DirectX via a ReShade proxy DLL, so this is
//! shared; RenoDX additionally routes confirmed-Vulkan games to the shared Vulkan
//! layer ([`HostKind::Vulkan`]).

use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi};
use serde::Serialize;

/// How a ReShade add-on hooks itself into a game, decided by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    /// A per-game ReShade proxy DLL loaded next to the executable (Direct3D, or an
    /// inconclusive read that defaults to a dxgi-style proxy).
    Proxy,
    /// The single global ReShade Vulkan implicit layer (a Vulkan game cannot load a
    /// proxy DLL). Shared across all Vulkan games; see
    /// `renderpilot_platform_windows::vulkan_layer`. Only RenoDX drives this;
    /// Luma is DX11-only.
    Vulkan,
}

/// Returns whether a DirectX ReShade add-on can target a game rendering with `api`.
#[must_use]
pub const fn api_supports_directx_addon(api: GraphicsApi) -> bool {
    matches!(
        api,
        GraphicsApi::D3D9 | GraphicsApi::D3D10 | GraphicsApi::D3D11 | GraphicsApi::D3D12
    )
}

/// Decides how an add-on hosts itself for a renderer, or `None` when it cannot.
///
/// Direct3D and an inconclusive (`Unknown`) read use a per-game [`HostKind::Proxy`]
/// (a curated title or engine signal is trusted over an empty detection); a
/// **confirmed** Vulkan renderer uses the global [`HostKind::Vulkan`] layer; a
/// **confirmed** OpenGL renderer is unsupported (`None`). This is the single gate
/// that turns a detected API into an install strategy.
#[must_use]
pub const fn host_decision(api: GraphicsApi) -> Option<HostKind> {
    match api {
        GraphicsApi::OpenGl => None,
        GraphicsApi::Vulkan => Some(HostKind::Vulkan),
        _ => Some(HostKind::Proxy),
    }
}

/// Picks the single graphics API to target from the detected set, applying the
/// "most capable DirectX wins, then DirectX over Vulkan/OpenGL" tie-break. Returns
/// [`GraphicsApi::Unknown`] when no known API was imported.
#[must_use]
pub fn primary_api(info: &ExeGraphicsInfo) -> GraphicsApi {
    info.apis()
        .iter()
        .copied()
        .max_by_key(|&api| api_rank(api))
        .unwrap_or(GraphicsApi::Unknown)
}

/// Preference order for the render target: the most capable DirectX version wins,
/// then DirectX over Vulkan/OpenGL.
fn api_rank(api: GraphicsApi) -> u8 {
    match api {
        GraphicsApi::D3D12 => 6,
        GraphicsApi::D3D11 => 5,
        GraphicsApi::D3D10 => 4,
        GraphicsApi::D3D9 => 3,
        GraphicsApi::Vulkan => 2,
        GraphicsApi::OpenGl => 1,
        GraphicsApi::Unknown => 0,
    }
}

/// Picks the ReShade proxy DLL from the graphics DLLs the executable *actually*
/// imports, in ReShade's hijack preference: `dxgi.dll` covers D3D10/11/12 (the
/// modern norm and what those games load for the swapchain), otherwise the
/// specific device DLL the game loads directly. Returns `None` when no DirectX
/// DLL was detected (dynamic loading / inconclusive); the caller then falls back
/// to a per-title override or a documented default, so this never guesses.
#[must_use]
pub fn proxy_dll(info: &ExeGraphicsInfo) -> Option<String> {
    let dlls = info.graphics_dlls();
    let imports = |name: &str| dlls.iter().any(|dll| dll == name);
    let proxy = if imports("dxgi.dll") {
        "dxgi.dll"
    } else if imports("d3d12.dll") {
        "d3d12.dll"
    } else if imports("d3d11.dll") {
        "d3d11.dll"
    } else if imports("d3d10.dll") || imports("d3d10_1.dll") || imports("d3d10core.dll") {
        "d3d10.dll"
    } else if imports("d3d9.dll") {
        "d3d9.dll"
    } else {
        return None;
    };
    Some(proxy.to_owned())
}

/// Default ReShade proxy DLL when the detected API is inconclusive: dxgi.dll
/// loads for D3D10/11/12, which covers essentially every modern add-on title.
pub const DEFAULT_PROXY_DLL: &str = "dxgi.dll";

/// The proxy DLL ReShade installs as: an explicit per-title override, else the
/// DLL the game actually imports (via [proxy_dll], in ReShade preference), else
/// the [DEFAULT_PROXY_DLL] — so the install hooks the DLL the game really loads
/// and an inconclusive read still resolves to a sane modern default.
pub fn resolve_proxy_dll(override_name: Option<&str>, graphics: &ExeGraphicsInfo) -> String {
    override_name
        .map(str::to_owned)
        .or_else(|| proxy_dll(graphics))
        .unwrap_or_else(|| DEFAULT_PROXY_DLL.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::Architecture;

    fn info(apis: &[GraphicsApi], arch: Option<Architecture>) -> ExeGraphicsInfo {
        ExeGraphicsInfo::new(apis.to_vec(), arch)
    }

    #[test]
    fn supports_directx_addon_only_for_directx() {
        for api in [
            GraphicsApi::D3D9,
            GraphicsApi::D3D10,
            GraphicsApi::D3D11,
            GraphicsApi::D3D12,
        ] {
            assert!(api_supports_directx_addon(api));
        }
        for api in [
            GraphicsApi::OpenGl,
            GraphicsApi::Vulkan,
            GraphicsApi::Unknown,
        ] {
            assert!(!api_supports_directx_addon(api));
        }
    }

    #[test]
    fn host_decision_routes_proxy_vulkan_and_declines_opengl() {
        for api in [
            GraphicsApi::D3D9,
            GraphicsApi::D3D10,
            GraphicsApi::D3D11,
            GraphicsApi::D3D12,
            GraphicsApi::Unknown,
        ] {
            assert_eq!(host_decision(api), Some(HostKind::Proxy));
        }
        assert_eq!(host_decision(GraphicsApi::Vulkan), Some(HostKind::Vulkan));
        assert_eq!(host_decision(GraphicsApi::OpenGl), None);
    }

    #[test]
    fn primary_api_picks_most_capable_directx() {
        let info = info(
            &[GraphicsApi::D3D11, GraphicsApi::D3D12, GraphicsApi::Vulkan],
            Some(Architecture::X64),
        );
        assert_eq!(primary_api(&info), GraphicsApi::D3D12);
    }

    #[test]
    fn primary_api_defaults_to_unknown_for_empty_set() {
        let info = info(&[], None);
        assert_eq!(primary_api(&info), GraphicsApi::Unknown);
    }

    fn graphics_with_dlls(dlls: &[&str]) -> ExeGraphicsInfo {
        ExeGraphicsInfo::new(Vec::new(), Some(Architecture::X64))
            .with_graphics_dlls(dlls.iter().map(|dll| (*dll).to_owned()).collect())
    }

    #[test]
    fn proxy_dll_prefers_dxgi_over_specific_device_dll() {
        assert_eq!(
            proxy_dll(&graphics_with_dlls(&["d3d12.dll", "dxgi.dll"])).as_deref(),
            Some("dxgi.dll")
        );
    }

    #[test]
    fn proxy_dll_uses_the_specific_dll_when_no_dxgi_is_imported() {
        assert_eq!(
            proxy_dll(&graphics_with_dlls(&["d3d11.dll"])).as_deref(),
            Some("d3d11.dll")
        );
        assert_eq!(
            proxy_dll(&graphics_with_dlls(&["d3d9.dll"])).as_deref(),
            Some("d3d9.dll")
        );
        assert_eq!(
            proxy_dll(&graphics_with_dlls(&["d3d10core.dll"])).as_deref(),
            Some("d3d10.dll")
        );
    }

    #[test]
    fn proxy_dll_is_none_without_a_directx_import() {
        assert_eq!(proxy_dll(&graphics_with_dlls(&[])), None);
        assert_eq!(proxy_dll(&graphics_with_dlls(&["vulkan-1.dll"])), None);
    }
}
