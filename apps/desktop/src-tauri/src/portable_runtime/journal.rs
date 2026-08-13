//! Stable protocol-v3 journal facade.
//!
//! Durable journal mechanics are deliberately split by responsibility.  Callers
//! keep this module path; only this facade coordinates append and reconciliation.

mod append;
mod image;
mod mutation;
mod outbox;
mod paths;
mod protocol;
mod reader;
mod reconcile;
#[cfg(test)]
mod reconcile_tests;
mod recovery_plan;
mod terminal;
#[cfg(test)]
mod test_support;
mod transition;

pub use protocol::RecoveryAction;
#[cfg(test)]
pub(in crate::portable_runtime) use protocol::{JOURNAL_PROTOCOL, TerminalReceiptV3};
pub(in crate::portable_runtime) use terminal::write_terminal_receipt;
pub(in crate::portable_runtime) use {
    paths::journal_path,
    protocol::{JournalAppendKind, JournalEntry, JournalPhase},
    reader::read_entries,
    recovery_plan::plan_recovery,
    terminal::terminal_receipt_exists,
    transition::RecoveryTransition,
};

use std::path::Path;

use super::{error::Result, supervisor::authority::SupervisorSessionAuthority};

/// Production append boundary. Earlier durable operations are reconciled
/// before its immutable append intent is recorded.
pub(in crate::portable_runtime) fn append_normal_with_outbox(
    path: &Path,
    generation_store_root: &Path,
    next: JournalEntry,
    authority: &SupervisorSessionAuthority,
) -> Result<JournalEntry> {
    reconcile::reconcile_operation_outbox(
        generation_store_root,
        &paths::journal_update_root(path)?,
    )?;
    append::append_normal(path, next, authority, generation_store_root)
}

pub(in crate::portable_runtime) fn append_recovery_with_outbox(
    path: &Path,
    generation_store_root: &Path,
    next: JournalEntry,
    authority: &SupervisorSessionAuthority,
    transition: RecoveryTransition,
) -> Result<JournalEntry> {
    reconcile::reconcile_operation_outbox(
        generation_store_root,
        &paths::journal_update_root(path)?,
    )?;
    append::append_recovery(path, next, authority, transition, generation_store_root)
}

/// Reconciles every immutable append/repair intent. No operation evidence is
/// removed; all resulting observations remain image-bound and authenticated.
pub(in crate::portable_runtime) fn reconcile_operation_outbox(
    generation_store_root: &Path,
    update_root: &Path,
) -> Result<()> {
    reconcile::reconcile_operation_outbox(generation_store_root, update_root)
}

/// True only when authenticated outbox evidence proves that this exact empty
/// transaction directory was abandoned before its origin `Prepared` append.
pub(in crate::portable_runtime) fn aborted_before_origin(
    generation_store_root: &Path,
    journal: &Path,
) -> Result<bool> {
    reconcile::aborted_before_origin(generation_store_root, journal)
}

#[cfg(test)]
pub(in crate::portable_runtime) use test_support::{
    append_malformed_selection_digest_fixture, append_normal, append_recovery, intended_prefix_len,
    record_origin_append_intent,
};
