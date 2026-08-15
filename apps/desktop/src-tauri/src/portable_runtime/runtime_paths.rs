use std::sync::OnceLock;

use renderpilot_orchestration::portable::{RuntimePathsV1, install_runtime_paths};

use super::{
    app_protocol::PortableAppSessionV2,
    error::{PortableRuntimeError, Result},
    image_authority::SelectedGenerationImage,
    root_authority::PortableRootAuthority,
    win32::object::running_app_identity,
};

const WEBVIEW_ENV: &str = "WEBVIEW2_USER_DATA_FOLDER";
const DB_ENV: &str = "RENDERPILOT_DB_PATH";

/// The sole App-side authority publication.  `RuntimePathsV1` remains the
/// unchanged orchestration compatibility projection, but it is never the
/// authority source by itself.
#[derive(Debug)]
pub(crate) struct AuthenticatedAppRuntime {
    paths: RuntimePathsV1,
    root: PortableRootAuthority,
    _selected_generation: SelectedGenerationImage,
}

impl AuthenticatedAppRuntime {
    pub(crate) fn paths(&self) -> &RuntimePathsV1 {
        &self.paths
    }

    pub(crate) fn root(&self) -> &PortableRootAuthority {
        &self.root
    }
}

static AUTHENTICATED_APP_RUNTIME: OnceLock<AuthenticatedAppRuntime> = OnceLock::new();

/// Validates startup, obtains retained root/generation/App capabilities, and
/// publishes authority before the compatibility path projection and env vars.
#[expect(
    unsafe_code,
    reason = "startup runs before threads and projects authenticated paths into legacy environment compatibility variables"
)]
pub fn install_from_startup(startup: &PortableAppSessionV2) -> Result<()> {
    startup.validate()?;
    startup
        .runtime_paths
        .validate()
        .map_err(|detail| PortableRuntimeError::new("portable_runtime_paths", detail))?;
    let root = PortableRootAuthority::open(&startup.runtime_paths.portable_root)?;
    if root.identity().as_str() != startup.portable_root_identity {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "retained portable root identity did not match the signed startup binding",
        ));
    }
    let selected_generation = SelectedGenerationImage::open(&root, &startup.generation_sha256)?;
    if selected_generation.generation_identity().as_str() != startup.generation_root_identity {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "retained selected generation identity did not match startup",
        ));
    }
    let running = running_app_identity(&std::env::current_exe()?)?;
    if &running != selected_generation.app().identity() {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "running App identity differed from retained selected App image",
        ));
    }
    if AUTHENTICATED_APP_RUNTIME.get().is_some()
        || renderpilot_orchestration::portable::runtime_paths().is_some()
    {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "portable App runtime authority was already published",
        ));
    }
    let paths = startup.runtime_paths.clone();
    AUTHENTICATED_APP_RUNTIME
        .set(AuthenticatedAppRuntime {
            paths: paths.clone(),
            root,
            _selected_generation: selected_generation,
        })
        .map_err(|_| {
            PortableRuntimeError::new(
                "portable_runtime_paths",
                "atomic authenticated runtime publication raced",
            )
        })?;
    // The compatibility cell has already been proven empty; the only permitted
    // projection is the exact authority tuple's unmodified RuntimePathsV1.
    install_runtime_paths(paths)
        .map_err(|detail| PortableRuntimeError::new("portable_runtime_paths", detail))?;
    let paths = current()?;
    // SAFETY: the authenticated portable child has not initialized logger,
    // WebView2, Tauri, or worker threads yet.
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

pub(crate) fn current_runtime() -> Result<&'static AuthenticatedAppRuntime> {
    AUTHENTICATED_APP_RUNTIME.get().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_runtime_paths_missing",
            "portable authenticated App runtime was not installed",
        )
    })
}

pub fn current() -> Result<&'static RuntimePathsV1> {
    Ok(current_runtime()?.paths())
}

pub(crate) fn current_root() -> Result<&'static PortableRootAuthority> {
    Ok(current_runtime()?.root())
}

#[cfg(test)]
mod tests {
    #[test]
    fn authenticated_runtime_publishes_before_projection_and_environment() {
        let source = include_str!("runtime_paths.rs");
        let capabilities = source
            .find("let root = PortableRootAuthority::open")
            .expect("retained root");
        let empty = source
            .find("AUTHENTICATED_APP_RUNTIME.get().is_some()")
            .expect("both cells checked");
        let publish = source
            .find("AUTHENTICATED_APP_RUNTIME\n        .set")
            .expect("authority publication");
        let projection = source
            .find("install_runtime_paths(paths)")
            .expect("compatibility projection");
        let environment = source
            .find("std::env::set_var")
            .expect("environment projection");
        assert!(capabilities < empty && empty < publish && publish < projection);
        assert!(projection < environment);
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(!production.contains("directory_identity_digest_no_reparse"));
        assert!(!production.contains("file_identity_digest_no_reparse"));
    }
}
