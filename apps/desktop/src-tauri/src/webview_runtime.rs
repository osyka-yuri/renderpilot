#![cfg(windows)]
#![expect(
    unsafe_code,
    reason = "WebView2 version comparison, pre-WebView update prompt, and startup data-path setup require controlled unsafe calls"
)]

use std::{
    cmp::Ordering,
    ffi::OsString,
    fmt::Display,
    path::{Path, PathBuf},
    process,
};

use tauri::Wry;
use webview2_com_sys::Microsoft::Web::WebView2::Win32::CompareBrowserVersions;
use windows_core::HSTRING;
use windows_sys::Win32::UI::{
    Shell::ShellExecuteW,
    WindowsAndMessaging::{
        IDYES, MB_ICONERROR, MB_SETFOREGROUND, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
    },
};

const DOWNLOAD_URL: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";
const INCOMPATIBLE_RUNTIME_EXIT_CODE: i32 = 2;

include!(concat!(env!("OUT_DIR"), "/webview_runtime_contract.rs"));

/// Pins the WebView2 user-data folder before Tauri creates a WebView.
///
/// The authenticated portable runtime path takes precedence. Installed builds
/// retain the existing compatible `%LOCALAPPDATA%\\RenderPilot\\WebView2`
/// fallback without making runtime privilege a startup concern.
pub(crate) fn configure_user_data_path() {
    #[cfg(feature = "portable")]
    if let Some(paths) = renderpilot_orchestration::portable::runtime_paths() {
        // SAFETY: portable RuntimePathsV1 was installed single-assignment
        // before logger, data, WebView, Tauri, and Context initialization.
        unsafe {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &paths.webview2_root);
        }
        return;
    }

    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none()
        && std::env::var_os("LOCALAPPDATA").is_some()
    {
        // SAFETY: single-threaded during startup, before any plugin init.
        unsafe {
            std::env::set_var(
                "WEBVIEW2_USER_DATA_FOLDER",
                installed_webview2_data_root().join("WebView2"),
            );
        }
    }
}

fn installed_webview2_data_root() -> PathBuf {
    resolve_installed_data_root(
        std::env::var_os(renderpilot_orchestration::portable::APP_DIR_ENV),
        std::env::var_os("LOCALAPPDATA"),
        &std::env::temp_dir(),
    )
}

fn resolve_installed_data_root(
    app_dir_env: Option<OsString>,
    local_appdata: Option<OsString>,
    temp_dir: &Path,
) -> PathBuf {
    if let Some(app_dir) = app_dir_env {
        let path = PathBuf::from(app_dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(local) = local_appdata {
        let path = PathBuf::from(local);
        if !path.as_os_str().is_empty() {
            return path.join("RenderPilot");
        }
    }
    temp_dir.join("RenderPilot")
}

pub(crate) fn enforce_minimum_version(context: &tauri::Context<Wry>) {
    let minimum_version = match configured_minimum_version(context) {
        Ok(version) => version,
        Err(error) => {
            log::error!("invalid WebView2 runtime contract: {error}");
            process::exit(INCOMPATIBLE_RUNTIME_EXIT_CODE);
        }
    };

    let installed_version = installed_runtime_version(tauri::webview_version());

    if is_supported(installed_version.as_deref(), minimum_version) {
        log::info!(
            "WebView2 Runtime {} satisfies the minimum version {minimum_version}",
            installed_version.as_deref().unwrap_or("unknown")
        );
        return;
    }

    let detected_version = installed_version.as_deref().unwrap_or("not detected");
    log::error!(
        "WebView2 Runtime {detected_version} does not satisfy the minimum version {minimum_version}"
    );

    if show_update_prompt(minimum_version, detected_version) {
        open_download_page();
    }

    process::exit(INCOMPATIBLE_RUNTIME_EXIT_CODE);
}

fn configured_minimum_version(context: &tauri::Context<Wry>) -> Result<&str, String> {
    let runtime_value = context
        .config()
        .bundle
        .windows
        .minimum_webview2_version
        .as_deref();

    // Tauri 2.11 parses this installer setting while building, but its Context
    // code generator currently omits it from the runtime WindowsConfig. The
    // build script therefore generates this fallback from the same canonical
    // tauri.conf.json. If Tauri starts embedding it, require an exact match.
    resolve_minimum_version(runtime_value, CONFIGURED_MINIMUM_WEBVIEW2_VERSION)
}

fn resolve_minimum_version<'a>(
    runtime_value: Option<&'a str>,
    generated_value: &'a str,
) -> Result<&'a str, String> {
    if generated_value.is_empty() {
        return Err("bundle.windows.minimumWebview2Version is empty".to_owned());
    }

    match runtime_value {
        Some(value) if value == generated_value => Ok(value),
        Some(value) => Err(format!(
            "runtime context has {value}, build contract has {generated_value}"
        )),
        None => Ok(generated_value),
    }
}

fn installed_runtime_version<E: Display>(result: Result<String, E>) -> Option<String> {
    match result {
        Ok(version) => Some(version),
        Err(error) => {
            log::error!("failed to determine the installed WebView2 Runtime version: {error}");
            None
        }
    }
}

