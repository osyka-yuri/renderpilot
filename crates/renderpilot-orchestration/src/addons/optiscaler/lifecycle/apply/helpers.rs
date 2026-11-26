use super::super::*;
use std::path::{Path, PathBuf};

pub(in crate::addons::optiscaler) fn release_has_private_runtime(
    release: &OptiScalerRelease,
) -> bool {
    release
        .members
        .iter()
        .any(|member| member.target.replace('\\', "/").starts_with("OptiScaler/"))
}

pub(in crate::addons::optiscaler) fn module_library_path(
    release: &OptiScalerRelease,
    module: &str,
    target_dir: &Path,
    native_path: Option<&Path>,
    fallback: &str,
) -> PathBuf {
    release
        .members
        .iter()
        .find(|member| member.module == module)
        .map(|member| member.target.as_str())
        .filter(|target| *target != "$proxy")
        .map(|target| target_dir.join(target))
        .or_else(|| native_path.map(Path::to_path_buf))
        .unwrap_or_else(|| target_dir.join(fallback))
}

pub(in crate::addons::optiscaler) fn ini_library_path(target_dir: &Path, path: &Path) -> String {
    let display_path = path.trim_prefix(target_dir);
    let normalized = display_path.to_string_lossy().replace('\\', "/");
    if normalized == "." || normalized.is_empty() {
        ".\\".to_owned()
    } else if path.is_absolute() && display_path == path {
        normalized.replace('/', "\\")
    } else {
        format!(".\\{}", normalized.replace('/', "\\"))
    }
}
