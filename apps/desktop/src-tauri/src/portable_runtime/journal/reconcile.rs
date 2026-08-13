use std::{collections::BTreeMap, path::Path};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

use super::{
    image::{CapturedJournalImage, capture_journal_image},
    outbox::{self, AppendIntentV1, AuthenticatedAppendIntent, ObservationOutcome},
    paths::{journal_identity, journal_path},
    protocol::JournalEntry,
    reader::read_valid_prefix_bytes,
};

/// Reconciles each authenticated transaction/object bucket before a new append
/// or recovery. Prefix matching starts only after the bucket is fixed; a
/// second same-transaction candidate remains a fail-closed ambiguity.
pub(super) fn reconcile_operation_outbox(
    generation_store_root: &Path,
    update_root: &Path,
) -> Result<()> {
    let mut buckets = BTreeMap::<(String, String), Vec<AuthenticatedAppendIntent>>::new();
    for intent in outbox::read_append_intents(generation_store_root)? {
        buckets
            .entry((
                intent.transaction_id.clone(),
                intent.journal_object_id.clone(),
            ))
            .or_default()
            .push(intent);
    }
    for ((transaction_id, journal_object_id), intents) in buckets {
        let path = journal_path(update_root, &transaction_id);
        let captured = capture_journal_image(&path)?;
        if captured.transaction_id() != transaction_id
            || captured.journal_object_id() != journal_object_id
        {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "authenticated append intent did not target the canonical journal object",
            ));
        }
        if captured.is_valid() {
            reconcile_valid_bucket(generation_store_root, &path, &intents, &captured)?;
        } else {
            reconcile_torn_prefix(generation_store_root, &path, &intents, &captured)?;
            let repaired = capture_journal_image(&path)?;
            reconcile_valid_bucket(generation_store_root, &path, &intents, &repaired)?;
        }
    }
    Ok(())
}

fn reconcile_valid_bucket(
    generation_store_root: &Path,
    path: &Path,
    intents: &[AuthenticatedAppendIntent],
    captured: &CapturedJournalImage,
) -> Result<()> {
    for authenticated in intents {
        let intent = &authenticated.intent;
        if captured.matches_base(&intent.base) || is_empty_unstarted_origin(captured, path, intent)?
        {
            complete_repair_observations(
                generation_store_root,
                path,
                &authenticated.key,
                intent,
                captured,
            )?;
        } else if outbox::matches_committed_target(captured, intent)? {
            outbox::observe_exact_current(
                generation_store_root,
                path,
                &intent.transaction_id,
                &authenticated.key,
                captured,
                ObservationOutcome::Committed,
            )?;
        } else {
            outbox::observe_exact_current(
                generation_store_root,
                path,
                &intent.transaction_id,
                &authenticated.key,
                captured,
                ObservationOutcome::SupersededByAuthoritativeJournal,
            )?;
        }
    }
    Ok(())
}

fn reconcile_torn_prefix(
    generation_store_root: &Path,
    path: &Path,
    intents: &[AuthenticatedAppendIntent],
    captured: &CapturedJournalImage,
) -> Result<()> {
    let candidates = intents
        .iter()
        .filter_map(|authenticated| {
            match outbox::target_prefix_len(captured, &authenticated.intent) {
                Ok(Some(_)) => Some(Ok(authenticated)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let [candidate] = candidates.as_slice() else {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "partial journal suffix was invalid or ambiguous inside its transaction bucket",
        ));
    };
    if !preflight_intended_transition(path, &candidate.intent, captured)? {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "partial journal suffix was not a legal intended transition",
        ));
    }
    let repair = outbox::record_repair_intent(
        generation_store_root,
        &candidate.key,
        &candidate.intent,
        captured,
    )?;
    let repaired = outbox::truncate_exact_tail(path, &repair, captured)?;
    if !repaired.matches_base(&candidate.intent.base)
        && !is_empty_unstarted_origin(&repaired, path, &candidate.intent)?
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "tail repair did not restore the immutable append base image",
        ));
    }
    complete_repair_observations(
        generation_store_root,
        path,
        &candidate.key,
        &candidate.intent,
        &repaired,
    )
}

/// Finishes retained repair evidence in one order for both the immediate
/// truncate-sync boundary and later valid-base retries: TailRemoved, then the
/// distinct post-repair outcome, then the eligible aborted-origin bridge.
fn complete_repair_observations(
    generation_store_root: &Path,
    path: &Path,
    intent_key: &str,
    intent: &AppendIntentV1,
    captured: &CapturedJournalImage,
) -> Result<()> {
    if outbox::retained_repair_matches(generation_store_root, intent_key, intent)? {
        outbox::observe_exact_current(
            generation_store_root,
            path,
            &intent.transaction_id,
            &outbox::repair_intent_key(intent_key),
            captured,
            ObservationOutcome::TailRemoved,
        )?;
    }
    observe_uncommitted_base(generation_store_root, path, intent_key, intent, captured)
}

