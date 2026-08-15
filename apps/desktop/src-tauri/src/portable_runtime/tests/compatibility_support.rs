//! Shared migration harnesses and schema fixtures. The future SQL below is a
//! test-only stand-in for a later signed App generation, never production SQL.

use std::{
    fs,
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
};

use renderpilot_orchestration::portable::RuntimePathsV1;
use renderpilot_storage_sqlite::SqliteStorage;
use rusqlite::Connection;

use super::{append_journal, hash, journal_entry, supervisor_session};
use crate::portable_runtime::{
    activation::exchange_catalog_migration,
    app_protocol::{
        AppControlMessage, AppStatusMessage, CatalogMigrationReport, PortableAppSessionV2,
        StartupMode, read_message, write_message,
    },
    error::{PortableRuntimeError, Result},
    journal::{JournalPhase, journal_path, read_entries},
    rpu::{
        MAXIMUM_SCHEMA as PORTABLE_SCHEMA_VERSION, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL,
    },
    signature::sha256_hex,
    supervisor::authority::SupervisorSessionAuthority,
    supervisor_activation::{CatalogMigrationTrial, CatalogPreparationContext, prepare_catalog},
};

pub(super) const RELEASED_V4_SCHEMA: &str = include_str!(
    "../../../../../../crates/renderpilot-storage-sqlite/tests/fixtures/catalog-v4.sql"
);

type MigrationExchange = fn(
    &PortableAppSessionV2,
    &Path,
    u32,
    &mut BufReader<Cursor<Vec<u8>>>,
    &mut Vec<u8>,
) -> Result<()>;

pub(super) struct InProcessMigrationTrial<'a, Exchange> {
    startup: &'a PortableAppSessionV2,
    catalog: &'a Path,
    schema_observed: u32,
    exchange: Exchange,
    pending_status: Option<Vec<u8>>,
    pub(super) last_report: Option<CatalogMigrationReport>,
}

impl<Exchange> CatalogMigrationTrial for InProcessMigrationTrial<'_, Exchange>
where
    Exchange: Fn(
        &PortableAppSessionV2,
        &Path,
        u32,
        &mut BufReader<Cursor<Vec<u8>>>,
        &mut Vec<u8>,
    ) -> Result<()>,
{
    fn send_catalog_message(&mut self, message: &AppControlMessage) -> Result<()> {
        if self.pending_status.is_some() {
            return Err(PortableRuntimeError::new(
                "portable_migration_test",
                "migration trial already had a pending App response",
            ));
        }
        let mut control_wire = Vec::new();
        write_message(&mut control_wire, message)?;
        let mut control = BufReader::new(Cursor::new(control_wire));
        let mut status_wire = Vec::new();
        (self.exchange)(
            self.startup,
            self.catalog,
            self.schema_observed,
            &mut control,
            &mut status_wire,
        )?;
        self.pending_status = Some(status_wire);
        Ok(())
    }

    fn receive_catalog_message(&mut self) -> Result<AppStatusMessage> {
        let status_wire = self.pending_status.take().ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_migration_test",
                "migration trial had no App response",
            )
        })?;
        let message = read_message(&mut BufReader::new(Cursor::new(status_wire)))?;
        if let AppStatusMessage::MigrationAck(ack) = &message {
            self.last_report = Some(ack.report.clone());
        }
        Ok(message)
    }
}

pub(super) struct ScriptedMigrationTrial {
    pub(super) fail_send: bool,
    pub(super) fail_receive: bool,
    pub(super) response: Option<AppStatusMessage>,
    pub(super) sent: Option<AppControlMessage>,
}

impl CatalogMigrationTrial for ScriptedMigrationTrial {
    fn send_catalog_message(&mut self, message: &AppControlMessage) -> Result<()> {
        if self.fail_send {
            return Err(PortableRuntimeError::new(
                "portable_migration_test",
                "scripted send failure",
            ));
        }
        self.sent = Some(message.clone());
        Ok(())
    }

    fn receive_catalog_message(&mut self) -> Result<AppStatusMessage> {
        if self.fail_receive {
            return Err(PortableRuntimeError::new(
                "portable_migration_test",
                "scripted receive failure",
            ));
        }
        self.response.take().ok_or_else(|| {
            PortableRuntimeError::new("portable_migration_test", "scripted response was absent")
        })
    }
}

pub(super) struct PreparedMigrationHandshake {
    pub(super) startup: PortableAppSessionV2,
    pub(super) journal: PathBuf,
    generation: String,
    previous: String,
    pub(super) supervisor_session: SupervisorSessionAuthority,
}

