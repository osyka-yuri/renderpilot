//! Resolve launcher identity for a game install folder.
//!
//! Auto- and manual scans both go through [`crate::ManualFolderGameSource`].
//! Without folder-level launcher detection, every re-scan would be tagged
//! `Launcher::Manual` even when the path belongs to Steam, GOG, or Epic.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use renderpilot_domain::Launcher;
use serde::Deserialize;

use crate::steam_appmanifest::{SteamInstallDetails, steam_install_details};

/// Optional override for Epic manifests directory (tests only).
static EPIC_MANIFESTS_DIR_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Launcher-backed identity discovered for one install folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallIdentityDetails {
    /// Store / launcher that owns this install.
    pub launcher: Launcher,
    /// Launcher-specific id when known (Steam app id, GOG product id, Epic app name).
    pub external_id: Option<String>,
    /// Display title from launcher metadata when available.
    pub display_name: Option<String>,
}

impl From<SteamInstallDetails> for InstallIdentityDetails {
    fn from(steam: SteamInstallDetails) -> Self {
        Self {
            launcher: Launcher::Steam,
            external_id: Some(steam.app_id),
            display_name: steam.display_name,
        }
    }
}

/// Detects launcher identity for `game_install_root`.
///
/// Order of preference:
/// 1. Steam (`…/steamapps/common/<dir>` + matching `appmanifest_*.acf`)
/// 2. GOG (`goggame-*.info` inside the install dir)
/// 3. Epic (`*.item` manifest whose `InstallLocation` matches this dir)
///
/// Returns `None` when no launcher metadata is found (true manual install).
pub fn detect_install_identity(game_install_root: &Path) -> Option<InstallIdentityDetails> {
    steam_identity(game_install_root)
        .or_else(|| gog_identity(game_install_root))
        .or_else(|| epic_identity(game_install_root))
}

fn steam_identity(game_install_root: &Path) -> Option<InstallIdentityDetails> {
    steam_install_details(game_install_root).map(InstallIdentityDetails::from)
}

// -----------------------------------------------------------------------------
// GOG — `goggame-<id>.info` inside the install directory
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GogInfo {
    #[serde(rename = "gameId", default)]
    game_id: Option<serde_json::Value>,
    #[serde(default)]
    name: Option<String>,
}

