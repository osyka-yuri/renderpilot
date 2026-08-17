use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use rusqlite::{Connection, OpenFlags};

use crate::{
    PortableCatalogSchemaErrorKind, PortableCatalogSchemaTransition, SqliteStorage,
    inspect_portable_catalog_schema, transition_portable_catalog_schema,
};

use super::{
    CURRENT_SCHEMA_VERSION, apply,
    ddl::portable_path_tags,
    portable_catalog::{initialize_fresh_portable_catalog, validate_current_portable_catalog},
    steps, version,
};

static NEXT_TEMP_CATALOG: AtomicU64 = AtomicU64::new(0);
const RELEASED_V4_SCHEMA: &str = include_str!("../../tests/fixtures/catalog-v4.sql");
const RELEASED_V16_SCAN_SCHEMA: &str =
    include_str!("../../tests/fixtures/catalog-v16-scan-state.sql");

struct TemporaryCatalog {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryCatalog {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_CATALOG.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "renderpilot-portable-catalog-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create isolated portable catalog directory");
        let path = directory.join("catalog.sqlite");
        Self { directory, path }
    }
}

impl Drop for TemporaryCatalog {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn v15_to_current_transition_writes_the_exact_rows_and_version() {
    let catalog = TemporaryCatalog::new("v15-to-current");
    prepare_v15_catalog(&catalog.path);

    let report = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect("exact v15-to-current transition");

    assert_eq!(report.source_version, 15);
    assert_eq!(report.target_version, CURRENT_SCHEMA_VERSION as u32);
    assert_eq!(
        report.portable_data_path_tag,
        portable_path_tags::PORTABLE_DATA_PATH_TAG
    );
    assert!(report.external_paths_preserved);
    assert!(report.virtual_paths_omitted);

