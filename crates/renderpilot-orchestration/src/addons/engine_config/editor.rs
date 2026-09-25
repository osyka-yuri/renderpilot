use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::document::*;
use super::{EngineIniEntry, EngineIniRecipeSet, IniEncoding, ascii_key};

/// A receipt for exact contributions made by one successful apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineIniReceipt {
    /// Receipt format version.
    pub schema_version: u32,
    /// Exact target path represented as a UTF-8 path string.
    pub path: String,
    /// Whether the file was created by RenderPilot.
    pub file_created: bool,
    /// Encoding observed before the apply.
    pub encoding: IniEncoding,
    /// Digest of the exact pre-image.
    pub before_digest: String,
    /// Digest of the exact post-image.
    pub after_digest: String,
    /// Deterministic recipe set identity.
    pub recipe_fingerprint: String,
    /// Exact inserted assignment bodies.  Foreign comments are never stored.
    pub contributions: Vec<EngineIniContribution>,
    /// Exact section headers introduced into a pre-existing file or the new
    /// file's complete header set.
    #[serde(default)]
    pub created_headers: Vec<Vec<u8>>,
    /// Newline bytes introduced immediately before each created header. This
    /// vector is parallel to `created_headers`.
    #[serde(default)]
    pub created_header_prefixes: Vec<Vec<u8>>,
    /// Canonical section spelling for each created header.
    #[serde(default)]
    pub created_header_groups: Vec<String>,
    /// Stable ordinal within the created-header group.
    #[serde(default)]
    pub created_header_ordinals: Vec<u32>,
}

/// One exact assignment body owned by a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineIniContribution {
    /// Case-preserving target section.
    pub section: String,
    /// Case-preserving target key.
    pub key: String,
    /// Desired value.
    pub value: String,
    /// Exact bytes inserted for the assignment, including its line terminator.
    pub line: Vec<u8>,
    /// Hash of the bytes immediately before this contribution at insertion.
    pub left_anchor: String,
    /// Hash of the bytes immediately after this contribution at insertion.
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

/// Result of applying or releasing a typed recipe set in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIniEdit {
    /// New bytes to publish.
    pub bytes: Vec<u8>,
    /// Durable receipt for the contribution set.
    pub receipt: Option<EngineIniReceipt>,
}

/// Result of proving and removing a receipt from the current bytes.
///
/// `complete` is false when one or more owned contributions could not be
/// proven (for example a changed value, duplicate, or lost outer anchor). The
/// returned bytes still remove every contribution that was uniquely provable;
/// callers must then clear ownership for the uncertain remainder rather than
/// keeping a journal that could block uninstall forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIniRelease {
    /// Current bytes after uniquely provable owned spans were removed.
    pub bytes: Vec<u8>,
    /// Whether every receipt contribution/header was proven and removed.
    pub complete: bool,
}

/// Result of reconciling a durable pending transition with the on-disk digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineIniRecovery {
    /// Disk is exactly the expected post-image; promote the pending receipt.
    PromoteAfter,
    /// Disk is exactly the pre-image; abandon the pending transition.
    CancelPending,
    /// Neither image is present; leave the pending journal for manual repair.
    RecoveryRequired,
}

/// Classifies a pending transition without changing the filesystem.
#[must_use]
pub fn reconcile_pending_transition(
    before_digest: &str,
    after_digest: &str,
    pending: Option<&EngineIniReceipt>,
    disk: Option<&[u8]>,
) -> EngineIniRecovery {
    let Some(bytes) = disk else {
        return EngineIniRecovery::RecoveryRequired;
    };
    let observed = digest(bytes);
    if observed == after_digest
        && pending.is_none_or(|receipt| receipt.after_digest == after_digest)
    {
        EngineIniRecovery::PromoteAfter
    } else if observed == before_digest
        && pending.is_none_or(|receipt| receipt.before_digest == before_digest)
    {
        EngineIniRecovery::CancelPending
    } else {
        EngineIniRecovery::RecoveryRequired
    }
}

