use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::MutexGuard,
};

use renderpilot_application::{ArtifactRepository, ComponentRepository, GameRepository};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameIdentity, GameInstallation, GameRuntime, GraphicsTechnology, Launcher, LibraryArtifact,
    PathRef, Platform, Sha256Hash, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::{
    apply_operation_plan, build_swap_plan, get_game_cards, rollback_operation, scan_manual_folder,
    utils::{dashboard_risk_level, normalized_path_string as path_string, technology_tags},
};
use crate::hash::sha256_hex;
use crate::test_env::lock_process_env;
use crate::{backup_manager::BACKUP_ROOT_DIR_ENV, catalog::CATALOG_DB_PATH_ENV};

#[test]
fn scan_manual_folder_updates_catalog_and_returns_detected_components() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("catalog.db");
    let _guard = DesktopCatalogEnvGuard::new(db_path);

    let game_dir = tempfile::tempdir().expect("game dir");

    fs::write(
        game_dir.path().join("nvngx_dlss.dll"),
        b"desktop-scan-bytes",
    )
    .expect("test dll should be written");

    let result = scan_manual_folder(game_dir.path().to_path_buf()).expect("scan should succeed");
    let game_cards = get_game_cards().expect("game cards should succeed");

    let games = result["games"].as_array().expect("games array");
    assert_eq!(games.len(), 1);

    let details = &games[0];
    assert_eq!(
        details["components"]
            .as_array()
            .expect("components array")
            .len(),
        1
    );
    assert_eq!(
        details["components"][0]["technology"],
        "dlss_super_resolution"
    );

    let cards = game_cards.as_array().expect("game cards array");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["install_path"], path_string(game_dir.path()));
    assert_eq!(
        cards[0]["title"],
        game_dir
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("folder name should be utf-8")
    );
    assert_eq!(cards[0]["technology_tags"][0], "dlss_super_resolution");
    assert_eq!(cards[0]["component_count"], 1);
    assert_eq!(cards[0]["updates_available"], false);
    assert_eq!(cards[0]["risk_level"], "low");
    assert_eq!(cards[0]["backup_available"], false);
    assert!(
        cards[0]["cover_updated_at_ms"].is_null(),
        "cover timestamp should be absent before artwork is stored",
    );
}

#[test]
fn scan_manual_folder_parent_dir_produces_separate_games_per_subdirectory() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("catalog.db");
    let _guard = DesktopCatalogEnvGuard::new(db_path);

    let parent_dir = tempfile::tempdir().expect("parent dir");

    let game_a = parent_dir.path().join("GameA");
    let game_b = parent_dir.path().join("GameB");

    fs::create_dir_all(&game_a).expect("GameA dir should be created");
    fs::create_dir_all(&game_b).expect("GameB dir should be created");
    fs::write(game_a.join("nvngx_dlss.dll"), b"game-a-bytes").expect("GameA dll should be written");
    fs::write(game_b.join("nvngx_dlss.dll"), b"game-b-bytes").expect("GameB dll should be written");

    let result = scan_manual_folder(parent_dir.path().to_path_buf()).expect("scan should succeed");
    let game_cards = get_game_cards().expect("game cards should succeed");

    let games = result["games"].as_array().expect("games array");
    assert_eq!(
        games.len(),
        2,
        "should detect two separate game installations"
    );

    let cards = game_cards.as_array().expect("game cards array");
    assert_eq!(cards.len(), 2, "catalog should contain two game cards");

    let install_paths: Vec<&str> = cards
        .iter()
        .map(|card| card["install_path"].as_str().expect("install_path string"))
        .collect();

    assert!(
        install_paths.contains(&path_string(&game_a).as_str()),
        "GameA install path should be in catalog"
    );
    assert!(
        install_paths.contains(&path_string(&game_b).as_str()),
        "GameB install path should be in catalog"
    );

    let titles: Vec<&str> = cards
        .iter()
        .map(|card| card["title"].as_str().expect("title string"))
        .collect();

    assert!(
        titles.contains(&"GameA"),
        "GameA title should be in catalog"
    );
    assert!(
        titles.contains(&"GameB"),
        "GameB title should be in catalog"
    );
}

