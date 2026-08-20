use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_hex,
};

use super::store::{OPERATION_OUTBOX_PROTOCOL, operation_root, publish_outbox_record};
use crate::portable_runtime::journal::{
    image::{JournalImage, absent_image},
    paths::{is_digest, journal_identity},
    protocol::{JournalAppendKind, JournalEntry, JournalPhase},
    transition::entry_is_valid_after,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(in crate::portable_runtime::journal) struct AppendIntentV1 {
    pub(in crate::portable_runtime::journal) protocol: u16,
    pub(in crate::portable_runtime::journal) transaction_id: String,
    pub(in crate::portable_runtime::journal) base: JournalImage,
    pub(in crate::portable_runtime::journal) target_phase: JournalPhase,
    pub(in crate::portable_runtime::journal) target_line: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(in crate::portable_runtime::journal) struct AuthenticatedAppendIntent {
    pub(in crate::portable_runtime::journal) key: String,
    pub(in crate::portable_runtime::journal) transaction_id: String,
    pub(in crate::portable_runtime::journal) journal_object_id: String,
    pub(in crate::portable_runtime::journal) intent: AppendIntentV1,
}

pub(in crate::portable_runtime::journal) fn append_intent_key(
    intent: &AppendIntentV1,
) -> Result<String> {
    serde_json::to_vec(intent)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))
}

pub(in crate::portable_runtime::journal) fn new_append_intent(
    path: &Path,
    base: JournalImage,
    target_phase: JournalPhase,
    target_line: Vec<u8>,
) -> Result<AppendIntentV1> {
    let (transaction_id, _) = journal_identity(path)?;
    let intent = AppendIntentV1 {
        protocol: OPERATION_OUTBOX_PROTOCOL,
        transaction_id,
        base,
        target_phase,
        target_line,
    };
    if !valid_intent_shape(&intent) {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "append intent had an invalid immutable shape",
        ));
    }
    Ok(intent)
}

pub(in crate::portable_runtime::journal) fn record_append_intent(
    generation_store_root: &Path,
    intent: &AppendIntentV1,
) -> Result<()> {
    let key = append_intent_key(intent)?;
    let payload = serde_json::to_vec(intent)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    let destination = operation_root(generation_store_root)
        .join("append-intents")
        .join(format!("{key}.sealed"));
    publish_outbox_record(
        &destination,
        &format!("journal-outbox-append:{}:{key}", intent.transaction_id),
        &payload,
    )
}

pub(in crate::portable_runtime::journal) fn read_append_intents(
    generation_store_root: &Path,
) -> Result<Vec<AuthenticatedAppendIntent>> {
    let root = operation_root(generation_store_root).join("append-intents");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut intents = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "attempts" && entry.file_type()?.is_dir() {
            continue;
        }
        if entry.file_type()?.is_symlink()
            || !entry.file_type()?.is_file()
            || !name.ends_with(".sealed")
        {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "append-intent inventory contained an unknown leaf",
            ));
        }
        let key = name.trim_end_matches(".sealed").to_owned();
        if !is_digest(&key) {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "append-intent filename was not a stable replay key",
            ));
        }
        let bytes = fs::read(entry.path())?;
        let transaction = transaction_from_append_object(&bytes, &key)?;
        let intent: AppendIntentV1 = serde_json::from_slice(&provenance::open(
            SealDomain::Journal,
            &format!("journal-outbox-append:{transaction}:{key}"),
            &bytes,
        )?)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
        if intent.protocol != OPERATION_OUTBOX_PROTOCOL
            || intent.transaction_id != transaction
            || !is_digest(&intent.transaction_id)
            || append_intent_key(&intent)? != key
            || !valid_intent_shape(&intent)
        {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "append intent was malformed or did not bind its replay key",
            ));
        }
        intents.push(AuthenticatedAppendIntent {
            key,
            journal_object_id: format!("journal:{}", intent.transaction_id),
            transaction_id: intent.transaction_id.clone(),
            intent,
        });
    }
    intents.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(intents)
}

