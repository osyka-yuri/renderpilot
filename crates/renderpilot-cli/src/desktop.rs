//! Desktop UI facade over the existing CLI/core orchestration.
//!
//! This module exposes JSON-friendly entry points that Tauri commands can call
//! without embedding business logic in the command handlers themselves.

use std::{collections::BTreeSet, path::PathBuf};

use renderpilot_application::OperationPlan;
use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsComponent, OperationId, Swappability,
};
use serde::Serialize;
use serde_json::Value;

use crate::{catalog, hash, output, CliError, VERSION};

const APPLY_CONFIRMATION_TOKEN_DOMAIN: &[u8] = b"renderpilot:apply-operation-plan";

/// Scans one manually selected folder and returns refreshed details for the detected game.
pub fn scan_manual_folder(path: PathBuf) -> Result<Value, CliError> {
    let result = catalog::scan_folder(path)?;

    game_details_value(result.game.id().clone())
}

/// Lists all games currently stored in the local catalog.
pub fn list_games() -> Result<Value, CliError> {
    let games = catalog::list_games()?;

    serde_json::to_value(GameListOutput { games }).map_err(Into::into)
}

/// Lists persisted games as lightweight cards for the desktop Games feature.
pub fn get_game_cards() -> Result<Value, CliError> {
    let games = catalog::list_games()?;
    let cards = games
        .iter()
        .map(GameCardOutput::from_game)
        .collect::<Result<Vec<_>, _>>()?;

    serde_json::to_value(cards).map_err(Into::into)
}

/// Loads one game with detected components, candidates, and operation history.
pub fn get_game_details(game_id: impl Into<String>) -> Result<Value, CliError> {
    game_details_value(parse_game_id(game_id.into())?)
}

/// Persists a swap operation plan and returns the serialized plan details.
pub fn build_swap_plan(
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
) -> Result<Value, CliError> {
    let result = catalog::build_swap_plan(
        parse_game_id(game_id.into())?,
        parse_component_id(component_id.into())?,
        parse_artifact_id(artifact_id.into())?,
    )?;

    serde_json::to_value(OperationPlanOutput::from(&result.plan)).map_err(Into::into)
}

/// Creates or refreshes the backup for an operation, then applies it.
pub fn apply_operation(operation_id: impl Into<String>) -> Result<Value, CliError> {
    let operation_id = parse_operation_id(operation_id.into())?;

    apply_operation_with_backup(operation_id)
}

/// Applies a previously built operation plan after the UI echoes back its confirmation token.
pub fn apply_operation_plan(
    operation_id: impl Into<String>,
    confirmation_token: impl Into<String>,
) -> Result<Value, CliError> {
    let operation_id = parse_operation_id(operation_id.into())?;
    let confirmation_token = confirmation_token.into();

    ensure_confirmation_token(&operation_id, &confirmation_token)?;

    apply_operation_with_backup(operation_id)
}

fn apply_operation_with_backup(operation_id: OperationId) -> Result<Value, CliError> {
    let _backup = catalog::create_backup(operation_id.clone(), VERSION)?;
    let result = catalog::apply_operation(operation_id)?;

    output::apply_operation_value(&result).map_err(Into::into)
}

/// Rolls back one previously executed operation.
pub fn rollback_operation(operation_id: impl Into<String>) -> Result<Value, CliError> {
    let result = catalog::rollback_operation(parse_operation_id(operation_id.into())?)?;

    output::rollback_operation_value(&result).map_err(Into::into)
}

#[derive(Debug, Serialize)]
struct GameListOutput {
    games: Vec<renderpilot_domain::GameInstallation>,
}

#[derive(Debug, Serialize)]
struct GameCardOutput {
    game_id: String,
    title: String,
    launcher: String,
    platform: String,
    runtime: String,
    install_path: String,
    external_id: Option<String>,
    technology_tags: Vec<String>,
    component_count: usize,
    updates_available: bool,
    update_count: usize,
    risk_level: String,
    backup_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
}

