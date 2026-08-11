use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    publication::publish_bytes_no_replace,
    signature::sha256_hex,
    win32::file::NoReplacePublication,
};

pub const SELECTION_PROTOCOL: u16 = 3;
const LEGACY_SELECTION_PROTOCOL: u16 = 2;

/// The durable selection reducer has two distinct concepts: its validated tip
/// and the generation (if any) that is currently selected. A `Cleared` tip is
/// durable evidence that no generation is currently selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SelectionState {
    Selected { generation_sha256: String },
    Cleared,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRecordV2 {
    pub protocol: u16,
    pub sequence: u64,
    pub generation_sha256: String,
    pub previous_record_sha256: Option<String>,
    pub journal_sequence: u64,
    #[serde(default)]
    pub record_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRecordV3 {
    pub protocol: u16,
    pub sequence: u64,
    pub state: SelectionState,
    pub previous_record_sha256: Option<String>,
    /// The one normal journal transaction whose SelectionCommitted slot this
    /// selection reserves or whose failed selection it compensates.
    pub journal_transaction_id: String,
    pub journal_sequence: u64,
    /// A recovery compensation can only immediately follow the failed tip it
    /// closes. This makes a retry prove that it is reusing its own durable work.
    #[serde(default)]
    pub compensates_selection_record_sha256: Option<String>,
    #[serde(default)]
    pub record_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionRecord {
    V2(SelectionRecordV2),
    V3(SelectionRecordV3),
}

impl SelectionRecord {
    fn sequence(&self) -> u64 {
        match self {
            Self::V2(record) => record.sequence,
            Self::V3(record) => record.sequence,
        }
    }

    fn previous_record_sha256(&self) -> Option<&str> {
        match self {
            Self::V2(record) => record.previous_record_sha256.as_deref(),
            Self::V3(record) => record.previous_record_sha256.as_deref(),
        }
    }

    fn selected_generation_sha256(&self) -> Option<&str> {
        match self {
            Self::V2(record) => Some(&record.generation_sha256),
            Self::V3(SelectionRecordV3 {
                state: SelectionState::Selected { generation_sha256 },
                ..
            }) => Some(generation_sha256),
            Self::V3(SelectionRecordV3 {
                state: SelectionState::Cleared,
                ..
            }) => None,
        }
    }

    fn v3(&self) -> Option<&SelectionRecordV3> {
        match self {
            Self::V2(_) => None,
            Self::V3(record) => Some(record),
        }
    }

    fn self_hash_is_valid(&self) -> Result<bool> {
        match self {
            Self::V2(record) => {
                let mut unsigned = record.clone();
                let digest = std::mem::take(&mut unsigned.record_sha256);
                Ok(digest
                    == sha256_hex(&serde_json::to_vec(&unsigned).map_err(|error| {
                        PortableRuntimeError::new("portable_selection_invalid", error.to_string())
                    })?))
            }
            Self::V3(record) => {
                let mut unsigned = record.clone();
                let digest = std::mem::take(&mut unsigned.record_sha256);
                Ok(digest
                    == sha256_hex(&serde_json::to_vec(&unsigned).map_err(|error| {
                        PortableRuntimeError::new("portable_selection_invalid", error.to_string())
                    })?))
            }
        }
    }

    fn fields_are_valid(&self) -> Result<bool> {
        let valid = match self {
            Self::V2(record) => {
                record.protocol == LEGACY_SELECTION_PROTOCOL
                    && is_hash(&record.generation_sha256)
                    && record.journal_sequence > 0
            }
            Self::V3(record) => {
                record.protocol == SELECTION_PROTOCOL
                    && is_hash(&record.journal_transaction_id)
                    && record.journal_sequence > 0
                    && record
                        .compensates_selection_record_sha256
                        .as_deref()
                        .is_none_or(is_hash)
                    && match &record.state {
                        SelectionState::Selected { generation_sha256 } => {
                            is_hash(generation_sha256)
                        }
                        SelectionState::Cleared => {
                            record.compensates_selection_record_sha256.is_some()
                        }
                    }
            }
        };
        Ok(valid && self.self_hash_is_valid()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionTip {
    pub record: SelectionRecord,
    pub record_sha256: String,
}

impl SelectionTip {
    pub fn selected_generation_sha256(&self) -> Option<&str> {
        self.record.selected_generation_sha256()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentSelection {
    pub generation_sha256: String,
    pub record_sha256: String,
}

pub fn selection_root(generation_store_root: &Path) -> PathBuf {
    generation_store_root.join("selection")
}

pub fn append_selected(
    root: &Path,
    generation_sha256: &str,
    journal_transaction_id: &str,
    journal_sequence: u64,
) -> Result<(PathBuf, String)> {
    append_record(
        root,
        SelectionState::Selected {
            generation_sha256: generation_sha256.to_owned(),
        },
        journal_transaction_id,
        journal_sequence,
        None,
    )
}

pub fn append_compensating_selected(
    root: &Path,
    generation_sha256: &str,
    journal_transaction_id: &str,
    journal_sequence: u64,
    compensates_selection_record_sha256: &str,
) -> Result<(PathBuf, String)> {
    append_record(
        root,
        SelectionState::Selected {
            generation_sha256: generation_sha256.to_owned(),
        },
        journal_transaction_id,
        journal_sequence,
        Some(compensates_selection_record_sha256.to_owned()),
    )
}

pub fn append_cleared(
    root: &Path,
    journal_transaction_id: &str,
    journal_sequence: u64,
    compensates_selection_record_sha256: &str,
) -> Result<(PathBuf, String)> {
    append_record(
        root,
        SelectionState::Cleared,
        journal_transaction_id,
        journal_sequence,
        Some(compensates_selection_record_sha256.to_owned()),
    )
}

fn append_record(
    root: &Path,
    state: SelectionState,
    journal_transaction_id: &str,
    journal_sequence: u64,
    compensates_selection_record_sha256: Option<String>,
) -> Result<(PathBuf, String)> {
    if !is_hash(journal_transaction_id)
        || journal_sequence == 0
        || compensates_selection_record_sha256
            .as_deref()
            .is_some_and(|hash| !is_hash(hash))
        || matches!(&state, SelectionState::Selected { generation_sha256 } if !is_hash(generation_sha256))
    {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "selection protocol, journal binding, or generation hash was invalid",
        ));
    }
    let existing = read_selection(root)?;
    let previous_record_sha256 = existing.last().map(|tip| tip.record_sha256.clone());
    if compensates_selection_record_sha256.is_some()
        && compensates_selection_record_sha256.as_deref() != previous_record_sha256.as_deref()
    {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "selection compensation did not immediately follow its failed tip",
        ));
    }
    let mut record = SelectionRecordV3 {
        protocol: SELECTION_PROTOCOL,
        sequence: existing.len() as u64 + 1,
        state,
        previous_record_sha256,
        journal_transaction_id: journal_transaction_id.to_owned(),
        journal_sequence,
        compensates_selection_record_sha256,
        record_sha256: String::new(),
    };
    record.record_sha256 = sha256_hex(&serde_json::to_vec(&record).map_err(|error| {
        PortableRuntimeError::new("portable_selection_encode", error.to_string())
    })?);
    let plaintext = serde_json::to_vec(&record).map_err(|error| {
        PortableRuntimeError::new("portable_selection_encode", error.to_string())
    })?;
    let hash = sha256_hex(&plaintext);
    let object_id = format!("selection:{hash}");
    let bytes = provenance::seal(SealDomain::Selection, &object_id, &plaintext)?;
    let path = root.join(format!("{hash}.json"));
    let pending_root = root
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_selection_path", "selection root had no parent")
        })?
        .join("selection-pending");
    match publish_bytes_no_replace(&path, &pending_root, &bytes)? {
        NoReplacePublication::Published => {
            provenance::observe(SealDomain::Selection, &object_id, &bytes)?;
        }
        NoReplacePublication::Occupied if std::fs::read(&path)? == bytes => {}
        NoReplacePublication::Occupied => {
            return Err(PortableRuntimeError::new(
                "portable_selection_invalid",
                "occupied selection record did not contain the expected bytes",
            ));
        }
    }
    if validated_selection_tip(root)?
        .as_ref()
        .map(|tip| &tip.record_sha256)
        != Some(&hash)
    {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "published selection record did not become the canonical tip",
        ));
    }
    Ok((path, hash))
}

