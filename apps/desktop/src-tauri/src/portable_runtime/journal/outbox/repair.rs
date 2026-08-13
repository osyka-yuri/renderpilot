use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_hex,
};

use super::{
    append_intent::{AppendIntentV1, valid_image_shape},
    store::{OPERATION_OUTBOX_PROTOCOL, operation_root, publish_outbox_record},
};
use crate::portable_runtime::journal::{
    image::CapturedJournalImage, mutation::ExactJournalMutation, paths::is_digest,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(in crate::portable_runtime::journal) struct RepairIntentV1 {
    protocol: u16,
    transaction_id: String,
    base: crate::portable_runtime::journal::image::JournalImage,
    before_byte_len: u64,
    before_file_sha256: String,
    removed_suffix_sha256: String,
}

pub(in crate::portable_runtime::journal) fn record_repair_intent(
    generation_store_root: &Path,
    intent_key: &str,
    intent: &AppendIntentV1,
    before: &CapturedJournalImage,
) -> Result<RepairIntentV1> {
    let base_len = usize::try_from(intent.base.byte_len).map_err(|_| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal length did not fit usize",
        )
    })?;
    let suffix = before.bytes().get(base_len..).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "captured repair image was shorter than its immutable base",
        )
    })?;
    let repair = RepairIntentV1 {
        protocol: OPERATION_OUTBOX_PROTOCOL,
        transaction_id: intent.transaction_id.clone(),
        base: intent.base.clone(),
        before_byte_len: before.bytes().len() as u64,
        before_file_sha256: sha256_hex(before.bytes()),
        removed_suffix_sha256: sha256_hex(suffix),
    };
    let key = repair_intent_key(intent_key);
    let payload = serde_json::to_vec(&repair)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    let destination = operation_root(generation_store_root)
        .join("repair-intents")
        .join(format!("{key}.sealed"));
    publish_outbox_record(
        &destination,
        &format!("journal-outbox-repair:{}:{key}", intent.transaction_id),
        &payload,
    )?;
    Ok(repair)
}

pub(in crate::portable_runtime::journal) fn repair_intent_key(intent_key: &str) -> String {
    sha256_hex(format!("repair:{intent_key}").as_bytes())
}

/// The append intent and retained repair intent jointly name the distinct
/// post-repair observation. It can never merge with untouched Before evidence.
pub(in crate::portable_runtime::journal) fn tail_repair_replay_subject(intent_key: &str) -> String {
    let repair_key = repair_intent_key(intent_key);
    sha256_hex(format!("append:{intent_key}:repair:{repair_key}").as_bytes())
}

pub(in crate::portable_runtime::journal) fn retained_repair_matches(
    generation_store_root: &Path,
    intent_key: &str,
    intent: &AppendIntentV1,
) -> Result<bool> {
    let key = repair_intent_key(intent_key);
    let destination = operation_root(generation_store_root)
        .join("repair-intents")
        .join(format!("{key}.sealed"));
    let bytes = match fs::read(destination) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let repair: RepairIntentV1 = serde_json::from_slice(&provenance::open(
        SealDomain::Journal,
        &format!("journal-outbox-repair:{}:{key}", intent.transaction_id),
        &bytes,
    )?)
    .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    if !valid_repair_shape(&repair) {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "retained repair intent was malformed",
        ));
    }
    Ok(repair.transaction_id == intent.transaction_id && repair.base == intent.base)
}

pub(in crate::portable_runtime::journal) fn truncate_exact_tail(
    path: &Path,
    repair: &RepairIntentV1,
    before: &CapturedJournalImage,
) -> Result<CapturedJournalImage> {
    let base_len = usize::try_from(repair.base.byte_len).map_err(|_| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal length did not fit usize",
        )
    })?;
    if !before.image().exists
        || before.bytes().len() as u64 != repair.before_byte_len
        || sha256_hex(before.bytes()) != repair.before_file_sha256
        || before
            .bytes()
            .get(..base_len)
            .is_none_or(|base| sha256_hex(base) != repair.base.file_sha256)
        || before
            .bytes()
            .get(base_len..)
            .is_none_or(|suffix| sha256_hex(suffix) != repair.removed_suffix_sha256)
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal repair intent did not bind the captured suffix",
        ));
    }
    ExactJournalMutation::truncate(path, before, repair.base.byte_len)
}

fn valid_repair_shape(repair: &RepairIntentV1) -> bool {
    repair.protocol == OPERATION_OUTBOX_PROTOCOL
        && is_digest(&repair.transaction_id)
        && valid_image_shape(&repair.base)
        && repair.before_byte_len > repair.base.byte_len
        && is_digest(&repair.before_file_sha256)
        && is_digest(&repair.removed_suffix_sha256)
}
