use renderpilot_orchestration::portable::RuntimePathsV1;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    rpu::{MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL},
    signature::sha256_hex,
};

const COMMITTED_SEQUENCE_OFFSET: u64 = 3;

/// The normal activation journal always places `Committed` three entries after
/// `SelectionCommitted`: PermitSent, ActivationAcknowledged, then Committed.
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
#[serde(deny_unknown_fields)]
pub struct ActivationTrialStartupMode {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedSelectionStartupMode {
    pub selection_record_sha256: String,
    pub committed_journal_sequence: u64,
    pub committed_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartupMode {
    ActivationTrial(ActivationTrialStartupMode),
    CommittedSelection(CommittedSelectionStartupMode),
}

impl StartupMode {
    pub const fn activation_trial() -> Self {
        Self::ActivationTrial(ActivationTrialStartupMode {})
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateCurrentCatalogMigration {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeAfterSnapshotCatalogMigration {
    pub snapshot_receipt_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogMigrationOperation {
    ValidateCurrent(ValidateCurrentCatalogMigration),
    UpgradeAfterSnapshot(UpgradeAfterSnapshotCatalogMigration),
}

impl CatalogMigrationOperation {
    pub const fn validate_current() -> Self {
        Self::ValidateCurrent(ValidateCurrentCatalogMigration {})
    }

    pub fn upgrade_after_snapshot(snapshot_receipt_sha256: impl Into<String>) -> Self {
        Self::UpgradeAfterSnapshot(UpgradeAfterSnapshotCatalogMigration {
            snapshot_receipt_sha256: snapshot_receipt_sha256.into(),
        })
    }

    pub fn snapshot_receipt_sha256(&self) -> Option<&str> {
        match self {
            Self::ValidateCurrent(_) => None,
            Self::UpgradeAfterSnapshot(operation) => Some(&operation.snapshot_receipt_sha256),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMigrationReport {
    pub source_version: u32,
    pub target_version: u32,
    pub catalog_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableAppSessionV2 {
    pub app_session_protocol: String,
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
    pub migration_permit_nonce: String,
    pub commit_permit_nonce: String,
}

impl PortableAppSessionV2 {
    pub fn validate(&self) -> Result<()> {
        if self.app_session_protocol != PORTABLE_APP_SESSION_PROTOCOL
            || !is_sha256(&self.generation_sha256)
            || self.minimum_schema != MINIMUM_SCHEMA
            || self.maximum_schema != MAXIMUM_SCHEMA
            || self.epoch.is_empty()
            || self.transaction_id.is_empty()
            || !is_sha256(&self.supervisor_session_transcript_sha256)
            || !is_sha256(&self.portable_root_identity)
            || !is_sha256(&self.generation_root_identity)
            || self.challenge.len() < 32
            || self.migration_permit_nonce.len() < 32
            || self.commit_permit_nonce.len() < 32
        {
            return Err(PortableRuntimeError::new(
                "portable_startup_invalid",
                "startup record did not satisfy PortableAppSessionV2",
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPortableUpdateRequest {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadPortableUpdateRequest {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyPortableUpdateRequest {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PortableUpdateRequest {
    Check(CheckPortableUpdateRequest),
    Download(DownloadPortableUpdateRequest),
    Apply(ApplyPortableUpdateRequest),
}

impl PortableUpdateRequest {
    pub const fn check() -> Self {
        Self::Check(CheckPortableUpdateRequest {})
    }

    pub const fn download() -> Self {
        Self::Download(DownloadPortableUpdateRequest {})
    }

    pub const fn apply() -> Self {
        Self::Apply(ApplyPortableUpdateRequest {})
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPortableUpdateResponse {
    pub available: bool,
    pub current_version: String,
    pub version: String,
    pub date: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadedPortableUpdateResponse {
    pub content_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyAcceptedPortableUpdateResponse {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RejectedPortableUpdateResponse {
    pub code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum PortableUpdateResponse {
    Check(CheckPortableUpdateResponse),
    Downloaded(DownloadedPortableUpdateResponse),
    ApplyAccepted(ApplyAcceptedPortableUpdateResponse),
    Rejected(RejectedPortableUpdateResponse),
}

impl PortableUpdateResponse {
    pub fn check(
        available: bool,
        current_version: impl Into<String>,
        version: impl Into<String>,
        date: Option<String>,
        body: impl Into<String>,
    ) -> Self {
        Self::Check(CheckPortableUpdateResponse {
            available,
            current_version: current_version.into(),
            version: version.into(),
            date,
            body: body.into(),
        })
    }

    pub const fn downloaded(content_length: u64) -> Self {
        Self::Downloaded(DownloadedPortableUpdateResponse { content_length })
    }

    pub const fn apply_accepted() -> Self {
        Self::ApplyAccepted(ApplyAcceptedPortableUpdateResponse {})
    }

    pub fn rejected(code: impl Into<String>) -> Self {
        Self::Rejected(RejectedPortableUpdateResponse { code: code.into() })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationPermit {
    pub operation: CatalogMigrationOperation,
    pub source_schema: u32,
    pub target_schema: u32,
    pub permit_nonce: String,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPermit {
    pub activation_nonce: String,
    pub selection_record_sha256: String,
    pub journal_sequence: u64,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitPermit {
    pub selection_record_sha256: String,
    pub committed_journal_sequence: u64,
    pub permit_nonce: String,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateResponse {
    pub request_id: Arc<str>,
    pub response: PortableUpdateResponse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum PortableUpdateEvent {
    #[serde(rename = "download_started")]
    Started { content_length: Option<u64> },
    #[serde(rename = "download_progress")]
    Progress { chunk_length: u64 },
    #[serde(rename = "download_finished")]
    Finished {},
}

impl PortableUpdateEvent {
    pub const fn download_started(content_length: Option<u64>) -> Self {
        Self::Started { content_length }
    }

    pub const fn download_progress(chunk_length: u64) -> Self {
        Self::Progress { chunk_length }
    }

    pub const fn download_finished() -> Self {
        Self::Finished {}
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateEvent {
    pub request_id: Arc<str>,
    pub event: PortableUpdateEvent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppControlMessage {
    Startup(Box<PortableAppSessionV2>),
    MigrationPermit(MigrationPermit),
    ActivationPermit(ActivationPermit),
    CommitPermit(CommitPermit),
    UpdateEvent(UpdateEvent),
    UpdateResponse(UpdateResponse),
}

impl AppControlMessage {
    pub fn startup(startup: PortableAppSessionV2) -> Self {
        Self::Startup(Box::new(startup))
    }

    pub fn migration_permit(
        operation: CatalogMigrationOperation,
        source_schema: u32,
        target_schema: u32,
        permit_nonce: impl Into<String>,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::MigrationPermit(MigrationPermit {
            operation,
            source_schema,
            target_schema,
            permit_nonce: permit_nonce.into(),
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn activation_permit(
        activation_nonce: impl Into<String>,
        selection_record_sha256: impl Into<String>,
        journal_sequence: u64,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::ActivationPermit(ActivationPermit {
            activation_nonce: activation_nonce.into(),
            selection_record_sha256: selection_record_sha256.into(),
            journal_sequence,
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn commit_permit(
        selection_record_sha256: impl Into<String>,
        committed_journal_sequence: u64,
        permit_nonce: impl Into<String>,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::CommitPermit(CommitPermit {
            selection_record_sha256: selection_record_sha256.into(),
            committed_journal_sequence,
            permit_nonce: permit_nonce.into(),
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn update_response(
        request_id: impl Into<Arc<str>>,
        response: PortableUpdateResponse,
    ) -> Self {
        Self::UpdateResponse(UpdateResponse {
            request_id: request_id.into(),
            response,
        })
    }

    pub fn update_event(request_id: impl Into<Arc<str>>, event: PortableUpdateEvent) -> Self {
        Self::UpdateEvent(UpdateEvent {
            request_id: request_id.into(),
            event,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrialHello {
    pub challenge: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrialReady {
    pub transcript_sha256: String,
    pub runtime_paths_sha256: String,
    pub schema_observed: u32,
    pub db_query_only: bool,
    pub webview_profile_ready: bool,
    pub ui_bundle_ready: bool,
    pub visible_window_ready: bool,
    pub event_loop_roundtrip: bool,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationAck {
    pub report: CatalogMigrationReport,
    pub snapshot_receipt_sha256: Option<String>,
    pub permit_nonce: String,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationAck {
    pub activation_nonce: String,
    pub selection_record_sha256: String,
    pub visible_window_ready: bool,
    pub event_loop_roundtrip: bool,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitAck {
    pub selection_record_sha256: String,
    pub committed_journal_sequence: u64,
    pub permit_nonce: String,
    pub supervisor_session_transcript_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRequest {
    pub request_id: Arc<str>,
    pub request: PortableUpdateRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppStatusMessage {
    TrialHello(TrialHello),
    TrialReady(TrialReady),
    MigrationAck(MigrationAck),
    ActivationAck(ActivationAck),
    CommitAck(CommitAck),
    UpdateRequest(UpdateRequest),
}

impl AppStatusMessage {
    pub fn trial_hello(challenge: impl Into<String>) -> Self {
        Self::TrialHello(TrialHello {
            challenge: challenge.into(),
        })
    }

    pub const fn trial_ready(readiness: TrialReady) -> Self {
        Self::TrialReady(readiness)
    }

    pub fn migration_ack(
        report: CatalogMigrationReport,
        snapshot_receipt_sha256: Option<String>,
        permit_nonce: impl Into<String>,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::MigrationAck(MigrationAck {
            report,
            snapshot_receipt_sha256,
            permit_nonce: permit_nonce.into(),
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn activation_ack(
        activation_nonce: impl Into<String>,
        selection_record_sha256: impl Into<String>,
        visible_window_ready: bool,
        event_loop_roundtrip: bool,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::ActivationAck(ActivationAck {
            activation_nonce: activation_nonce.into(),
            selection_record_sha256: selection_record_sha256.into(),
            visible_window_ready,
            event_loop_roundtrip,
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn commit_ack(
        selection_record_sha256: impl Into<String>,
        committed_journal_sequence: u64,
        permit_nonce: impl Into<String>,
        supervisor_session_transcript_sha256: impl Into<String>,
    ) -> Self {
        Self::CommitAck(CommitAck {
            selection_record_sha256: selection_record_sha256.into(),
            committed_journal_sequence,
            permit_nonce: permit_nonce.into(),
            supervisor_session_transcript_sha256: supervisor_session_transcript_sha256.into(),
        })
    }

    pub fn update_request(request_id: impl Into<Arc<str>>, request: PortableUpdateRequest) -> Self {
        Self::UpdateRequest(UpdateRequest {
            request_id: request_id.into(),
            request,
        })
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}
