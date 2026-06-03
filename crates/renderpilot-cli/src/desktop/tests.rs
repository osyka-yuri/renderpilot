use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::MutexGuard,
};

use renderpilot_application::{ArtifactRepository, ComponentRepository, GameRepository};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameIdentity, GameInstallation, GameRuntime, GraphicsComponent, GraphicsTechnology, Launcher,
    LibraryArtifact, PathRef, Platform, Sha256Hash, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;
use serde_json::Value;

use super::{
    apply_swap, get_catalog_setting, get_game_details, query_game_cards, rollback_component,
    scan_manual_folder, set_catalog_setting,
    utils::{dashboard_risk_level, library_tags, normalized_path_string as path_string},
};

#[cfg(windows)]
mod auto_scan;

use crate::catalog::CATALOG_DB_PATH_ENV;
use crate::hash::sha256_hex;
use crate::test_env::lock_process_env;

const DLSS_DLL: &str = "nvngx_dlss.dll";
const DEFAULT_PAGE_LIMIT: i64 = 10_000;

fn query_all_game_cards() -> Result<GameCardsQueryResult, crate::CliError> {
    GameCardsQueryResult::query(
        "",
        Vec::new(),
        Vec::new(),
        "title",
        "asc",
        DEFAULT_PAGE_LIMIT,
        0,
    )
}

#[test]
fn scan_manual_folder_updates_catalog_and_returns_detected_components() {
    let _fixture = DesktopFixture::new("scan-manual-single-game");
    let game_dir = tempfile::tempdir().expect("game dir");

    write_component_file(game_dir.path(), DLSS_DLL, b"desktop-scan-bytes");

    let result = scan_manual_folder(game_dir.path().to_path_buf()).expect("scan should succeed");
    let game_cards = query_all_game_cards().expect("game cards should succeed");

    let games = json_array_field(&result, "games");
    assert_eq!(games.len(), 1);

    let details = &games[0];
    let components = json_array_field(details, "components");
    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["technology"], "dlss_super_resolution");

    let cards = game_cards.items();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["install_path"], path_string(game_dir.path()));
    assert_eq!(cards[0]["title"], folder_title(game_dir.path()));
    assert_eq!(cards[0]["library_tags"][0], "dlss_super_resolution");
    assert_eq!(cards[0]["component_count"], 1);
    assert_eq!(cards[0]["updates_available"], false);
    assert_eq!(cards[0]["risk_level"], "low");
    assert_eq!(cards[0]["rollback_available"], false);
    assert!(
        cards[0]["cover_updated_at_ms"].is_null(),
        "cover timestamp should be absent before artwork is stored",
    );
}

#[test]
fn scan_manual_folder_parent_dir_produces_separate_games_per_subdirectory() {
    let _fixture = DesktopFixture::new("scan-manual-parent-dir");
    let parent_dir = tempfile::tempdir().expect("parent dir");

    let game_a = parent_dir.path().join("GameA");
    let game_b = parent_dir.path().join("GameB");

    write_component_file(&game_a, DLSS_DLL, b"game-a-bytes");
    write_component_file(&game_b, DLSS_DLL, b"game-b-bytes");

    let result = scan_manual_folder(parent_dir.path().to_path_buf()).expect("scan should succeed");
    let game_cards = query_all_game_cards().expect("game cards should succeed");

    let games = json_array_field(&result, "games");
    assert_eq!(
        games.len(),
        2,
        "should detect two separate game installations",
    );

    let cards = game_cards.items();
    assert_eq!(cards.len(), 2, "catalog should contain two game cards");

    assert_eq!(
        card_field_string_set(cards, "install_path"),
        BTreeSet::from([path_string(&game_a), path_string(&game_b)]),
    );
    assert_eq!(
        card_field_string_set(cards, "title"),
        string_set(&["GameA", "GameB"]),
    );
}