pub(in crate::portable_runtime::journal) fn decode_target(
    path: &Path,
    intent: &AppendIntentV1,
) -> Result<JournalEntry> {
    let (_, object_id) = journal_identity(path)?;
    let plaintext = provenance::open(SealDomain::Journal, &object_id, &intent.target_line)?;
    serde_json::from_slice(&plaintext)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))
}

pub(in crate::portable_runtime::journal) fn intended_entry_is_valid(
    path: &Path,
    intent: &AppendIntentV1,
    prior: &[JournalEntry],
    prior_head: Option<&str>,
) -> Result<bool> {
    let target = decode_target(path, intent)?;
    Ok(target.transaction_id == intent.transaction_id
        && target.phase == intent.target_phase
        && entry_is_valid_after(&target, prior, prior_head, &intent.transaction_id)?)
}

pub(in crate::portable_runtime::journal) fn intent_is_exact_aborted_origin(
    path: &Path,
    intent: &AppendIntentV1,
) -> Result<bool> {
    if intent.base != absent_image() {
        return Ok(false);
    }
    let target = decode_target(path, intent)?;
    Ok(target.phase == JournalPhase::Prepared
        && matches!(target.append_kind, JournalAppendKind::Origin)
        && target.transaction_id == intent.transaction_id
        && entry_is_valid_after(&target, &[], None, &intent.transaction_id)?)
}

fn transaction_from_append_object(bytes: &[u8], key: &str) -> Result<String> {
    let envelope: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| PortableRuntimeError::new("portable_journal_outbox", error.to_string()))?;
    let object_id = envelope
        .get("object_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_journal_outbox",
                "intent envelope lacked object id",
            )
        })?;
    let suffix = format!(":{key}");
    object_id
        .strip_circumfix("journal-outbox-append:", &suffix)
        .filter(|value| is_digest(value))
        .map(str::to_owned)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_journal_outbox",
                "intent envelope object id was invalid",
            )
        })
}

fn valid_intent_shape(intent: &AppendIntentV1) -> bool {
    valid_image_shape(&intent.base)
        && !intent.target_line.is_empty()
        && intent.target_line.last().is_none_or(|byte| *byte != b'\n')
}

pub(in crate::portable_runtime::journal) fn valid_image_shape(image: &JournalImage) -> bool {
    is_digest(&image.file_sha256)
        && match (
            image.exists,
            image.byte_len,
            &image.sealed_head_sha256,
            image.last_sequence,
            image.last_phase,
        ) {
            (false, 0, None, None, None) => image.file_sha256 == sha256_hex(b""),
            (true, 0, None, None, None) => image.file_sha256 == sha256_hex(b""),
            (true, _, Some(head), Some(_), Some(_)) => is_digest(head),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::transaction_from_append_object;

    const KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TRANSACTION: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn envelope(object_id: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "object_id": object_id })).expect("envelope")
    }

    #[test]
    fn append_object_parser_rejects_wrong_prefix() {
        let error = transaction_from_append_object(
            &envelope(&format!("journal-outbox-wrong:{TRANSACTION}:{KEY}")),
            KEY,
        )
        .expect_err("wrong prefix");
        assert_eq!(
            error.to_string(),
            "portable_journal_outbox: intent envelope object id was invalid"
        );
    }

    #[test]
    fn append_object_parser_rejects_dynamic_key_mismatch() {
        let other_key = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let error = transaction_from_append_object(
            &envelope(&format!("journal-outbox-append:{TRANSACTION}:{other_key}")),
            KEY,
        )
        .expect_err("dynamic key mismatch");
        assert_eq!(
            error.to_string(),
            "portable_journal_outbox: intent envelope object id was invalid"
        );
    }

    #[test]
    fn append_object_parser_rejects_invalid_transaction_digest() {
        let error = transaction_from_append_object(
            &envelope(&format!("journal-outbox-append:not-a-digest:{KEY}")),
            KEY,
        )
        .expect_err("invalid transaction digest");
        assert_eq!(
            error.to_string(),
            "portable_journal_outbox: intent envelope object id was invalid"
        );
    }
}
