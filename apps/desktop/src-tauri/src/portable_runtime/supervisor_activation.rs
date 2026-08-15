//! Per-generation activation transaction owned by the portable supervisor.

use std::path::Path;

use super::{
    app_process::TrialProcess,
    app_protocol::{
        AppControlMessage, AppStatusMessage, CatalogMigrationOperation, PortableAppSessionV2,
        StartupMode, committed_sequence_for_selection,
    },
    cleanup::cleanup_snapshot_after_terminal,
    diagnostics_files::PortableDiagnosticSession,
    error::{PortableRuntimeError, Result},
    journal::{
        JournalAppendKind, JournalEntry, JournalPhase, append_normal_with_outbox, journal_path,
        write_terminal_receipt,
    },
    migration::{
        begin_supervised_migration, commit_supervised_migration, verify_generation_report,
    },
    random::hex_32,
    rpu::{
        PORTABLE_APP_SESSION_PROTOCOL, PORTABLE_SUPERVISOR_CAPABILITY, schema_range_is_supported,
    },
    selection::{append_selected, require_canonical_normal_selection},
    signature::sha256_hex,
    snapshot::{create as create_snapshot, verify_unchanged as verify_snapshot_unchanged},
    win32::job::KillOnCloseJob,
};

use super::supervisor::authority::SupervisorSessionAuthority;
use crate::diagnostics::{PortableFailureSite, PortableMilestone};

pub(super) struct CurrentGeneration {
    pub(super) generation_root: std::path::PathBuf,
    pub(super) app: std::path::PathBuf,
    pub(super) generation_sha256: String,
    pub(super) app_sha256: String,
    pub(super) version: String,
    pub(super) minimum_supervisor_protocol: u16,
    pub(super) app_session_protocol: String,
    pub(super) minimum_schema: u32,
    pub(super) maximum_schema: u32,
    /// The generation selected before this transaction. This semantic lineage
    /// drives selection compensation even when that predecessor is not a live
    /// App process in the present supervisor session.
    pub(super) selection_predecessor_generation_sha256: Option<String>,
    /// The predecessor App that this supervisor actually observed quiesced.
    /// It alone authorizes the OldAppQuiesced journal phase.
    pub(super) quiesced_predecessor_generation_sha256: Option<String>,
}

pub(super) struct ActivatedTrial {
    pub(super) trial: TrialProcess,
    pub(super) journal: std::path::PathBuf,
}

#[derive(Clone, Copy)]
pub(super) struct ActivationContext<'a> {
    pub(super) root: &'a Path,
    pub(super) update_root: &'a Path,
    pub(super) selection_root: &'a Path,
    pub(super) job: &'a KillOnCloseJob,
    pub(super) epoch: &'a str,
    pub(super) supervisor_session: &'a SupervisorSessionAuthority,
    pub(super) generation_root_identity: &'a str,
    pub(super) portable_root_identity: &'a str,
}

pub(super) fn activate_generation_with_diagnostics(
    context: ActivationContext<'_>,
    current: &CurrentGeneration,
    mut diagnostics: Option<&mut PortableDiagnosticSession>,
) -> Result<ActivatedTrial> {
    let mut site = PortableFailureSite::ActivationStart;
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        let status = diagnostics.milestone(PortableMilestone::ActivationStarted);
        super::diagnostics_files::report_emit_failure(status);
    }
    let result = activate_generation_inner(context, current, &mut site, &mut diagnostics);
    if let Err(error) = &result
        && let Some(diagnostics) = &mut diagnostics
    {
        let status = diagnostics.failure(site, super::diagnostics_files::failure_class(error));
        super::diagnostics_files::report_emit_failure(status);
    }
    result
}