/// Fail-closed editor error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineIniError {
    /// Invalid recipe input.
    InvalidRecipe(String),
    /// Two typed recipes disagree on one setting.
    RecipeConflict {
        /// Conflicting section name.
        section: String,
        /// Conflicting key name.
        key: String,
    },
    /// Input encoding is unsupported or malformed.
    InvalidEncoding(String),
    /// Target section/key structure is ambiguous.
    AmbiguousTarget(String),
    /// A value already exists with different semantics.
    ExistingValueConflict {
        /// Existing section name.
        section: String,
        /// Existing key name.
        key: String,
    },
    /// Receipt cannot prove unique ownership after foreign edits.
    OwnershipUnclear(String),
}

impl fmt::Display for EngineIniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRecipe(message) => write!(f, "invalid Engine.ini recipe: {message}"),
            Self::RecipeConflict { section, key } => {
                write!(f, "conflicting Engine.ini recipes for [{section}] {key}")
            }
            Self::InvalidEncoding(message) => write!(f, "invalid Engine.ini encoding: {message}"),
            Self::AmbiguousTarget(message) => write!(f, "ambiguous Engine.ini target: {message}"),
            Self::ExistingValueConflict { section, key } => {
                write!(
                    f,
                    "existing Engine.ini value conflicts at [{section}] {key}"
                )
            }
            Self::OwnershipUnclear(message) => {
                write!(f, "Engine.ini ownership is unclear: {message}")
            }
        }
    }
}

impl std::error::Error for EngineIniError {}

/// Applies a recipe set to an existing or absent Engine.ini byte image.
pub fn apply_engine_ini(
    path: &Path,
    before: Option<&[u8]>,
    recipes: &EngineIniRecipeSet,
) -> Result<EngineIniEdit, EngineIniError> {
    let (encoding, document) = match before {
        Some(bytes) => decode_document(bytes)?,
        None => (IniEncoding::Utf8, IniDocument::empty()),
    };
    let sections = document.sections()?;
    let missing = validate_and_collect_missing(&sections, recipes)?;
    if missing.is_empty() {
        let bytes = before.map_or_else(|| encode_document(&document), ToOwned::to_owned);
        return Ok(EngineIniEdit {
            bytes,
            receipt: None,
        });
    }

    let layout = match before {
        None => render_new_with_headers(&missing, encoding),
        Some(existing_bytes) => {
            splice_existing_document(existing_bytes, &missing, &sections, encoding)?
        }
    };

    let after_digest = digest(&layout.bytes);
    let before_digest = before.map_or_else(|| digest(&[]), digest);
    let contribution_newline = if before.is_none() {
        "\r\n"
    } else {
        sections.preferred_newline()
    };
    let contributions = collect_contributions(
        &layout.bytes,
        &missing,
        layout.introduced_prefixes,
        contribution_newline,
        encoding,
    )?;

    Ok(EngineIniEdit {
        bytes: layout.bytes,
        receipt: Some(EngineIniReceipt {
            schema_version: 1,
            path: path.to_string_lossy().into_owned(),
            file_created: before.is_none(),
            encoding,
            before_digest,
            after_digest,
            recipe_fingerprint: recipes.fingerprint().to_owned(),
            contributions,
            created_headers: layout.created_headers,
            created_header_prefixes: layout.created_header_prefixes,
            created_header_groups: layout.created_header_groups,
            created_header_ordinals: layout.created_header_ordinals,
        }),
    })
}

fn validate_and_collect_missing(
    sections: &IniSections<'_>,
    recipes: &EngineIniRecipeSet,
) -> Result<Vec<EngineIniEntry>, EngineIniError> {
    let mut missing = Vec::new();
    for entry in recipes.entries() {
        match sections.find_value(&entry.section, &entry.key, &entry.value) {
            FindValue::Exact => {}
            FindValue::Missing => missing.push(entry.clone()),
            FindValue::Conflict => {
                return Err(EngineIniError::ExistingValueConflict {
                    section: entry.section.clone(),
                    key: entry.key.clone(),
                });
            }
            FindValue::Ambiguous => {
                return Err(EngineIniError::AmbiguousTarget(format!(
                    "duplicate section or key [{}] {}",
                    entry.section, entry.key
                )));
            }
        }
    }
    Ok(missing)
}

