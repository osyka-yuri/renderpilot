//! AMD FSR split-set knowledge — the single source of truth for the FidelityFX
//! SDK 2.x DLL names and the rules that distinguish and place them.
//!
//! FSR 3.1.4+ and FSR 4 ship as split DLLs: a loader (no effect code; it only
//! loads effect providers) plus per-effect providers — upscaler, frame
//! generation, denoiser (Ray Regeneration), radiance cache. The split lets a
//! game ship only the effect types it uses, so real-world layouts vary:
//!
//! * **Unified FSR 3.1** — a single `amd_fidelityfx_dx12.dll`
//!   (`amd_fidelityfx_vk.dll` for Vulkan). The AMD driver can upgrade this to
//!   FSR 4 at runtime on supported hardware — no local upscaler DLL involved.
//! * **Upgraded split set** — the loader installed *as* the entry-point name
//!   (what an FSR 3.1 game loads) with the upscaler + frame generation beside
//!   it; this is how RenderPilot upgrades FSR 3.1 → 4.
//! * **Native FSR 4** — the game loads `amd_fidelityfx_loader_<api>.dll`
//!   under its own name; no entry point exists.
//! * **Independent effect stack beside the entry point** — a loader under its
//!   own name plus only the effect DLLs the game integrates, *next to* a
//!   unified entry point: e.g. `amd_fidelityfx_dx12.dll` (upscaling/FG,
//!   driver-upgradeable) alongside `amd_fidelityfx_loader_dx12.dll` +
//!   `amd_fidelityfx_denoiser_dx12.dll` (Ray Regeneration). That loader+effect
//!   pair is a working stack of its own: an upscaling swap must never
//!   overwrite it, retarget onto it, or remove it.
//!
//! Two members — the denoiser (Ray Regeneration) and the radiance cache — are
//! *optional effects*: developer-integrated features the loader only loads
//! when the game's own code requests them. They pair with the loader the game
//! ships; replacing them version-matched alongside a package upgrade is a
//! pending follow-up, never an automatic add.
//!
//! Candidate filtering, plan building, and the executor all reason about these
//! names — they live here once so they can never drift apart. The module is
//! organized by altitude: single-file-name predicates (`naming`), whole-set
//! predicates (`lineage`), representative selection and ordering
//! (`representative`), and install-target resolution (`install_targets`) — all
//! re-exported flat as the module's one public API.

mod install_targets;
mod lineage;
mod naming;
mod representative;

pub use install_targets::resolve_artifact_install_target;
pub use lineage::{
    has_entry_point, is_native_fsr4, is_split_set, same_release_build, upscaler_represents_set,
};
pub use naming::{
    fsr_graphics_api, is_entry_point, is_loader, is_optional_effect, is_split_marker,
    is_split_member, is_upscaling_member, FsrApi, ENTRY_POINT_FILE_DX12, ENTRY_POINT_FILE_VK,
};
pub use representative::{
    display_component_file, primary_rank, sort_representative_first, version_representative,
};

#[cfg(test)]
mod tests;