fn activate_generation_inner(
    context: ActivationContext<'_>,
    current: &CurrentGeneration,
    site: &mut PortableFailureSite,
    diagnostics: &mut Option<&mut PortableDiagnosticSession>,
) -> Result<ActivatedTrial> {
    let ActivationContext {
        root,
        update_root,
        selection_root,
        job,
        epoch,
        supervisor_session,
        generation_root_identity,
        portable_root_identity,
    } = context;
    if current.minimum_supervisor_protocol != PORTABLE_SUPERVISOR_CAPABILITY
        || current.app_session_protocol != PORTABLE_APP_SESSION_PROTOCOL
        || !schema_range_is_supported(current.minimum_schema, current.maximum_schema)
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_contract",
            "selected generation did not satisfy the stable supervisor/App contract",
        ));
    }
    let paths = renderpilot_orchestration::portable::RuntimePathsV1::from_portable_root(
        root.to_owned(),
        &current.generation_root,
        &current.app,
    )
    .map_err(|detail| PortableRuntimeError::new("portable_runtime_paths", detail))?;
    let transaction = hex_32()?;
    let journal = journal_path(update_root, &transaction);
    let selection_predecessor = current.selection_predecessor_generation_sha256.clone();
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::Prepared,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: "prepared",
        selection_record_sha256: None,
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::GenerationPublished,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: "published",
        selection_record_sha256: None,
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    if current.quiesced_predecessor_generation_sha256.is_some() {
        append_entry(JournalAppend {
            path: &journal,
            phase: JournalPhase::OldAppQuiesced,
            transaction: &transaction,
            generation: &current.generation_sha256,
            previous: selection_predecessor.clone(),
            transcript: "old-app-quiesced",
            selection_record_sha256: None,
            supervisor_session,
            generation_store_root: &paths.generation_store_root,
        })?;
    }
    let startup = PortableAppSessionV2 {
        app_session_protocol: current.app_session_protocol.clone(),
        epoch: epoch.to_owned(),
        generation_sha256: current.generation_sha256.clone(),
        minimum_schema: current.minimum_schema,
        maximum_schema: current.maximum_schema,
        transaction_id: transaction.clone(),
        supervisor_session_transcript_sha256: supervisor_session.transcript_sha256().to_owned(),
        portable_root_identity: portable_root_identity.to_owned(),
        generation_root_identity: generation_root_identity.to_owned(),
        mode: StartupMode::activation_trial(),
        runtime_paths: paths.clone(),
        challenge: hex_32()?,
        migration_permit_nonce: hex_32()?,
        commit_permit_nonce: hex_32()?,
    };
    let mut trial = TrialProcess::spawn(&current.app, job, &startup)?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::TrialSpawned,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &startup.transcript_sha256()?,
        selection_record_sha256: None,
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    *site = PortableFailureSite::ActivationReady;
    let schema_observed = trial.wait_trial_ready(&startup)?;
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        let status = diagnostics.milestone(PortableMilestone::ActivationReady);
        super::diagnostics_files::report_emit_failure(status);
    }
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::TrialReady,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &startup.transcript_sha256()?,
        selection_record_sha256: None,
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    *site = PortableFailureSite::ActivationMigration;
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        let status = diagnostics.milestone(PortableMilestone::ActivationMigration);
        super::diagnostics_files::report_emit_failure(status);
    }
    prepare_catalog(
        CatalogPreparationContext::new(
            &startup,
            &journal,
            &paths,
            &current.generation_sha256,
            selection_predecessor.as_deref(),
            supervisor_session,
        ),
        &mut trial,
        schema_observed,
    )?;
    // Each activation owns a fresh normal v3 selection, even if it activates
    // the same generation as the last completed supervisor session.
    *site = PortableFailureSite::ActivationCommit;
    let selection_journal_sequence = super::journal::read_entries(&journal)?.len() as u64 + 1;
    let (_path, selection_hash) = append_selected(
        selection_root,
        &current.generation_sha256,
        &transaction,
        selection_journal_sequence,
    )?;
    let selection_entry = append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::SelectionCommitted,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &selection_hash,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    require_canonical_normal_selection(
        selection_root,
        &transaction,
        selection_entry.sequence,
        &current.generation_sha256,
        &selection_hash,
    )?;
    let activation_nonce = hex_32()?;
    trial.send(&AppControlMessage::activation_permit(
        activation_nonce.clone(),
        selection_hash.clone(),
        selection_entry.sequence,
        startup.supervisor_session_transcript_sha256.clone(),
    ))?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::PermitSent,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &selection_hash,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    if !activation_ack_matches(
        &trial.receive()?,
        &activation_nonce,
        &selection_hash,
        &startup.supervisor_session_transcript_sha256,
    ) {
        return Err(PortableRuntimeError::new(
            "portable_activation",
            "App did not prove visible read-only activation",
        ));
    }
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::ActivationAcknowledged,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &selection_hash,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    let expected_committed_sequence = committed_sequence_for_selection(selection_entry.sequence)?;
    let committed = append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::Committed,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &startup.transcript_sha256()?,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    if committed.sequence != expected_committed_sequence {
        return Err(PortableRuntimeError::new(
            "portable_protocol_sequence",
            "Committed did not preserve the SelectionCommitted sequence relation",
        ));
    }
    trial.send(&AppControlMessage::commit_permit(
        selection_hash.clone(),
        committed.sequence,
        startup.commit_permit_nonce.clone(),
        startup.supervisor_session_transcript_sha256.clone(),
    ))?;
    if !commit_ack_matches(
        &trial.receive()?,
        &selection_hash,
        committed.sequence,
        &startup.commit_permit_nonce,
        &startup.supervisor_session_transcript_sha256,
    ) {
        return Err(PortableRuntimeError::new(
            "portable_activation",
            "App did not acknowledge the exact CommitPermit",
        ));
    }
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::CommitObserved,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor,
        transcript: &selection_hash,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    write_terminal_receipt(&journal, supervisor_session)?;
    cleanup_snapshot_after_terminal(&journal)?;
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        let status = diagnostics.milestone(PortableMilestone::ActivationCommitted);
        super::diagnostics_files::report_emit_failure(status);
    }
    Ok(ActivatedTrial { trial, journal })
}