struct SplicedLayout {
    bytes: Vec<u8>,
    created_headers: Vec<Vec<u8>>,
    created_header_prefixes: Vec<Vec<u8>>,
    created_header_groups: Vec<String>,
    created_header_ordinals: Vec<u32>,
    introduced_prefixes: BTreeMap<(String, String), Vec<u8>>,
}

fn render_new_with_headers(missing: &[EngineIniEntry], encoding: IniEncoding) -> SplicedLayout {
    let bytes = render_new_document(missing, encoding);
    let mut created_headers = Vec::new();
    let mut created_header_prefixes = Vec::new();
    let mut created_header_groups = Vec::new();
    let mut created_header_ordinals = Vec::new();
    for (ordinal, group) in group_by_section(missing.to_vec()).into_iter().enumerate() {
        created_headers.push(encode_line(
            &format!("[{}]", group[0].section),
            "\r\n",
            encoding,
        ));
        created_header_prefixes.push(Vec::new());
        created_header_groups.push(group[0].section.clone());
        created_header_ordinals.push(u32::try_from(ordinal).expect("recipe groups fit u32"));
    }
    SplicedLayout {
        bytes,
        created_headers,
        created_header_prefixes,
        created_header_groups,
        created_header_ordinals,
        introduced_prefixes: BTreeMap::new(),
    }
}

fn splice_existing_document(
    before: &[u8],
    missing: &[EngineIniEntry],
    sections: &IniSections<'_>,
    encoding: IniEncoding,
) -> Result<SplicedLayout, EngineIniError> {
    let mut bytes = before.to_vec();
    let mut created_headers = Vec::new();
    let mut created_header_prefixes = Vec::new();
    let mut created_header_groups = Vec::new();
    let mut created_header_ordinals = Vec::new();
    let mut introduced_prefixes = BTreeMap::<(String, String), Vec<u8>>::new();

    let mut existing = Vec::new();
    let mut absent = Vec::new();
    for entry in missing {
        if sections.has_section(&entry.section)? {
            existing.push(entry.clone());
        } else {
            absent.push(entry.clone());
        }
    }
    let mut insertions = Vec::new();
    for group in group_by_section(existing) {
        let insertion = sections.insertion_offset(&group[0].section)?;
        let newline = sections.preferred_newline();
        let prefix = if insertion > 0 && !ends_with_newline(&bytes[..insertion], encoding) {
            encode_text(newline, encoding)
        } else {
            Vec::new()
        };
        let mut block = prefix.clone();
        for (index, entry) in group.iter().enumerate() {
            if index == 0 {
                introduced_prefixes.insert(
                    (ascii_key(&entry.section), ascii_key(&entry.key)),
                    prefix.clone(),
                );
            }
            block.extend(encode_assignment(
                &entry.key,
                &entry.value,
                newline,
                encoding,
            ));
        }
        insertions.push((insertion, block));
    }
    insertions.sort_by_key(|left| std::cmp::Reverse(left.0));
    for (insertion, block) in insertions {
        bytes.splice(insertion..insertion, block);
    }
    if !absent.is_empty() {
        let newline = sections.preferred_newline();
        let mut block = Vec::new();
        let initial_prefix = if !bytes.is_empty() && !ends_with_newline(&bytes, encoding) {
            encode_text(newline, encoding)
        } else {
            Vec::new()
        };
        block.extend(initial_prefix.clone());
        for (index, group) in group_by_section(absent).into_iter().enumerate() {
            if index > 0 {
                block.extend(encode_text(newline, encoding));
            }
            let header = encode_line(&format!("[{}]", group[0].section), newline, encoding);
            created_headers.push(header.clone());
            created_header_prefixes.push(if index == 0 {
                initial_prefix.clone()
            } else {
                encode_text(newline, encoding)
            });
            created_header_groups.push(group[0].section.clone());
            created_header_ordinals.push(u32::try_from(index).expect("recipe groups fit u32"));
            block.extend(header);
            for entry in group {
                block.extend(encode_assignment(
                    &entry.key,
                    &entry.value,
                    newline,
                    encoding,
                ));
            }
        }
        bytes.extend(block);
    }

    Ok(SplicedLayout {
        bytes,
        created_headers,
        created_header_prefixes,
        created_header_groups,
        created_header_ordinals,
        introduced_prefixes,
    })
}

