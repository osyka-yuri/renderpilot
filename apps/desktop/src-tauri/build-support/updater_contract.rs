//! Pure renderer for the build-generated updater trust contract.

/// Requires an updater public key and renders the effective updater endpoints.
pub fn render(config: &serde_json::Value) -> Result<String, String> {
    let _public_key = config
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "plugins.updater.pubkey must be configured".to_owned())?;
    let endpoints = config
        .pointer("/plugins/updater/endpoints")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "plugins.updater.endpoints must be configured".to_owned())?;
    if endpoints.is_empty() {
        return Err("plugins.updater.endpoints must not be empty".to_owned());
    }
    let endpoints = endpoints
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.starts_with("https://"))
                .ok_or_else(|| "updater endpoints must be non-empty HTTPS URLs".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let endpoints_literal = serde_json::to_string(&endpoints)
        .map_err(|error| format!("updater endpoints must serialize: {error}"))?;
    Ok(format!(
        "pub(crate) const UPDATER_ENDPOINTS: &[&str] = &{endpoints_literal};\n"
    ))
}