pub(super) trait CatalogMigrationTrial {
    fn send_catalog_message(&mut self, message: &AppControlMessage) -> Result<()>;
    fn receive_catalog_message(&mut self) -> Result<AppStatusMessage>;
}

pub(super) fn activation_ack_matches(
    message: &AppStatusMessage,
    activation_nonce: &str,
    selection_record_sha256: &str,
    supervisor_session_transcript_sha256: &str,
) -> bool {
    matches!(
        message,
        AppStatusMessage::ActivationAck(ack)
            if ack.visible_window_ready
                && ack.event_loop_roundtrip
                && ack.activation_nonce == activation_nonce
                && ack.selection_record_sha256 == selection_record_sha256
                && ack.supervisor_session_transcript_sha256
                    == supervisor_session_transcript_sha256
    )
}

pub(super) fn commit_ack_matches(
    message: &AppStatusMessage,
    selection_record_sha256: &str,
    committed_journal_sequence: u64,
    permit_nonce: &str,
    supervisor_session_transcript_sha256: &str,
) -> bool {
    matches!(
        message,
        AppStatusMessage::CommitAck(ack)
            if ack.selection_record_sha256 == selection_record_sha256
                && ack.committed_journal_sequence == committed_journal_sequence
                && ack.permit_nonce == permit_nonce
                && ack.supervisor_session_transcript_sha256
                    == supervisor_session_transcript_sha256
    )
}

impl CatalogMigrationTrial for TrialProcess {
    fn send_catalog_message(&mut self, message: &AppControlMessage) -> Result<()> {
        self.send(message)
    }

    fn receive_catalog_message(&mut self) -> Result<AppStatusMessage> {
        self.receive()
    }
}

#[derive(Clone, Copy)]
pub(super) struct CatalogPreparationContext<'a> {
    startup: &'a PortableAppSessionV2,
    journal: &'a Path,
    paths: &'a renderpilot_orchestration::portable::RuntimePathsV1,
    generation: &'a str,
    previous: Option<&'a str>,
    supervisor_session: &'a SupervisorSessionAuthority,
}

impl<'a> CatalogPreparationContext<'a> {
    pub(super) fn new(
        startup: &'a PortableAppSessionV2,
        journal: &'a Path,
        paths: &'a renderpilot_orchestration::portable::RuntimePathsV1,
        generation: &'a str,
        previous: Option<&'a str>,
        supervisor_session: &'a SupervisorSessionAuthority,
    ) -> Self {
        Self {
            startup,
            journal,
            paths,
            generation,
            previous,
            supervisor_session,
        }
    }
}

