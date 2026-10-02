use renderpilot_domain::{AddonKind, EngineConfigReceipt, GameInstallation};
use renderpilot_storage_sqlite::{EngineConfigJournalOwner, LocalCleanupOwners};
use sha2::{Digest, Sha256};

use crate::{
    Context, ServiceError,
    addons::engine_config::service::{self, RecoveryOutcome, ReleaseOutcome},
    catalog::leftovers::dto::{CleanupStep, CleanupStepOutcome, LeftoverCategory},
    game_mutation_lock::GameMutationGuard,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::catalog::leftovers) enum EnginePendingClass {
    None,
    OwnRelease,
    Foreign,
}

pub(in crate::catalog::leftovers) fn release_operation_id(
    game: &GameInstallation,
    kind: AddonKind,
    receipt: &EngineConfigReceipt,
) -> String {
    let identity = format!(
        "{}\n{}\n{}\n{}\n{}",
        game.id().as_str(),
        game.install_path().as_str(),
        kind.as_str(),
        receipt.path,
        receipt.after_digest,
    );
    format!(
        "leftovers-engine-{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    )
}

pub(in crate::catalog::leftovers) fn classify_engine_pending(
    game: &GameInstallation,
    owner: &EngineConfigJournalOwner,
) -> EnginePendingClass {
    let journal = &owner.journal;
    if journal.validate_for_kind(owner.kind).is_err() {
        return EnginePendingClass::Foreign;
    }
    let Some(pending) = journal.pending.as_ref() else {
        return EnginePendingClass::None;
    };
    let Some(prior) = pending.prior.as_ref() else {
        return EnginePendingClass::Foreign;
    };
    let expected_id = release_operation_id(game, owner.kind, prior);
    if journal.stable.as_ref() != Some(prior)
        || pending.after.is_some()
        || pending.operation_id != expected_id
        || pending.stage_name != format!(".renderpilot-engine-{expected_id}.stage")
    {
        EnginePendingClass::Foreign
    } else {
        EnginePendingClass::OwnRelease
    }
}

/// Releases one exact current Engine.ini receipt through the ordinary journal
/// service. Only the deterministic release transition for this receipt may be
/// recovered; other pending rows remain untouched.
pub(in crate::catalog::leftovers) fn release_engine_owner(
    context: &Context,
    game_guard: &GameMutationGuard,
    expected: &LocalCleanupOwners,
    item_id: &str,
) -> Result<CleanupStep, ServiceError> {
    let game = expected.game();
    if game_guard.game_id() != game.id() || !expected.availability().is_absent() {
        return Err(ServiceError::invalid_input(
            "Engine.ini cleanup requires the same absent registered game",
        ));
    }
    let owner = expected.engine_journal().ok_or_else(|| {
        ServiceError::invalid_input("Engine.ini owner changed after cleanup was proposed")
    })?;
    let receipt = owner.journal.stable.as_ref().or_else(|| {
        owner
            .journal
            .pending
            .as_ref()
            .and_then(|pending| pending.prior.as_ref())
    });
    let Some(receipt) = receipt else {
        return Err(ServiceError::invalid_input(
            "Engine.ini owner has no release receipt",
        ));
    };
    let expected_item = super::super::projection::canonical_item_id(
        game.id(),
        LeftoverCategory::EngineConfig,
        Some(&receipt.path),
        &format!("engine-config:{}", owner.kind.as_str()),
    );
    if item_id != expected_item {
        return Err(ServiceError::invalid_input(
            "Engine.ini cleanup item does not match the current receipt",
        ));
    }
    let current = context.storage().read_local_cleanup_owners(game.id())?;
    if current.game() != game
        || !current.availability().is_absent()
        || current.engine_journal() != Some(owner)
    {
        return Err(ServiceError::invalid_input(
            "Engine.ini receipt changed before release",
        ));
    }
    if context
        .storage()
        .pending_file_mutations_for_game(game.id())?
        .iter()
        .any(|row| row.feature.contains("engine_config"))
    {
        return Err(ServiceError::invalid_input(
            "an Engine.ini filesystem operation is still pending",
        ));
    }

    let operation_id = release_operation_id(game, owner.kind, receipt);
    let mut recovered = false;
    match classify_engine_pending(game, owner) {
        EnginePendingClass::Foreign => {
            return Err(ServiceError::invalid_input(
                "Engine.ini journal contains a foreign pending transition",
            ));
        }
        EnginePendingClass::OwnRelease => {
            match service::recover_pending(context.storage(), game.id(), owner.kind)
                .map_err(|error| ServiceError::command_failed(error.to_string()))?
            {
                RecoveryOutcome::PromotedAfter => {
                    recovered = true;
                }
                RecoveryOutcome::CancelledBefore => {}
                RecoveryOutcome::NoPending => {
                    return Err(ServiceError::invalid_input(
                        "Engine.ini pending transition changed before recovery",
                    ));
                }
            }
        }
        EnginePendingClass::None => {}
    }

    if !recovered {
        let refreshed = context.storage().read_local_cleanup_owners(game.id())?;
        if refreshed.game() != game || !refreshed.availability().is_absent() {
            return Err(ServiceError::invalid_input(
                "registered game changed before Engine.ini release",
            ));
        }
        let refreshed_owner = refreshed.engine_journal().ok_or_else(|| {
            ServiceError::invalid_input("Engine.ini owner disappeared before release")
        })?;
        if refreshed_owner.kind != owner.kind {
            return Err(ServiceError::invalid_input(
                "Engine.ini owner kind changed before release",
            ));
        }
        if refreshed_owner.journal.pending.is_some()
            || refreshed_owner.journal.stable.as_ref() != Some(receipt)
        {
            return Err(ServiceError::invalid_input(
                "Engine.ini receipt changed before release",
            ));
        }
        match service::release_record_by_kind(
            context.storage(),
            game.id(),
            owner.kind,
            &operation_id,
        )? {
            ReleaseOutcome::Released => {}
            ReleaseOutcome::NotConfigured => {
                return Err(ServiceError::invalid_input(
                    "Engine.ini receipt disappeared before release",
                ));
            }
        }
    }

    let after = context.storage().read_local_cleanup_owners(game.id())?;
    if after.game() != game || !after.availability().is_absent() || after.engine_journal().is_some()
    {
        return Err(ServiceError::invalid_input(
            "Engine.ini release did not clear its canonical owner",
        ));
    }
    Ok(CleanupStep {
        item_id: item_id.to_owned(),
        category: LeftoverCategory::EngineConfig,
        outcome: if recovered {
            CleanupStepOutcome::Recovered
        } else {
            CleanupStepOutcome::Released
        },
        issue: None,
    })
}