    let connection = Connection::open(&catalog.path).expect("open upgraded catalog");
    assert_eq!(
        version::read(&connection).expect("read upgraded version"),
        CURRENT_SCHEMA_VERSION
    );
    portable_path_tags::validate(&connection).expect("canonical portable path-tag rows");
}

#[test]
fn released_v4_catalog_migrates_to_current_without_losing_user_data() {
    let catalog = TemporaryCatalog::new("v4-to-current");
    prepare_v4_catalog(&catalog.path);

    let report = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect("migrate released v4 catalog");

    assert_eq!(report.source_version, 4);
    assert_eq!(report.target_version, CURRENT_SCHEMA_VERSION as u32);

    let connection = Connection::open(&catalog.path).expect("open migrated v4 catalog");
    assert_eq!(
        connection
            .query_row(
                "SELECT title FROM games WHERE id = 'preserved-game'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("read preserved v4 game"),
        "Preserved game"
    );
    assert_eq!(
        version::read(&connection).expect("read migrated v4 version"),
        CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn current_validation_is_observational() {
    let catalog = TemporaryCatalog::new("current-observational");
    prepare_current_catalog(&catalog.path);
    let before = fs::read(&catalog.path).expect("read catalog before validation");

    let observed_version =
        inspect_portable_catalog_schema(&catalog.path).expect("inspect current catalog");
    let transition_report = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::ValidateCurrent,
    )
    .expect("validate current catalog");

    assert_eq!(observed_version, CURRENT_SCHEMA_VERSION as u32);
    assert_eq!(
        transition_report.source_version,
        CURRENT_SCHEMA_VERSION as u32
    );
    assert_eq!(
        transition_report.target_version,
        CURRENT_SCHEMA_VERSION as u32
    );
    assert_eq!(
        transition_report.portable_data_path_tag,
        portable_path_tags::PORTABLE_DATA_PATH_TAG
    );
    assert_eq!(
        fs::read(&catalog.path).expect("read catalog after validation"),
        before,
        "read-only validation must not modify the catalog bytes"
    );
}

#[test]
fn v15_inspection_validates_without_requiring_portable_path_tags_or_mutating_bytes() {
    let catalog = TemporaryCatalog::new("v15-observational");
    prepare_v15_catalog(&catalog.path);
    let before = fs::read(&catalog.path).expect("read v15 catalog before inspection");

    let observed_version =
        inspect_portable_catalog_schema(&catalog.path).expect("inspect valid v15 catalog");

    assert_eq!(observed_version, 15);
    assert_eq!(
        fs::read(&catalog.path).expect("read v15 catalog after inspection"),
        before,
        "v15 inspection must be observational"
    );
}

#[test]
fn v16_inspection_validates_released_weak_cache_shape_without_mutating_bytes() {
    let catalog = TemporaryCatalog::new("v16-observational");
    prepare_v16_catalog(&catalog.path);
    let before = fs::read(&catalog.path).expect("read v16 catalog before inspection");

    let observed_version =
        inspect_portable_catalog_schema(&catalog.path).expect("inspect valid v16 catalog");

    assert_eq!(observed_version, 16);
    assert_eq!(
        fs::read(&catalog.path).expect("read v16 catalog after inspection"),
        before,
        "v16 inspection must be observational"
    );
}

#[test]
fn current_upgrade_retry_validates_without_reapplying_the_transition() {
    let catalog = TemporaryCatalog::new("current-retry");
    prepare_v15_catalog(&catalog.path);

    transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect("initial exact transition");
    let before_retry = fs::read(&catalog.path).expect("read catalog before retry");

    let retry = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect("current retry validates");

    assert_eq!(retry.source_version, CURRENT_SCHEMA_VERSION as u32);
    assert_eq!(retry.target_version, CURRENT_SCHEMA_VERSION as u32);
    assert_eq!(
        fs::read(&catalog.path).expect("read catalog after retry"),
        before_retry,
        "retry must validate the completed transition without rewriting the catalog"
    );
}

#[test]
fn absent_unknown_future_and_malformed_catalogs_are_not_mutated() {
    let absent = TemporaryCatalog::new("absent");
    let absent_error = transition_portable_catalog_schema(
        &absent.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect_err("absent catalog must not be created");
    assert_eq!(absent_error.kind(), PortableCatalogSchemaErrorKind::Open);
    assert!(Error::source(&absent_error).is_some());
    let absent_inspection = inspect_portable_catalog_schema(&absent.path)
        .expect_err("absent catalog inspection must not create a catalog");
    assert_eq!(
        absent_inspection.kind(),
        PortableCatalogSchemaErrorKind::Open
    );
    assert!(
        !absent.path.exists(),
        "no-create open must leave the path absent"
    );

    for (label, schema_version) in [
        ("pre-v1", 3),
        ("unreleased-gap", 7),
        ("future", CURRENT_SCHEMA_VERSION + 1),
    ] {
        let catalog = TemporaryCatalog::new(label);
        prepare_current_catalog(&catalog.path);
        set_version(&catalog.path, schema_version);
        let before = fs::read(&catalog.path).expect("read unsupported catalog before transition");

        let error = transition_portable_catalog_schema(
            &catalog.path,
            PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
        )
        .expect_err("unsupported version must be rejected");

        assert_eq!(
            error.kind(),
            PortableCatalogSchemaErrorKind::UnsupportedVersion
        );
        assert_eq!(
            fs::read(&catalog.path).expect("read unsupported catalog after transition"),
            before,
            "unsupported catalog must remain byte-stable"
        );
    }

    let malformed = TemporaryCatalog::new("malformed");
    prepare_v15_catalog(&malformed.path);
    {
        let connection = Connection::open(&malformed.path).expect("open v15 catalog to corrupt");
        connection
            .execute_batch("DROP TABLE games;")
            .expect("make v15 contract malformed");
    }
    let before = fs::read(&malformed.path).expect("read malformed catalog before transition");

    let error = transition_portable_catalog_schema(
        &malformed.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect_err("malformed v15 catalog must fail the transition precondition");

    assert_eq!(
        error.kind(),
        PortableCatalogSchemaErrorKind::TransitionPrecondition
    );
    assert_eq!(
        fs::read(&malformed.path).expect("read malformed catalog after transition"),
        before,
        "malformed catalog must remain byte-stable"
    );
}

#[test]
fn strict_fresh_portable_open_initializes_once_and_supports_a_repository_operation() {
    let catalog = TemporaryCatalog::new("strict-fresh-success");

    let storage = SqliteStorage::open_fresh_portable(&catalog.path)
        .expect("initialize an absent fresh portable catalog");
    storage
        .set_setting("portable.marker", "fresh")
        .expect("write through the returned storage handle");
    assert_eq!(
        storage
            .get_setting("portable.marker")
            .expect("read through the returned storage handle"),
        Some("fresh".to_owned())
    );
    assert_eq!(storage.journal_mode().expect("journal mode"), "wal");
    drop(storage);

    assert_eq!(
        Connection::open(&catalog.path)
            .expect("open initialized catalog")
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("current portable schema version"),
        CURRENT_SCHEMA_VERSION
    );
    assert_no_backups(&catalog.directory);
}

#[test]
fn strict_fresh_portable_open_rejects_existing_or_contaminated_catalogs_without_backups() {
    let zero_with_user_object = TemporaryCatalog::new("strict-fresh-user-object");
    Connection::open(&zero_with_user_object.path)
        .expect("open zero version catalog")
        .execute_batch("CREATE TABLE foreign_state (id INTEGER PRIMARY KEY)")
        .expect("add foreign user object");

    let current = TemporaryCatalog::new("strict-fresh-current");
    prepare_current_catalog(&current.path);
    let legacy = TemporaryCatalog::new("strict-fresh-legacy");
    prepare_v15_catalog(&legacy.path);

    for catalog in [&zero_with_user_object, &current, &legacy] {
        let before = fs::read(&catalog.path).expect("read catalog before rejected fresh open");
        SqliteStorage::open_fresh_portable(&catalog.path)
            .expect_err("fresh portable open must reject an existing catalog");
        assert_eq!(
            fs::read(&catalog.path).expect("read catalog after rejected fresh open"),
            before,
            "rejected fresh open must not repair, migrate, or rewrite the catalog"
        );
        assert_no_backups(&catalog.directory);
    }
}

#[test]
fn strict_current_portable_open_requires_the_exact_current_catalog_and_preserves_marker_data() {
    let current = TemporaryCatalog::new("strict-current-success");
    prepare_current_catalog(&current.path);
    let fixture = SqliteStorage::open(&current.path).expect("open current fixture");
    fixture
        .set_setting("portable.marker", "preserved")
        .expect("write marker fixture");
    drop(fixture);

    let storage = SqliteStorage::open_current_portable(&current.path)
        .expect("open exact current portable catalog");
    assert_eq!(
        storage.get_setting("portable.marker").expect("read marker"),
        Some("preserved".to_owned())
    );
    assert_eq!(storage.journal_mode().expect("journal mode"), "wal");
    drop(storage);
    assert_no_backups(&current.directory);
}

#[test]
fn g_obs_02_strict_current_validation_is_authorizer_observational() {
    let current = TemporaryCatalog::new("strict-current-observational");
    prepare_current_catalog(&current.path);
    {
        let connection = Connection::open(&current.path).expect("open current fixture");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('portable.marker', 'preserved')",
                [],
            )
            .expect("write marker fixture");

        let denied = install_observational_authorizer(&connection);
        validate_current_portable_catalog(&connection)
            .expect("strict current validation must not execute a mutation");
        assert_eq!(
            denied.load(Ordering::Relaxed),
            0,
            "strict current validation must not attempt a denied mutation"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM settings WHERE key = 'portable.marker'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("read preserved marker"),
            "preserved"
        );
        assert_eq!(
            version::read(&connection).expect("read preserved schema version"),
            CURRENT_SCHEMA_VERSION
        );
    }
    assert_no_backups(&current.directory);

    let zero = TemporaryCatalog::new("strict-current-zero-observational");
    drop(Connection::open(&zero.path).expect("create zero-version catalog"));
    let legacy = TemporaryCatalog::new("strict-current-legacy-observational");
    prepare_v15_catalog(&legacy.path);
    let future = TemporaryCatalog::new("strict-current-future-observational");
    prepare_current_catalog(&future.path);
    set_version(&future.path, CURRENT_SCHEMA_VERSION + 1);
    let malformed = TemporaryCatalog::new("strict-current-malformed-observational");
    prepare_current_catalog(&malformed.path);
    Connection::open(&malformed.path)
        .expect("open malformed current catalog")
        .execute_batch("DROP TABLE games;")
        .expect("remove required current table");

    for catalog in [&zero, &legacy, &future, &malformed] {
        let before = fs::read(&catalog.path).expect("read invalid catalog before validation");
        let connection = Connection::open(&catalog.path).expect("open invalid catalog");
        let denied = install_observational_authorizer(&connection);

        validate_current_portable_catalog(&connection)
            .expect_err("strict current validation must reject invalid catalogs");
        assert_eq!(
            denied.load(Ordering::Relaxed),
            0,
            "invalid strict-current catalog must not attempt a denied mutation"
        );
        drop(connection);
        assert_eq!(
            fs::read(&catalog.path).expect("read invalid catalog after validation"),
            before,
            "invalid strict-current validation must not mutate the catalog"
        );
        assert_no_backups(&catalog.directory);
    }
}

#[test]
fn strict_current_portable_open_rejects_missing_zero_legacy_future_and_malformed_catalogs() {
    let missing = TemporaryCatalog::new("strict-current-missing");
    assert!(SqliteStorage::open_current_portable(&missing.path).is_err());
    assert!(
        !missing.path.exists(),
        "current open must not create a catalog"
    );

    let zero = TemporaryCatalog::new("strict-current-zero");
    drop(Connection::open(&zero.path).expect("create zero version catalog"));
    let legacy = TemporaryCatalog::new("strict-current-legacy");
    prepare_v15_catalog(&legacy.path);
    let future = TemporaryCatalog::new("strict-current-future");
    prepare_current_catalog(&future.path);
    set_version(&future.path, CURRENT_SCHEMA_VERSION + 1);
    let malformed = TemporaryCatalog::new("strict-current-malformed");
    prepare_current_catalog(&malformed.path);
    Connection::open(&malformed.path)
        .expect("open malformed current catalog")
        .execute_batch("DROP TABLE games;")
        .expect("remove required current table");

    for catalog in [&zero, &legacy, &future, &malformed] {
        let before = fs::read(&catalog.path).expect("read rejected current catalog");
        SqliteStorage::open_current_portable(&catalog.path)
            .expect_err("current portable open must reject a non-exact catalog");
        assert_eq!(
            fs::read(&catalog.path).expect("read catalog after rejected current open"),
            before,
            "rejected current open must remain observational"
        );
        assert_no_backups(&catalog.directory);
    }
}

#[test]
fn fresh_portable_initialization_rolls_back_after_begin_and_can_retry() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

    let catalog = TemporaryCatalog::new("strict-fresh-rollback");
    let mut connection = Connection::open_with_flags(
        &catalog.path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .expect("open fresh fixture connection");
    connection
        .authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::CreateTable { .. } => Authorization::Deny,
            _ => Authorization::Allow,
        }))
        .expect("install post-begin failure authorizer");
    initialize_fresh_portable_catalog(&mut connection)
        .expect_err("authorizer must fail fresh baseline after BEGIN IMMEDIATE");
    connection
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .expect("remove failure authorizer");
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("read rolled-back version"),
        0
    );
    let user_objects: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'index', 'view', 'trigger') AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("read rolled-back objects");
    assert_eq!(
        user_objects, 0,
        "failed fresh initialization must roll back"
    );
    drop(connection);

    let retried = SqliteStorage::open_fresh_portable(&catalog.path)
        .expect("fresh initialization must be retryable after rollback");
    assert_eq!(
        retried
            .get_setting("portable.marker")
            .expect("read empty retry catalog"),
        None
    );
    drop(retried);
    assert_no_backups(&catalog.directory);
}