fn gog_identity(install_dir: &Path) -> Option<InstallIdentityDetails> {
    for entry in fs::read_dir(install_dir).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        let Some(file_name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
        else {
            continue;
        };

        if file_name.strip_circumfix("goggame-", ".info").is_none() {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if let Some(details) = gog_identity_from_info(&file_name, &content) {
            return Some(details);
        }
    }

    None
}

fn gog_identity_from_info(file_name: &str, content: &str) -> Option<InstallIdentityDetails> {
    let info: GogInfo = serde_json::from_str(content).ok()?;

    // Presence of a parseable goggame info file is enough to tag the install as
    // GOG. Companion / DLC info files still carry a product id in the filename.
    let external_id = info
        .game_id
        .as_ref()
        .and_then(json_value_as_id)
        .or_else(|| gog_product_id_from_file_name(file_name))?;

    let display_name = info.name.filter(|name| !name.trim().is_empty());

    Some(InstallIdentityDetails {
        launcher: Launcher::Gog,
        external_id: Some(external_id),
        display_name,
    })
}

fn gog_product_id_from_file_name(file_name: &str) -> Option<String> {
    let id = file_name.strip_circumfix("goggame-", ".info")?;

    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    Some(id.to_owned())
}

fn json_value_as_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

// -----------------------------------------------------------------------------
// Epic — `*.item` manifest whose `InstallLocation` is this directory
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EpicItem {
    #[serde(rename = "InstallLocation", default)]
    install_location: String,
    #[serde(rename = "AppName", default)]
    app_name: String,
    #[serde(rename = "CatalogItemId", default)]
    catalog_item_id: String,
    #[serde(rename = "DisplayName", default)]
    display_name: String,
    #[serde(rename = "LaunchExecutable", default)]
    launch_executable: String,
}

fn epic_identity(install_dir: &Path) -> Option<InstallIdentityDetails> {
    epic_identity_in_manifests_dir(install_dir, &epic_manifests_dir())
}

fn epic_identity_in_manifests_dir(
    install_dir: &Path,
    manifests_dir: &Path,
) -> Option<InstallIdentityDetails> {
    for entry in fs::read_dir(manifests_dir).ok()?.filter_map(Result::ok) {
        let path = entry.path();
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("item"))
        {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if let Some(details) = epic_identity_from_manifest(&content, install_dir) {
            return Some(details);
        }
    }

    None
}

fn epic_identity_from_manifest(
    content: &str,
    install_dir: &Path,
) -> Option<InstallIdentityDetails> {
    let item: EpicItem = serde_json::from_str(content).ok()?;

    if item.install_location.trim().is_empty() {
        return None;
    }

    if !same_install_dir(&item.install_location, install_dir) {
        return None;
    }

    // Prefer AppName (stable launcher id used by Epic tooling). Fall back to
    // CatalogItemId. LaunchExecutable alone is not an external id.
    let external_id = first_non_empty(&[&item.app_name, &item.catalog_item_id]);
    let display_name = first_non_empty(&[&item.display_name]);

    // Require at least one identifying field beyond install location so random
    // JSON under Manifests is not treated as an Epic game.
    if external_id.is_none() && display_name.is_none() && item.launch_executable.trim().is_empty() {
        return None;
    }

    Some(InstallIdentityDetails {
        launcher: Launcher::Epic,
        external_id,
        display_name,
    })
}

fn epic_manifests_dir() -> PathBuf {
    if let Ok(guard) = EPIC_MANIFESTS_DIR_OVERRIDE.lock()
        && let Some(override_dir) = guard.as_ref()
    {
        return override_dir.clone();
    }

    std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests")
}

fn first_non_empty(values: &[&str]) -> Option<String> {
    values
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

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
    use std::sync::Mutex as StdMutex;
    use tempfile::tempdir;

    /// Serializes tests that touch the process-global Epic manifests override.
    static EPIC_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    struct EpicManifestsOverride {
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl EpicManifestsOverride {
        fn set(path: PathBuf) -> Self {
            let guard = EPIC_TEST_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *EPIC_MANIFESTS_DIR_OVERRIDE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
            Self { _guard: guard }
        }
    }

    impl Drop for EpicManifestsOverride {
        fn drop(&mut self) {
            *EPIC_MANIFESTS_DIR_OVERRIDE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
    }

    #[test]
    fn gog_identity_reads_game_id_and_name_from_info() {
        let root = tempdir().expect("temp dir");
        fs::write(
            root.path().join("goggame-1207659999.info"),
            r#"{
                "gameId": "1207659999",
                "name": "The Witcher 3: Wild Hunt",
                "playTasks": [
                    { "type": "FileTask", "isPrimary": true, "path": "bin\\x64\\witcher3.exe" }
                ]
            }"#,
        )
        .expect("info");

        let details = detect_install_identity(root.path()).expect("gog identity");

        assert_eq!(details.launcher, Launcher::Gog);
        assert_eq!(details.external_id.as_deref(), Some("1207659999"));
        assert_eq!(
            details.display_name.as_deref(),
            Some("The Witcher 3: Wild Hunt")
        );
    }

    #[test]
    fn gog_identity_falls_back_to_file_name_product_id() {
        let root = tempdir().expect("temp dir");
        fs::write(
            root.path().join("goggame-42.info"),
            r#"{ "playTasks": [ { "type": "FileTask", "path": "Game.exe" } ] }"#,
        )
        .expect("info");

        let details = detect_install_identity(root.path()).expect("gog identity");

        assert_eq!(details.launcher, Launcher::Gog);
        assert_eq!(details.external_id.as_deref(), Some("42"));
    }

    #[test]
    fn gog_identity_skips_invalid_info_and_uses_next_valid_file() {
        let root = tempdir().expect("temp dir");
        fs::write(root.path().join("goggame-1.info"), "not-json{{{").expect("bad info");
        fs::write(
            root.path().join("goggame-99.info"),
            r#"{ "gameId": "99", "name": "Valid GOG Game" }"#,
        )
        .expect("good info");

        let details = detect_install_identity(root.path()).expect("gog identity");

        assert_eq!(details.launcher, Launcher::Gog);
        assert_eq!(details.external_id.as_deref(), Some("99"));
        assert_eq!(details.display_name.as_deref(), Some("Valid GOG Game"));
    }

    #[test]
    fn steam_identity_is_preferred_over_gog_file_in_same_folder() {
        let root = tempdir().expect("temp dir");
        let steamapps = root.path().join("steamapps");
        let common = steamapps.join("common").join("Portal");
        fs::create_dir_all(&common).expect("dirs");
        fs::write(
            steamapps.join("appmanifest_400.acf"),
            r#""AppState"
{
    "appid" "400"
    "installdir" "Portal"
    "name" "Portal"
}
"#,
        )
        .expect("acf");
        // Odd but possible: a GOG leftover file under a Steam install.
        fs::write(
            common.join("goggame-1.info"),
            r#"{ "gameId": "1", "name": "Not Portal" }"#,
        )
        .expect("gog info");

        let details = detect_install_identity(&common).expect("steam identity");

        assert_eq!(details.launcher, Launcher::Steam);
        assert_eq!(details.external_id.as_deref(), Some("400"));
        assert_eq!(details.display_name.as_deref(), Some("Portal"));
    }

    #[test]
    fn epic_identity_matches_install_location_via_parser() {
        let install = tempdir().expect("install dir");

        let details = epic_identity_from_manifest(
            &format!(
                r#"{{
                    "InstallLocation": "{}",
                    "AppName": "Fortnite",
                    "DisplayName": "Fortnite",
                    "LaunchExecutable": "FortniteClient-Win64-Shipping.exe"
                }}"#,
                install.path().display().to_string().replace('\\', "\\\\")
            ),
            install.path(),
        )
        .expect("epic identity");

        assert_eq!(details.launcher, Launcher::Epic);
        assert_eq!(details.external_id.as_deref(), Some("Fortnite"));
        assert_eq!(details.display_name.as_deref(), Some("Fortnite"));
    }

    #[test]
    fn epic_identity_reads_manifests_dir_end_to_end() {
        let install = tempdir().expect("install dir");
        let manifests = tempdir().expect("manifests dir");
        let _override = EpicManifestsOverride::set(manifests.path().to_path_buf());

        fs::write(
            manifests.path().join("Fortnite.item"),
            format!(
                r#"{{
                    "InstallLocation": "{}",
                    "AppName": "Fortnite",
                    "DisplayName": "Fortnite",
                    "LaunchExecutable": "FortniteClient-Win64-Shipping.exe"
                }}"#,
                install.path().display().to_string().replace('\\', "\\\\")
            ),
        )
        .expect("item");

        let details = detect_install_identity(install.path()).expect("epic identity");

        assert_eq!(details.launcher, Launcher::Epic);
        assert_eq!(details.external_id.as_deref(), Some("Fortnite"));
        assert_eq!(details.display_name.as_deref(), Some("Fortnite"));
    }

    #[test]
    fn bare_folder_has_no_install_identity() {
        let root = tempdir().expect("temp dir");

        assert!(detect_install_identity(root.path()).is_none());
    }
}
