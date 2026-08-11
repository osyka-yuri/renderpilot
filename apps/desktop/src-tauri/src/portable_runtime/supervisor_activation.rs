//! Per-generation activation transaction owned by the portable supervisor.

use std::path::Path;

use super::{
    app_process::TrialProcess,
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableStartupV3, StartupMode,
        committed_sequence_for_selection,
    },
    cleanup::cleanup_snapshot_after_terminal,
    error::{PortableRuntimeError, Result},
    journal::{
        JournalAppendKind, JournalEntry, JournalPhase, append_normal, journal_path,
        write_terminal_receipt,
    },
    migration::{
        PORTABLE_SCHEMA_VERSION, migrate_to_current, read_schema_version, validate_current_schema,
    },
    random::hex_32,
    selection::{append_selected, require_canonical_normal_selection},
    signature::sha256_hex,
    snapshot::create as create_snapshot,
    win32::job::KillOnCloseJob,
};

use super::supervisor::authority::SupervisorSessionAuthority;

pub(super) struct CurrentGeneration {
    pub(super) generation_root: std::path::PathBuf,
    pub(super) app: std::path::PathBuf,
    pub(super) generation_sha256: String,
    pub(super) version: String,
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
}

pub(super) fn activate_generation(
    context: ActivationContext<'_>,
    current: &CurrentGeneration,
) -> Result<ActivatedTrial> {
    let ActivationContext {
        root,
        update_root,
        selection_root,
        job,
        epoch,
        supervisor_session,
        generation_root_identity,
    } = context;
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
        })?;
    }
    let paths = renderpilot_orchestration::portable::RuntimePathsV1::from_portable_root(
        root.to_owned(),
        &current.generation_root,
        &current.app,
    )
    .map_err(|detail| PortableRuntimeError::new("portable_runtime_paths", detail))?;
    let startup = PortableStartupV3 {
        protocol: super::app_protocol::STARTUP_PROTOCOL,
        epoch: epoch.to_owned(),
        generation_sha256: current.generation_sha256.clone(),
        minimum_schema: current.minimum_schema,
        maximum_schema: current.maximum_schema,
        transaction_id: transaction.clone(),
        supervisor_session_transcript_sha256: supervisor_session.transcript_sha256().to_owned(),
        portable_root_identity: super::win32::directory::directory_identity_digest_no_reparse(
            root,
        )?,
        generation_root_identity: generation_root_identity.to_owned(),
        mode: StartupMode::ActivationTrial,
        runtime_paths: paths.clone(),
        challenge: hex_32()?,
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
    })?;
    trial.wait_trial_ready(&startup)?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::TrialReady,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &startup.transcript_sha256()?,
        selection_record_sha256: None,
        supervisor_session,
    })?;
    let migration_receipt = migrate_if_existing(
        &journal,
        &paths,
        &transaction,
        &current.generation_sha256,
        selection_predecessor.clone(),
        supervisor_session,
    )?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::MigrationCommitted,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &migration_receipt,
        selection_record_sha256: None,
        supervisor_session,
    })?;
    // Each activation owns a fresh normal v3 selection, even if it activates
    // the same generation as the last completed supervisor session.
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
    })?;
    require_canonical_normal_selection(
        selection_root,
        &transaction,
        selection_entry.sequence,
        &current.generation_sha256,
        &selection_hash,
    )?;
    let activation_nonce = hex_32()?;
    trial.send(&AppControlMessage::ActivationPermit {
        activation_nonce: activation_nonce.clone(),
        selection_record_sha256: selection_hash.clone(),
        journal_sequence: selection_entry.sequence,
        supervisor_session_transcript_sha256: startup.supervisor_session_transcript_sha256.clone(),
    })?;
    append_entry(JournalAppend {
        path: &journal,
        phase: JournalPhase::PermitSent,
        transaction: &transaction,
        generation: &current.generation_sha256,
        previous: selection_predecessor.clone(),
        transcript: &selection_hash,
        selection_record_sha256: Some(&selection_hash),
        supervisor_session,
    })?;
    match trial.receive()? {
        AppStatusMessage::ActivationAck {
            activation_nonce: received,
            selection_record_sha256: selected,
            visible_window_ready: true,
            event_loop_roundtrip: true,
            supervisor_session_transcript_sha256,
        } if received == activation_nonce
            && selected == selection_hash
            && supervisor_session_transcript_sha256
                == startup.supervisor_session_transcript_sha256 => {}
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_activation",
                "App did not prove visible read-only activation",
            ));
        }
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
    })?;
    if committed.sequence != expected_committed_sequence {
        return Err(PortableRuntimeError::new(
            "portable_protocol_sequence",
            "Committed did not preserve the SelectionCommitted sequence relation",
        ));
    }
    trial.send(&AppControlMessage::CommitPermit {
        selection_record_sha256: selection_hash.clone(),
        committed_journal_sequence: committed.sequence,
        permit_nonce: startup.commit_permit_nonce.clone(),
        supervisor_session_transcript_sha256: startup.supervisor_session_transcript_sha256.clone(),
    })?;
    match trial.receive()? {
        AppStatusMessage::CommitAck {
            selection_record_sha256,
            committed_journal_sequence,
            permit_nonce,
            supervisor_session_transcript_sha256,
        } if selection_record_sha256 == selection_hash
            && committed_journal_sequence == committed.sequence
            && permit_nonce == startup.commit_permit_nonce
            && supervisor_session_transcript_sha256
                == startup.supervisor_session_transcript_sha256 =>
        {
            append_entry(JournalAppend {
                path: &journal,
                phase: JournalPhase::CommitObserved,
                transaction: &transaction,
                generation: &current.generation_sha256,
                previous: selection_predecessor,
                transcript: &selection_hash,
                selection_record_sha256: Some(&selection_hash),
                supervisor_session,
            })?;
            write_terminal_receipt(&journal, supervisor_session)?;
            cleanup_snapshot_after_terminal(&journal)?;
        }
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_activation",
                "App did not acknowledge the exact CommitPermit",
            ));
        }
    }
    Ok(ActivatedTrial { trial, journal })
}