#[test]
fn scan_manual_folder_removes_stale_parent_entry_on_multi_scan() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("catalog.db");
    let _guard = DesktopCatalogEnvGuard::new(db_path);

    let parent_dir = tempfile::tempdir().expect("parent dir");

    let game_a = parent_dir.path().join("GameStaleA");
    let game_b = parent_dir.path().join("GameStaleB");

    fs::create_dir_all(&game_a).expect("GameStaleA dir should be created");
    fs::create_dir_all(&game_b).expect("GameStaleB dir should be created");
    fs::write(game_a.join("nvngx_dlss.dll"), b"a-bytes").expect("dll a should be written");
    fs::write(game_b.join("nvngx_dlss.dll"), b"b-bytes").expect("dll b should be written");

    scan_manual_folder(parent_dir.path().to_path_buf()).expect("first scan should succeed");
    scan_manual_folder(parent_dir.path().to_path_buf()).expect("second scan should succeed");

    let game_cards = get_game_cards().expect("game cards should succeed");
    let cards = game_cards.as_array().expect("game cards array");

    assert_eq!(
        cards.len(),
        2,
        "parent ghost entry should not exist after multi-scan"
    );

    let titles: Vec<&str> = cards
        .iter()
        .map(|card| card["title"].as_str().expect("title"))
        .collect();

    assert!(titles.contains(&"GameStaleA"));
    assert!(titles.contains(&"GameStaleB"));
}

#[test]
fn scan_manual_folder_no_duplicates_when_scanning_subdirectory_after_parent() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("catalog.db");
    let _guard = DesktopCatalogEnvGuard::new(db_path);

    let parent = tempfile::tempdir().expect("parent dir");

    let game_a = parent.path().join("Store").join("common").join("GameAlpha");
    let game_b = parent.path().join("Store").join("common").join("GameBeta");

    fs::create_dir_all(&game_a).expect("GameAlpha dir should be created");
    fs::create_dir_all(&game_b).expect("GameBeta dir should be created");
    fs::write(game_a.join("nvngx_dlss.dll"), b"alpha-bytes")
        .expect("GameAlpha dll should be written");
    fs::write(game_b.join("nvngx_dlss.dll"), b"beta-bytes")
        .expect("GameBeta dll should be written");

    scan_manual_folder(parent.path().to_path_buf()).expect("parent scan should succeed");
    scan_manual_folder(parent.path().join("Store")).expect("store scan should succeed");

    let game_cards = get_game_cards().expect("game cards should succeed");
    let cards = game_cards.as_array().expect("game cards array");

    let titles: Vec<&str> = cards
        .iter()
        .map(|card| card["title"].as_str().unwrap_or("?"))
        .collect();

    assert_eq!(
        cards.len(),
        2,
        "scanning parent then sub-directory must produce exactly 2 entries, got: {}",
        titles.join(", ")
    );
    assert!(titles.contains(&"GameAlpha"), "GameAlpha should be present");
    assert!(titles.contains(&"GameBeta"), "GameBeta should be present");
}

#[test]
fn desktop_apply_creates_backup_and_rollback_restores_original_bytes() {
    let fixture = DesktopFixture::new("desktop-apply-rollback");
    let game_folder = tempfile::tempdir().expect("game dir");
    let artifact_folder = tempfile::tempdir().expect("artifact dir");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");

    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let game = sample_game(
        &format!("manual:{install_path}"),
        "Desktop Flow Game",
        &install_path,
    );

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:desktop:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&source_path),
            Some("3.5.0"),
            &sha256_hex(b"original-bytes"),
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:desktop:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &sha256_hex(b"replacement-bytes"),
        None,
    ));

    let plan = build_swap_plan(
        game.id().as_str(),
        "component:desktop:dlss",
        "artifact:desktop:dlss-3.7",
    )
    .expect("plan should build");

    let operation_id = plan["operation_id"]
        .as_str()
        .expect("operation id should be string")
        .to_owned();
    let confirmation_token = plan["confirmation_token"]
        .as_str()
        .expect("confirmation token should be string")
        .to_owned();

    let applied = apply_operation_plan(operation_id.clone(), confirmation_token)
        .expect("apply should succeed");

    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["items"].as_array().expect("items array").len(), 1);
    assert_eq!(
        fs::read(&source_path).expect("source bytes should be readable"),
        b"replacement-bytes"
    );

    let rolled_back = rollback_operation(operation_id).expect("rollback should succeed");

    assert_eq!(rolled_back["status"], "rolled_back");
    assert_eq!(
        fs::read(&source_path).expect("restored bytes should be readable"),
        b"original-bytes"
    );
}