impl GameCardOutput {
    fn from_game(game: &renderpilot_domain::GameInstallation) -> Result<Self, CliError> {
        let details = catalog::get_game_details(game.id().clone())?;
        let metrics = GameCardMetrics::from_details(&details);

        Ok(Self {
            game_id: game.id().as_str().to_owned(),
            title: game.identity().title().to_owned(),
            launcher: format!("{:?}", game.identity().launcher()),
            platform: format!("{:?}", game.platform()),
            runtime: format!("{:?}", game.runtime()),
            install_path: game.install_path().as_str().to_owned(),
            external_id: game.identity().external_id().map(str::to_owned),
            technology_tags: metrics.technology_tags,
            component_count: metrics.component_count,
            updates_available: metrics.update_count > 0,
            update_count: metrics.update_count,
            risk_level: metrics.risk_level,
            backup_available: metrics.backup_available,
            operation_count: metrics.operation_count,
            last_operation_status: metrics.last_operation_status,
        })
    }
}

struct GameCardMetrics {
    technology_tags: Vec<String>,
    component_count: usize,
    update_count: usize,
    risk_level: String,
    backup_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
}

impl GameCardMetrics {
    fn from_details(details: &catalog::GameDetailsCatalogResult) -> Self {
        Self {
            technology_tags: technology_tags(&details.components),
            component_count: details.components.len(),
            update_count: details.candidate_groups.len(),
            risk_level: dashboard_risk_level(&details.components).to_owned(),
            backup_available: details
                .operations
                .operations
                .iter()
                .any(|entry| entry.backup_count > 0),
            operation_count: details.operations.operations.len(),
            last_operation_status: details
                .operations
                .operations
                .iter()
                .max_by_key(|entry| entry.operation.created_at.as_i64())
                .map(|entry| entry.operation.status.as_str().to_owned()),
        }
    }
}

fn technology_tags(components: &[GraphicsComponent]) -> Vec<String> {
    components
        .iter()
        .map(|component| component.technology().as_slug().to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Serialize)]
struct GameDetailsOutput {
    game: renderpilot_domain::GameInstallation,
    components: Vec<renderpilot_domain::GraphicsComponent>,
    candidate_groups: Value,
    operations: Value,
}

#[derive(Debug, Serialize)]
struct OperationPlanOutput {
    operation_id: String,
    confirmation_token: String,
    game_id: String,
    operation_type: String,
    target_path: String,
    replacement_path: String,
    original_version: Option<String>,
    replacement_version: Option<String>,
    original_sha256: Option<String>,
    replacement_sha256: Option<String>,
    risk_level: String,
    requires_backup: bool,
    requires_elevation: bool,
    artifact_id: String,
    blockers: Vec<String>,
    warnings: Vec<String>,
}

impl From<&OperationPlan> for OperationPlanOutput {
    fn from(plan: &OperationPlan) -> Self {
        Self {
            operation_id: plan.operation_id().as_str().to_owned(),
            confirmation_token: confirmation_token_for_operation(plan.operation_id()),
            game_id: plan.game_id().as_str().to_owned(),
            operation_type: plan.operation_type().as_str().to_owned(),
            target_path: plan.target_path().as_str().to_owned(),
            replacement_path: plan.replacement_path().as_str().to_owned(),
            original_version: plan
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: plan
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: plan.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: plan
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
            risk_level: plan.risk_level().as_str().to_owned(),
            requires_backup: plan.requires_backup(),
            requires_elevation: plan.requires_elevation(),
            artifact_id: plan.artifact_id().as_str().to_owned(),
            blockers: plan
                .blockers()
                .iter()
                .map(|blocker| blocker.as_str().to_owned())
                .collect(),
            warnings: plan
                .warnings()
                .iter()
                .map(|warning| warning.as_str().to_owned())
                .collect(),
        }
    }
}