#[test]
fn scan_manual_folder_removes_stale_parent_entry_on_multi_scan() {
    let _fixture = DesktopFixture::new("scan-manual-removes-stale-parent");
    let parent_dir = tempfile::tempdir().expect("parent dir");

    let game_a = parent_dir.path().join("GameStaleA");
    let game_b = parent_dir.path().join("GameStaleB");

    write_component_file(&game_a, DLSS_DLL, b"a-bytes");
    write_component_file(&game_b, DLSS_DLL, b"b-bytes");

    scan_manual_folder(parent_dir.path().to_path_buf()).expect("first scan should succeed");
    scan_manual_folder(parent_dir.path().to_path_buf()).expect("second scan should succeed");

    let game_cards = query_all_game_cards().expect("game cards should succeed");
    let cards = game_cards.items();

    assert_eq!(
        cards.len(),
        2,
        "parent ghost entry should not exist after multi-scan",
    );
    assert_eq!(
        card_field_string_set(cards, "title"),
        string_set(&["GameStaleA", "GameStaleB"]),
    );
}

#[test]
fn scan_manual_folder_no_duplicates_when_scanning_subdirectory_after_parent() {
    let _fixture = DesktopFixture::new("scan-manual-no-parent-subdir-duplicates");
    let parent = tempfile::tempdir().expect("parent dir");

    let game_a = parent.path().join("Store").join("common").join("GameAlpha");
    let game_b = parent.path().join("Store").join("common").join("GameBeta");

    write_component_file(&game_a, DLSS_DLL, b"alpha-bytes");
    write_component_file(&game_b, DLSS_DLL, b"beta-bytes");

    scan_manual_folder(parent.path().to_path_buf()).expect("parent scan should succeed");
    scan_manual_folder(parent.path().join("Store")).expect("store scan should succeed");

    let game_cards = query_all_game_cards().expect("game cards should succeed");
    let cards = game_cards.items();
    let titles = card_field_string_set(cards, "title");

    assert_eq!(
        cards.len(),
        2,
        "scanning parent then sub-directory must produce exactly 2 entries, got: {}",
        titles.iter().cloned().collect::<Vec<_>>().join(", "),
    );
    assert!(titles.contains("GameAlpha"), "GameAlpha should be present");
    assert!(titles.contains("GameBeta"), "GameBeta should be present");
}

#[test]
fn query_game_cards_applies_search_library_and_include_without_filters() {
    let fixture = DesktopFixture::new("query-game-cards-filters");

    let game_a = sample_game("manual:C:/Games/Alpha", "Alpha Quest", "C:/Games/Alpha");
    let game_b = sample_game("manual:C:/Games/Beta", "Beta Ops", "C:/Games/Beta");
    let game_c = sample_game("manual:C:/Games/Gamma", "Gamma", "C:/Games/Gamma");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);
    fixture.store_game(&game_c);

    fixture.store_components(
        game_a.id(),
        &[sample_component_from_bytes(
            "component:alpha:dlss",
            game_a.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/Alpha/nvngx_dlss.dll",
            None,
            b"alpha",
        )],
    );
    fixture.store_components(
        game_b.id(),
        &[sample_component_from_bytes(
            "component:beta:streamline",
            game_b.id().as_str(),
            GraphicsTechnology::NvidiaStreamline,
            Swappability::Swappable,
            "C:/Games/Beta/sl.interposer.dll",
            None,
            b"beta",
        )],
    );
    fixture.store_components(game_c.id(), &[]);

    let result = GameCardsQueryResult::query(
        "a",
        vec![String::from("dlss_super_resolution")],
        Vec::new(),
        "title",
        "asc",
        50,
        0,
    )
    .expect("query should succeed");

    let items = result.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Alpha Quest");
    assert_eq!(result.total(), 1);
}

