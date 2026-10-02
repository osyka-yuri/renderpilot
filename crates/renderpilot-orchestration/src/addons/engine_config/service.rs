//! Durable orchestration for the shared Unreal `Engine.ini` boundary.
//!
//! The editor and publisher are intentionally pure/low-level modules.  This
//! layer owns the ordering that makes them safe: inspect and merge first,
//! persist a Pending journal, publish once, then finalize through a null-safe
//! CAS.  It is shared by RenoDX and Luma; callers provide only a typed recipe
//! set and the installed add-on identity.

use std::fmt;
use std::path::{Path, PathBuf};

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, EngineConfigContribution, EngineConfigJournal, EngineConfigReceipt,
    EngineConfigTransition, GameId,
};

use super::publication::{self, EngineIniPreflight, EngineIniPublicationError};
use super::{
    EngineIniError, EngineIniReceipt, EngineIniRecipeSet, EngineIniResolution, IniEncoding,
    apply_engine_ini, release_engine_ini_report,
};

/// Storage boundary used by the shared service.  Implementations must return
/// the exact persisted JSON token and must not deserialize/reserialize it
/// between the read and CAS calls.
pub trait EngineConfigJournalStore {
    /// Reads the exact nullable journal token for one add-on kind.
    fn engine_config_journal_token(
        &self,
        game_id: &GameId,
        kind: AddonKind,
    ) -> AppResult<Option<String>>;

    /// Replaces the token only when the expected raw token still matches.
    fn compare_and_swap_engine_config_journal(
        &self,
        game_id: &GameId,
        kind: AddonKind,
        expected_raw: Option<&str>,
        journal: Option<&EngineConfigJournal>,
    ) -> AppResult<bool>;
}

impl EngineConfigJournalStore for renderpilot_storage_sqlite::SqliteStorage {
    fn engine_config_journal_token(
        &self,
        game_id: &GameId,
        kind: AddonKind,
    ) -> AppResult<Option<String>> {
        renderpilot_storage_sqlite::SqliteStorage::engine_config_journal_token(self, game_id, kind)
    }

    fn compare_and_swap_engine_config_journal(
        &self,
        game_id: &GameId,
        kind: AddonKind,
        expected_raw: Option<&str>,
        journal: Option<&EngineConfigJournal>,
    ) -> AppResult<bool> {
        renderpilot_storage_sqlite::SqliteStorage::compare_and_swap_engine_config_journal(
            self,
            game_id,
            kind,
            expected_raw,
            journal,
        )
    }
}

/// Stable report state shown by availability and explicit actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineConfigStatus {
    /// The add-on/game does not expose typed Engine.ini guidance.
    NotApplicable,
    /// The project identity or supported path was not proven; show manual
    /// guidance without attempting a filesystem mutation.
    ManualOnly,
    /// The game has not created any bounded Unreal config directory yet.
    PendingFirstLaunch,
    /// A typed recipe exists and the target is safe to apply.
    Ready,
    /// The installed recipe set is already satisfied or has been finalized.
    Configured,
    /// A prior receipt exists but the current bytes no longer match it.
    NeedsRepair,
    /// Multiple bounded targets or typed values conflict.
    Conflict,
    /// A durable Pending transition needs recovery before another action.
    RecoveryRequired,
}

/// Pure availability snapshot.  Resolving this value never writes Engine.ini
/// or the journal and is safe to call repeatedly after game launch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EngineConfigAvailability {
    /// Stable report state.
    pub status: EngineConfigStatus,
    /// Exact bounded target, when resolution proved one.
    pub path: Option<PathBuf>,
    /// Whether the backend can offer an Apply/Reapply mutation now.
    pub can_apply: bool,
}

/// A fully preflighted publication.  Constructing it is read-only; the
/// caller must pass it to [`apply_preflight`] only after the Pending CAS.
#[derive(Debug, Clone)]
pub struct EngineConfigApplyPlan {
    path: PathBuf,
    publication: EngineIniPreflight,
    prior: EngineConfigJournal,
    pending: EngineConfigJournal,
    after_receipt: EngineConfigReceipt,
    /// The desired bytes are already on disk, but the receipt needs to be
    /// rebased (for example after a harmless foreign comment).  This plan
    /// updates only the durable ownership metadata; it must never enter the
    /// Pending/publisher path.
    journal_only: bool,
}

