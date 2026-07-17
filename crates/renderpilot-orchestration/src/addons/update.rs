//! The tool-agnostic update-verdict vocabulary shared by every add-on.
//!
//! "Is there an upstream update?" is answered the same way for every tool: a cheap
//! `HEAD`/ETag pre-check that can only ever conclude *current* (a rotated validator
//! never implies a content change), falling back to an authoritative SHA-256 digest
//! compare; per-source verdicts are then combined. This module owns that vocabulary
//! and the combine rule; the tool layer supplies which sources to check and the
//! (tool-specific) re-fetch that produces the digest.

use serde::Serialize;

/// Whether a tracked source (or a whole install) has an upstream update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    /// Current — matches the recorded identity.
    Current,
    /// A newer version is available upstream.
    Available,
    /// Could not determine (network failure, or no recorded source).
    Unknown,
    /// The content matches a different known channel than the selected
    /// one — a channel mismatch, not an update.
    ChannelMismatch,
    /// The backend needs stronger validation before it can claim current/update
    /// (e.g. a nightly host whose only signal is a PE-version match).
    UnknownNeedsValidation,
}

/// The cheap `HEAD` fast-path: only a *present and matching* validator is conclusive
/// (the source is [`UpdateStatus::Current`]). A changed or absent validator returns
/// `None` — the caller must do the digest compare, since an ETag rotation alone does
/// not imply a content change.
#[must_use]
pub fn validator_fast_path(stored: Option<&str>, current: Option<&str>) -> Option<UpdateStatus> {
    match (stored, current) {
        (Some(stored), Some(current)) if stored == current => Some(UpdateStatus::Current),
        _ => None,
    }
}

/// The authoritative verdict: equal content digests ⇒ current, else an update.
#[must_use]
pub fn digest_verdict(stored_digest: &str, fetched_digest: &str) -> UpdateStatus {
    if stored_digest == fetched_digest {
        UpdateStatus::Current
    } else {
        UpdateStatus::Available
    }
}

/// Combines two per-source verdicts by priority: an available update wins; then a
/// channel mismatch; then needs-validation; then unknown; current only when both
/// are current.
#[must_use]
pub fn combine(a: UpdateStatus, b: UpdateStatus) -> UpdateStatus {
    use UpdateStatus as U;
    match (a, b) {
        (U::Available, _) | (_, U::Available) => U::Available,
        (U::ChannelMismatch, _) | (_, U::ChannelMismatch) => U::ChannelMismatch,
        (U::UnknownNeedsValidation, _) | (_, U::UnknownNeedsValidation) => {
            U::UnknownNeedsValidation
        }
        (U::Current, U::Current) => U::Current,
        _ => U::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_validator_is_current() {
        assert_eq!(
            validator_fast_path(Some("\"v1\""), Some("\"v1\"")),
            Some(UpdateStatus::Current)
        );
    }

    #[test]
    fn changed_validator_defers_to_digest() {
        // The headline fix: a rotated ETag must NOT report "available"; the fast
        // path declines so the authoritative digest compare decides.
        assert_eq!(validator_fast_path(Some("\"v1\""), Some("\"v2\"")), None);
    }

    #[test]
    fn absent_validator_defers_to_digest() {
        assert_eq!(validator_fast_path(None, Some("\"v1\"")), None);
        assert_eq!(validator_fast_path(Some("\"v1\""), None), None);
        assert_eq!(validator_fast_path(None, None), None);
    }

    #[test]
    fn digest_decides_current_vs_available() {
        assert_eq!(digest_verdict("abc", "abc"), UpdateStatus::Current);
        assert_eq!(digest_verdict("abc", "def"), UpdateStatus::Available);
    }

    #[test]
    fn rotated_etag_with_unchanged_content_stays_current() {
        // Models GitHub Pages rotating the ETag without a content change: the
        // validator differs (→ digest compare), and the digest is identical.
        assert_eq!(validator_fast_path(Some("\"old\""), Some("\"new\"")), None);
        assert_eq!(
            digest_verdict("same-digest", "same-digest"),
            UpdateStatus::Current
        );
    }

    #[test]
    fn combine_reports_available_when_either_part_changed() {
        use UpdateStatus::*;
        // A changed host with a current add-on → available.
        assert_eq!(combine(Current, Available), Available);
        assert_eq!(combine(Available, Current), Available);
        // Both current → current.
        assert_eq!(combine(Current, Current), Current);
        // A failed check with nothing known-available is unknown, not a false update.
        assert_eq!(combine(Current, Unknown), Unknown);
        assert_eq!(combine(Unknown, Current), Unknown);
        // Availability wins even over an unknown.
        assert_eq!(combine(Available, Unknown), Available);
    }

    #[test]
    fn combine_priority_available_beats_channel_mismatch() {
        use UpdateStatus::*;
        assert_eq!(combine(Available, ChannelMismatch), Available);
        assert_eq!(combine(ChannelMismatch, Available), Available);
    }

    #[test]
    fn combine_channel_mismatch_beats_unknown_needs_validation() {
        use UpdateStatus::*;
        assert_eq!(
            combine(ChannelMismatch, UnknownNeedsValidation),
            ChannelMismatch
        );
        assert_eq!(
            combine(UnknownNeedsValidation, ChannelMismatch),
            ChannelMismatch
        );
        assert_eq!(combine(ChannelMismatch, Current), ChannelMismatch);
    }

    #[test]
    fn combine_unknown_needs_validation_beats_unknown() {
        use UpdateStatus::*;
        assert_eq!(
            combine(UnknownNeedsValidation, Unknown),
            UnknownNeedsValidation
        );
        assert_eq!(
            combine(Unknown, UnknownNeedsValidation),
            UnknownNeedsValidation
        );
        assert_eq!(
            combine(UnknownNeedsValidation, Current),
            UnknownNeedsValidation
        );
    }
}