#[test]
fn query_game_cards_returns_total_available_libraries_and_paged_items() {
    let fixture = DesktopFixture::new("query-game-cards-page");

    let game_a = sample_game("manual:C:/Games/One", "One", "C:/Games/One");
    let game_b = sample_game("manual:C:/Games/Two", "Two", "C:/Games/Two");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);

    fixture.store_components(
        game_a.id(),
        &[sample_component_from_bytes(
            "component:one:dlssg",
            game_a.id().as_str(),
            GraphicsTechnology::DlssFrameGeneration,
            Swappability::Swappable,
            "C:/Games/One/nvngx_dlssg.dll",
            None,
            b"one",
        )],
    );
    fixture.store_components(
        game_b.id(),
        &[sample_component_from_bytes(
            "component:two:xess",
            game_b.id().as_str(),
            GraphicsTechnology::IntelXeSs,
            Swappability::Swappable,
            "C:/Games/Two/libxess.dll",
            None,
            b"two",
        )],
    );

    let result = GameCardsQueryResult::query("", Vec::new(), Vec::new(), "title", "asc", 1, 0)
        .expect("query should succeed");

    let available = result.available_libraries();

    assert_eq!(result.items().len(), 1, "limit=1 should page items");
    assert_eq!(result.total(), 2, "total should be count before paging");
    assert!(
        available.contains("dlss_frame_generation"),
        "available libraries should include detected tags",
    );
    assert!(
        available.contains("intel_xess"),
        "available libraries should include detected tags",
    );
    assert!(
        !result.query_fingerprint().is_empty(),
        "fingerprint should be present",
    );
}

#[test]
fn query_game_cards_excludes_unknown_from_tags_available_libraries_and_visible_count() {
    let fixture = DesktopFixture::new("query-game-cards-hide-unknown");

    let game = sample_game("manual:C:/Games/Visible", "Visible", "C:/Games/Visible");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component_from_bytes(
                "component:visible:dlss",
                game.id().as_str(),
                GraphicsTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/Visible/nvngx_dlss.dll",
                None,
                b"visible-dlss",
            ),
            sample_component_from_bytes(
                "component:visible:unknown",
                game.id().as_str(),
                GraphicsTechnology::Unknown,
                Swappability::ReadOnly,
                "C:/Games/Visible/mystery.dll",
                None,
                b"visible-unknown",
            ),
        ],
    );

    let result = query_all_game_cards().expect("query should succeed");
    let items = result.items();

    assert_eq!(items.len(), 1);
    assert_eq!(
        json_string_array_set(&items[0], "library_tags"),
        string_set(&["dlss_super_resolution"])
    );
    assert_eq!(items[0]["component_count"], 1);
    assert!(!result.available_libraries().contains("unknown"));
}

#[test]
fn query_game_cards_excludes_unknown_from_visible_update_count() {
    let fixture = DesktopFixture::new("query-game-cards-hide-unknown-updates");

    let game = sample_game(
        "manual:C:/Games/VisibleUpdates",
        "Visible Updates",
        "C:/Games/VisibleUpdates",
    );

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component_from_bytes(
                "component:visible-updates:dlss",
                game.id().as_str(),
                GraphicsTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/VisibleUpdates/nvngx_dlss.dll",
                Some("1.0.0"),
                b"visible-updates-dlss",
            ),
            sample_component_from_bytes(
                "component:visible-updates:unknown",
                game.id().as_str(),
                GraphicsTechnology::Unknown,
                Swappability::ReadOnly,
                "C:/Games/VisibleUpdates/mystery.dll",
                Some("1.0.0"),
                b"visible-updates-unknown",
            ),
        ],
    );
    fixture.store_artifact(sample_artifact_from_bytes(
        "artifact:visible-updates:dlss",
        GraphicsTechnology::DlssSuperResolution,
        "C:/Artifacts/visible-updates/nvngx_dlss.dll",
        Some("2.0.0"),
        b"artifact-visible-updates-dlss",
        Some(game.id().as_str()),
    ));
    fixture.store_artifact(sample_artifact_from_bytes(
        "artifact:visible-updates:unknown",
        GraphicsTechnology::Unknown,
        "C:/Artifacts/visible-updates/mystery.dll",
        Some("2.0.0"),
        b"artifact-visible-updates-unknown",
        Some(game.id().as_str()),
    ));

    let result = query_all_game_cards().expect("query should succeed");
    let items = result.items();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["update_count"], 1);
    assert_eq!(items[0]["updates_available"], true);
}