#[test]
fn desktop_apply_rejects_invalid_confirmation_token() {
    let fixture = DesktopFixture::new("desktop-invalid-confirmation-token");
    let game_folder = tempfile::tempdir().expect("game dir");
    let artifact_folder = tempfile::tempdir().expect("artifact dir");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");

    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let game = sample_game(
        &format!("manual:{install_path}"),
        "Desktop Invalid Token Game",
        &install_path,
    );

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:desktop:invalid-token",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&source_path),
            Some("3.5.0"),
            &sha256_hex(b"original-bytes"),
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:desktop:invalid-token-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &sha256_hex(b"replacement-bytes"),
        None,
    ));

    let plan = build_swap_plan(
        game.id().as_str(),
        "component:desktop:invalid-token",
        "artifact:desktop:invalid-token-3.7",
    )
    .expect("plan should build");

    let operation_id = plan["operation_id"]
        .as_str()
        .expect("operation id should be string");

    let error = apply_operation_plan(operation_id, "invalid-confirmation-token")
        .expect_err("invalid token should fail");

    assert!(error
        .to_string()
        .contains("confirmation token mismatch for operation"));
    assert_eq!(
        fs::read(&source_path).expect("source bytes should be readable"),
        b"original-bytes"
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
        sample_component(
            "component:risk:low",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/RiskFixture/low/nvngx_dlss.dll",
            None,
            &sha256_hex(b"risk-low"),
        ),
        sample_component(
            "component:risk:medium",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::ReadOnly,
            "C:/Games/RiskFixture/medium/nvngx_dlss.dll",
            None,
            &sha256_hex(b"risk-medium"),
        ),
        sample_component(
            "component:risk:high",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Unsafe,
            "C:/Games/RiskFixture/high/nvngx_dlss.dll",
            None,
            &sha256_hex(b"risk-high"),
        ),
    ];

    assert_eq!(dashboard_risk_level(&components).as_str(), "high");
}

#[test]
fn technology_tags_are_deduplicated() {
    let game_id = "manual:C:/Games/TagsFixture";
    let components = vec![
        sample_component(
            "component:tags:dlss-a",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/TagsFixture/a/nvngx_dlss.dll",
            None,
            &sha256_hex(b"tags-dlss-a"),
        ),
        sample_component(
            "component:tags:dlss-b",
            game_id,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/TagsFixture/b/nvngx_dlss.dll",
            None,
            &sha256_hex(b"tags-dlss-b"),
        ),
    ];

    assert_eq!(
        technology_tags(&components),
        vec!["dlss_super_resolution".to_owned()]
    );
}

#[test]
fn normalized_path_string_uses_forward_slashes() {
    let path = Path::new(r"C:\Games\RenderPilot\nvngx_dlss.dll");

    assert_eq!(path_string(path), "C:/Games/RenderPilot/nvngx_dlss.dll");
}

struct DesktopFixture {
    _temp_dir: tempfile::TempDir,
    _env: DesktopCatalogEnvGuard,
    storage: SqliteStorage,
}

impl DesktopFixture {
    fn new(_name: &str) -> Self {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
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

    fn store_components(
        &self,
        game_id: &GameId,
        components: &[renderpilot_domain::GraphicsComponent],
    ) {
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
    previous_backup_root: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl DesktopCatalogEnvGuard {
    fn new(db_path: PathBuf) -> Self {
        let lock = lock_process_env();
        let previous_db = env::var_os(CATALOG_DB_PATH_ENV);
        let previous_backup_root = env::var_os(BACKUP_ROOT_DIR_ENV);
        let backup_root = db_path.with_extension("backups");

        env::set_var(CATALOG_DB_PATH_ENV, &db_path);
        env::set_var(BACKUP_ROOT_DIR_ENV, &backup_root);

        Self {
            previous_db,
            previous_backup_root,
            _lock: lock,
        }
    }
}

impl Drop for DesktopCatalogEnvGuard {
    fn drop(&mut self) {
        restore_env_var(CATALOG_DB_PATH_ENV, &self.previous_db);
        restore_env_var(BACKUP_ROOT_DIR_ENV, &self.previous_backup_root);
    }
}

fn restore_env_var(key: &str, previous: &Option<OsString>) {
    match previous {
        Some(value) => env::set_var(key, value),
        None => env::remove_var(key),
    }
}

fn sample_game(id: &str, title: &str, install_path: &str) -> GameInstallation {
    let identity = GameIdentity::new(
        GameId::new(id).expect("game id should be valid"),
        title,
        Launcher::Manual,
    )
    .expect("game identity should be valid");

    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install_path).expect("install path should be valid"),
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
) -> renderpilot_domain::GraphicsComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    renderpilot_domain::GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    )
    .with_file(file)
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
        file,
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