pub fn read_selection(root: &Path) -> Result<Vec<SelectionTip>> {
    let mut records = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(records),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection namespace contained a non-file",
            ));
        }
        let name = entry.file_name().into_string().map_err(|_| {
            PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection name was not Unicode",
            )
        })?;
        let hash = name.strip_suffix(".json").ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection file name was invalid",
            )
        })?;
        if !is_hash(hash) {
            return Err(PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection file identity was not a canonical digest",
            ));
        }
        let bytes = provenance::open(
            SealDomain::Selection,
            &format!("selection:{hash}"),
            &std::fs::read(entry.path())?,
        )?;
        if sha256_hex(&bytes) != hash {
            return Err(PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection file identity did not match sealed payload",
            ));
        }
        let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            PortableRuntimeError::new("portable_selection_invalid", error.to_string())
        })?;
        let protocol = raw
            .get("protocol")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                PortableRuntimeError::new(
                    "portable_selection_invalid",
                    "selection protocol was absent",
                )
            })?;
        let record = match protocol {
            2 => SelectionRecord::V2(serde_json::from_value(raw).map_err(|error| {
                PortableRuntimeError::new("portable_selection_invalid", error.to_string())
            })?),
            3 => SelectionRecord::V3(serde_json::from_value(raw).map_err(|error| {
                PortableRuntimeError::new("portable_selection_invalid", error.to_string())
            })?),
            _ => {
                return Err(PortableRuntimeError::new(
                    "portable_selection_invalid",
                    "selection forest was corrupt or used a future protocol",
                ));
            }
        };
        records.push(SelectionTip {
            record,
            record_sha256: hash.to_owned(),
        });
    }
    records.sort_by_key(|tip| tip.record.sequence());
    let mut previous = None;
    for (index, tip) in records.iter().enumerate() {
        let v3_compensation_is_linked = tip.record.v3().is_none_or(|record| {
            record.compensates_selection_record_sha256.is_none()
                || record.compensates_selection_record_sha256.as_deref()
                    == tip.record.previous_record_sha256()
        });
        if tip.record.sequence() != index as u64 + 1
            || tip.record.previous_record_sha256() != previous.as_deref()
            || !tip.record.fields_are_valid()?
            || !v3_compensation_is_linked
        {
            return Err(PortableRuntimeError::new(
                "portable_selection_invalid",
                "selection forest was corrupt or used a future protocol",
            ));
        }
        previous = Some(tip.record_sha256.clone());
    }
    Ok(records)
}

