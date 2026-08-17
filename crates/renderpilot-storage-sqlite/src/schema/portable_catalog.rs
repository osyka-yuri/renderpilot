//! Bounded, file-backed portable catalog schema transition boundary.
//!
//! The supervisor owns snapshots, receipts, and recovery. This module owns
//! only the released v1.x catalog migration chain and its validation.

use std::{error::Error, fmt, path::Path};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use super::{
    CURRENT_PORTABLE_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION, MINIMUM_PORTABLE_SCHEMA_VERSION,
    backup, ddl::portable_path_tags, steps, validation, version,
};

const _: () =
    assert!(MINIMUM_PORTABLE_SCHEMA_VERSION as i32 == steps::MINIMUM_PORTABLE_SCHEMA_VERSION);

/// Observed portable-catalog schema facts after exact validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableCatalogSchemaReport {
    /// The `PRAGMA user_version` observed before this operation.
    pub source_version: u32,
    /// The exact portable-catalog schema version after this operation.
    pub target_version: u32,
    /// Canonical tag identifying the portable catalog-data path contract.
    pub portable_data_path_tag: String,
    /// Whether persisted external game-install paths were intentionally retained.
    pub external_paths_preserved: bool,
    /// Whether UI-only virtual paths were intentionally omitted from persistence.
    pub virtual_paths_omitted: bool,
}

/// The only portable catalog schema operations supported by this adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableCatalogSchemaTransition {
    /// Observe and exactly validate an already-current catalog.
    ValidateCurrent,
    /// Upgrade a supported released v1.x catalog after a supervisor snapshot.
    UpgradeToCurrentAfterSnapshot,
}

/// Stable category for a portable catalog schema operation failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableCatalogSchemaErrorKind {
    /// The existing catalog could not be opened with the required no-create mode.
    Open,
    /// The catalog version is absent, unknown, or newer than this transition supports.
    UnsupportedVersion,
    /// The requested transition's explicit preconditions were not met.
    TransitionPrecondition,
    /// The exact catalog DDL transaction could not be started, applied, or committed.
    Ddl,
    /// The post-commit WAL checkpoint failed.
    Checkpoint,
    /// Schema, canonical path-tag, or integrity validation failed.
    Validation,
}

impl fmt::Display for PortableCatalogSchemaErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Open => "open",
            Self::UnsupportedVersion => "unsupported version",
            Self::TransitionPrecondition => "transition precondition",
            Self::Ddl => "ddl",
            Self::Checkpoint => "checkpoint",
            Self::Validation => "validation",
        };
        formatter.write_str(name)
    }
}

/// Opaque portable catalog schema operation failure.
#[derive(Debug)]
pub struct PortableCatalogSchemaError {
    kind: PortableCatalogSchemaErrorKind,
    context: &'static str,
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl PortableCatalogSchemaError {
    fn with_source(
        kind: PortableCatalogSchemaErrorKind,
        context: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            context,
            source: Box::new(source),
        }
    }

    fn message(
        kind: PortableCatalogSchemaErrorKind,
        context: &'static str,
        message: String,
    ) -> Self {
        Self::with_source(kind, context, PortableCatalogSchemaMessage(message))
    }

    /// Returns the stable failure category without exposing the internal error payload.
    #[must_use]
    pub fn kind(&self) -> PortableCatalogSchemaErrorKind {
        self.kind
    }
}

impl fmt::Display for PortableCatalogSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "portable catalog schema {}: {}",
            self.context, self.source
        )
    }
}

impl Error for PortableCatalogSchemaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug)]
struct PortableCatalogSchemaMessage(String);

impl fmt::Display for PortableCatalogSchemaMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PortableCatalogSchemaMessage {}

/// Opens an existing catalog without creating it, then observationally validates
/// a supported released v1.x schema and returns its public version.
pub fn inspect_portable_catalog_schema(path: &Path) -> Result<u32, PortableCatalogSchemaError> {
    let connection = open_existing_read_only(path)?;
    let source_version = read_version(&connection)?;
    match source_version {
        CURRENT_PORTABLE_SCHEMA_VERSION => validate_current(&connection)?,
        15 => validate_legacy_precondition(&connection, false)?,
        16 => validate_legacy_precondition(&connection, true)?,
        version if supported_upgrade_source(version).is_some() => {
            validate_integrity_precondition(&connection)?;
        }
        version => {
            return Err(unsupported_version(
                "portable inspection supports the released portable schema range",
                version,
            ));
        }
    }
    Ok(source_version)
}

