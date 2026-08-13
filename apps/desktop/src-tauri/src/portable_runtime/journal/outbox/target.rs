use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

use super::append_intent::AppendIntentV1;
use crate::portable_runtime::journal::image::CapturedJournalImage;

/// Purely classifies one captured source image against an immutable intent.
/// It never reads a path, rejects the base alone, and rejects overlong suffixes.
pub(in crate::portable_runtime::journal) fn target_prefix_len(
    captured: &CapturedJournalImage,
    intent: &AppendIntentV1,
) -> Result<Option<usize>> {
    let base_len = usize::try_from(intent.base.byte_len).map_err(|_| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal length did not fit usize",
        )
    })?;
    Ok(target_prefix_len_bytes(
        captured.bytes(),
        base_len,
        &intent.base.file_sha256,
        &intent.target_line,
    ))
}

/// Pure committed-image proof from one capture. Bytes and semantic tail must
/// agree with the immutable base and one exact target line.
pub(in crate::portable_runtime::journal) fn matches_committed_target(
    captured: &CapturedJournalImage,
    intent: &AppendIntentV1,
) -> Result<bool> {
    if !captured.is_valid() {
        return Ok(false);
    }
    let base_len = usize::try_from(intent.base.byte_len).map_err(|_| {
        PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal length did not fit usize",
        )
    })?;
    let expected_len = base_len
        .checked_add(intent.target_line.len())
        .and_then(|length| length.checked_add(1))
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_journal_outbox", "journal length overflow")
        })?;
    if captured.bytes().len() != expected_len
        || captured
            .bytes()
            .get(..base_len)
            .is_none_or(|base| sha256_hex(base) != intent.base.file_sha256)
        || captured
            .bytes()
            .get(base_len..)
            .is_none_or(|suffix| suffix != [intent.target_line.as_slice(), b"\n"].concat())
    {
        return Ok(false);
    }
    let expected_sequence = intent
        .base
        .last_sequence
        .unwrap_or_default()
        .checked_add(1)
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_journal_outbox", "journal sequence overflow")
        })?;
    let prefix = captured.valid_prefix()?;
    Ok(captured.image().last_sequence == Some(expected_sequence)
        && captured.image().last_phase == Some(intent.target_phase)
        && prefix.entries.last().is_some_and(|entry| {
            entry.transaction_id == intent.transaction_id && entry.phase == intent.target_phase
        }))
}

/// Stronger append proof: B must be the exact byte extension of A as well as
/// a semantically valid committed target. Neither comparison reads a path.
pub(in crate::portable_runtime::journal) fn matches_exact_committed_target(
    captured: &CapturedJournalImage,
    base: &CapturedJournalImage,
    intent: &AppendIntentV1,
) -> Result<bool> {
    if !base.matches_base(&intent.base) {
        return Ok(false);
    }
    let base_len = base.bytes().len();
    let expected_len = base_len
        .checked_add(intent.target_line.len())
        .and_then(|length| length.checked_add(1))
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_journal_outbox", "journal length overflow")
        })?;
    Ok(captured.bytes().len() == expected_len
        && captured.bytes().get(..base_len) == Some(base.bytes())
        && captured
            .bytes()
            .get(base_len..)
            .is_some_and(|suffix| suffix == [intent.target_line.as_slice(), b"\n"].concat())
        && matches_committed_target(captured, intent)?)
}

#[cfg(test)]
pub(in crate::portable_runtime::journal) fn target_prefix_len_for_test(
    base: &[u8],
    target_line: &[u8],
    candidate: &[u8],
) -> Option<usize> {
    target_prefix_len_bytes(candidate, base.len(), &sha256_hex(base), target_line)
}

fn target_prefix_len_bytes(
    bytes: &[u8],
    base_len: usize,
    base_sha256: &str,
    target_line: &[u8],
) -> Option<usize> {
    if bytes.len() <= base_len
        || bytes
            .get(..base_len)
            .is_none_or(|base| sha256_hex(base) != base_sha256)
    {
        return None;
    }
    let suffix = &bytes[base_len..];
    (suffix.len() <= target_line.len() && target_line.starts_with(suffix)).then_some(suffix.len())
}