impl PreparedMigrationHandshake {
    pub(super) fn new(
        paths: &RuntimePathsV1,
        transaction: &str,
        maximum_schema: u32,
    ) -> Result<Self> {
        let generation = hash('a');
        let previous = hash('b');
        let supervisor_session = supervisor_session('1');
        let startup = PortableAppSessionV2 {
            app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
            epoch: hash('c'),
            generation_sha256: generation.clone(),
            minimum_schema: MINIMUM_SCHEMA,
            maximum_schema,
            transaction_id: transaction.to_owned(),
            supervisor_session_transcript_sha256: supervisor_session.transcript_sha256().to_owned(),
            portable_root_identity: hash('d'),
            generation_root_identity: hash('e'),
            mode: StartupMode::activation_trial(),
            runtime_paths: paths.clone(),
            challenge: hash('f'),
            migration_permit_nonce: hash('1'),
            commit_permit_nonce: hash('2'),
        };
        let journal = journal_path(&paths.update_root, transaction);
        let activation_id = sha256_hex(
            format!("renderpilot-portable-activation-v3\0{transaction}\0{generation}").as_bytes(),
        );
        for phase in [
            JournalPhase::Prepared,
            JournalPhase::GenerationPublished,
            JournalPhase::TrialSpawned,
            JournalPhase::TrialReady,
        ] {
            let mut entry = journal_entry(phase);
            entry.transaction_id = transaction.to_owned();
            entry.activation_id = activation_id.clone();
            entry.selected_generation_sha256 = generation.clone();
            entry.previous_sha256 = Some(previous.clone());
            entry.selection_record_sha256 = None;
            append_journal(&journal, entry)?;
        }
        Ok(Self {
            startup,
            journal,
            generation,
            previous,
            supervisor_session,
        })
    }

    pub(super) fn prepare_catalog(
        &self,
        paths: &RuntimePathsV1,
        trial: &mut impl CatalogMigrationTrial,
        source_schema: u32,
    ) -> Result<()> {
        prepare_catalog(
            CatalogPreparationContext::new(
                &self.startup,
                &self.journal,
                paths,
                &self.generation,
                Some(&self.previous),
                &self.supervisor_session,
            ),
            trial,
            source_schema,
        )
    }
}

pub(super) fn migrate_through_protocol(
    paths: &RuntimePathsV1,
    schema_observed: u32,
    transaction: &str,
) -> Result<CatalogMigrationReport> {
    let handshake = PreparedMigrationHandshake::new(paths, transaction, PORTABLE_SCHEMA_VERSION)?;
    handshake.startup.validate()?;
    let mut trial = in_process_current_trial(&handshake, paths, schema_observed);
    handshake.prepare_catalog(paths, &mut trial, schema_observed)?;
    let entries = read_entries(&handshake.journal)?;
    if entries.last().map(|entry| entry.phase) != Some(JournalPhase::MigrationCommitted) {
        return Err(PortableRuntimeError::new(
            "portable_migration_test",
            "migration handshake did not reach MigrationCommitted",
        ));
    }
    trial.last_report.ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_migration_test",
            "migration handshake returned no App report",
        )
    })
}

pub(super) fn in_process_current_trial<'a>(
    handshake: &'a PreparedMigrationHandshake,
    paths: &'a RuntimePathsV1,
    schema_observed: u32,
) -> InProcessMigrationTrial<'a, MigrationExchange> {
    InProcessMigrationTrial {
        startup: &handshake.startup,
        catalog: &paths.catalog_db_path,
        schema_observed,
        exchange: exchange_current_catalog_migration,
        pending_status: None,
        last_report: None,
    }
}

fn exchange_current_catalog_migration(
    startup: &PortableAppSessionV2,
    catalog: &Path,
    schema_observed: u32,
    control: &mut BufReader<Cursor<Vec<u8>>>,
    status: &mut Vec<u8>,
) -> Result<()> {
    exchange_catalog_migration(startup, catalog, schema_observed, control, status)
}

pub(super) fn portable_paths(root: &Path) -> RuntimePathsV1 {
    let portable_root = root.join("portable");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join(hash('a'));
    let app = generation.join("renderpilot-app.exe");
    RuntimePathsV1::from_portable_root(portable_root, &generation, &app)
        .expect("derive portable paths")
}

pub(super) fn catalog_with_version(path: &Path, version: u32) {
    if version == 15 {
        let storage = SqliteStorage::open(path).expect("create exact current catalog fixture");
        drop(storage);
        Connection::open(path)
            .expect("open current catalog for v15 fixture")
            .execute_batch("DROP TABLE portable_path_tags; PRAGMA user_version = 15;")
            .expect("reduce current catalog to the exact v15 boundary");
        return;
    }
    let connection = Connection::open(path).expect("create SQLite fixture");
    connection
        .execute_batch(&format!(
            "PRAGMA user_version = {version}; CREATE TABLE legacy(id INTEGER);"
        ))
        .expect("write legacy schema fixture");
}

pub(super) fn user_version(path: &Path) -> u32 {
    let connection = Connection::open(path).expect("open SQLite fixture");
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version")
}

pub(super) fn catalog_v4_with_user_data(path: &Path) {
    let connection = Connection::open(path).expect("create released v4 catalog");
    connection
        .execute_batch(RELEASED_V4_SCHEMA)
        .expect("apply released v4 schema");
    connection
        .execute_batch(
            "
            PRAGMA user_version = 4;
            INSERT INTO games (
                id, title, launcher, platform, runtime, install_path, executable_candidates_json
            ) VALUES (
                'preserved-game', 'Preserved game', 'manual', 'windows', 'native',
                'C:/Games/Preserved', '[]'
            );",
        )
        .expect("stamp v4 and insert user data");
}

pub(super) fn create_current_catalog(path: &Path) {
    drop(SqliteStorage::open(path).expect("create current catalog fixture"));
}

pub(super) fn create_data_root(paths: &RuntimePathsV1) {
    fs::create_dir_all(&paths.data_root).expect("create portable data root");
}