fn collect_contributions(
    bytes: &[u8],
    inserted_entries: &[EngineIniEntry],
    mut introduced_prefixes: BTreeMap<(String, String), Vec<u8>>,
    contribution_newline: &str,
    encoding: IniEncoding,
) -> Result<Vec<EngineIniContribution>, EngineIniError> {
    let mut contributions = Vec::new();
    let mut group_ordinals = BTreeMap::<String, u32>::new();
    let (_, final_document) = decode_document(bytes)?;
    for entry in inserted_entries {
        let line = encode_assignment(&entry.key, &entry.value, contribution_newline, encoding);
        let matches = contribution_ranges_in_document(
            bytes,
            &final_document,
            &entry.section,
            &entry.key,
            &entry.value,
            &line,
        );
        let Some((start, end)) = (matches.len() == 1).then_some(matches[0]) else {
            return Err(EngineIniError::OwnershipUnclear(format!(
                "new assignment [{}] {} has {} exact syntactic matches",
                entry.section,
                entry.key,
                matches.len()
            )));
        };
        let introduced_prefix = introduced_prefixes
            .remove(&(ascii_key(&entry.section), ascii_key(&entry.key)))
            .unwrap_or_default();
        let group = entry.section.clone();
        let group_key = ascii_key(&group);
        let ordinal = group_ordinals.entry(group_key).or_default();
        let current_ordinal = *ordinal;
        *ordinal = (*ordinal).saturating_add(1);
        let owned_start = start
            .checked_sub(introduced_prefix.len())
            .filter(|prefix_start| {
                bytes
                    .get(*prefix_start..start)
                    .is_some_and(|value| value == introduced_prefix.as_slice())
            })
            .unwrap_or(start);
        contributions.push(EngineIniContribution {
            section: entry.section.clone(),
            key: entry.key.clone(),
            value: entry.value.clone(),
            left_anchor: digest(&bytes[..owned_start]),
            right_anchor: digest(&bytes[end..]),
            line,
            introduced_prefix,
            group,
            ordinal: current_ordinal,
        });
    }
    Ok(contributions)
}