fn game_details_value(game_id: GameId) -> Result<Value, CliError> {
    let details = catalog::get_game_details(game_id.clone())?;
    let candidate_groups = output::candidate_groups_value(details.candidate_groups)?;
    let operations = output::operation_summaries_value(&details.operations)?;

    serde_json::to_value(GameDetailsOutput {
        game: details.game,
        components: details.components,
        candidate_groups,
        operations,
    })
    .map_err(Into::into)
}

fn dashboard_risk_level(components: &[GraphicsComponent]) -> &'static str {
    if components.is_empty() {
        return "unknown";
    }

    if components.iter().any(|component| {
        matches!(
            component.swappability(),
            Swappability::Unsafe | Swappability::IntegratedIntoEngine
        )
    }) {
        return "high";
    }

    if components.iter().any(|component| {
        matches!(
            component.swappability(),
            Swappability::BundleOnly | Swappability::ReadOnly
        )
    }) {
        return "medium";
    }

    if components
        .iter()
        .any(|component| component.swappability() == Swappability::Swappable)
    {
        return "low";
    }

    "unknown"
}

fn confirmation_token_for_operation(operation_id: &OperationId) -> String {
    hash::sha256_hex_parts(&[
        APPLY_CONFIRMATION_TOKEN_DOMAIN,
        b":",
        operation_id.as_str().as_bytes(),
    ])
}

fn ensure_confirmation_token(
    operation_id: &OperationId,
    confirmation_token: &str,
) -> Result<(), CliError> {
    let expected = confirmation_token_for_operation(operation_id);

    if expected == confirmation_token {
        return Ok(());
    }

    Err(CliError::CommandFailed(format!(
        "confirmation token mismatch for operation {}",
        operation_id.as_str()
    )))
}

fn parse_game_id(value: impl Into<String>) -> Result<GameId, CliError> {
    parse_identifier(value, CliError::InvalidGameId)
}

fn parse_component_id(value: impl Into<String>) -> Result<ComponentId, CliError> {
    parse_identifier(value, CliError::InvalidComponentId)
}

fn parse_artifact_id(value: impl Into<String>) -> Result<ArtifactId, CliError> {
    parse_identifier(value, CliError::InvalidArtifactId)
}

fn parse_operation_id(value: impl Into<String>) -> Result<OperationId, CliError> {
    parse_identifier(value, CliError::InvalidOperationId)
}