impl EngineConfigApplyPlan {
    /// Target path proven during read-only preflight.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Result of an apply that did not need a filesystem mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Existing values already satisfy the typed recipe set.
    AlreadyConfigured,
    /// The target bytes already satisfy the recipe set and only the durable
    /// ownership receipt was rebased.  No filesystem publication occurred.
    MetadataReconciled,
    /// The pending transition was published and finalized.
    Applied,
}

/// Result of inspecting and resolving one durable Pending transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// No Pending transition was present.
    NoPending,
    /// The expected post-image was already on disk; the stable receipt was
    /// finalized without touching the target bytes.
    PromotedAfter,
    /// The exact pre-image was still on disk; the Pending transition was
    /// cancelled and the prior stable receipt restored.
    CancelledBefore,
}

/// Result of releasing a stable receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    /// No stable receipt was present.
    NotConfigured,
    /// The receipt was released and the journal was cleared.
    Released,
}

/// Releases the shared Engine.ini contribution owned by an installed record.
///
/// This is the single lifecycle bridge used by both tool adapters immediately
/// before their database delete/peer commit.  It deliberately derives the
/// target from the persisted receipt instead of accepting a caller path, so a
/// stale or mismatched path cannot redirect release.  Any publication, journal
/// or CAS error is returned and therefore blocks the owning uninstall route.
pub fn release_record<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    record: &renderpilot_domain::InstalledAddon,
    operation_id: &str,
) -> Result<ReleaseOutcome, crate::ServiceError> {
    release_record_by_kind(store, game_id, record.kind(), operation_id)
}

/// Releases an independently owned Engine.ini contribution by its canonical
/// add-on kind. The target is still derived from the persisted receipt, so the
/// caller cannot redirect cleanup by supplying a replacement path.
pub fn release_record_by_kind<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
    operation_id: &str,
) -> Result<ReleaseOutcome, crate::ServiceError> {
    // Finish any durable transition first, then read the journal again. The
    // canonical storage token is the authoritative lifecycle state.
    recover_pending(store, game_id, kind)
        .map_err(|error| crate::ServiceError::command_failed(error.to_string()))?;
    let raw = store
        .engine_config_journal_token(game_id, kind)
        .map_err(|error| crate::ServiceError::command_failed(error.to_string()))?;
    let journal = parse_journal(kind, raw.as_deref())
        .map_err(|error| crate::ServiceError::command_failed(error.to_string()))?;
    let Some(stable) = journal.stable.as_ref() else {
        return Ok(ReleaseOutcome::NotConfigured);
    };
    release(store, game_id, kind, Path::new(&stable.path), operation_id)
        .map_err(|error| crate::ServiceError::command_failed(error.to_string()))
}

/// Fail-closed service error.  Publication errors are kept distinct from
/// storage/CAS failures so callers can report recovery-required accurately.
#[derive(Debug)]
pub enum EngineConfigServiceError {
    /// Typed editor rejected the input or found foreign conflict.
    Editor(EngineIniError),
    /// Audited publication failed.
    Publication(EngineIniPublicationError),
    /// Journal token could not be decoded or violated its invariant.
    Journal(String),
    /// CAS lost a concurrent writer or the row disappeared.
    ConcurrentJournalChange,
    /// The journal is Pending and must be recovered before another action.
    RecoveryRequired,
    /// A stable receipt does not describe the current target bytes.
    NeedsRepair,
    /// The caller supplied an invalid operation identifier.
    InvalidOperationId,
    /// Storage-layer failure.
    Storage(String),
}

impl fmt::Display for EngineConfigServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Editor(error) => write!(formatter, "Engine.ini editor: {error}"),
            Self::Publication(error) => write!(formatter, "Engine.ini publication: {error}"),
            Self::Journal(error) => write!(formatter, "Engine.ini journal: {error}"),
            Self::ConcurrentJournalChange => {
                formatter.write_str("Engine.ini journal changed concurrently")
            }
            Self::RecoveryRequired => formatter.write_str("Engine.ini recovery is required"),
            Self::NeedsRepair => {
                formatter.write_str("Engine.ini no longer matches its ownership receipt")
            }
            Self::InvalidOperationId => {
                formatter.write_str("Engine.ini operation id is empty or unsafe")
            }
            Self::Storage(error) => write!(formatter, "Engine.ini journal storage: {error}"),
        }
    }
}

