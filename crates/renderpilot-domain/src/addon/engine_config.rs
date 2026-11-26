//! Durable ownership journal for shared Unreal Engine configuration.
//!
//! The journal is intentionally a domain value rather than a filesystem
//! implementation detail.  Storage persists it as one canonical JSON token;
//! runtime publication changes it through compare-and-swap only.

use serde::{Deserialize, Serialize};

use crate::AddonKind;

/// Stable receipt for one published Engine.ini contribution set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfigReceipt {
    /// Receipt schema version.
    pub schema_version: u32,
    /// Target Engine.ini path.
    pub path: String,
    /// Whether RenderPilot created the target file.
    pub file_created: bool,
    /// Encoding name retained by the runtime publisher.
    pub encoding: String,
    /// Exact pre-image digest.
    pub before_digest: String,
    /// Exact post-image digest.
    pub after_digest: String,
    /// Deterministic typed recipe-set fingerprint.
    pub recipe_fingerprint: String,
    /// Exact owned assignment contributions.
    pub contributions: Vec<EngineConfigContribution>,
    /// Exact section headers introduced by the operation.  Kept separate
    /// from assignments so release can remove an empty owned section without
    /// touching foreign comments/whitespace.
    #[serde(default)]
    pub created_headers: Vec<Vec<u8>>,
    /// Newline bytes introduced immediately before each created header. The
    /// vector is parallel to `created_headers` and lets release restore a
    /// pre-existing file that had no final line terminator.
    #[serde(default)]
    pub created_header_prefixes: Vec<Vec<u8>>,
    /// Canonical section spelling for each created header.
    #[serde(default)]
    pub created_header_groups: Vec<String>,
    /// Stable ordinal within the created-header group.
    #[serde(default)]
    pub created_header_ordinals: Vec<u32>,
}

/// One exact assignment body owned by a stable receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfigContribution {
    /// Case-preserving section spelling.
    pub section: String,
    /// Case-preserving key spelling.
    pub key: String,
    /// Desired scalar value.
    pub value: String,
    /// Exact serialized assignment line (including terminator).
    pub line: Vec<u8>,
    /// Fixed left anchor digest at publication.
    pub left_anchor: String,
    /// Fixed right anchor digest at publication.
    pub right_anchor: String,
    /// Newline bytes introduced immediately before this assignment because
    /// the original insertion boundary was not line-terminated.
    #[serde(default)]
    pub introduced_prefix: Vec<u8>,
    /// Canonical ownership group, normally the target section spelling.
    #[serde(default)]
    pub group: String,
    /// Stable ordinal within the ownership group.
    #[serde(default)]
    pub ordinal: u32,
}

/// Before/after transition recorded before any visible filesystem mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfigTransition {
    /// Stable operation identifier.
    pub operation_id: String,
    /// Deterministic stage leaf name.
    pub stage_name: String,
    /// Stable receipt before the transition, if any.
    pub prior: Option<EngineConfigReceipt>,
    /// Receipt expected after successful publication, if any.
    pub after: Option<EngineConfigReceipt>,
    /// Exact digest expected before publication.
    pub before_digest: String,
    /// Exact digest expected after publication.
    pub after_digest: String,
}

/// Durable Engine.ini state for one installed RenoDX or Luma add-on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EngineConfigJournal {
    /// Last finalized ownership receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable: Option<EngineConfigReceipt>,
    /// In-flight transition written before the first visible mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<EngineConfigTransition>,
}

impl EngineConfigJournal {
    /// Validates the closed journal schema and add-on ownership boundary.
    pub fn validate_for_kind(&self, kind: AddonKind) -> Result<(), EngineConfigJournalError> {
        if !matches!(kind, AddonKind::RenoDx | AddonKind::Luma) {
            return Err(EngineConfigJournalError::UnsupportedAddonKind);
        }
        validate_receipt(self.stable.as_ref())?;
        if let Some(pending) = &self.pending {
            if !is_safe_operation_id(&pending.operation_id)
                || pending
                    .stage_name
                    .strip_circumfix(".renderpilot-engine-", ".stage")
                    != Some(pending.operation_id.as_str())
            {
                return Err(EngineConfigJournalError::InvalidTransition);
            }
            if pending.before_digest.len() != 64 || pending.after_digest.len() != 64 {
                return Err(EngineConfigJournalError::InvalidTransition);
            }
            validate_receipt(pending.prior.as_ref())?;
            validate_receipt(pending.after.as_ref())?;
            if pending.prior != self.stable || (pending.prior.is_none() && pending.after.is_none())
            {
                return Err(EngineConfigJournalError::InvalidTransition);
            }
        }
        Ok(())
    }

    /// Returns whether this journal has an in-flight transition.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Empty journal is represented by SQL NULL rather than a JSON object.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stable.is_none() && self.pending.is_none()
    }
}

fn validate_receipt(receipt: Option<&EngineConfigReceipt>) -> Result<(), EngineConfigJournalError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    if receipt.schema_version != 1
        || receipt.path.trim().is_empty()
        || !matches!(
            receipt.encoding.as_str(),
            "utf8" | "utf8_bom" | "utf16_le" | "utf16_be"
        )
        || !is_digest(&receipt.before_digest)
        || !is_digest(&receipt.after_digest)
        || !is_digest(&receipt.recipe_fingerprint)
    {
        return Err(EngineConfigJournalError::InvalidReceipt);
    }
    if receipt.contributions.iter().any(|entry| {
        entry.section.trim().is_empty()
            || entry.key.trim().is_empty()
            || !is_digest(&entry.left_anchor)
            || !is_digest(&entry.right_anchor)
            || entry.line.is_empty()
            || entry.group.trim().is_empty()
    }) {
        return Err(EngineConfigJournalError::InvalidReceipt);
    }
    if receipt.created_header_prefixes.len() != receipt.created_headers.len()
        || receipt.created_header_groups.len() != receipt.created_headers.len()
        || receipt.created_header_ordinals.len() != receipt.created_headers.len()
        || receipt
            .created_header_groups
            .iter()
            .any(|group| group.trim().is_empty())
    {
        return Err(EngineConfigJournalError::InvalidReceipt);
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn is_safe_operation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Closed journal invariant failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineConfigJournalError {
    /// Journal attached to a non-RenoDX/Luma add-on.
    UnsupportedAddonKind,
    /// Receipt fields are incomplete or malformed.
    InvalidReceipt,
    /// Transition fields are incomplete or malformed.
    InvalidTransition,
}

impl std::fmt::Display for EngineConfigJournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedAddonKind => {
                write!(f, "Engine.ini journal is unsupported for this add-on kind")
            }
            Self::InvalidReceipt => write!(f, "invalid Engine.ini journal receipt"),
            Self::InvalidTransition => write!(f, "invalid Engine.ini journal transition"),
        }
    }
}

impl std::error::Error for EngineConfigJournalError {}
