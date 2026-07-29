//! Directory exclusions for bounded probes and authoritative full scans.

use std::{ffi::OsStr, path::Path};

use super::InstallWalkMode;

const SYSTEM_DIRECTORY_NAMES: &[&str] = &[
    "windows",
    "system32",
    "syswow64",
    "system volume information",
    "$recycle.bin",
];
const TOOL_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "_dlsswapper_backups",
    "_renderpilot_backups",
];
const FULL_SCAN_EXCLUDED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "_dlsswapper_backups",
    "_renderpilot_backups",
];

pub(super) fn is_skipped_directory(path: &Path, mode: InstallWalkMode) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };

    match mode {
        InstallWalkMode::Probe => SYSTEM_DIRECTORY_NAMES
            .iter()
            .chain(TOOL_DIRECTORY_NAMES)
            .any(|excluded| name.eq_ignore_ascii_case(OsStr::new(excluded))),
        InstallWalkMode::Full => FULL_SCAN_EXCLUDED_DIRECTORY_NAMES
            .iter()
            .any(|excluded| name.eq_ignore_ascii_case(OsStr::new(excluded))),
    }
}