impl std::error::Error for EngineConfigServiceError {}

impl From<EngineIniError> for EngineConfigServiceError {
    fn from(error: EngineIniError) -> Self {
        Self::Editor(error)
    }
}

impl From<EngineIniPublicationError> for EngineConfigServiceError {
    fn from(error: EngineIniPublicationError) -> Self {
        Self::Publication(error)
    }
}

fn storage_error(error: impl fmt::Display) -> EngineConfigServiceError {
    EngineConfigServiceError::Storage(error.to_string())
}

fn parse_journal(
    kind: AddonKind,
    raw: Option<&str>,
) -> Result<EngineConfigJournal, EngineConfigServiceError> {
    let journal: EngineConfigJournal = raw
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| EngineConfigServiceError::Journal(error.to_string()))?
        .unwrap_or_default();
    journal
        .validate_for_kind(kind)
        .map_err(|error| EngineConfigServiceError::Journal(error.to_string()))?;
    if let Some(raw) = raw {
        let canonical = serde_json::to_string(&journal)
            .map_err(|error| EngineConfigServiceError::Journal(error.to_string()))?;
        if canonical != raw {
            return Err(EngineConfigServiceError::Journal(
                "journal token is not canonical".to_owned(),
            ));
        }
    }
    Ok(journal)
}

fn ensure_operation_id(operation_id: &str) -> Result<(), EngineConfigServiceError> {
    if operation_id.is_empty()
        || operation_id.len() > 96
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(EngineConfigServiceError::InvalidOperationId);
    }
    Ok(())
}

fn digest_before(preflight: &EngineIniPreflight) -> String {
    preflight
        .before
        .as_deref()
        .map_or_else(|| publication::digest_bytes(&[]), publication::digest_bytes)
}

fn encoding_name(encoding: IniEncoding) -> String {
    match encoding {
        IniEncoding::Utf8 => "utf8",
        IniEncoding::Utf8Bom => "utf8_bom",
        IniEncoding::Utf16Le => "utf16_le",
        IniEncoding::Utf16Be => "utf16_be",
    }
    .to_owned()
}

fn receipt_to_domain(receipt: EngineIniReceipt) -> EngineConfigReceipt {
    EngineConfigReceipt {
        schema_version: receipt.schema_version,
        path: receipt.path,
        file_created: receipt.file_created,
        encoding: encoding_name(receipt.encoding),
        before_digest: receipt.before_digest,
        after_digest: receipt.after_digest,
        recipe_fingerprint: receipt.recipe_fingerprint,
        contributions: receipt
            .contributions
            .into_iter()
            .map(|entry| EngineConfigContribution {
                section: entry.section,
                key: entry.key,
                value: entry.value,
                line: entry.line,
                left_anchor: entry.left_anchor,
                right_anchor: entry.right_anchor,
                introduced_prefix: entry.introduced_prefix,
                group: entry.group,
                ordinal: entry.ordinal,
            })
            .collect(),
        created_headers: receipt.created_headers,
        created_header_prefixes: receipt.created_header_prefixes,
        created_header_groups: receipt.created_header_groups,
        created_header_ordinals: receipt.created_header_ordinals,
    }
}

fn receipt_to_runtime(
    receipt: &EngineConfigReceipt,
) -> Result<EngineIniReceipt, EngineConfigServiceError> {
    let encoding = match receipt.encoding.as_str() {
        "utf8" => IniEncoding::Utf8,
        "utf8_bom" => IniEncoding::Utf8Bom,
        "utf16_le" => IniEncoding::Utf16Le,
        "utf16_be" => IniEncoding::Utf16Be,
        other => {
            return Err(EngineConfigServiceError::Journal(format!(
                "unsupported encoding {other}"
            )));
        }
    };
    Ok(EngineIniReceipt {
        schema_version: receipt.schema_version,
        path: receipt.path.clone(),
        file_created: receipt.file_created,
        encoding,
        before_digest: receipt.before_digest.clone(),
        after_digest: receipt.after_digest.clone(),
        recipe_fingerprint: receipt.recipe_fingerprint.clone(),
        contributions: receipt
            .contributions
            .iter()
            .map(|entry| super::EngineIniContribution {
                section: entry.section.clone(),
                key: entry.key.clone(),
                value: entry.value.clone(),
                line: entry.line.clone(),
                left_anchor: entry.left_anchor.clone(),
                right_anchor: entry.right_anchor.clone(),
                introduced_prefix: entry.introduced_prefix.clone(),
                group: entry.group.clone(),
                ordinal: entry.ordinal,
            })
            .collect(),
        created_headers: receipt.created_headers.clone(),
        created_header_prefixes: receipt.created_header_prefixes.clone(),
        created_header_groups: receipt.created_header_groups.clone(),
        created_header_ordinals: receipt.created_header_ordinals.clone(),
    })
}