/// Runs one exact portable catalog schema operation against an existing catalog.
///
/// This never invokes the general schema apply/rebuild/repair path and never
/// creates a catalog. The `AfterSnapshot` transition relies on the supervisor's
/// already-completed snapshot boundary; it does not inspect or write receipts.
pub fn transition_portable_catalog_schema(
    path: &Path,
    transition: PortableCatalogSchemaTransition,
) -> Result<PortableCatalogSchemaReport, PortableCatalogSchemaError> {
    match transition {
        PortableCatalogSchemaTransition::ValidateCurrent => {
            let connection = open_existing_read_only(path)?;
            inspect_current(&connection)
        }
        PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot => {
            upgrade_after_snapshot(path)
        }
    }
}

/// Applies exactly the current baseline to an empty portable catalog on the
/// caller's already-open connection. This is deliberately separate from the
/// general schema application path: it never backs up, repairs, rebuilds, or
/// infers a historical schema.
pub(crate) fn initialize_fresh_portable_catalog(
    connection: &mut Connection,
) -> Result<(), PortableCatalogSchemaError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            PortableCatalogSchemaError::with_source(
                PortableCatalogSchemaErrorKind::Ddl,
                "could not begin fresh portable catalog transaction",
                error,
            )
        })?;
    require_empty_fresh_catalog(&transaction)?;
    super::rebuild::apply_baseline(&transaction).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Ddl,
            "could not apply fresh portable catalog baseline",
            error,
        )
    })?;
    version::write(&transaction, CURRENT_SCHEMA_VERSION).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Ddl,
            "could not write fresh portable catalog version",
            error,
        )
    })?;
    validate_current(&transaction)?;
    transaction.commit().map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Ddl,
            "could not commit fresh portable catalog baseline",
            error,
        )
    })?;
    validate_current_portable_catalog(connection)
}

/// Validates an already-current portable catalog on an already-open
/// connection. It performs no schema transition or catalog repair.
pub(crate) fn validate_current_portable_catalog(
    connection: &Connection,
) -> Result<(), PortableCatalogSchemaError> {
    inspect_current(connection).map(|_| ())
}

fn open_existing_read_only(path: &Path) -> Result<Connection, PortableCatalogSchemaError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Open,
            "could not open existing catalog read-only",
            error,
        )
    })
}

fn open_existing_read_write(path: &Path) -> Result<Connection, PortableCatalogSchemaError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Open,
            "could not open existing catalog read-write",
            error,
        )
    })
}

fn inspect_current(
    connection: &Connection,
) -> Result<PortableCatalogSchemaReport, PortableCatalogSchemaError> {
    let source_version = read_version(connection)?;
    if source_version != CURRENT_PORTABLE_SCHEMA_VERSION {
        return Err(unsupported_version(
            "current portable catalog validation requires the current schema version",
            source_version,
        ));
    }

    validate_current(connection)?;
    Ok(report(source_version))
}

fn require_empty_fresh_catalog(connection: &Connection) -> Result<(), PortableCatalogSchemaError> {
    let observed_version = read_version(connection)?;
    if observed_version != 0 {
        return Err(unsupported_version(
            "fresh portable catalog initialization requires user_version zero",
            observed_version,
        ));
    }
    let user_objects: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type IN ('table', 'index', 'view', 'trigger') \
             AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| {
            PortableCatalogSchemaError::with_source(
                PortableCatalogSchemaErrorKind::TransitionPrecondition,
                "could not inspect fresh portable catalog objects",
                error,
            )
        })?;
    if user_objects != 0 {
        return Err(PortableCatalogSchemaError::message(
            PortableCatalogSchemaErrorKind::TransitionPrecondition,
            "fresh portable catalog contained user SQLite objects",
            "fresh portable catalog must not contain user SQLite objects".to_owned(),
        ));
    }
    Ok(())
}

