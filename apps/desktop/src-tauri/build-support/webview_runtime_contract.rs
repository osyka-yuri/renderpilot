use serde::Deserialize;

const VERSION_COMPONENT_COUNT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebViewRuntimeContract {
    pub minimum_version: String,
    pub major: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TauriConfig {
    bundle: BundleConfig,
}

#[derive(Deserialize)]
struct BundleConfig {
    windows: WindowsConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsConfig {
    minimum_webview2_version: String,
}

pub fn parse_contract(source: &str) -> Result<WebViewRuntimeContract, String> {
    let config: TauriConfig = serde_json::from_str(source)
        .map_err(|error| format!("invalid Tauri runtime contract JSON: {error}"))?;
    let minimum_version = config.bundle.windows.minimum_webview2_version;
    let components = parse_version_components(&minimum_version)?;

    if components[0] == 0 {
        return Err(
            "bundle.windows.minimumWebview2Version major component must be positive".to_owned(),
        );
    }

    Ok(WebViewRuntimeContract {
        minimum_version,
        major: components[0],
    })
}

pub fn render_contract(contract: &WebViewRuntimeContract) -> String {
    format!(
        "const CONFIGURED_MINIMUM_WEBVIEW2_VERSION: &str = {:?};\n",
        contract.minimum_version
    )
}

fn parse_version_components(version: &str) -> Result<[u32; VERSION_COMPONENT_COUNT], String> {
    let mut components = [0; VERSION_COMPONENT_COUNT];
    let mut parts = version.split('.');
    for component in &mut components {
        let part = parts.next().ok_or_else(version_format_error)?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(version_format_error());
        }
        *component = part.parse::<u32>().map_err(|_| version_format_error())?;
    }
    if parts.next().is_some() {
        return Err(version_format_error());
    }

    Ok(components)
}

fn version_format_error() -> String {
    "bundle.windows.minimumWebview2Version must contain four numeric u32 components".to_owned()
}