#[test]
fn query_game_cards_sorts_risk_by_domain_severity() {
    let fixture = DesktopFixture::new("query-game-cards-risk-order");

    let low_game = sample_game("manual:C:/Games/Low", "Low", "C:/Games/Low");
    let medium_game = sample_game("manual:C:/Games/Medium", "Medium", "C:/Games/Medium");
    let high_game = sample_game("manual:C:/Games/High", "High", "C:/Games/High");

    fixture.store_game(&low_game);
    fixture.store_game(&medium_game);
    fixture.store_game(&high_game);

    fixture.store_components(
        low_game.id(),
        &[sample_component_from_bytes(
            "component:low",
            low_game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/Low/a.dll",
            None,
            b"low",
        )],
    );
    fixture.store_components(
        medium_game.id(),
        &[sample_component_from_bytes(
            "component:medium",
            medium_game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::ReadOnly,
            "C:/Games/Medium/a.dll",
            None,
            b"medium",
        )],
    );
    fixture.store_components(
        high_game.id(),
        &[sample_component_from_bytes(
            "component:high",
            high_game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Unsafe,
            "C:/Games/High/a.dll",
            None,
            b"high",
        )],
    );

    let result = GameCardsQueryResult::query("", Vec::new(), Vec::new(), "risk", "desc", 10, 0)
        .expect("query should succeed");

    let items = result.items();
    assert_eq!(items[0]["risk_level"], "high");
    assert_eq!(items[1]["risk_level"], "medium");
    assert_eq!(items[2]["risk_level"], "low");
}

#[test]
fn set_catalog_setting_blank_value_removes_existing_setting() {
    let _fixture = DesktopFixture::new("settings-blank-removes");

    set_catalog_setting("games_filters_v3", r#"{"libraries":["x"]}"#).expect("set should succeed");
    set_catalog_setting("games_filters_v3", "   ").expect("blank set should delete row");

    let value = get_catalog_setting("games_filters_v3").expect("get should succeed");
    assert!(value["value"].is_null());
}

#[test]
fn desktop_apply_creates_sidecar_and_rollback_restores_original_bytes() {
    let fixture = DesktopFixture::new("desktop-apply-rollback");
    let game_folder = tempfile::tempdir().expect("game dir");
    let artifact_folder = tempfile::tempdir().expect("artifact dir");

    let source_path = game_folder.path().join(DLSS_DLL);
    let artifact_path = artifact_folder.path().join(DLSS_DLL);

    write_file(&source_path, b"original-bytes");
    write_file(&artifact_path, b"replacement-bytes");

    let install_path = path_string(game_folder.path());
    let game = sample_game(
        &format!("manual:{install_path}"),
        "Desktop Flow Game",
        &install_path,
    );

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component_from_bytes(
            "component:desktop:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&source_path),
            Some("3.5.0"),
            b"original-bytes",
        )],
    );
    fixture.store_artifact(sample_artifact_from_bytes(
        "artifact:desktop:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        b"replacement-bytes",
        None,
    ));

    let applied = apply_swap(
        game.id().as_str(),
        "component:desktop:dlss",
        "artifact:desktop:dlss-3.7",
    )
    .expect("apply should succeed");

    assert_eq!(applied["game_id"], game.id().as_str());
    assert_eq!(applied["component_id"], "component:desktop:dlss");
    assert_eq!(
        fs::read(&source_path).expect("source bytes should be readable"),
        b"replacement-bytes",
    );

    let sidecar_path = source_path.with_extension("dll.bak");
    assert!(
        sidecar_path.exists(),
        ".bak sidecar should exist next to target after apply"
    );
    assert_eq!(
        fs::read(&sidecar_path).expect("sidecar bytes should be readable"),
        b"original-bytes",
        ".bak sidecar should contain original bytes"
    );

    let rolled_back = rollback_component(game.id().as_str(), "component:desktop:dlss")
        .expect("rollback should succeed");

    assert_eq!(rolled_back["game_id"], game.id().as_str());
    assert_eq!(rolled_back["component_id"], "component:desktop:dlss");
    assert_eq!(
        fs::read(&source_path).expect("restored bytes should be readable"),
        b"original-bytes",
    );
}