fn parse_identifier<T>(
    value: impl Into<String>,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let value = value.into();

    T::try_from(value.clone()).map_err(|_| invalid(value))
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        sync::MutexGuard,
        time::{SystemTime, UNIX_EPOCH},
    };

    use renderpilot_application::{ArtifactRepository, ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GameIdentity, GameInstallation, GameRuntime, GraphicsTechnology, Launcher, LibraryArtifact,
        PathRef, Platform, Sha256Hash, Swappability, Version,
    };
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::{
        apply_operation_plan, build_swap_plan, get_game_cards, rollback_operation,
        scan_manual_folder,
    };
    use crate::hash::sha256_hex;
    use crate::test_env::lock_process_env;
    use crate::{backup_manager::BACKUP_ROOT_DIR_ENV, catalog::CATALOG_DB_PATH_ENV};

    #[test]
    fn scan_manual_folder_updates_catalog_and_returns_detected_components() {
        let _guard = DesktopCatalogEnvGuard::new(temp_db_path("desktop-scan"));
        let game_dir = TempGameFolder::new("desktop-scan-game");

        fs::create_dir_all(game_dir.path()).expect("game dir should exist");
        fs::write(
            game_dir.path().join("nvngx_dlss.dll"),
            b"desktop-scan-bytes",
        )
        .expect("test dll should be written");

        let details =
            scan_manual_folder(game_dir.path().to_path_buf()).expect("scan should succeed");
        let game_cards = get_game_cards().expect("game cards should succeed");

        assert_eq!(
            details["components"]
                .as_array()
                .expect("components array")
                .len(),
            1
        );
        assert_eq!(
            details["components"][0]["technology"],
            "DlssSuperResolution"
        );
        assert_eq!(game_cards.as_array().expect("game cards array").len(), 1);
        assert_eq!(game_cards[0]["install_path"], path_string(game_dir.path()));
        assert_eq!(
            game_cards[0]["title"],
            game_dir
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("folder name should be utf-8")
        );
        assert_eq!(game_cards[0]["technology_tags"][0], "dlss_super_resolution");
        assert_eq!(game_cards[0]["component_count"], 1);
        assert_eq!(game_cards[0]["updates_available"], false);
        assert_eq!(game_cards[0]["risk_level"], "low");
        assert_eq!(game_cards[0]["backup_available"], false);
    }

    #[test]
    fn desktop_apply_creates_backup_and_rollback_restores_original_bytes() {
        let fixture = DesktopFixture::new("desktop-apply-rollback");
        let game_folder = TempGameFolder::new("desktop-apply-game");
        let artifact_folder = TempGameFolder::new("desktop-apply-artifact");

        fs::create_dir_all(game_folder.path()).expect("game dir should exist");
        fs::create_dir_all(artifact_folder.path()).expect("artifact dir should exist");

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
        let game_folder = TempGameFolder::new("desktop-invalid-token-game");
        let artifact_folder = TempGameFolder::new("desktop-invalid-token-artifact");

        fs::create_dir_all(game_folder.path()).expect("game dir should exist");
        fs::create_dir_all(artifact_folder.path()).expect("artifact dir should exist");

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

    struct DesktopFixture {
        _env: DesktopCatalogEnvGuard,
        storage: SqliteStorage,
    }

    impl DesktopFixture {
        fn new(name: &str) -> Self {
            let db_path = temp_db_path(name);
            let env = DesktopCatalogEnvGuard::new(db_path.clone());
            let storage = SqliteStorage::open(&db_path).expect("sqlite should open");

            Self { _env: env, storage }
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
        db_path: PathBuf,
        backup_root: PathBuf,
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
                db_path,
                backup_root,
                _lock: lock,
            }
        }
    }

    impl Drop for DesktopCatalogEnvGuard {
        fn drop(&mut self) {
            if let Some(previous_db) = &self.previous_db {
                env::set_var(CATALOG_DB_PATH_ENV, previous_db);
            } else {
                env::remove_var(CATALOG_DB_PATH_ENV);
            }

            if let Some(previous_backup_root) = &self.previous_backup_root {
                env::set_var(BACKUP_ROOT_DIR_ENV, previous_backup_root);
            } else {
                env::remove_var(BACKUP_ROOT_DIR_ENV);
            }

            if self.backup_root.exists() {
                let _ = fs::remove_dir_all(&self.backup_root);
            }

            cleanup_sqlite_files(&self.db_path);
        }
    }

    #[derive(Debug)]
    struct TempGameFolder {
        path: PathBuf,
    }

    impl TempGameFolder {
        fn new(name: &str) -> Self {
            Self {
                path: unique_temp_path(name, ""),
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempGameFolder {
        fn drop(&mut self) {
            if self.path.exists() {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        unique_temp_path(name, ".db")
    }

    fn unique_temp_path(name: &str, suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        env::temp_dir().join(format!("renderpilot-{name}-{nanos}{suffix}"))
    }

    fn cleanup_sqlite_files(db_path: &Path) {
        remove_file_if_exists(db_path);
        remove_file_if_exists(&PathBuf::from(format!("{}-shm", db_path.to_string_lossy())));
        remove_file_if_exists(&PathBuf::from(format!("{}-wal", db_path.to_string_lossy())));
    }

    fn remove_file_if_exists(path: &Path) {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    fn path_string(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
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
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
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
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
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
}
