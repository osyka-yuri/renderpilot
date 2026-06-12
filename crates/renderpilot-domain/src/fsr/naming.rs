//! Name-based predicates for AMD FSR naming conventions.
//!
//! Every function in this module reasons about a single file name string.
//! Component-level relationships (split detection, lineage, representative
//! selection) live in the sibling `lineage` and `representative` modules.

/// The FSR entry-point DLL an FSR 3.1 DX12 game (or one we already upgraded) loads.
pub const ENTRY_POINT_FILE_DX12: &str = "amd_fidelityfx_dx12.dll";

/// The FSR entry-point DLL an FSR 3.1 Vulkan game (or one we already upgraded) loads.
pub const ENTRY_POINT_FILE_VK: &str = "amd_fidelityfx_vk.dll";

// File-name prefixes of the individual FidelityFX SDK 2.x split DLLs; the
// `_<api>.dll` suffix (`_dx12.dll` / `_vk.dll`) follows each prefix.
const LOADER_PREFIX: &str = "amd_fidelityfx_loader_";
const UPSCALER_PREFIX: &str = "amd_fidelityfx_upscaler_";
const FRAME_GENERATION_PREFIX: &str = "amd_fidelityfx_framegeneration_";
const DENOISER_PREFIX: &str = "amd_fidelityfx_denoiser_";
const RADIANCE_CACHE_PREFIX: &str = "amd_fidelityfx_radiancecache_";

fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    let prefix_len = prefix.len();
    s.len() >= prefix_len && s.as_bytes()[..prefix_len].eq_ignore_ascii_case(prefix.as_bytes())
}

fn ends_with_ignore_ascii_case(s: &str, suffix: &str) -> bool {
    let len = s.len();
    let suf_len = suffix.len();
    len >= suf_len && s.as_bytes()[len - suf_len..].eq_ignore_ascii_case(suffix.as_bytes())
}

/// Whether `file_name` is a split-set member shipped only inside a package: a core
/// member (loader, upscaler, frame generation) or an optional effect (denoiser,
/// radiance cache). Such members are never offered as standalone single-file
/// artifacts — only inside a version-matched package.
#[must_use]
pub fn is_split_member(file_name: &str) -> bool {
    is_loader(file_name) || is_upscaling_member(file_name) || is_optional_effect(file_name)
}

/// Whether `file_name` is an *optional* FSR effect — the denoiser (Ray
/// Regeneration) or the radiance cache.
///
/// These are developer-integrated features the loader loads only when the game
/// implements them. They pair with the loader the game ships, so an upscaling
/// swap must leave them (and that loader) alone; a version-matched replacement
/// alongside a package upgrade is a pending follow-up, and adding one to a game
/// that lacks it is never correct.
#[must_use]
pub fn is_optional_effect(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, DENOISER_PREFIX)
        || starts_with_ignore_ascii_case(file_name, RADIANCE_CACHE_PREFIX)
}

/// Whether `file_name` is the upscaler — the marker that distinguishes an FSR
/// 3.1.4+/FSR 4 *split* set from the unified single-file FSR 3.x backend.
#[must_use]
pub fn is_split_marker(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, UPSCALER_PREFIX)
}

/// Whether `file_name` is a member of the split **upscaling** stack — the
/// upscaler or frame generation.
///
/// These are the members a unified FSR 3.x backend supersedes, so a downgrade
/// removes them. The loader under its own name and the optional effects
/// (denoiser, radiance cache) are NOT upscaling members: they form the game's
/// own effect stack (e.g. Ray Regeneration) and an upscaling swap must leave
/// them in place.
#[must_use]
pub fn is_upscaling_member(file_name: &str) -> bool {
    is_split_marker(file_name) || starts_with_ignore_ascii_case(file_name, FRAME_GENERATION_PREFIX)
}

/// Whether `file_name` is an FSR 3.1 unified entry point
/// ([`ENTRY_POINT_FILE_DX12`] or [`ENTRY_POINT_FILE_VK`]).
#[must_use]
pub fn is_entry_point(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case(ENTRY_POINT_FILE_DX12)
        || file_name.eq_ignore_ascii_case(ENTRY_POINT_FILE_VK)
}

/// Whether `file_name` is the FSR loader's own file name.
#[must_use]
pub fn is_loader(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, LOADER_PREFIX)
}

/// Identifies the graphics API an FSR DLL is bound to, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsrApi {
    /// DirectX 12 backend (`_dx12.dll`).
    Dx12,
    /// Vulkan backend (`_vk.dll`).
    Vulkan,
}

/// Returns the specific graphics API this FSR file belongs to, based on its suffix.
#[must_use]
pub fn fsr_graphics_api(file_name: &str) -> Option<FsrApi> {
    if ends_with_ignore_ascii_case(file_name, "_dx12.dll") {
        Some(FsrApi::Dx12)
    } else if ends_with_ignore_ascii_case(file_name, "_vk.dll") {
        Some(FsrApi::Vulkan)
    } else {
        None
    }
}
