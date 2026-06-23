//! RenoDX-specific policy applied on top of detection facts.
//!
//! The detection layer ([`renderpilot_detection::analyze_executable`]) reports
//! the set of graphics APIs a binary imports as a plain fact, without ranking.
//! This module owns the RenoDX product policy that picks the primary API and
//! decides whether RenoDX can target a given executable, keeping such
//! tool-specific knowledge out of the generic domain and detection layers.

use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi};

/// Returns whether RenoDX, a DirectX-only renovation engine, can target a game
/// rendering with `api`.
#[must_use]
pub const fn api_supports_renodx(api: GraphicsApi) -> bool {
    matches!(
        api,
        GraphicsApi::D3D9 | GraphicsApi::D3D10 | GraphicsApi::D3D11 | GraphicsApi::D3D12
    )
}

/// Whether `api` is a **confirmed** non-DirectX renderer (Vulkan or OpenGL) — one
/// that cannot load a dxgi-style proxy, ruling RenoDX out. An `Unknown`/inconclusive
/// read is *not* confirmed non-DirectX (it returns `false`), so the caller keeps
/// trusting a curated title or an engine signal. Note this is **not** the negation
/// of [`api_supports_renodx`], which also rejects `Unknown`.
#[must_use]
pub const fn is_non_directx_renderer(api: GraphicsApi) -> bool {
    matches!(api, GraphicsApi::Vulkan | GraphicsApi::OpenGl)
}

/// Picks the single graphics API RenoDX should target from the detected set,
/// applying the "most capable DirectX wins, then DirectX over Vulkan/OpenGL"
/// tie-break. Returns [`GraphicsApi::Unknown`] when no known API was imported.
#[must_use]
pub fn primary_api(info: &ExeGraphicsInfo) -> GraphicsApi {
    info.apis()
        .iter()
        .copied()
        .max_by_key(|&api| api_rank(api))
        .unwrap_or(GraphicsApi::Unknown)
}

/// Preference order for the RenoDX target: the most capable DirectX version
/// wins, then DirectX over Vulkan/OpenGL.
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

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::Architecture;

    fn info(apis: &[GraphicsApi], arch: Option<Architecture>) -> ExeGraphicsInfo {
        ExeGraphicsInfo::new(apis.to_vec(), arch)
    }

    #[test]
    fn supports_renodx_only_for_directx() {
        for api in [
            GraphicsApi::D3D9,
            GraphicsApi::D3D10,
            GraphicsApi::D3D11,
            GraphicsApi::D3D12,
        ] {
            assert!(api_supports_renodx(api));
        }
        for api in [
            GraphicsApi::OpenGl,
            GraphicsApi::Vulkan,
            GraphicsApi::Unknown,
        ] {
            assert!(!api_supports_renodx(api));
        }
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
