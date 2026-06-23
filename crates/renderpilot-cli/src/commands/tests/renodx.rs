use renderpilot_orchestration::application::InstalledAddonRepository;
use renderpilot_orchestration::domain::{
    AddonKind, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole,
};

use crate::run;

use super::{args, CatalogFixture};

const GAME_ID: &str = "manual:C:/Games/RenoGame";

fn game_id() -> GameId {
    GameId::new(GAME_ID).expect("game id")
}

fn installed_record() -> InstalledAddon {
    InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/RenoGame/renodx-renogame.addon64").expect("path"),
    )
    .with_addon_version("snapshot-2026.06")
    // A Host source marks the install as managing the ReShade host.
    .with_tracked_source(TrackedSource::new(
        TrackedSourceRole::Host,
        "https://nightly.link/x64.zip",
        None,
        "host-digest",
    ))
}

#[test]
fn renodx_status_reports_not_installed_for_a_game_without_an_addon() {
    let _fixture = CatalogFixture::new("renodx-status-empty");

    let output = run(args(&["renodx", "status", "--game", GAME_ID])).expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
}

#[test]
fn renodx_status_reports_installed_after_a_record_is_stored() {
    let fixture = CatalogFixture::new("renodx-status-installed");
    fixture
        .storage()
        .upsert_installed_addon(&installed_record())
        .expect("seed record");

    let output = run(args(&["renodx", "status", "--game", GAME_ID])).expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "installed");
    assert_eq!(json["version"], "snapshot-2026.06");
    assert_eq!(json["reshade_managed_by_us"], true);
}

#[test]
fn renodx_uninstall_clears_the_record() {
    let fixture = CatalogFixture::new("renodx-uninstall");
    fixture
        .storage()
        .upsert_installed_addon(&installed_record())
        .expect("seed record");

    let output =
        run(args(&["renodx", "uninstall", "--game", GAME_ID])).expect("uninstall should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "not_installed");
    assert!(
        fixture
            .storage()
            .get_installed_addon(&game_id())
            .expect("query")
            .is_none(),
        "the install record should be cleared after uninstall",
    );
}

#[test]
fn renodx_uninstall_on_a_game_without_an_addon_is_an_error() {
    let _fixture = CatalogFixture::new("renodx-uninstall-empty");

    let error =
        run(args(&["renodx", "uninstall", "--game", GAME_ID])).expect_err("uninstall should fail");

    let message = error.to_string();
    assert!(
        message.contains("not installed"),
        "error should mention the missing install: {message}"
    );
}

#[test]
fn renodx_status_reports_foreign_host_when_no_host_source_is_recorded() {
    let fixture = CatalogFixture::new("renodx-status-foreign");
    // A foreign-host install records no Host source, so `reshade_managed_by_us`
    // reads as `false`.
    let foreign = InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/RenoGame/renodx-renogame.addon64").expect("path"),
    )
    .with_backed_up_file(PathRef::new("C:/Games/RenoGame/ReShade.ini").expect("path"));
    fixture
        .storage()
        .upsert_installed_addon(&foreign)
        .expect("seed foreign record");

    let output = run(args(&["renodx", "status", "--game", GAME_ID])).expect("status should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["status"], "installed");
    assert_eq!(json["reshade_managed_by_us"], false);
}
