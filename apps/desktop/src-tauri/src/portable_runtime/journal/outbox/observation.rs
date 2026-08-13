use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_hex,
};

use super::store::{OPERATION_OUTBOX_PROTOCOL, operation_root, publish_outbox_record};
use crate::portable_runtime::journal::image::{
    CapturedJournalImage, JournalImage, capture_exact_current,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::portable_runtime::journal) enum ObservationOutcome {
    Committed,
    NotCommittedBeforeMutation,
    NotCommittedAfterTailRepair,
    SupersededByAuthoritativeJournal,
    TailRemoved,
    AbortedBeforeOrigin,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct OperationObservationV1 {
    protocol: u16,
    replay_key: String,
    transaction_id: String,
    image: JournalImage,
    outcome: ObservationOutcome,
}

/// Records an image-bound receipt only while the source stays exactly the same
/// capture both before and after receipt publication.
pub(in crate::portable_runtime::journal) fn observe_exact_current(
    generation_store_root: &Path,
    path: &Path,
    transaction_id: &str,
    replay_subject: &str,
    image: &CapturedJournalImage,
    outcome: ObservationOutcome,
) -> Result<bool> {
    if !image.is_valid() || image.transaction_id() != transaction_id {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "observation did not bind one valid transaction image",
        ));
    }
    capture_exact_current(path, image)?;
    let replay_key = observation_key(replay_subject, outcome, image.image());
    let recorded = observe_once(
        generation_store_root,
        transaction_id,
        &replay_key,
        image.image().clone(),
        outcome,
    )?;
    capture_exact_current(path, image)?;
    Ok(recorded)
}

pub(in crate::portable_runtime::journal) fn observation_matches(
    generation_store_root: &Path,
    transaction_id: &str,
    replay_key: &str,
    image: JournalImage,
    outcome: ObservationOutcome,
) -> Result<bool> {
    let destination = operation_root(generation_store_root)
        .join("observations")
        .join(transaction_id)
        .join(format!("{replay_key}.sealed"));
    let existing = match fs::read(destination) {
        Ok(existing) => existing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let receipt = OperationObservationV1 {
        protocol: OPERATION_OUTBOX_PROTOCOL,
        replay_key: replay_key.to_owned(),
        transaction_id: transaction_id.to_owned(),
        image,
        outcome,
    };
    let expected = serde_json::to_vec(&receipt)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    Ok(provenance::open(
        SealDomain::Journal,
        &format!("journal-outbox-observation:{transaction_id}:{replay_key}"),
        &existing,
    )? == expected)
}

pub(in crate::portable_runtime::journal) fn observation_key(
    replay_subject: &str,
    outcome: ObservationOutcome,
    image: &JournalImage,
) -> String {
    sha256_hex(
        format!(
            "{replay_subject}:{outcome:?}:{}:{}:{}:{}:{}:{}",
            image.exists,
            image.byte_len,
            image.file_sha256,
            image.sealed_head_sha256.as_deref().unwrap_or_default(),
            image.last_sequence.unwrap_or_default(),
            image
                .last_phase
                .map(|phase| format!("{phase:?}"))
                .unwrap_or_default(),
        )
        .as_bytes(),
    )
}

fn observe_once(
    generation_store_root: &Path,
    transaction_id: &str,
    replay_key: &str,
    image: JournalImage,
    outcome: ObservationOutcome,
) -> Result<bool> {
    let receipt = OperationObservationV1 {
        protocol: OPERATION_OUTBOX_PROTOCOL,
        replay_key: replay_key.to_owned(),
        transaction_id: transaction_id.to_owned(),
        image,
        outcome,
    };
    let payload = serde_json::to_vec(&receipt)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    let destination = operation_root(generation_store_root)
        .join("observations")
        .join(transaction_id)
        .join(format!("{replay_key}.sealed"));
    if destination.exists() {
        let existing = fs::read(&destination)?;
        if provenance::open(
            SealDomain::Journal,
            &format!("journal-outbox-observation:{transaction_id}:{replay_key}"),
            &existing,
        )? == payload
        {
            return Ok(false);
        }
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "existing observation receipt did not match exact replay evidence",
        ));
    }
    publish_outbox_record(
        &destination,
        &format!("journal-outbox-observation:{transaction_id}:{replay_key}"),
        &payload,
    )?;
    Ok(true)
}
