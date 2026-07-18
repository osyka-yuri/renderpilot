//! Advisory Visual C++ Redistributable presence check.
//!
//! Luma's README documents a Visual C++ Redistributable requirement; without it,
//! the add-on's `LoadLibrary` silently fails with no visible symptom. This is a
//! heuristic file-presence check only (a `HKLM\...\VC\Runtimes\<arch>` registry
//! read would be a more precise future refinement) — it never blocks an install,
//! only informs an advisory UI callout.

use std::env;
use std::path::{Path, PathBuf};

use renderpilot_domain::Architecture;

/// Runtime files a 64-bit process needs, under `%SystemRoot%\System32`.
const X64_RUNTIME_FILES: &[&str] = &["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"];
/// Runtime files a 32-bit process needs, under `%SystemRoot%\SysWOW64`.
const X86_RUNTIME_FILES: &[&str] = &["vcruntime140.dll", "msvcp140.dll"];

/// Whether the Visual C++ Redistributable Luma needs appears to be present, or
/// `None` when this can't be determined (the system directory can't be read).
#[must_use]
pub(super) fn vcredist_present(arch: Architecture) -> Option<bool> {
    vcredist_present_under(&windows_dir(), arch)
}

/// Official Microsoft-hosted installer URL for the redistributable this
/// architecture needs — the advisory callout's download link. A 32-bit game
/// needs the x86 build even on a 64-bit Windows install, so this must track
/// the game's architecture, not the host machine's.
#[must_use]
pub(super) fn vcredist_installer_url(arch: Architecture) -> &'static str {
    match arch {
        Architecture::X64 => "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        Architecture::X86 => "https://aka.ms/vs/17/release/vc_redist.x86.exe",
    }
}

fn windows_dir() -> PathBuf {
    env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn vcredist_present_under(windows_dir: &Path, arch: Architecture) -> Option<bool> {
    let (subdir, files) = match arch {
        Architecture::X64 => ("System32", X64_RUNTIME_FILES),
        Architecture::X86 => ("SysWOW64", X86_RUNTIME_FILES),
    };
    let dir = windows_dir.join(subdir);
    if !dir.is_dir() {
        return None;
    }
    Some(files.iter().all(|name| dir.join(name).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").expect("write");
    }

    #[test]
    fn x64_checks_system32_for_all_three_runtime_files() {
        let root = tempdir().expect("tempdir");
        let system32 = root.path().join("System32");
        std::fs::create_dir(&system32).expect("mkdir");
        assert_eq!(
            vcredist_present_under(root.path(), Architecture::X64),
            Some(false)
        );

        write(&system32, "vcruntime140.dll");
        write(&system32, "vcruntime140_1.dll");
        write(&system32, "msvcp140.dll");
        assert_eq!(
            vcredist_present_under(root.path(), Architecture::X64),
            Some(true)
        );
    }

    #[test]
    fn x64_is_false_when_only_some_runtime_files_are_present() {
        let root = tempdir().expect("tempdir");
        let system32 = root.path().join("System32");
        std::fs::create_dir(&system32).expect("mkdir");
        write(&system32, "vcruntime140.dll");
        assert_eq!(
            vcredist_present_under(root.path(), Architecture::X64),
            Some(false)
        );
    }

    #[test]
    fn x86_checks_syswow64_for_two_runtime_files() {
        let root = tempdir().expect("tempdir");
        let syswow64 = root.path().join("SysWOW64");
        std::fs::create_dir(&syswow64).expect("mkdir");
        write(&syswow64, "vcruntime140.dll");
        write(&syswow64, "msvcp140.dll");
        assert_eq!(
            vcredist_present_under(root.path(), Architecture::X86),
            Some(true)
        );
    }

    #[test]
    fn unreadable_system_directory_is_undetermined() {
        let root = tempdir().expect("tempdir");
        assert_eq!(vcredist_present_under(root.path(), Architecture::X64), None);
    }

    #[test]
    fn installer_url_tracks_the_games_architecture_not_the_host() {
        assert!(vcredist_installer_url(Architecture::X64).ends_with("vc_redist.x64.exe"));
        assert!(vcredist_installer_url(Architecture::X86).ends_with("vc_redist.x86.exe"));
        assert_ne!(
            vcredist_installer_url(Architecture::X64),
            vcredist_installer_url(Architecture::X86)
        );
    }
}
