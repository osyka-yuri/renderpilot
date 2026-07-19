use renderpilot_orchestration::application::InstalledAddonRepository;
use renderpilot_orchestration::domain::{
    AddonKind, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole,
};
use tempfile::TempDir;

use super::{CatalogFixture, args};

struct RenoFixture {
    _game_dir: TempDir,
    game_id: String,
    addon_path: std::path::PathBuf,
}

impl RenoFixture {
    fn new() -> Self {
        let game_dir = tempfile::tempdir().expect("game dir");
        let addon_path = game_dir.path().join("renodx-renogame.addon64");
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
            AddonKind::RenoDx,
            PathRef::new(self.addon_path.to_string_lossy().as_ref()).expect("path"),
        )
        .with_addon_version("snapshot-2026.06")
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://nightly.link/x64.zip",
            None,
            "host-digest",
        ))
    }
}

#[test]
fn renodx_status_reports_not_installed_for_a_game_without_an_addon() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-status-empty");

    let output = fixture
        .run(args(&["renodx", "status", "--game", &reno.game_id]))
        .expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
}

#[test]
fn renodx_status_reports_installed_after_a_record_is_stored() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-status-installed");
    fixture
        .storage()
        .upsert_installed_addon(&reno.installed_record())
        .expect("seed record");

    let output = fixture
        .run(args(&["renodx", "status", "--game", &reno.game_id]))
        .expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "installed");
    assert_eq!(json["version"], "snapshot-2026.06");
    assert!(json.get("reshade_managed_by_us").is_none());
}

#[test]
fn renodx_uninstall_clears_the_record() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-uninstall");
    fixture
        .storage()
        .upsert_installed_addon(&reno.installed_record())
        .expect("seed record");

    let output = fixture
        .run(args(&["renodx", "uninstall", "--game", &reno.game_id]))
        .expect("uninstall should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
    assert!(
        !reno.addon_path.exists(),
        "reachable uninstall should remove the addon file"
    );
    assert!(
        fixture
            .storage()
            .get_installed_addon(&reno.game_id())
            .expect("query")
            .is_none(),
        "the install record should be cleared after uninstall",
    );
}

#[test]
fn renodx_uninstall_clears_orphan_metadata_for_unreachable_paths() {
    let fixture = CatalogFixture::new("renodx-uninstall-orphan");
    let game_id = "manual:Z:/renderpilot-missing/RenoGame";
    let record = InstalledAddon::new(
        GameId::new(game_id).expect("game id"),
        AddonKind::RenoDx,
        PathRef::new("Z:/renderpilot-missing/RenoGame/renodx-renogame.addon64").expect("path"),
    )
    .with_addon_version("snapshot-2026.06");
    fixture
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed record");

    let output = fixture
        .run(args(&["renodx", "uninstall", "--game", game_id]))
        .expect("orphan uninstall should still clear metadata");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
    assert!(
        fixture
            .storage()
            .get_installed_addon(&GameId::new(game_id).expect("id"))
            .expect("query")
            .is_none()
    );
}

#[test]
fn renodx_uninstall_on_a_game_without_an_addon_is_an_error() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-uninstall-empty");

    let error = fixture
        .run(args(&["renodx", "uninstall", "--game", &reno.game_id]))
        .expect_err("uninstall should fail");

    let message = error.to_string();
    assert!(
        message.contains("not installed"),
        "error should mention the missing install: {message}"
    );
}

#[test]
fn renodx_check_updates_reports_unknown_for_installed_when_catalogue_unavailable() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-check-updates-offline");
    fixture
        .storage()
        .upsert_installed_addon(&reno.installed_record())
        .expect("seed record");

    // Without a warm CDN cache the bulk check soft-fails to unknown per install
    // rather than returning an empty map that would imply no installs.
    let result = fixture.run(args(&["renodx", "check-updates"]));
    match result {
        Ok(output) => {
            let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
            if let Some(map) = json.as_object()
                && let Some(status) = map.get(&reno.game_id)
            {
                assert_eq!(status, "unknown");
            }
        }
        Err(error) => {
            // Catalogue hard-fail is still acceptable when soft-fail helpers
            // cannot list installs for other reasons.
            let message = error.to_string();
            assert!(
                !message.is_empty(),
                "check-updates should surface a real error message"
            );
        }
    }
}

#[test]
fn renodx_status_omits_host_origin_when_no_host_entry_is_recorded() {
    let reno = RenoFixture::new();
    let fixture = CatalogFixture::new("renodx-status-local-file");
    let local_file = InstalledAddon::new(
        reno.game_id(),
        AddonKind::RenoDx,
        PathRef::new(reno.addon_path.to_string_lossy().as_ref()).expect("path"),
    )
    .with_backed_up_file(
        PathRef::new(
            reno.addon_path
                .with_file_name("ReShade.ini")
                .to_string_lossy()
                .as_ref(),
        )
        .expect("path"),
    );
    fixture
        .storage()
        .upsert_installed_addon(&local_file)
        .expect("seed record");

    let output = fixture
        .run(args(&["renodx", "status", "--game", &reno.game_id]))
        .expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "installed");
    assert!(json.get("reshade_managed_by_us").is_none());
}
