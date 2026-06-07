use renderpilot_nvapi::NvapiSetting;

use crate::dlss::settings_catalog::{self, CatalogSetting};

/// Resolves an NVAPI setting implementation by its wire key. Returns `None`
/// when the key is not present in the bundled DLSS settings catalog.
pub fn lookup_setting(key: &str) -> Option<Box<dyn NvapiSetting>> {
    settings_catalog::find(key)
        .map(|def| Box::new(CatalogSetting::new(def)) as Box<dyn NvapiSetting>)
}

/// Every NVAPI setting RenderPilot supports, in catalog declaration order.
pub fn supported_settings() -> Vec<Box<dyn NvapiSetting>> {
    settings_catalog::catalog()
        .iter()
        .map(|def| Box::new(CatalogSetting::new(def)) as Box<dyn NvapiSetting>)
        .collect()
}