fn is_supported(installed_version: Option<&str>, minimum_version: &str) -> bool {
    let Some(installed_version) = installed_version else {
        return false;
    };

    match compare_browser_versions(installed_version, minimum_version) {
        Ok(Ordering::Equal | Ordering::Greater) => true,
        Ok(Ordering::Less) => false,
        Err(error) => {
            log::error!(
                "failed to compare WebView2 Runtime versions {installed_version:?} and {minimum_version:?}: {error}"
            );
            false
        }
    }
}

fn compare_browser_versions(
    installed_version: &str,
    minimum_version: &str,
) -> windows_core::Result<Ordering> {
    let installed_version = HSTRING::from(installed_version);
    let minimum_version = HSTRING::from(minimum_version);
    let mut comparison = 0;

    // SAFETY: both HSTRING values own valid, null-terminated UTF-16 buffers for
    // the duration of the call, and `comparison` is a writable out-parameter.
    unsafe {
        CompareBrowserVersions(&installed_version, &minimum_version, &raw mut comparison)?;
    }

    Ok(comparison.cmp(&0))
}

fn show_update_prompt(minimum_version: &str, detected_version: &str) -> bool {
    let title = to_wide("RenderPilot requires a WebView2 update");
    let message = to_wide(&format!(
        "RenderPilot requires Microsoft Edge WebView2 Runtime {minimum_version} or newer.\n\nDetected: {detected_version}\n\nOpen the official download page now?"
    ));

    // SAFETY: `title` and `message` are null-terminated UTF-16 buffers that
    // remain alive for the synchronous MessageBoxW call. A null owner is valid.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONERROR | MB_SETFOREGROUND,
        ) == IDYES
    }
}

fn open_download_page() {
    let operation = to_wide("open");
    let url = to_wide(DOWNLOAD_URL);
    // SAFETY: `operation` and `url` are live, null-terminated UTF-16 buffers.
    // The remaining optional ShellExecuteW string parameters may be null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result as isize <= 32 {
        log::error!("failed to open the WebView2 Runtime download page: ShellExecuteW={result:?}");
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CONFIGURED_MINIMUM_WEBVIEW2_VERSION as MINIMUM_VERSION, compare_browser_versions,
        installed_runtime_version, is_supported, resolve_installed_data_root,
        resolve_minimum_version, to_wide,
    };
    use std::{ffi::OsString, path::Path};

    #[test]
    fn installed_data_root_preserves_compatible_resolution_order() {
        let temp = Path::new("C:\\temp");
        assert_eq!(
            resolve_installed_data_root(
                Some(OsString::from("D:\\portable\\data")),
                Some(OsString::from("C:\\Users\\me\\AppData\\Local")),
                temp,
            ),
            Path::new("D:\\portable\\data")
        );
        assert_eq!(
            resolve_installed_data_root(
                None,
                Some(OsString::from("C:\\Users\\me\\AppData\\Local")),
                temp,
            ),
            Path::new("C:\\Users\\me\\AppData\\Local\\RenderPilot")
        );
        assert_eq!(
            resolve_installed_data_root(None, None, temp),
            Path::new("C:\\temp\\RenderPilot")
        );
    }

    #[test]
    fn generated_version_is_the_fallback_when_tauri_omits_the_runtime_value() {
        assert_eq!(
            resolve_minimum_version(None, MINIMUM_VERSION),
            Ok(MINIMUM_VERSION)
        );
    }

    #[test]
    fn matching_runtime_and_generated_versions_are_accepted() {
        assert_eq!(
            resolve_minimum_version(Some(MINIMUM_VERSION), MINIMUM_VERSION),
            Ok(MINIMUM_VERSION)
        );
    }

    #[test]
    fn empty_or_mismatched_runtime_contracts_are_rejected() {
        assert!(resolve_minimum_version(None, "").is_err());
        assert!(resolve_minimum_version(Some("137.0.0.0"), MINIMUM_VERSION).is_err());
    }

    #[test]
    fn missing_runtime_is_not_supported() {
        assert!(!is_supported(None, MINIMUM_VERSION));
    }

    #[test]
    fn runtime_lookup_error_is_not_supported() {
        let installed = installed_runtime_version(Err::<String, _>("lookup failed"));

        assert!(!is_supported(installed.as_deref(), MINIMUM_VERSION));
    }

    #[test]
    fn older_runtime_is_not_supported() {
        assert!(!is_supported(Some("135.0.3179.98"), MINIMUM_VERSION));
    }

    #[test]
    fn exact_minimum_runtime_is_supported() {
        assert!(is_supported(Some(MINIMUM_VERSION), MINIMUM_VERSION));
    }

    #[test]
    fn newer_patch_and_major_versions_are_supported() {
        assert!(is_supported(Some("136.0.3240.45"), MINIMUM_VERSION));
        assert!(is_supported(Some("137.0.3296.0"), MINIMUM_VERSION));
    }

    #[test]
    fn channel_suffix_is_compared_by_webview2() {
        let channel_version = format!("{MINIMUM_VERSION} dev");

        assert!(is_supported(Some(&channel_version), MINIMUM_VERSION));
    }

    #[test]
    fn invalid_version_is_rejected() {
        assert!(compare_browser_versions("not-a-version", MINIMUM_VERSION).is_err());
        assert!(!is_supported(Some("not-a-version"), MINIMUM_VERSION));
    }

    #[test]
    fn wide_strings_are_null_terminated_without_splitting_surrogate_pairs() {
        assert_eq!(to_wide("RP🎮"), vec![0x0052, 0x0050, 0xD83C, 0xDFAE, 0]);
    }
}