fn pending_journal(
    prior: EngineConfigJournal,
    after: EngineConfigReceipt,
    operation_id: &str,
    before_digest: String,
    after_digest: String,
) -> EngineConfigJournal {
    EngineConfigJournal {
        stable: prior.stable.clone(),
        pending: Some(EngineConfigTransition {
            operation_id: operation_id.to_owned(),
            stage_name: format!(".renderpilot-engine-{operation_id}.stage"),
            prior: prior.stable,
            after: Some(after),
            before_digest,
            after_digest,
        }),
    }
}

/// Builds a read-only plan for applying a typed recipe set.
pub fn prepare_apply(
    path: &Path,
    prior: EngineConfigJournal,
    recipes: &EngineIniRecipeSet,
    operation_id: &str,
) -> Result<Option<EngineConfigApplyPlan>, EngineConfigServiceError> {
    ensure_operation_id(operation_id)?;
    let observed = publication::preflight(path, Vec::new())?;
    let edit = if let Some(stable) = &prior.stable {
        super::reconcile_engine_ini(
            path,
            observed.before.as_deref(),
            &receipt_to_runtime(stable)?,
            recipes,
        )?
    } else {
        apply_engine_ini(path, observed.before.as_deref(), recipes)?
    };
    let Some(receipt) = edit.receipt else {
        return Ok(None);
    };

    let observed_bytes = observed.before.as_deref().unwrap_or(&[]);
    let before_digest = receipt.before_digest.clone();
    let after_digest = receipt.after_digest.clone();
    let mut after_receipt = receipt_to_domain(receipt);
    if edit.bytes == observed_bytes {
        // `reconcile_engine_ini` normally records the current input as its
        // before-digest.  When no bytes changed, that would manufacture a
        // receipt difference on every identical reapply even though the
        // stable receipt already owns this exact image.  Preserve the
        // durable original preimage in this no-byte-change case; genuine
        // foreign/revision metadata changes still differ through their
        // anchors, digest, or fingerprint and take the journal-only path.
        if let Some(stable) = prior.stable.as_ref() {
            after_receipt
                .before_digest
                .clone_from(&stable.before_digest);
        }
    }
    let journal_only = edit.bytes == observed_bytes;
    if journal_only {
        // A stable receipt that is byte-for-byte identical to the reconciled
        // result is already complete.  In particular, do not even CAS the
        // journal: repeated updates must be a true no-op.
        if prior.stable.as_ref() == Some(&after_receipt) {
            return Ok(None);
        }
    }

    // Re-read the target before carrying the prepared image into the plan
    // so the CAS cannot certify a stale observation.
    let publication = publication::preflight(path, edit.bytes)?;
    if publication.before != observed.before {
        return Err(EngineConfigServiceError::Publication(
            EngineIniPublicationError::PreimageChanged,
        ));
    }
    let pending = pending_journal(
        prior.clone(),
        after_receipt.clone(),
        operation_id,
        before_digest,
        after_digest,
    );
    Ok(Some(EngineConfigApplyPlan {
        path: path.to_path_buf(),
        publication,
        prior,
        pending,
        after_receipt,
        journal_only,
    }))
}

