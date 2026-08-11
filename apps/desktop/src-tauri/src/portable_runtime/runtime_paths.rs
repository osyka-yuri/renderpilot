use std::path::Path;

use renderpilot_orchestration::portable::{RuntimePathsV1, install_runtime_paths};

use super::{
    app_protocol::PortableStartupV3,
    error::{PortableRuntimeError, Result},
};

const WEBVIEW_ENV: &str = "WEBVIEW2_USER_DATA_FOLDER";
const DB_ENV: &str = "RENDERPILOT_DB_PATH";

/// Installs the authenticated portable path authority before logger, Tauri,
/// WebView2, SQLite, or any application context can be created.
#[expect(
    unsafe_code,
    reason = "startup runs before threads and replaces ambient portable path inputs with authenticated projections"
)]
pub fn install_from_startup(startup: &PortableStartupV3) -> Result<()> {
    startup.validate()?;
    let executable = std::env::current_exe()?;
    let executable_root = executable.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_runtime_paths", "portable App image had no parent")
    })?;
    let paths = &startup.runtime_paths;
    let portable_root_identity =
        super::win32::directory::directory_identity_digest_no_reparse(&paths.portable_root)?;
    let executable_root_identity =
        super::win32::directory::directory_identity_digest_no_reparse(executable_root)?;
    let selected_generation_identity =
        super::win32::directory::directory_identity_digest_no_reparse(
            &paths.selected_generation_root,
        )?;
    if portable_root_identity != startup.portable_root_identity
        || executable_root_identity != startup.generation_root_identity
        || selected_generation_identity != startup.generation_root_identity
        || !same_executable_identity(&executable, &paths.selected_app_executable)?
    {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "App image/root/generation identity did not match startup v3 binding",
        ));
    }
    let paths = startup.runtime_paths.clone();
    install_runtime_paths(paths)
        .map_err(|detail| PortableRuntimeError::new("portable_runtime_paths", detail))?;
    let paths = current()?;
    // SAFETY: this is the authenticated child entry before logger/WebView/Tauri
    // initialization and no thread has yet been started.
    unsafe {
        std::env::set_var(
            renderpilot_orchestration::portable::APP_DIR_ENV,
            &paths.data_root,
        );
        std::env::set_var(DB_ENV, &paths.catalog_db_path);
        std::env::set_var(WEBVIEW_ENV, &paths.webview2_root);
    }
    Ok(())
}

fn same_executable_identity(actual: &Path, selected: &Path) -> Result<bool> {
    let actual = super::win32::directory::file_identity_digest_no_reparse(actual)?;
    let selected = super::win32::directory::file_identity_digest_no_reparse(selected)?;
    Ok(actual == selected)
}

pub fn current() -> Result<&'static RuntimePathsV1> {
    renderpilot_orchestration::portable::runtime_paths().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_runtime_paths_missing",
            "portable startup was not installed",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::same_executable_identity;

    #[test]
    fn executable_identity_accepts_a_verbatim_path_alias() {
        let executable = std::env::current_exe().expect("current test executable");
        let alias = match executable.strip_prefix(r"\\?\") {
            Ok(path) => path.to_owned(),
            Err(_) => {
                let mut alias = std::ffi::OsString::from(r"\\?\");
                alias.push(executable.as_os_str());
                alias.into()
            }
        };

        assert_ne!(executable, alias);
        assert!(
            same_executable_identity(&executable, &alias).expect("compare executable identities")
        );
    }
}
