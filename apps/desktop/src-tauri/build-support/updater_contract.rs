//! Pure renderer for the build-generated updater trust contract.

/// Renders the effective updater public key as a Rust source constant.
pub fn render(config: &serde_json::Value) -> Result<String, String> {
    let public_key = config
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "plugins.updater.pubkey must be configured".to_owned())?;
    let public_key_literal = serde_json::to_string(public_key)
        .map_err(|error| format!("updater public key must serialize: {error}"))?;
    let endpoints = config
        .pointer("/plugins/updater/endpoints")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "plugins.updater.endpoints must be configured".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| value.starts_with("https://"))
                .ok_or_else(|| "updater endpoints must be non-empty HTTPS URLs".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if endpoints.is_empty() {
        return Err("plugins.updater.endpoints must not be empty".to_owned());
    }
    let endpoints_literal = serde_json::to_string(&endpoints)
        .map_err(|error| format!("updater endpoints must serialize: {error}"))?;
    Ok(format!(
        "pub(crate) const UPDATER_PUBLIC_KEY: &str = {public_key_literal};\n\
         pub(crate) const UPDATER_ENDPOINTS: &[&str] = &{endpoints_literal};\n"
    ))
}