/// Reconciles a previously published receipt with a new typed recipe set on
/// the same target.  Unchanged, provably owned assignments remain in place;
/// obsolete or changed owned assignments are removed first and then the
/// normal typed editor inserts the desired missing values.  Foreign values
/// are never overwritten.
pub fn reconcile_engine_ini(
    path: &Path,
    before: Option<&[u8]>,
    prior: &EngineIniReceipt,
    recipes: &EngineIniRecipeSet,
) -> Result<EngineIniEdit, EngineIniError> {
    let current = before.unwrap_or_default();
    let desired = recipes
        .entries()
        .iter()
        .map(|entry| {
            (
                (ascii_key(&entry.section), ascii_key(&entry.key)),
                entry.value.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut removable = prior.clone();
    removable.contributions.retain(|contribution| {
        desired
            .get(&(
                ascii_key(&contribution.section),
                ascii_key(&contribution.key),
            ))
            .is_none_or(|value| value != &contribution.value)
    });
    removable.created_headers.clear();
    removable.created_header_prefixes.clear();
    removable.created_header_groups.clear();
    removable.created_header_ordinals.clear();
    let mut retained = prior.clone();
    retained.contributions.retain(|contribution| {
        desired.get(&(
            ascii_key(&contribution.section),
            ascii_key(&contribution.key),
        )) == Some(&contribution.value)
    });
    // A revision may legitimately follow an unrelated user edit.  Anchors
    // are intentionally not a whole-file precondition; prove each obsolete
    // or changed assignment by its exact section/key/value occurrence before
    // allowing the release editor to remove only that body.  A changed,
    // missing, or duplicate old value remains an ownership conflict.
    let (_, current_doc) = decode_document(current)?;
    for contribution in &removable.contributions {
        let matches = contribution_ranges_in_document(
            current,
            &current_doc,
            &contribution.section,
            &contribution.key,
            &contribution.value,
            &contribution.line,
        );
        if matches.len() != 1 {
            return Err(EngineIniError::OwnershipUnclear(format!(
                "owned assignment [{}] {} cannot be revised uniquely",
                contribution.section, contribution.key
            )));
        }
    }
    transfer_introduced_separators(&mut removable, &mut retained, prior.encoding);
    let base_release = release_engine_ini_report(current, &removable)?;

    // Created headers are considered only after obsolete assignments have
    // been removed.  A foreign non-whitespace body keeps the header and makes
    // it foreign; the contribution receipt is still safe to continue with.
    let mut headers = prior.clone();
    headers.contributions.clear();
    let base = release_engine_ini_report(&base_release.bytes, &headers)?.bytes;

    let applied = apply_engine_ini(path, Some(&base), recipes)?;
    let final_bytes = applied.bytes;
    let mut contributions = Vec::new();
    let (_, final_doc) = decode_document(&final_bytes)?;
    for contribution in &retained.contributions {
        let identity = (
            ascii_key(&contribution.section),
            ascii_key(&contribution.key),
        );
        if desired.get(&identity) != Some(&contribution.value) {
            continue;
        }
        contributions.push(rebase_contribution_in_document(
            &final_bytes,
            &final_doc,
            contribution,
        )?);
    }

    let mut created_headers = Vec::new();
    let mut created_header_prefixes = Vec::new();
    let mut created_header_groups = Vec::new();
    let mut created_header_ordinals = Vec::new();
    for (((header, prefix), group), ordinal) in prior
        .created_headers
        .iter()
        .zip(&prior.created_header_prefixes)
        .zip(&prior.created_header_groups)
        .zip(&prior.created_header_ordinals)
    {
        if final_bytes
            .windows(header.len())
            .any(|window| window == header)
        {
            created_headers.push(header.clone());
            created_header_prefixes.push(prefix.clone());
            created_header_groups.push(group.clone());
            created_header_ordinals.push(*ordinal);
        }
    }
    if let Some(receipt) = applied.receipt {
        contributions.extend(receipt.contributions);
        created_headers.extend(receipt.created_headers);
        created_header_prefixes.extend(receipt.created_header_prefixes);
        created_header_groups.extend(receipt.created_header_groups);
        created_header_ordinals.extend(receipt.created_header_ordinals);
    }
    contributions.sort_by_key(|entry| (ascii_key(&entry.section), ascii_key(&entry.key)));
    let after_digest = digest(&final_bytes);
    Ok(EngineIniEdit {
        bytes: final_bytes,
        receipt: Some(EngineIniReceipt {
            schema_version: 1,
            path: path.to_string_lossy().into_owned(),
            file_created: prior.file_created,
            encoding: prior.encoding,
            before_digest: before.map_or_else(|| digest(&[]), digest),
            after_digest,
            recipe_fingerprint: recipes.fingerprint().to_owned(),
            contributions,
            created_headers,
            created_header_prefixes,
            created_header_groups,
            created_header_ordinals,
        }),
    })
}

fn rebase_contribution_in_document(
    bytes: &[u8],
    document: &IniDocument,
    contribution: &EngineIniContribution,
) -> Result<EngineIniContribution, EngineIniError> {
    let matches = contribution_ranges_in_document(
        bytes,
        document,
        &contribution.section,
        &contribution.key,
        &contribution.value,
        &contribution.line,
    );
    if matches.len() != 1 {
        return Err(EngineIniError::OwnershipUnclear(format!(
            "owned assignment [{}] {} cannot be rebased uniquely",
            contribution.section, contribution.key
        )));
    }
    let (start, end) = matches[0];
    let prefix = &contribution.introduced_prefix;
    let owned_start = start
        .checked_sub(prefix.len())
        .filter(|prefix_start| {
            bytes
                .get(*prefix_start..start)
                .is_some_and(|value| value == prefix.as_slice())
        })
        .unwrap_or(start);
    Ok(EngineIniContribution {
        left_anchor: digest(&bytes[..owned_start]),
        right_anchor: digest(&bytes[end..]),
        ..contribution.clone()
    })
}

/// When the first assignment in a section is revised away while a later
/// RenderPilot assignment remains, its introduced separator must move with
/// the retained assignment.  Otherwise removing the first line would join
/// the retained line to the user's preceding bytes; leaving the separator in
/// the old contribution would make the eventual final release leak it.
fn transfer_introduced_separators(
    removable: &mut EngineIniReceipt,
    retained: &mut EngineIniReceipt,
    encoding: IniEncoding,
) {
    for removed in &mut removable.contributions {
        if removed.introduced_prefix.is_empty() {
            continue;
        }
        let Some(next) = retained
            .contributions
            .iter_mut()
            .filter(|candidate| {
                candidate.group.eq_ignore_ascii_case(&removed.group)
                    && candidate.ordinal > removed.ordinal
                    && candidate.introduced_prefix.is_empty()
            })
            .min_by_key(|candidate| candidate.ordinal)
        else {
            continue;
        };
        next.introduced_prefix
            .clone_from(&removed.introduced_prefix);
        // Keep the removed line terminator as the separator before `next`.
        // The release matcher accepts this body-only temporary representation.
        removed.line = strip_line_ending(&removed.line, encoding);
    }
}

fn strip_line_ending(line: &[u8], encoding: IniEncoding) -> Vec<u8> {
    let terminator = match encoding {
        IniEncoding::Utf8 | IniEncoding::Utf8Bom => {
            if line.ends_with(b"\r\n") {
                2
            } else if line.ends_with(b"\n") || line.ends_with(b"\r") {
                1
            } else {
                0
            }
        }
        IniEncoding::Utf16Le => {
            if line.ends_with(&[13, 0, 10, 0]) {
                4
            } else if line.ends_with(&[10, 0]) || line.ends_with(&[13, 0]) {
                2
            } else {
                0
            }
        }
        IniEncoding::Utf16Be => {
            if line.ends_with(&[0, 13, 0, 10]) {
                4
            } else if line.ends_with(&[0, 10]) || line.ends_with(&[0, 13]) {
                2
            } else {
                0
            }
        }
    };
    line[..line.len().saturating_sub(terminator)].to_vec()
}

/// Releases every contribution that remains uniquely provable in the current
/// bytes.  Foreign edits are preserved.  A false `complete` result is a
/// durable ownership disclaimer: the caller may publish the returned bytes,
/// but must not retain a receipt for contributions that could not be proven.
pub fn release_engine_ini_report(
    before: &[u8],
    receipt: &EngineIniReceipt,
) -> Result<EngineIniRelease, EngineIniError> {
    if receipt.schema_version != 1 {
        return Ok(EngineIniRelease {
            bytes: before.to_vec(),
            complete: false,
        });
    }
    let mut bytes = before.to_vec();
    let mut owned_ranges = Vec::with_capacity(receipt.contributions.len());
    let mut complete = true;
    let (_, before_doc) = decode_document(&bytes)?;
    let separator = encode_text(before_doc.sections()?.preferred_newline(), receipt.encoding);
    for contribution in &receipt.contributions {
        let exact_matches = contribution_ranges_in_document(
            &bytes,
            &before_doc,
            &contribution.section,
            &contribution.key,
            &contribution.value,
            &contribution.line,
        );
        let (matches, body_only) =
            if exact_matches.is_empty() && !contribution.line.ends_with(&separator) {
                let mut expected_line = contribution.line.clone();
                expected_line.extend_from_slice(&separator);
                let full_matches = contribution_ranges_in_document(
                    &bytes,
                    &before_doc,
                    &contribution.section,
                    &contribution.key,
                    &contribution.value,
                    &expected_line,
                );
                (
                    full_matches
                        .into_iter()
                        .map(|(start, end)| (start, end.saturating_sub(separator.len())))
                        .collect::<Vec<_>>(),
                    true,
                )
            } else {
                (exact_matches, false)
            };
        let anchored = if body_only {
            Vec::new()
        } else {
            matches
                .iter()
                .copied()
                .filter(|(start, end)| {
                    let owned_start = start
                        .checked_sub(contribution.introduced_prefix.len())
                        .filter(|prefix_start| {
                            bytes.get(*prefix_start..*start).is_some_and(|value| {
                                value == contribution.introduced_prefix.as_slice()
                            })
                        });
                    owned_start.is_some_and(|prefix_start| {
                        digest(&bytes[..prefix_start]) == contribution.left_anchor
                            && digest(&bytes[*end..]) == contribution.right_anchor
                    })
                })
                .collect::<Vec<_>>()
        };
        let (owned_start, end) = if anchored.len() == 1 {
            let (start, end) = anchored[0];
            let owned_start = start
                .checked_sub(contribution.introduced_prefix.len())
                .expect("validated introduced prefix length");
            (owned_start, end)
        } else if anchored.is_empty() && matches.len() == 1 {
            // A foreign edit may invalidate the receipt's outer anchors while
            // leaving our exact assignment intact.  Remove only the assignment
            // body in that case; the introduced whitespace/newline remains
            // foreign because its ownership can no longer be proven.
            complete = false;
            let (start, end) = matches[0];
            let owned_start = if body_only {
                start
                    .checked_sub(contribution.introduced_prefix.len())
                    .unwrap_or(start)
            } else {
                start
            };
            (owned_start, end)
        } else {
            // Changed, missing, duplicate, or otherwise ambiguous content is
            // not safe to release.  Leave it in place and disclaim only this
            // part of the receipt.
            complete = false;
            continue;
        };
        if owned_ranges
            .iter()
            .any(|(other_start, other_end)| owned_start < *other_end && *other_start < end)
        {
            complete = false;
            continue;
        }
        owned_ranges.push((owned_start, end));
    }
    owned_ranges.sort_unstable_by_key(|left| std::cmp::Reverse(left.0));
    for (start, end) in owned_ranges {
        bytes.drain(start..end);
    }
    if receipt.created_headers.len() != receipt.created_header_prefixes.len()
        || receipt.created_headers.len() != receipt.created_header_groups.len()
        || receipt.created_headers.len() != receipt.created_header_ordinals.len()
    {
        return Ok(EngineIniRelease {
            bytes,
            complete: false,
        });
    }
    for (header, prefix) in receipt
        .created_headers
        .iter()
        .zip(&receipt.created_header_prefixes)
    {
        let mut matches = bytes
            .windows(header.len())
            .enumerate()
            .filter_map(|(index, window)| (window == header.as_slice()).then_some(index));
        let index = match (matches.next(), matches.next()) {
            (Some(index), None) => index,
            _ => {
                complete = false;
                continue;
            }
        };
        if !header_body_is_empty(&bytes, index) {
            complete = false;
            continue;
        }
        let Some(owned_start) = index.checked_sub(prefix.len()).filter(|prefix_start| {
            bytes
                .get(*prefix_start..index)
                .is_some_and(|value| value == prefix.as_slice())
        }) else {
            complete = false;
            continue;
        };
        bytes.drain(owned_start..index + header.len());
    }
    if receipt.file_created && is_bom_only(&bytes, receipt.encoding) {
        bytes.clear();
    }
    Ok(EngineIniRelease { bytes, complete })
}