/// Applies a preflighted plan with Pending-before-visible-mutation ordering.
pub fn apply_preflight<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
    expected_raw: Option<&str>,
    plan: EngineConfigApplyPlan,
) -> Result<ApplyOutcome, EngineConfigServiceError> {
    if plan.journal_only {
        let stable = EngineConfigJournal {
            stable: Some(plan.after_receipt),
            pending: None,
        };
        if !store
            .compare_and_swap_engine_config_journal(game_id, kind, expected_raw, Some(&stable))
            .map_err(storage_error)?
        {
            return Err(EngineConfigServiceError::ConcurrentJournalChange);
        }
        return Ok(ApplyOutcome::MetadataReconciled);
    }

    if !store
        .compare_and_swap_engine_config_journal(game_id, kind, expected_raw, Some(&plan.pending))
        .map_err(storage_error)?
    {
        return Err(EngineConfigServiceError::ConcurrentJournalChange);
    }
    let operation_id = plan
        .pending
        .pending
        .as_ref()
        .map(|pending| pending.operation_id.as_str())
        .ok_or_else(|| {
            EngineConfigServiceError::Journal(
                "apply plan is missing its pending operation".to_owned(),
            )
        })?;
    if let Err(error) = publication::publish(&plan.publication, operation_id) {
        // If publication failed while the exact before-image is still on disk,
        // the stage was not visible and the Pending token can be cancelled.
        // Any other observation remains Pending for recovery.
        let current = publication::preflight(&plan.path, Vec::new())
            .ok()
            .and_then(|value| value.before);
        let current_digest = current
            .as_deref()
            .map_or_else(|| publication::digest_bytes(&[]), publication::digest_bytes);
        if current_digest == plan.after_receipt.before_digest {
            let pending_raw = serde_json::to_string(&plan.pending).ok();
            let _ = store.compare_and_swap_engine_config_journal(
                game_id,
                kind,
                pending_raw.as_deref(),
                Some(&plan.prior),
            );
        }
        return Err(error.into());
    }
    let stable = EngineConfigJournal {
        stable: Some(plan.after_receipt),
        pending: None,
    };
    let pending_raw = serde_json::to_string(&plan.pending)
        .map_err(|error| EngineConfigServiceError::Journal(error.to_string()))?;
    if !store
        .compare_and_swap_engine_config_journal(game_id, kind, Some(&pending_raw), Some(&stable))
        .map_err(storage_error)?
    {
        return Err(EngineConfigServiceError::RecoveryRequired);
    }
    Ok(ApplyOutcome::Applied)
}

/// Recovers one Pending transition without guessing or changing Engine.ini.
///
/// The target is classified solely by the durable before/after digests.  A
/// private stage is removed only when it is a regular non-reparse file whose
/// bytes match the expected after-image; unexpected collisions remain in
/// place and keep the journal Pending for manual recovery.
pub fn recover_pending<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
) -> Result<RecoveryOutcome, EngineConfigServiceError> {
    let raw = store
        .engine_config_journal_token(game_id, kind)
        .map_err(storage_error)?;
    let journal = parse_journal(kind, raw.as_deref())?;
    let Some(pending) = journal.pending.as_ref() else {
        return Ok(RecoveryOutcome::NoPending);
    };
    let receipt = pending
        .after
        .as_ref()
        .or(pending.prior.as_ref())
        .ok_or_else(|| EngineConfigServiceError::Journal("pending target is missing".to_owned()))?;
    let path = Path::new(&receipt.path);
    let observed = publication::preflight(path, Vec::new())?;
    let after_receipt = pending.after.as_ref().map(receipt_to_runtime).transpose()?;
    let decision = super::reconcile_pending_transition(
        &pending.before_digest,
        &pending.after_digest,
        after_receipt.as_ref(),
        // A missing target is the exact empty pre-image used for a newly
        // created file (and the empty post-image used when that file is
        // released).  Feed that representation to the digest classifier;
        // an actually different/missing image remains RecoveryRequired.
        observed.before.as_deref().or(Some(&[])),
    );
    let stage_path = path
        .parent()
        .ok_or_else(|| {
            EngineConfigServiceError::Journal("pending target has no parent".to_owned())
        })?
        .join(&pending.stage_name);
    let stage_present = match std::fs::symlink_metadata(&stage_path) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(EngineConfigServiceError::Storage(error.to_string())),
    };
    if stage_present
        && !publication::cleanup_owned_stage(path, &pending.stage_name, &pending.after_digest)?
    {
        return Err(EngineConfigServiceError::RecoveryRequired);
    }
    let next = match decision {
        super::EngineIniRecovery::PromoteAfter => EngineConfigJournal {
            // Release transitions intentionally have no after receipt: the
            // owned contribution has been removed and the durable result is
            // SQL NULL.  Apply transitions promote their complete receipt.
            stable: pending.after.clone(),
            pending: None,
        },
        super::EngineIniRecovery::CancelPending => EngineConfigJournal {
            stable: pending.prior.clone(),
            pending: None,
        },
        super::EngineIniRecovery::RecoveryRequired => {
            return Err(EngineConfigServiceError::RecoveryRequired);
        }
    };
    if !store
        .compare_and_swap_engine_config_journal(game_id, kind, raw.as_deref(), Some(&next))
        .map_err(storage_error)?
    {
        return Err(EngineConfigServiceError::ConcurrentJournalChange);
    }
    Ok(match decision {
        super::EngineIniRecovery::PromoteAfter => RecoveryOutcome::PromotedAfter,
        super::EngineIniRecovery::CancelPending => RecoveryOutcome::CancelledBefore,
        super::EngineIniRecovery::RecoveryRequired => unreachable!(),
    })
}

