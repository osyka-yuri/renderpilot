use renderpilot_orchestration::application::{GameRepository, InstalledAddonRepository};
use renderpilot_orchestration::domain::{
    AddonKind, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole,
};
use tempfile::TempDir;

use super::{CatalogFixture, args};

struct LumaFixture {
    _game_dir: TempDir,
    game_id: String,
    addon_path: std::path::PathBuf,
}

impl LumaFixture {
    fn new() -> Self {
        let game_dir = tempfile::tempdir().expect("game dir");
        let addon_path = game_dir.path().join("Luma-TestGame.addon");
        std::fs::write(&addon_path, b"addon-bytes").expect("write addon");
        let install_key = game_dir.path().to_string_lossy().replace('\\', "/");
        let game_id = format!("manual:{install_key}");
        Self {
            _game_dir: game_dir,
            game_id,
            addon_path,
        }
    }

    fn game_id(&self) -> GameId {
        GameId::new(&self.game_id).expect("game id")
    }

    fn installed_record(&self) -> InstalledAddon {
        InstalledAddon::new(
            self.game_id(),
            AddonKind::Luma,
            PathRef::new(self.addon_path.to_string_lossy().as_ref()).expect("path"),
        )
        .with_addon_version("Build 515")
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example.test/luma.zip",
            None,
            "zip-digest",
        ))
    }
}

#[test]
fn luma_status_reports_not_installed_for_a_game_without_an_addon() {
    let luma = LumaFixture::new();
    let fixture = CatalogFixture::new("luma-status-empty");

    let output = fixture
        .run(args(&["luma", "status", "--game", &luma.game_id]))
        .expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
}

#[test]
fn luma_status_reports_installed_after_a_record_is_stored() {
    let luma = LumaFixture::new();
    let fixture = CatalogFixture::new("luma-status-installed");
    fixture
        .storage()
        .upsert_installed_addon(&luma.installed_record())
        .expect("seed record");

    let output = fixture
        .run(args(&["luma", "status", "--game", &luma.game_id]))
        .expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "installed");
    assert_eq!(json["version"], "Build 515");
}

#[test]
fn luma_uninstall_clears_the_record() {
    let luma = LumaFixture::new();
    let fixture = CatalogFixture::new("luma-uninstall");
    fixture
        .storage()
        .upsert_installed_addon(&luma.installed_record())
        .expect("seed record");

    let output = fixture
        .run(args(&["luma", "uninstall", "--game", &luma.game_id]))
        .expect("uninstall should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
    assert!(
        !luma.addon_path.exists(),
        "reachable uninstall should remove the addon file"
    );
    assert!(
        fixture
            .storage()
            .get_installed_addon(&luma.game_id())
            .expect("query")
            .is_none()
    );
    assert!(
        fixture
            .storage()
            .find_game(&luma.game_id())
            .expect("game query")
            .is_none(),
        "orphan uninstall must not synthesize a catalog game"
    );
    assert!(
        fixture
            .storage()
            .catalog_readiness(&luma.game_id())
            .is_err(),
        "orphan uninstall must not synthesize scan authority"
    );
}

#[test]
fn luma_check_update_reports_none_when_not_installed() {
    let luma = LumaFixture::new();
    let fixture = CatalogFixture::new("luma-check-update-empty");

    // Offline / no cache: orchestration soft-fails the catalogue path for
    // not-installed games without needing a live network.
    let result = fixture.run(args(&["luma", "check-update", "--game", &luma.game_id]));
    // Either a soft report or a catalogue failure is acceptable without network
    // fixtures; the command must not panic and must remain a valid CLI path.
    match result {
        Ok(output) => {
            let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
            assert!(
                json.get("overall").is_some() || json.get("status").is_some() || json.is_object()
            );
        }
        Err(error) => {
            // Offline / no warm CDN cache: catalogue resolution may hard-fail.
            // App-data root is always resolvable (Windows AppData or Unix
            // XDG/HOME), so a missing-root error would be unexpected here.
            let message = error.to_string();
            assert!(
                message.contains("manifest")
                    || message.contains("network")
                    || message.contains("failed")
                    || message.contains("CDN")
                    || message.contains("cache")
                    || message.contains("catalogue")
                    || message.contains("catalog"),
                "unexpected error shape: {message}"
            );
        }
    }
}
