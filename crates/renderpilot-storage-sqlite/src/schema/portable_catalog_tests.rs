use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use rusqlite::Connection;

use crate::{
    PortableCatalogSchemaErrorKind, PortableCatalogSchemaTransition,
    inspect_portable_catalog_schema, transition_portable_catalog_schema,
};

use super::{CURRENT_SCHEMA_VERSION, apply, ddl::portable_path_tags, steps, version};

static NEXT_TEMP_CATALOG: AtomicU64 = AtomicU64::new(0);
const RELEASED_V4_SCHEMA: &str = include_str!("../../tests/fixtures/catalog-v4.sql");

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
    let catalog = TemporaryCatalog::new("v15-to-v16");
    prepare_v15_catalog(&catalog.path);

    let report = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .expect("exact v15-to-v16 transition");

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
fn v16_validation_is_observational() {
    let catalog = TemporaryCatalog::new("v16-observational");
    prepare_current_catalog(&catalog.path);
    let before = fs::read(&catalog.path).expect("read catalog before validation");

    let observed_version =
        inspect_portable_catalog_schema(&catalog.path).expect("inspect v16 catalog");
    let transition_report = transition_portable_catalog_schema(
        &catalog.path,
        PortableCatalogSchemaTransition::ValidateCurrent,
    )
    .expect("validate v16 catalog");

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
fn v15_inspection_validates_without_requiring_v16_path_tags_or_mutating_bytes() {
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
fn v16_upgrade_retry_validates_without_reapplying_the_transition() {
    let catalog = TemporaryCatalog::new("v16-retry");
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
    .expect("v16 retry validates");

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

    for (label, schema_version) in [("pre-v1", 3), ("unreleased-gap", 7), ("future", 17)] {
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
    prepare_current_catalog(path);
    let connection = Connection::open(path).expect("open current catalog for v15 fixture");
    connection
        .execute_batch(&format!("DROP TABLE {};", portable_path_tags::TABLE_NAME))
        .expect("remove v16-only path-tag table");
    version::write(&connection, 15).expect("stamp v15 fixture");
}

fn set_version(path: &Path, schema_version: i32) {
    let connection = Connection::open(path).expect("open catalog to set version");
    version::write(&connection, schema_version).expect("set fixture schema version");
}