/// End-to-end explicit apply/reapply.  It performs no work when the typed
/// recipe set is already satisfied.
pub fn apply<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
    path: &Path,
    recipes: &EngineIniRecipeSet,
    operation_id: &str,
) -> Result<ApplyOutcome, EngineConfigServiceError> {
    ensure_operation_id(operation_id)?;
    let raw = store
        .engine_config_journal_token(game_id, kind)
        .map_err(storage_error)?;
    let prior = parse_journal(kind, raw.as_deref())?;
    if prior.is_pending() {
        return Err(EngineConfigServiceError::RecoveryRequired);
    }
    if let Some(stable) = prior.stable.as_ref()
        && !super::same_path_identity(Path::new(&stable.path), path)
    {
        // Prove both sides before releasing the old ownership.  The new
        // target is deliberately preflighted with an empty journal so a
        // recipe/value conflict cannot discard the old, still-owned bytes.
        let _ = prepare_apply(path, EngineConfigJournal::default(), recipes, operation_id)?;
        let release_operation = relocation_release_operation_id(operation_id);
        release(
            store,
            game_id,
            kind,
            Path::new(&stable.path),
            &release_operation,
        )?;
        // Re-read the raw token after the durable old-target release and
        // apply the new target as a fresh None→Pending transition.
        return apply(store, game_id, kind, path, recipes, operation_id);
    }
    let Some(plan) = prepare_apply(path, prior, recipes, operation_id)? else {
        return Ok(ApplyOutcome::AlreadyConfigured);
    };
    apply_preflight(store, game_id, kind, raw.as_deref(), plan)
}

fn relocation_release_operation_id(operation_id: &str) -> String {
    const SUFFIX: &str = "-old";
    let max_prefix = 96 - SUFFIX.len();
    format!(
        "{}{}",
        &operation_id[..operation_id.len().min(max_prefix)],
        SUFFIX
    )
}

/// Applies a typed recipe set only when the bounded resolver proved an
/// existing target (or a target inside an existing config directory).  A
/// pending-first-launch/manual/conflict resolution is deliberately a no-op;
/// the caller can surface that state through availability without attempting
/// to manufacture Unreal's directory tree.
pub fn apply_resolved<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
    resolution: &EngineIniResolution,
    recipes: Option<&EngineIniRecipeSet>,
    operation_id: &str,
) -> Result<Option<ApplyOutcome>, EngineConfigServiceError> {
    let Some(path) = resolution.path() else {
        return Ok(None);
    };
    let Some(recipes) = recipes else {
        return Ok(None);
    };
    apply(store, game_id, kind, path, recipes, operation_id).map(Some)
}