#[test]
fn dashboard_risk_level_returns_unknown_without_components() {
    assert_eq!(dashboard_risk_level(&[]).as_str(), "unknown");
}

#[test]
fn dashboard_risk_level_returns_highest_component_risk() {
    let game_id = "manual:C:/Games/RiskFixture";
    let components = vec![
        sample_component_from_bytes(
            "component:risk:low",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/RiskFixture/low/nvngx_dlss.dll",
            None,
            b"risk-low",
        ),
        sample_component_from_bytes(
            "component:risk:medium",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::ReadOnly,
            "C:/Games/RiskFixture/medium/nvngx_dlss.dll",
            None,
            b"risk-medium",
        ),
        sample_component_from_bytes(
            "component:risk:high",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Unsafe,
            "C:/Games/RiskFixture/high/nvngx_dlss.dll",
            None,
            b"risk-high",
        ),
    ];

    assert_eq!(dashboard_risk_level(&components).as_str(), "high");
}

#[test]
fn library_tags_are_deduplicated() {
    let game_id = "manual:C:/Games/TagsFixture";
    let components = vec![
        sample_component_from_bytes(
            "component:tags:dlss-a",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/TagsFixture/a/nvngx_dlss.dll",
            None,
            b"tags-dlss-a",
        ),
        sample_component_from_bytes(
            "component:tags:dlss-b",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/TagsFixture/b/nvngx_dlss.dll",
            None,
            b"tags-dlss-b",
        ),
    ];

    assert_eq!(
        library_tags(&components),
        vec!["dlss_super_resolution".to_owned()],
    );
}

#[test]
fn library_tags_exclude_unknown_components() {
    let game_id = "manual:C:/Games/TagsUnknownFixture";
    let components = vec![
        sample_component_from_bytes(
            "component:tags-known",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/TagsUnknownFixture/nvngx_dlss.dll",
            None,
            b"tags-known",
        ),
        sample_component_from_bytes(
            "component:tags-unknown",
            game_id,
            GraphicsTechnology::Unknown,
            Swappability::ReadOnly,
            "C:/Games/TagsUnknownFixture/mystery.dll",
            None,
            b"tags-unknown",
        ),
    ];

    assert_eq!(
        library_tags(&components),
        vec!["dlss_super_resolution".to_owned()],
    );
}

#[test]
fn get_game_details_excludes_unknown_components_and_candidate_groups() {
    let fixture = DesktopFixture::new("get-game-details-hide-unknown");

    let game = sample_game("manual:C:/Games/Details", "Details", "C:/Games/Details");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component_from_bytes(
                "component:details:dlss",
                game.id().as_str(),
                GraphicsTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/Details/nvngx_dlss.dll",
                Some("1.0.0"),
                b"details-dlss",
            ),
            sample_component_from_bytes(
                "component:details:unknown",
                game.id().as_str(),
                GraphicsTechnology::Unknown,
                Swappability::ReadOnly,
                "C:/Games/Details/mystery.dll",
                Some("1.0.0"),
                b"details-unknown",
            ),
        ],
    );
    fixture.store_artifact(sample_artifact_from_bytes(
        "artifact:details:dlss",
        GraphicsTechnology::DlssSuperResolution,
        "C:/Artifacts/nvngx_dlss.dll",
        Some("2.0.0"),
        b"artifact-dlss",
        Some(game.id().as_str()),
    ));
    fixture.store_artifact(sample_artifact_from_bytes(
        "artifact:details:unknown",
        GraphicsTechnology::Unknown,
        "C:/Artifacts/mystery.dll",
        Some("2.0.0"),
        b"artifact-unknown",
        Some(game.id().as_str()),
    ));

    let result = get_game_details(game.id().as_str()).expect("details should load");
    let components = json_array_field(&result, "components");
    let candidate_groups = json_array_field(&result, "candidate_groups");

    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["technology"], "dlss_super_resolution");
    assert_eq!(candidate_groups.len(), 1);
    assert_eq!(candidate_groups[0]["technology"], "dlss_super_resolution");
}

