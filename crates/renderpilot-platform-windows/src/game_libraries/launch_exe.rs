//! Authoritative launch-executable resolution from launcher metadata.
//!
//! A folder scan can only guess which binary is the game; the launcher itself
//! records the exact executable it runs. This reads that record for a given
//! install directory — GOG's per-install `goggame-*.info` and Epic's `*.item`
//! manifests — and returns the launch executable's basename. The JSON parsing is
//! pure (unit-tested with fixtures); locating the files on disk is the only
//! platform-specific part.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::paths::{env_path, has_extension_ignore_ascii_case};

/// The authoritative launch-executable basename for an install directory, read
/// from its launcher's metadata, or `None` when no launcher names one.
///
/// Tries GOG (a `goggame-*.info` inside the directory) then Epic (the `*.item`
/// manifest whose `InstallLocation` is this directory). The returned name is just
/// the basename; callers match it against the directory's scanned executables.
#[must_use]
pub fn launcher_launch_executable(install_dir: &Path) -> Option<String> {
    gog_launch_executable(install_dir).or_else(|| epic_launch_executable(install_dir))
}

// -----------------------------------------------------------------------------
// GOG — `goggame-<id>.info` inside the install directory
// -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct GogInfo {
    #[serde(rename = "playTasks", default)]
    play_tasks: Vec<GogPlayTask>,
}

#[derive(Deserialize)]
struct GogPlayTask {
    #[serde(rename = "type", default)]
    task_type: String,
    #[serde(rename = "isPrimary", default)]
    is_primary: bool,
    #[serde(default)]
    path: String,
}

/// The launch-exe basename from a GOG `goggame-*.info` document: the primary file
/// task's `path`, else the first file task's. Pure — fixture-testable.
fn gog_launch_exe_from_info(content: &str) -> Option<String> {
    let info: GogInfo = serde_json::from_str(content).ok()?;
    let mut first_file_task: Option<&str> = None;
    for task in &info.play_tasks {
        if !task.task_type.eq_ignore_ascii_case("FileTask") || task.path.is_empty() {
            continue;
        }
        if task.is_primary {
            return Some(base_name(&task.path));
        }
        first_file_task.get_or_insert(task.path.as_str());
    }
    first_file_task.map(base_name)
}

fn gog_launch_executable(install_dir: &Path) -> Option<String> {
    for entry in fs::read_dir(install_dir).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        let is_info = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_ascii_lowercase)
            .is_some_and(|name| name.starts_with("goggame-") && name.ends_with(".info"));
        if !is_info {
            continue;
        }
        if let Some(exe) = fs::read_to_string(&path)
            .ok()
            .and_then(|content| gog_launch_exe_from_info(&content))
        {
            return Some(exe);
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Epic — `*.item` manifest whose `InstallLocation` is this directory
// -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct EpicItem {
    #[serde(rename = "InstallLocation", default)]
    install_location: String,
    #[serde(rename = "LaunchExecutable", default)]
    launch_executable: String,
}

/// Parses an Epic `*.item` manifest, keeping it only when it names a launch
/// executable. Pure — fixture-testable.
fn epic_item_from_manifest(content: &str) -> Option<EpicItem> {
    let item: EpicItem = serde_json::from_str(content).ok()?;
    (!item.launch_executable.is_empty()).then_some(item)
}

fn epic_manifests_dir() -> PathBuf {
    env_path("PROGRAMDATA")
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests")
}

fn epic_launch_executable(install_dir: &Path) -> Option<String> {
    for entry in fs::read_dir(epic_manifests_dir())
        .ok()?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !has_extension_ignore_ascii_case(&path, "item") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(item) = epic_item_from_manifest(&content) else {
            continue;
        };
        if same_install_dir(&item.install_location, install_dir) {
            return Some(base_name(&item.launch_executable));
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Shared helpers
// -----------------------------------------------------------------------------

/// Basename of a path that may use either separator (launcher manifests mix `/`
/// and `\`).
fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_owned()
}

/// Whether a launcher-recorded install location refers to the same directory,
/// tolerant of separator, trailing-slash, and case differences (Windows paths).
fn same_install_dir(recorded: &str, dir: &Path) -> bool {
    normalize_dir(recorded) == normalize_dir(&dir.to_string_lossy())
}

fn normalize_dir(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn gog_info_returns_primary_file_task_basename() {
        let json = r#"{ "playTasks": [
            { "type": "FileTask", "isPrimary": false, "path": "launcher/launch.exe" },
            { "type": "FileTask", "isPrimary": true,  "path": "bin\\Game.exe" },
            { "type": "URLTask",  "isPrimary": true,  "path": "http://x" }
        ] }"#;
        assert_eq!(gog_launch_exe_from_info(json).as_deref(), Some("Game.exe"));
    }

    #[test]
    fn gog_info_falls_back_to_first_file_task() {
        let json = r#"{ "playTasks": [ { "type": "FileTask", "path": "Only.exe" } ] }"#;
        assert_eq!(gog_launch_exe_from_info(json).as_deref(), Some("Only.exe"));
    }

    #[test]
    fn gog_info_none_without_file_task_or_invalid() {
        assert_eq!(gog_launch_exe_from_info(r#"{ "playTasks": [] }"#), None);
        assert_eq!(gog_launch_exe_from_info("not json"), None);
    }

    #[test]
    fn gog_launch_executable_reads_info_in_dir() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("goggame-1207659999.info"),
            r#"{ "playTasks": [ { "type": "FileTask", "isPrimary": true, "path": "Witcher3.exe" } ] }"#,
        )
        .unwrap();
        assert_eq!(
            gog_launch_executable(tmp.path()).as_deref(),
            Some("Witcher3.exe")
        );
    }

    #[test]
    fn epic_item_extracts_launch_exe_and_location() {
        let json = r#"{ "InstallLocation": "C:\\Games\\Foo", "LaunchExecutable": "Bin/Foo.exe" }"#;
        let item = epic_item_from_manifest(json).expect("launch exe present");
        assert_eq!(base_name(&item.launch_executable), "Foo.exe");
        assert!(same_install_dir(
            &item.install_location,
            Path::new("C:/Games/Foo")
        ));
    }

    #[test]
    fn epic_item_none_without_launch_executable() {
        assert!(epic_item_from_manifest(r#"{ "InstallLocation": "C:\\X" }"#).is_none());
    }

    #[test]
    fn base_name_handles_both_separators() {
        assert_eq!(base_name("a/b/c.exe"), "c.exe");
        assert_eq!(base_name(r"a\b\c.exe"), "c.exe");
        assert_eq!(base_name("c.exe"), "c.exe");
    }

    #[test]
    fn same_install_dir_ignores_separator_trailing_slash_and_case() {
        assert!(same_install_dir(r"C:\Games\Foo", Path::new("c:/games/foo")));
        assert!(same_install_dir("C:/Games/Foo/", Path::new("C:/Games/Foo")));
        assert!(!same_install_dir(
            r"C:\Games\Foo",
            Path::new("C:/Games/Bar")
        ));
    }
}
