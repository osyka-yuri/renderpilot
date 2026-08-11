//! Pure loading and merge policy for the effective Tauri configuration.

/// Parses the checked-in Tauri config and applies the optional `TAURI_CONFIG`
/// JSON merge-patch with the same semantics used by the Tauri CLI.
pub fn effective_config(source: &str, overlay: Option<&str>) -> Result<serde_json::Value, String> {
    let mut config: serde_json::Value = serde_json::from_str(source)
        .map_err(|error| format!("Tauri config must be valid JSON: {error}"))?;
    if let Some(overlay) = overlay {
        let overlay: serde_json::Value = serde_json::from_str(overlay)
            .map_err(|error| format!("TAURI_CONFIG must be valid JSON: {error}"))?;
        json_patch::merge(&mut config, &overlay);
    }
    Ok(config)
}