fn upgrade_after_snapshot(
    path: &Path,
) -> Result<PortableCatalogSchemaReport, PortableCatalogSchemaError> {
    let mut connection = open_existing_read_write(path)?;
    let source_version = read_version(&connection)?;

    if source_version == CURRENT_PORTABLE_SCHEMA_VERSION {
        validate_current(&connection)?;
        return Ok(report(source_version));
    }
    let Some(source_schema) = supported_upgrade_source(source_version) else {
        return Err(unsupported_version(
            "portable upgrade supports the released portable schema range",
            source_version,
        ));
    };

    match source_version {
        15 => validate_legacy_precondition(&connection, false)?,
        16 => validate_legacy_precondition(&connection, true)?,
        _ => validate_integrity_precondition(&connection)?,
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            PortableCatalogSchemaError::with_source(
                PortableCatalogSchemaErrorKind::Ddl,
                "could not begin portable catalog migration transaction",
                error,
            )
        })?;
    steps::run_from(&transaction, source_schema).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Ddl,
            "could not apply released portable catalog migration chain",
            error,
        )
    })?;
    validate_current(&transaction)?;
    transaction.commit().map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Ddl,
            "could not commit portable catalog migration",
            error,
        )
    })?;

    backup::checkpoint_wal(&connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Checkpoint,
            "could not checkpoint portable catalog migration",
            error,
        )
    })?;
    validate_current(&connection)?;

    Ok(report(source_version))
}

fn read_version(connection: &Connection) -> Result<u32, PortableCatalogSchemaError> {
    let version = version::read(connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Validation,
            "could not read catalog schema version",
            error,
        )
    })?;
    u32::try_from(version).map_err(|_| {
        PortableCatalogSchemaError::message(
            PortableCatalogSchemaErrorKind::UnsupportedVersion,
            "catalog schema version was negative",
            format!("catalog schema version v{version} is not supported"),
        )
    })
}

fn validate_legacy_precondition(
    connection: &Connection,
    require_portable_path_tags: bool,
) -> Result<(), PortableCatalogSchemaError> {
    validation::validate_legacy_portable_catalog_observational(
        connection,
        require_portable_path_tags,
    )
    .map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::TransitionPrecondition,
            "released legacy catalog does not satisfy its exact transition precondition",
            error,
        )
    })?;
    validation::validate_database_integrity(connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::TransitionPrecondition,
            "released legacy catalog failed integrity precondition",
            error,
        )
    })
}

fn validate_integrity_precondition(
    connection: &Connection,
) -> Result<(), PortableCatalogSchemaError> {
    validation::validate_database_integrity(connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::TransitionPrecondition,
            "released v1.x catalog failed integrity precondition",
            error,
        )
    })
}

fn supported_upgrade_source(version: u32) -> Option<i32> {
    if version < MINIMUM_PORTABLE_SCHEMA_VERSION {
        return None;
    }
    let version = i32::try_from(version).ok()?;
    steps::can_upgrade_from(version).then_some(version)
}

fn validate_current(connection: &Connection) -> Result<(), PortableCatalogSchemaError> {
    let observed_version = read_version(connection)?;
    if observed_version != CURRENT_PORTABLE_SCHEMA_VERSION {
        return Err(PortableCatalogSchemaError::message(
            PortableCatalogSchemaErrorKind::Validation,
            "catalog version changed during validation",
            format!("expected v{CURRENT_PORTABLE_SCHEMA_VERSION}, found v{observed_version}"),
        ));
    }

    validation::validate_catalog_schema_observational(connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Validation,
            "current portable schema or portable path-tag validation failed",
            error,
        )
    })?;
    validation::validate_database_integrity(connection).map_err(|error| {
        PortableCatalogSchemaError::with_source(
            PortableCatalogSchemaErrorKind::Validation,
            "current portable schema integrity validation failed",
            error,
        )
    })
}

fn unsupported_version(context: &'static str, version: u32) -> PortableCatalogSchemaError {
    PortableCatalogSchemaError::message(
        PortableCatalogSchemaErrorKind::UnsupportedVersion,
        context,
        format!("catalog schema version v{version} is not supported"),
    )
}

fn report(source_version: u32) -> PortableCatalogSchemaReport {
    PortableCatalogSchemaReport {
        source_version,
        target_version: CURRENT_PORTABLE_SCHEMA_VERSION,
        portable_data_path_tag: portable_path_tags::PORTABLE_DATA_PATH_TAG.to_owned(),
        external_paths_preserved: true,
        virtual_paths_omitted: true,
    }
}