/// Releases the stable contribution set using the same durable Pending/CAS
/// boundary.  The caller must invoke this before deleting the installed row.
pub fn release<S: EngineConfigJournalStore>(
    store: &S,
    game_id: &GameId,
    kind: AddonKind,
    path: &Path,
    operation_id: &str,
) -> Result<ReleaseOutcome, EngineConfigServiceError> {
    ensure_operation_id(operation_id)?;
    let raw = store
        .engine_config_journal_token(game_id, kind)
        .map_err(storage_error)?;
    let prior = parse_journal(kind, raw.as_deref())?;
    if prior.is_pending() {
        return Err(EngineConfigServiceError::RecoveryRequired);
    }
    let Some(stable) = prior.stable.as_ref() else {
        return Ok(ReleaseOutcome::NotConfigured);
    };
    let observed = publication::preflight(path, Vec::new())?;
    let Some(current) = observed.before.as_deref() else {
        // A missing Engine.ini is a structural disclaimer, not an I/O
        // failure. There is no remaining file to mutate and ownership cannot
        // be proved, so clear the journal before uninstall continues.
        if !store
            .compare_and_swap_engine_config_journal(game_id, kind, raw.as_deref(), None)
            .map_err(storage_error)?
        {
            return Err(EngineConfigServiceError::ConcurrentJournalChange);
        }
        return Ok(ReleaseOutcome::Released);
    };
    let runtime_receipt = receipt_to_runtime(stable)?;
    let released = release_engine_ini_report(current, &runtime_receipt)?;
    if released.bytes == current {
        // Nothing uniquely provable was removable.  This is a structural
        // ownership disclaimer (changed/missing/ambiguous content), not a
        // publication failure; preserve all foreign bytes and unblock the
        // owning add-on uninstall by clearing the receipt.
        if !store
            .compare_and_swap_engine_config_journal(game_id, kind, raw.as_deref(), None)
            .map_err(storage_error)?
        {
            return Err(EngineConfigServiceError::ConcurrentJournalChange);
        }
        return Ok(ReleaseOutcome::Released);
    }
    let before_digest = digest_before(&observed);
    let after_digest = publication::digest_bytes(&released.bytes);
    let transition = EngineConfigTransition {
        operation_id: operation_id.to_owned(),
        stage_name: format!(".renderpilot-engine-{operation_id}.stage"),
        prior: Some(stable.clone()),
        after: None,
        before_digest,
        after_digest,
    };
    let pending = EngineConfigJournal {
        stable: Some(stable.clone()),
        pending: Some(transition),
    };
    if !store
        .compare_and_swap_engine_config_journal(game_id, kind, raw.as_deref(), Some(&pending))
        .map_err(storage_error)?
    {
        return Err(EngineConfigServiceError::ConcurrentJournalChange);
    }
    let delete_file = stable.file_created && released.bytes.is_empty() && released.complete;
    let publication_result = if delete_file {
        publication::delete_created_empty(path, current).and_then(|deleted| {
            if deleted {
                Ok(())
            } else {
                Err(EngineIniPublicationError::PreimageChanged)
            }
        })
    } else {
        let plan = publication::preflight(path, released.bytes)?;
        if plan.before != observed.before {
            Err(EngineIniPublicationError::PreimageChanged)
        } else {
            publication::publish(&plan, operation_id)
        }
    };
    if let Err(error) = publication_result {
        return Err(error.into());
    }
    let pending_raw = serde_json::to_string(&pending)
        .map_err(|error| EngineConfigServiceError::Journal(error.to_string()))?;
    if !store
        .compare_and_swap_engine_config_journal(game_id, kind, Some(&pending_raw), None)
        .map_err(storage_error)?
    {
        return Err(EngineConfigServiceError::RecoveryRequired);
    }
    Ok(ReleaseOutcome::Released)
}