fn preflight_intended_transition(
    path: &Path,
    intent: &AppendIntentV1,
    captured: &CapturedJournalImage,
) -> Result<bool> {
    let base_len = usize::try_from(intent.base.byte_len).map_err(|_| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal length did not fit usize",
        )
    })?;
    let Some(base_bytes) = captured.bytes().get(..base_len) else {
        return Ok(false);
    };
    let prefix = read_valid_prefix_bytes(path, base_bytes)?;
    if !base_image_matches_prefix(
        intent,
        base_bytes,
        &prefix.entries,
        prefix.head_sha256.as_deref(),
    ) {
        return Ok(false);
    }
    outbox::intended_entry_is_valid(path, intent, &prefix.entries, prefix.head_sha256.as_deref())
}

fn base_image_matches_prefix(
    intent: &AppendIntentV1,
    bytes: &[u8],
    entries: &[JournalEntry],
    head: Option<&str>,
) -> bool {
    intent.base.byte_len == bytes.len() as u64
        && intent.base.file_sha256 == sha256_hex(bytes)
        && intent.base.sealed_head_sha256.as_deref() == head
        && intent.base.last_sequence == entries.last().map(|entry| entry.sequence)
        && intent.base.last_phase == entries.last().map(|entry| entry.phase)
}

fn is_empty_unstarted_origin(
    captured: &CapturedJournalImage,
    path: &Path,
    intent: &AppendIntentV1,
) -> Result<bool> {
    let image = captured.image();
    Ok(captured.is_valid()
        && image.exists
        && image.byte_len == 0
        && image.sealed_head_sha256.is_none()
        && image.last_sequence.is_none()
        && image.last_phase.is_none()
        && outbox::intent_is_exact_aborted_origin(path, intent)?)
}

fn observe_uncommitted_base(
    generation_store_root: &Path,
    path: &Path,
    intent_key: &str,
    intent: &AppendIntentV1,
    captured: &CapturedJournalImage,
) -> Result<()> {
    let repaired = outbox::retained_repair_matches(generation_store_root, intent_key, intent)?;
    let aborted_origin = intent_is_aborted_origin_image(captured, path, intent)?;
    if repaired || !aborted_origin {
        let outcome = if repaired {
            ObservationOutcome::NotCommittedAfterTailRepair
        } else {
            ObservationOutcome::NotCommittedBeforeMutation
        };
        let replay_subject = if repaired {
            outbox::tail_repair_replay_subject(intent_key)
        } else {
            intent_key.to_owned()
        };
        outbox::observe_exact_current(
            generation_store_root,
            path,
            &intent.transaction_id,
            &replay_subject,
            captured,
            outcome,
        )?;
    }
    if aborted_origin {
        outbox::observe_exact_current(
            generation_store_root,
            path,
            &intent.transaction_id,
            intent_key,
            captured,
            ObservationOutcome::AbortedBeforeOrigin,
        )?;
    }
    Ok(())
}

fn intent_is_aborted_origin_image(
    captured: &CapturedJournalImage,
    path: &Path,
    intent: &AppendIntentV1,
) -> Result<bool> {
    let image = captured.image();
    Ok(captured.is_valid()
        && image.byte_len == 0
        && image.sealed_head_sha256.is_none()
        && image.last_sequence.is_none()
        && image.last_phase.is_none()
        && outbox::intent_is_exact_aborted_origin(path, intent)?)
}

/// The recovery bridge accepts no inferred empty journal. It reads one exact,
/// sealed AbortedBeforeOrigin observation for one exact Origin->Prepared intent.
pub(super) fn aborted_before_origin(generation_store_root: &Path, journal: &Path) -> Result<bool> {
    let (transaction_id, _) = journal_identity(journal)?;
    let captured = capture_journal_image(journal)?;
    let image = captured.image();
    if !captured.is_valid()
        || image.byte_len != 0
        || image.sealed_head_sha256.is_some()
        || image.last_sequence.is_some()
        || image.last_phase.is_some()
    {
        return Ok(false);
    }
    let mut matches = 0;
    for authenticated in outbox::read_append_intents(generation_store_root)? {
        let intent = &authenticated.intent;
        if intent.transaction_id != transaction_id
            || !outbox::intent_is_exact_aborted_origin(journal, intent)?
        {
            continue;
        }
        if outbox::observation_matches(
            generation_store_root,
            &transaction_id,
            &outbox::observation_key(
                &authenticated.key,
                ObservationOutcome::AbortedBeforeOrigin,
                image,
            ),
            image.clone(),
            ObservationOutcome::AbortedBeforeOrigin,
        )? {
            matches += 1;
        }
    }
    if matches > 1 {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "multiple aborted-origin observations matched one transaction",
        ));
    }
    Ok(matches == 1)
}