pub(super) fn migrate_if_existing(
    journal: &Path,
    paths: &renderpilot_orchestration::portable::RuntimePathsV1,
    transaction: &str,
    generation: &str,
    previous: Option<String>,
    supervisor_session: &SupervisorSessionAuthority,
) -> Result<String> {
    if !paths.catalog_db_path.exists() {
        return Ok("fresh-schema-16".to_owned());
    }
    let version = read_schema_version(&paths.catalog_db_path)?;
    if version == PORTABLE_SCHEMA_VERSION {
        let receipt = validate_current_schema(&paths.catalog_db_path)?;
        return serde_json::to_string(&receipt).map_err(|error| {
            PortableRuntimeError::new("portable_migration_receipt", error.to_string())
        });
    }
    let snapshot = create_snapshot(&paths.catalog_db_path, &paths.update_root, transaction)?;
    append_entry(JournalAppend {
        path: journal,
        phase: JournalPhase::SnapshotCommitted,
        transaction,
        generation,
        previous,
        transcript: &snapshot.receipt_sha256,
        selection_record_sha256: None,
        supervisor_session,
    })?;
    let receipt_path = paths
        .update_root
        .join("transactions")
        .join(transaction)
        .join("migration-receipt.json");
    let receipt = migrate_to_current(&paths.catalog_db_path, &snapshot, &receipt_path)?;
    serde_json::to_string(&receipt)
        .map_err(|error| PortableRuntimeError::new("portable_migration_receipt", error.to_string()))
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
    } = input;
    append_normal(
        path,
        JournalEntry {
            protocol: 0,
            sequence: 0,
            phase,
            transaction_id: transaction.to_owned(),
            activation_id: sha256_hex(
                format!("renderpilot-portable-activation-v3\\0{transaction}\\0{generation}")
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
