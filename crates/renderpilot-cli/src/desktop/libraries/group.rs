//! Group-key mapping for library identifiers.

/// Returns the canonical group key for a given library identifier.
pub(super) fn library_id_to_group_key(library_id: &str) -> &'static str {
    match library_id {
        "nvngx_dlss" => "dlss",
        "nvngx_dlssg" => "dlss_g",
        "nvngx_dlssd" => "dlss_d",
        "amd_fidelityfx_dx12" => "fsr_31_dx12",
        "amd_fidelityfx_vk" => "fsr_31_vk",
        "amd_fidelityfx_loader_dx12" => "fsr_loader_dx12",
        "amd_fidelityfx_upscaler_dx12" => "fsr_upscaler_dx12",
        "amd_fidelityfx_framegeneration_dx12" => "fsr_framegeneration_dx12",
        "amd_fidelityfx_denoiser_dx12" => "fsr_denoiser_dx12",
        "amd_fidelityfx_radiancecache_dx12" => "fsr_radiancecache_dx12",
        "libxell" => "xell",
        "libxess" => "xess",
        "libxess_dx11" => "xess_dx11",
        "libxess_fg" => "xess_fg",
        _ => "other",
    }
}
