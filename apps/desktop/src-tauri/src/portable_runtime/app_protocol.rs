use std::io::{BufRead, BufReader, Write};

use renderpilot_orchestration::portable::RuntimePathsV1;
use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

pub const STARTUP_PROTOCOL: u16 = 3;
const COMMITTED_SEQUENCE_OFFSET: u64 = 3;

/// The normal activation journal always places `Committed` three entries after
/// `SelectionCommitted`: PermitSent, ActivationAcknowledged, then Committed.
/// Keep the checked relation shared by the supervisor and managed App without
/// changing the wire DTO that carries the two concrete sequences.
pub fn committed_sequence_for_selection(selection_journal_sequence: u64) -> Result<u64> {
    selection_journal_sequence
        .checked_add(COMMITTED_SEQUENCE_OFFSET)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_protocol_sequence",
                "SelectionCommitted sequence overflowed the CommitPermit relation",
            )
        })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum StartupMode {
    ActivationTrial,
    CommittedSelection {
        selection_record_sha256: String,
        committed_journal_sequence: u64,
        committed_transcript_sha256: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PortableStartupV3 {
    pub protocol: u16,
    pub epoch: String,
    pub generation_sha256: String,
    pub minimum_schema: u32,
    pub maximum_schema: u32,
    pub transaction_id: String,
    /// Hash of the live supervisor-session transcript. Every proof and permit
    /// repeats this value to prevent cross-session protocol mixing.
    pub supervisor_session_transcript_sha256: String,
    pub portable_root_identity: String,
    pub generation_root_identity: String,
    pub mode: StartupMode,
    pub runtime_paths: RuntimePathsV1,
    pub challenge: String,
    pub commit_permit_nonce: String,
}

impl PortableStartupV3 {
    pub fn validate(&self) -> Result<()> {
        if self.protocol != STARTUP_PROTOCOL
            || !is_sha256(&self.generation_sha256)
            || self.minimum_schema != super::rpu::MINIMUM_SCHEMA
            || self.maximum_schema != super::rpu::MAXIMUM_SCHEMA
            || self.epoch.is_empty()
            || self.transaction_id.is_empty()
            || !is_sha256(&self.supervisor_session_transcript_sha256)
            || !is_sha256(&self.portable_root_identity)
            || !is_sha256(&self.generation_root_identity)
            || self.challenge.len() < 32
            || self.commit_permit_nonce.len() < 32
        {
            return Err(PortableRuntimeError::new(
                "portable_startup_invalid",
                "startup record did not satisfy PortableStartupV3",
            ));
        }
        self.runtime_paths
            .validate()
            .map_err(|detail| PortableRuntimeError::new("portable_startup_paths", detail))
    }

    pub fn transcript_sha256(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self).map_err(|error| {
            PortableRuntimeError::new("portable_protocol_encode", error.to_string())
        })?;
        Ok(sha256_hex(&bytes))
    }

    pub fn runtime_paths_sha256(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&self.runtime_paths).map_err(|error| {
            PortableRuntimeError::new("portable_protocol_encode", error.to_string())
        })?;
        Ok(sha256_hex(&bytes))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PortableUpdateRequest {
    Check,
    Download,
    Apply,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PortableUpdateResponse {
    Check {
        available: bool,
        current_version: String,
        version: String,
        date: Option<String>,
        body: String,
    },
    Downloaded {
        content_length: u64,
    },
    ApplyAccepted,
    Rejected {
        code: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppControlMessage {
    Startup(Box<PortableStartupV3>),
    ActivationPermit {
        activation_nonce: String,
        selection_record_sha256: String,
        journal_sequence: u64,
        supervisor_session_transcript_sha256: String,
    },
    CommitPermit {
        selection_record_sha256: String,
        committed_journal_sequence: u64,
        permit_nonce: String,
        supervisor_session_transcript_sha256: String,
    },
    UpdateResponse {
        request_id: String,
        response: PortableUpdateResponse,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppStatusMessage {
    TrialHello {
        challenge: String,
    },
    TrialReady {
        transcript_sha256: String,
        runtime_paths_sha256: String,
        schema_observed: u32,
        db_query_only: bool,
        webview_profile_ready: bool,
        ui_bundle_ready: bool,
        visible_window_ready: bool,
        event_loop_roundtrip: bool,
        supervisor_session_transcript_sha256: String,
    },
    ActivationAck {
        activation_nonce: String,
        selection_record_sha256: String,
        visible_window_ready: bool,
        event_loop_roundtrip: bool,
        supervisor_session_transcript_sha256: String,
    },
    CommitAck {
        selection_record_sha256: String,
        committed_journal_sequence: u64,
        permit_nonce: String,
        supervisor_session_transcript_sha256: String,
    },
    UpdateRequest {
        request_id: String,
        request: PortableUpdateRequest,
    },
}

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *writer, message).map_err(|error| {
        PortableRuntimeError::new("portable_protocol_encode", error.to_string())
    })?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub fn read_message<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> Result<T> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.is_empty() || line.len() > 64 * 1024 {
        return Err(PortableRuntimeError::new(
            "portable_protocol_invalid",
            "empty or oversized protocol message",
        ));
    }
    serde_json::from_str(&line)
        .map_err(|error| PortableRuntimeError::new("portable_protocol_invalid", error.to_string()))
}

pub fn reader(file: std::fs::File) -> BufReader<std::fs::File> {
    BufReader::new(file)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}
