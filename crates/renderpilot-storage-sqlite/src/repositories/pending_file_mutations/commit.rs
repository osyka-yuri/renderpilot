//! Database commit-boundary validation for crash-recoverable file mutations.
//!
//! The facade owns only module wiring and the repository-facing re-exports.
//! Each validator lives with the durable contract it proves; no validation is
//! duplicated across these modules.

mod auxiliary;
mod cleanup;
mod fold;
mod generic;
mod journal;
mod materialization;
mod namespace;
mod preimage;
mod prelude;
mod progress;
mod relocation;
mod types;

#[cfg(test)]
mod tests;

pub(super) use auxiliary::*;
pub(in crate::repositories) use auxiliary::{
    validate_optiscaler_directory_receipt_metadata, validate_optiscaler_file_receipt_metadata,
};
pub(super) use cleanup::*;
pub(super) use fold::*;
pub(crate) use generic::mark_file_mutation_committed_within_transaction;
#[cfg(test)]
pub(in crate::repositories) use generic::validate_optiscaler_journal_json;
pub(crate) use generic::validate_prepared_mutation_commit_within_transaction;
pub(in crate::repositories) use generic::{
    parse_optiscaler_journal_for_recovery, validate_optiscaler_binding_within_transaction,
    validate_optiscaler_journal_for_begin, validate_optiscaler_journal_for_cas,
    validate_optiscaler_journal_for_committed_terminal, validate_optiscaler_journal_for_prepared,
    validate_optiscaler_journal_for_rollback_terminal,
};
pub(super) use journal::*;
pub(super) use materialization::*;
pub(super) use namespace::*;
pub(super) use preimage::*;
pub(super) use progress::*;
pub(super) use relocation::*;
pub(in crate::repositories) use types::{
    OptiScalerAggregateBinding, OptiScalerAuxiliaryPreservation, OptiScalerBoundPath,
    OptiScalerBoundTransition, OptiScalerOwnedPreservation, ReusedAcquisitionMode,
};
