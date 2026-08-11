//! Fail-closed selection for the Windows application manifest embedded by
//! `tauri-build`. Kept independent from Cargo/Tauri so this file has focused
//! unit coverage without compiling the desktop application.

use std::{env, path::Path};

const MANIFEST_SELECTOR_ENV: &str = "RENDERPILOT_WINDOWS_MANIFEST";
const DEVELOPMENT_MANIFEST: &str = include_str!("windows-manifests/development.manifest.xml");
const PRODUCTION_MANIFEST: &str = include_str!("windows-manifests/production.manifest.xml");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsManifest {
    Development,
    Production,
}

impl WindowsManifest {
    pub const fn contents(self) -> &'static str {
        match self {
            Self::Development => DEVELOPMENT_MANIFEST,
            Self::Production => PRODUCTION_MANIFEST,
        }
    }
}

/// Selects the manifest for a Cargo target. A non-Windows target is inert: its
/// profile and selector do not affect the build.
pub fn select(
    target_os: &str,
    profile: &str,
    selector: Option<&str>,
) -> Result<Option<WindowsManifest>, String> {
    if target_os != "windows" {
        return Ok(None);
    }

    match profile {
        "release" => match selector {
            Some("production") => Ok(Some(WindowsManifest::Production)),
            Some("release-tooling") => Ok(Some(WindowsManifest::Development)),
            None => Err(format!(
                "release Windows builds require {MANIFEST_SELECTOR_ENV}=production or release-tooling"
            )),
            Some(value) => Err(format!(
                "release Windows builds reject {MANIFEST_SELECTOR_ENV}={value:?}; expected production or release-tooling"
            )),
        },
        _ => match selector {
            None | Some("development") => Ok(Some(WindowsManifest::Development)),
            Some(value) => Err(format!(
                "non-release Windows builds reject {MANIFEST_SELECTOR_ENV}={value:?}; expected development or an unset value"
            )),
        },
    }
}

pub fn select_from_environment() -> Result<Option<WindowsManifest>, String> {
    let target_os = env::var("CARGO_CFG_TARGET_OS")
        .map_err(|_| "CARGO_CFG_TARGET_OS must be set by Cargo".to_owned())?;
    if target_os != "windows" {
        return Ok(None);
    }

    let profile = env::var("PROFILE").map_err(|_| "PROFILE must be set by Cargo".to_owned())?;
    select(
        &target_os,
        &profile,
        env::var(MANIFEST_SELECTOR_ENV).ok().as_deref(),
    )
}

pub fn emit_rerun_directives(manifest_dir: &Path) {
    println!("cargo:rerun-if-env-changed={MANIFEST_SELECTOR_ENV}");
    for manifest in [
        "build-support/windows-manifests/development.manifest.xml",
        "build-support/windows-manifests/production.manifest.xml",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join(manifest).display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{WindowsManifest, select};

    #[test]
    fn selects_production_only_for_explicit_windows_release() {
        assert_eq!(
            select("windows", "release", Some("production")),
            Ok(Some(WindowsManifest::Production))
        );
    }

    #[test]
    fn selects_development_for_explicit_windows_release_tooling() {
        assert_eq!(
            select("windows", "release", Some("release-tooling")),
            Ok(Some(WindowsManifest::Development))
        );
    }

    #[test]
    fn rejects_unqualified_or_unknown_windows_release() {
        for selector in [None, Some("development"), Some("unknown")] {
            assert!(select("windows", "release", selector).is_err());
        }
    }

    #[test]
    fn selects_development_for_unset_or_development_non_release() {
        for selector in [None, Some("development")] {
            assert_eq!(
                select("windows", "debug", selector),
                Ok(Some(WindowsManifest::Development))
            );
        }
    }

    #[test]
    fn rejects_other_non_release_selectors() {
        for selector in [Some("production"), Some("release-tooling"), Some("unknown")] {
            assert!(select("windows", "debug", selector).is_err());
        }
    }

    #[test]
    fn leaves_non_windows_targets_inert() {
        for target in ["linux", "macos"] {
            assert_eq!(select(target, "release", None), Ok(None));
            assert_eq!(select(target, "release", Some("unknown")), Ok(None));
        }
    }
}
