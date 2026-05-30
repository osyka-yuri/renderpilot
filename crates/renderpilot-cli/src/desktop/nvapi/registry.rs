use renderpilot_nvapi::{dlss::DlssRenderPresetSetting, NvapiSetting};

/// Resolves and retrieves a specific NVAPI setting implementation using its unique wire key. 
/// Returns `None` if the provided key does not map to any recognized setting.
pub fn lookup_setting(key: &str) -> Option<Box<dyn NvapiSetting>> {
    match key {
        k if k == DlssRenderPresetSetting::KEY => Some(Box::new(DlssRenderPresetSetting)),
        _ => None,
    }
}

/// Generates a comprehensive, stable collection of all NVAPI settings currently supported by the application.
pub fn supported_settings() -> Vec<Box<dyn NvapiSetting>> {
    vec![Box::new(DlssRenderPresetSetting)]
}
