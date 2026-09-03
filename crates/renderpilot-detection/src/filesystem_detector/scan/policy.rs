//! Directory exclusions for bounded probes and authoritative full scans.

use std::path::Path;

use super::InstallWalkMode;

/// Operating-system, volume-root, and expensive dependency-tree directories skipped
/// during advisory drive-wide probes.
const PROBE_EXCLUDED_DIRECTORY_NAMES: &[&str] = &[
    "windows",
    "system32",
    "syswow64",
    "system volume information",
    "$recycle.bin",
    "node_modules",
];

/// Non-shipping build target directories treated as non-runtime across all scan modes
/// (e.g. NVIDIA DLSS and Streamline Unreal Engine plugins ship development binaries
/// under `Development/` alongside shipping runtime DLLs).
const NON_RUNTIME_DIRECTORY_NAMES: &[&str] = &["development"];

/// Evaluates whether a directory should be skipped during installation traversal.
///
/// Dot-prefixed directories (`.*`) and non-runtime build directories are skipped
/// across all scan modes. System and large dependency directories are additionally
/// skipped in [`InstallWalkMode::Probe`].
pub(super) fn is_skipped_directory(path: &Path, mode: InstallWalkMode) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    // Authority-sensitive proofs must inspect the entire confirmed install
    // root. Their caller owns any narrow, explicit file exclusions.
    if mode == InstallWalkMode::FullStrict {
        return false;
    }

    // Dot-prefixed directories are treated as non-runtime metadata/tool directories.
    if name.starts_with('.') {
        return true;
    }

    // Non-runtime build output configurations.
    if NON_RUNTIME_DIRECTORY_NAMES
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
    {
        return true;
    }

    // System and large dependency trees skipped during broad drive probes.
    if mode == InstallWalkMode::Probe
        && PROBE_EXCLUDED_DIRECTORY_NAMES
            .iter()
            .any(|excluded| name.eq_ignore_ascii_case(excluded))
    {
        return true;
    }

    false
}