#[test]
fn normalized_path_string_uses_forward_slashes() {
    let path = Path::new(r"C:\Games\RenderPilot\nvngx_dlss.dll");

    assert_eq!(path_string(path), "C:/Games/RenderPilot/nvngx_dlss.dll");
}

struct GameCardsQueryResult {
    value: Value,
}

impl GameCardsQueryResult {
    fn query(
        search_query: impl Into<String>,
        selected_libraries: Vec<String>,
        selected_launchers: Vec<String>,
        sort_field: impl Into<String>,
        sort_direction: impl Into<String>,
        page_limit: i64,
        page_offset: i64,
    ) -> Result<Self, crate::CliError> {
        let value = query_game_cards(
            search_query.into(),
            selected_libraries,
            selected_launchers,
            sort_field.into(),
            sort_direction.into(),
            page_limit,
            page_offset,
        )?;

        Ok(Self { value })
    }

    fn items(&self) -> &[Value] {
        json_array_field(&self.value, "items")
    }

    fn total(&self) -> i64 {
        json_i64_field(&self.value, "total")
    }

    fn available_libraries(&self) -> BTreeSet<String> {
        json_string_array_set(&self.value, "availableLibraries")
    }

    fn query_fingerprint(&self) -> &str {
        json_str_field(&self.value, "queryFingerprint")
    }
}

struct DesktopFixture {
    _temp_dir: tempfile::TempDir,
    _env: DesktopCatalogEnvGuard,
    storage: SqliteStorage,
}

impl DesktopFixture {
    fn new(name: &str) -> Self {
        let temp_dir = tempfile::Builder::new()
            .prefix(name)
            .tempdir()
            .expect("temp dir should be created");

        let db_path = temp_dir.path().join("catalog.db");
        let env = DesktopCatalogEnvGuard::new(db_path.clone());
        let storage = SqliteStorage::open(&db_path).expect("sqlite should open");

        Self {
            _temp_dir: temp_dir,
            _env: env,
            storage,
        }
    }

    fn store_game(&self, game: &GameInstallation) {
        self.storage.upsert_game(game).expect("game should store");
    }

    fn store_components(&self, game_id: &GameId, components: &[GraphicsComponent]) {
        self.storage
            .replace_components_for_game(game_id, components)
            .expect("components should store");
    }

    fn store_artifact(&self, artifact: LibraryArtifact) {
        self.storage
            .upsert_artifact(&artifact)
            .expect("artifact should store");
    }
}

struct DesktopCatalogEnvGuard {
    previous_db: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl DesktopCatalogEnvGuard {
    fn new(db_path: PathBuf) -> Self {
        let lock = lock_process_env();
        let previous_db = env::var_os(CATALOG_DB_PATH_ENV);

        env::set_var(CATALOG_DB_PATH_ENV, &db_path);

        Self {
            previous_db,
            _lock: lock,
        }
    }
}

impl Drop for DesktopCatalogEnvGuard {
    fn drop(&mut self) {
        restore_env_var(CATALOG_DB_PATH_ENV, &self.previous_db);
    }
}

fn restore_env_var(key: &str, previous: &Option<OsString>) {
    match previous {
        Some(value) => env::set_var(key, value),
        None => env::remove_var(key),
    }
}

fn sample_game(id: &str, title: &str, install_path: &str) -> GameInstallation {
    sample_game_with_launcher(id, title, install_path, Launcher::Manual, None)
}

fn sample_game_with_launcher(
    id: &str,
    title: &str,
    install_path: &str,
    launcher: Launcher,
    external_id: Option<&str>,
) -> GameInstallation {
    let mut identity = GameIdentity::new(
        GameId::new(id).expect("game id should be valid"),
        title,
        launcher,
    )
    .expect("game identity should be valid");

    if let Some(external_id) = external_id {
        identity = identity
            .with_external_id(external_id)
            .expect("external id should be valid");
    }

    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install_path).expect("install path should be valid"),
    )
}