fn assert_no_backups(directory: &Path) {
    assert!(
        fs::read_dir(directory)
            .expect("read portable catalog directory")
            .flatten()
            .all(|entry| entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "bak")),
        "strict portable opens must not create general schema backups"
    );
}

fn install_observational_authorizer(connection: &Connection) -> Arc<AtomicU64> {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

    let denied = Arc::new(AtomicU64::new(0));
    let denied_actions = Arc::clone(&denied);
    connection
        .authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::Read { .. }
            | AuthAction::Select
            | AuthAction::Pragma {
                pragma_value: None, ..
            }
            | AuthAction::Pragma {
                pragma_name:
                    "table_info" | "table_xinfo" | "table_list" | "index_info" | "index_xinfo"
                    | "index_list" | "foreign_key_list" | "foreign_key_check" | "integrity_check",
                ..
            }
            | AuthAction::Function { .. } => Authorization::Allow,
            _ => {
                denied_actions.fetch_add(1, Ordering::Relaxed);
                Authorization::Deny
            }
        }))
        .expect("install observational SQLite authorizer");

    denied
}

fn prepare_current_catalog(path: &Path) {
    let mut connection = Connection::open(path).expect("open current catalog");
    apply(&mut connection).expect("initialize current catalog");
}

fn prepare_v4_catalog(path: &Path) {
    let connection = Connection::open(path).expect("open released v4 catalog");
    connection
        .execute_batch(RELEASED_V4_SCHEMA)
        .expect("apply released v4 schema");
    version::write(&connection, steps::MINIMUM_PORTABLE_SCHEMA_VERSION)
        .expect("stamp released v4 fixture");
    connection
        .execute(
            "
            INSERT INTO games (
                id,
                title,
                launcher,
                platform,
                runtime,
                install_path,
                executable_candidates_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]')
            ",
            [
                "preserved-game",
                "Preserved game",
                "manual",
                "windows",
                "native",
                "C:/Games/Preserved",
            ],
        )
        .expect("insert v4 user data");
}

fn prepare_v15_catalog(path: &Path) {
    prepare_v16_catalog(path);
    let connection = Connection::open(path).expect("open v16 catalog for v15 fixture");
    connection
        .execute_batch(&format!("DROP TABLE {};", portable_path_tags::TABLE_NAME))
        .expect("remove v16-only path-tag table");
    version::write(&connection, 15).expect("stamp v15 fixture");
}

fn prepare_v16_catalog(path: &Path) {
    prepare_current_catalog(path);
    let connection = Connection::open(path).expect("open current catalog for v16 fixture");
    connection
        .execute_batch(RELEASED_V16_SCAN_SCHEMA)
        .expect("restore released v16 scan state");
    version::write(&connection, 16).expect("stamp v16 fixture");
}

fn set_version(path: &Path, schema_version: i32) {
    let connection = Connection::open(path).expect("open catalog to set version");
    version::write(&connection, schema_version).expect("set fixture schema version");
}