pub fn validated_selection_tip(root: &Path) -> Result<Option<SelectionTip>> {
    Ok(read_selection(root)?.pop())
}

/// Requires the current reducer tip to be the normal v3 selection committed by
/// one exact journal transaction. Recovery uses this before it records an
/// observed commit or its final receipt, so an older/reused selection cannot
/// authorize a different transaction.
pub fn require_canonical_normal_selection(
    root: &Path,
    transaction_id: &str,
    journal_sequence: u64,
    generation_sha256: &str,
    record_sha256: &str,
) -> Result<()> {
    let tip = validated_selection_tip(root)?.ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_selection_invalid",
            "journal-bound normal selection was absent from the canonical tip",
        )
    })?;
    let SelectionRecord::V3(record) = &tip.record else {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "canonical selection was not a normal v3 record",
        ));
    };
    if tip.record_sha256 != record_sha256
        || record.journal_transaction_id != transaction_id
        || record.journal_sequence != journal_sequence
        || record.compensates_selection_record_sha256.is_some()
        || record.state
            != (SelectionState::Selected {
                generation_sha256: generation_sha256.to_owned(),
            })
    {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "canonical selection did not exactly bind the normal journal transaction",
        ));
    }
    Ok(())
}

pub fn current_selection(root: &Path) -> Result<Option<CurrentSelection>> {
    Ok(validated_selection_tip(root)?.and_then(|tip| {
        let generation_sha256 = tip.selected_generation_sha256()?.to_owned();
        Some(CurrentSelection {
            generation_sha256,
            record_sha256: tip.record_sha256,
        })
    }))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}