fn sample_component_from_bytes(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    path: &str,
    version: Option<&str>,
    bytes: &[u8],
) -> GraphicsComponent {
    let sha256 = sha256_hex(bytes);

    sample_component(
        component_id,
        game_id,
        technology,
        swappability,
        path,
        version,
        &sha256,
    )
}

fn sample_component(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    path: &str,
    version: Option<&str>,
    sha256: &str,
) -> GraphicsComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    )
    .with_file(file)
}

fn sample_artifact_from_bytes(
    artifact_id: &str,
    technology: GraphicsTechnology,
    path: &str,
    version: Option<&str>,
    bytes: &[u8],
    source_game_id: Option<&str>,
) -> LibraryArtifact {
    let sha256 = sha256_hex(bytes);

    sample_artifact(
        artifact_id,
        technology,
        path,
        version,
        &sha256,
        source_game_id,
    )
}

fn sample_artifact(
    artifact_id: &str,
    technology: GraphicsTechnology,
    path: &str,
    version: Option<&str>,
    sha256: &str,
    source_game_id: Option<&str>,
) -> LibraryArtifact {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("artifact path should have file name");

    let mut file = ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    let artifact = LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        file_name,
        vec![file],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source("scan-folder")
    .expect("source should be valid");

    match source_game_id {
        Some(source_game_id) => artifact.with_source_game_id(
            GameId::new(source_game_id).expect("source game id should be valid"),
        ),
        None => artifact,
    }
}

fn write_component_file(dir: &Path, file_name: &str, bytes: &[u8]) -> PathBuf {
    fs::create_dir_all(dir).unwrap_or_else(|error| {
        panic!(
            "component directory should be created: {}: {error}",
            dir.display(),
        )
    });

    let path = dir.join(file_name);
    write_file(&path, bytes);

    path
}

fn write_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes)
        .unwrap_or_else(|error| panic!("test file should be written: {}: {error}", path.display()));
}

fn folder_title(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("folder name should be utf-8")
}

#[cfg(windows)]
fn stored_install_paths(storage: &SqliteStorage) -> BTreeSet<String> {
    storage
        .list_games()
        .expect("list games should succeed")
        .iter()
        .map(|game| game.install_path().as_str().to_owned())
        .collect()
}

fn card_field_string_set(cards: &[Value], field_name: &str) -> BTreeSet<String> {
    cards
        .iter()
        .map(|card| json_str_field(card, field_name).to_owned())
        .collect()
}

fn json_array_field<'a>(value: &'a Value, field_name: &str) -> &'a [Value] {
    value
        .get(field_name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("expected `{field_name}` to be an array"))
}

fn json_str_field<'a>(value: &'a Value, field_name: &str) -> &'a str {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected `{field_name}` to be a string"))
}

fn json_i64_field(value: &Value, field_name: &str) -> i64 {
    let field = value
        .get(field_name)
        .unwrap_or_else(|| panic!("expected `{field_name}` field to exist"));

    field
        .as_i64()
        .or_else(|| field.as_u64().and_then(|value| i64::try_from(value).ok()))
        .unwrap_or_else(|| panic!("expected `{field_name}` to be an integer"))
}

fn json_string_array_set(value: &Value, field_name: &str) -> BTreeSet<String> {
    json_array_field(value, field_name)
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("expected `{field_name}` items to be strings"))
                .to_owned()
        })
        .collect()
}

fn string_set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