/// Reads the bounded target and computes the complete availability state for a
/// materialized guidance set. This function is deliberately read-only: it may
/// parse/merge an in-memory post-image, but never persists a journal or writes
/// Engine.ini.
#[must_use]
pub fn inspect_availability(
    resolution: &EngineIniResolution,
    recipes: Option<&EngineIniRecipeSet>,
    manual_only: bool,
    journal: Option<&EngineConfigJournal>,
) -> EngineConfigAvailability {
    let path = resolution.path().map(Path::to_path_buf);
    let typed_recipe = recipes.is_some();
    let mut status = if !typed_recipe {
        if manual_only {
            EngineConfigStatus::ManualOnly
        } else {
            EngineConfigStatus::NotApplicable
        }
    } else if journal.is_some_and(EngineConfigJournal::is_pending) {
        EngineConfigStatus::RecoveryRequired
    } else {
        match resolution {
            EngineIniResolution::ManualOnly => EngineConfigStatus::ManualOnly,
            EngineIniResolution::PendingFirstLaunch => EngineConfigStatus::PendingFirstLaunch,
            EngineIniResolution::Conflict => EngineConfigStatus::Conflict,
            EngineIniResolution::Ready(_) | EngineIniResolution::ReadyToCreate(_) => {
                EngineConfigStatus::Ready
            }
        }
    };

    if matches!(status, EngineConfigStatus::Ready) {
        let before = match path.as_deref() {
            Some(path) => match std::fs::symlink_metadata(path) {
                Ok(metadata) if is_plain_file(&metadata) => std::fs::read(path).ok(),
                Ok(_) => {
                    status = EngineConfigStatus::Conflict;
                    None
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    status = EngineConfigStatus::NeedsRepair;
                    None
                }
            },
            None => None,
        };
        if matches!(status, EngineConfigStatus::Ready) {
            if let Some(stable) = journal.and_then(|value| value.stable.as_ref()) {
                // A whole-file digest is intentionally not an availability
                // gate: users may add harmless comments/settings around our
                // owned lines.  Reconcile against the receipt instead and
                // classify only a missing/drifted managed contribution as a
                // repair.  A receipt for another proven target still needs
                // relocation/apply rather than being reported configured.
                let receipt_matches_target = path.as_deref().is_some_and(|target| {
                    super::same_path_identity(Path::new(&stable.path), target)
                });
                if !receipt_matches_target {
                    status = EngineConfigStatus::NeedsRepair;
                } else if let Some(recipes) = recipes {
                    match receipt_to_runtime(stable).and_then(|runtime| {
                        super::reconcile_engine_ini(
                            Path::new(&stable.path),
                            before.as_deref(),
                            &runtime,
                            recipes,
                        )
                        .map_err(EngineConfigServiceError::from)
                    }) {
                        Ok(edit) if edit.bytes == before.as_deref().unwrap_or(&[]) => {
                            status = EngineConfigStatus::Configured;
                        }
                        Ok(_) => status = EngineConfigStatus::NeedsRepair,
                        Err(EngineConfigServiceError::Editor(
                            EngineIniError::ExistingValueConflict { .. },
                        ))
                        | Err(EngineConfigServiceError::Editor(EngineIniError::AmbiguousTarget(
                            _,
                        ))) => {
                            status = EngineConfigStatus::Conflict;
                        }
                        Err(_) => status = EngineConfigStatus::NeedsRepair,
                    }
                }
            }
            if matches!(status, EngineConfigStatus::Ready)
                && let (Some(path), Some(recipes)) = (path.as_deref(), recipes)
            {
                match apply_engine_ini(path, before.as_deref(), recipes) {
                    Ok(edit) if edit.receipt.is_none() => {
                        status = EngineConfigStatus::Configured;
                    }
                    Ok(_) => {}
                    Err(EngineIniError::ExistingValueConflict { .. })
                    | Err(EngineIniError::AmbiguousTarget(_)) => {
                        status = EngineConfigStatus::Conflict;
                    }
                    Err(_) => status = EngineConfigStatus::NeedsRepair,
                }
            }
        }
    }
    EngineConfigAvailability {
        status,
        path,
        can_apply: matches!(
            status,
            EngineConfigStatus::Ready
                | EngineConfigStatus::NeedsRepair
                | EngineConfigStatus::RecoveryRequired
        ),
    }
}

fn is_plain_file(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }

    #[cfg(not(windows))]
    true
}

/// Resolves the bounded target from the authoritative game analysis proof.
/// No caller-provided project name or arbitrary path is accepted here.
pub(crate) fn resolve_for_analysis(
    analysis: &crate::addons::game_analysis::GameAnalysis,
    local_app_data: Option<&Path>,
) -> EngineIniResolution {
    let Some(identity) = analysis.unreal_project_identity() else {
        return EngineIniResolution::ManualOnly;
    };
    super::resolve_unreal_engine_ini(
        Some(&identity.project_root),
        Some(&identity.project_name),
        local_app_data,
    )
}

#[cfg(test)]
mod tests;