pub(super) fn prepare_catalog(
    context: CatalogPreparationContext<'_>,
    trial: &mut impl CatalogMigrationTrial,
    source_schema: u32,
) -> Result<()> {
    let CatalogPreparationContext {
        startup,
        journal,
        paths,
        generation,
        previous,
        supervisor_session,
    } = context;
    let transaction = &startup.transaction_id;
    let transcript = if source_schema == 0 {
        format!(
            "fresh-catalog-uninitialized-v1:target={}",
            startup.maximum_schema
        )
    } else {
        let snapshot = if source_schema == startup.maximum_schema {
            None
        } else {
            let snapshot =
                create_snapshot(&paths.catalog_db_path, &paths.update_root, transaction)?;
            verify_snapshot_unchanged(&snapshot)?;
            append_entry(JournalAppend {
                path: journal,
                phase: JournalPhase::SnapshotCommitted,
                transaction,
                generation,
                previous: previous.map(str::to_owned),
                transcript: &snapshot.receipt_sha256,
                selection_record_sha256: None,
                supervisor_session,
                generation_store_root: &paths.generation_store_root,
            })?;
            begin_supervised_migration(transaction, source_schema, startup.maximum_schema)?;
            Some(snapshot)
        };
        let operation = match &snapshot {
            Some(snapshot) => {
                CatalogMigrationOperation::upgrade_after_snapshot(snapshot.receipt_sha256.clone())
            }
            None => CatalogMigrationOperation::validate_current(),
        };
        trial.send_catalog_message(&AppControlMessage::migration_permit(
            operation.clone(),
            source_schema,
            startup.maximum_schema,
            startup.migration_permit_nonce.clone(),
            startup.supervisor_session_transcript_sha256.clone(),
        ))?;
        let report = match trial.receive_catalog_message()? {
            AppStatusMessage::MigrationAck(ack)
                if ack.snapshot_receipt_sha256.as_deref()
                    == operation.snapshot_receipt_sha256()
                    && ack.permit_nonce == startup.migration_permit_nonce
                    && ack.supervisor_session_transcript_sha256
                        == startup.supervisor_session_transcript_sha256 =>
            {
                ack.report
            }
            _ => {
                return Err(PortableRuntimeError::new(
                    "portable_migration_contract",
                    "App did not acknowledge the exact supervised migration permit",
                ));
            }
        };
        verify_generation_report(
            &paths.catalog_db_path,
            source_schema,
            startup.maximum_schema,
            &report,
        )?;
        if let Some(snapshot) = snapshot.as_ref() {
            let receipt_path = paths
                .update_root
                .join("transactions")
                .join(transaction)
                .join("migration-receipt.json");
            commit_supervised_migration(&receipt_path, transaction, snapshot, &report)?;
        }
        serde_json::to_string(&report).map_err(|error| {
            PortableRuntimeError::new("portable_migration_receipt", error.to_string())
        })?
    };
    append_entry(JournalAppend {
        path: journal,
        phase: JournalPhase::MigrationCommitted,
        transaction,
        generation,
        previous: previous.map(str::to_owned),
        transcript: &transcript,
        selection_record_sha256: None,
        supervisor_session,
        generation_store_root: &paths.generation_store_root,
    })?;
    Ok(())
}

struct JournalAppend<'a> {
    path: &'a Path,
    phase: JournalPhase,
    transaction: &'a str,
    generation: &'a str,
    previous: Option<String>,
    transcript: &'a str,
    selection_record_sha256: Option<&'a str>,
    supervisor_session: &'a SupervisorSessionAuthority,
    generation_store_root: &'a Path,
}

fn append_entry(input: JournalAppend<'_>) -> Result<JournalEntry> {
    let JournalAppend {
        path,
        phase,
        transaction,
        generation,
        previous,
        transcript,
        selection_record_sha256,
        supervisor_session,
        generation_store_root,
    } = input;
    append_normal_with_outbox(
        path,
        generation_store_root,
        JournalEntry {
            protocol: 0,
            sequence: 0,
            phase,
            transaction_id: transaction.to_owned(),
            activation_id: sha256_hex(
                format!("renderpilot-portable-activation-v3\0{transaction}\0{generation}")
                    .as_bytes(),
            ),
            selected_generation_sha256: generation.to_owned(),
            previous_sha256: previous,
            transcript_sha256: sha256_hex(transcript.as_bytes()),
            origin_session_sha256: String::new(),
            writer_session_sha256: String::new(),
            predecessor_writer_session_sha256: None,
            append_kind: JournalAppendKind::Normal,
            previous_entry_sha256: None,
            phase_receipt_sha256: String::new(),
            selection_record_sha256: selection_record_sha256.map(str::to_owned),
        },
        supervisor_session,
    )
}
