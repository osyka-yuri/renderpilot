//! Read-only DLSS-Fix ownership/action projection.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::records;
use crate::addons::renodx::dlss_fix::resolve_dlss_fix;
use crate::addons::renodx::dlss_fix_binding::{self, DlssFixBindingState as BindingState};
use crate::addons::renodx::dto::availability::{
    DlssFixAction, DlssFixAvailability, DlssFixBindingState,
};
use crate::file_mutation::V2DiskObservation;
use crate::{Context, ServiceError};

/// Returns explicit DLSS-Fix actions without reconciling disk or persistence.
/// Availability must remain a read query while a mutation is pending; mutation
/// boundaries recover and then converge the partial record under the game lock.
pub fn availability(
    context: &Context,
    game_id: &GameId,
) -> Result<DlssFixAvailability, ServiceError> {
    // A pending transaction is a recoverable mutation boundary state, not
    // inconsistent DLSS ownership evidence. Only an exact companion feature is
    // actionable here; unrelated pending rows never manufacture a DLSS action.
    if context
        .storage()
        .pending_file_mutations_for_game(game_id)?
        .iter()
        .any(|row| renderpilot_domain::mutation_features::is_renodx_dlss_fix_feature(&row.feature))
    {
        return Ok(DlssFixAvailability::RecoveryPending {
            actions: vec![DlssFixAction::RetryRecovery],
        });
    }
    let Some(record) = records::active_record_of_kind(context, game_id, AddonKind::RenoDx)? else {
        return Ok(DlssFixAvailability::Binding {
            state: DlssFixBindingState::None,
            actions: Vec::new(),
        });
    };
    let binding = dlss_fix_binding::resolve(&record);
    let state = map_state(binding.state);
    let actions = match binding.state {
        BindingState::Invalid => vec![DlssFixAction::ValidationRequired],
        BindingState::None => resolve_dlss_fix(context.storage(), game_id)?
            .is_some()
            .then_some(DlssFixAction::Install)
            .into_iter()
            .collect(),
        BindingState::SourceOnly | BindingState::OwnedOnly => {
            vec![DlssFixAction::Repair, DlssFixAction::Remove]
        }
        BindingState::Bound => match binding.observation {
            V2DiskObservation::Regular { .. } => vec![DlssFixAction::Update, DlssFixAction::Remove],
            V2DiskObservation::Absent => vec![DlssFixAction::Repair, DlssFixAction::Remove],
            V2DiskObservation::NonRegular | V2DiskObservation::Unreadable => {
                vec![DlssFixAction::ValidationRequired]
            }
        },
    };
    Ok(DlssFixAvailability::Binding { state, actions })
}

fn map_state(state: BindingState) -> DlssFixBindingState {
    match state {
        BindingState::None => DlssFixBindingState::None,
        BindingState::SourceOnly => DlssFixBindingState::SourceOnly,
        BindingState::OwnedOnly => DlssFixBindingState::OwnedOnly,
        BindingState::Bound => DlssFixBindingState::Bound,
        BindingState::Invalid => DlssFixBindingState::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{InstalledAddon, PathRef, TrackedSource, TrackedSourceRole};
    use tempfile::tempdir;

    use super::*;

    fn source(digest: &str) -> TrackedSource {
        TrackedSource::new(
            TrackedSourceRole::DlssFix,
            "https://example.test/renodx-dlssfix.addon64",
            None,
            digest,
        )
    }

    fn record(root: &Path, game_id: &GameId) -> InstalledAddon {
        let addon = root.join("renodx-game.addon64");
        fs::write(&addon, b"addon").expect("active add-on");
        InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy().into_owned()).expect("add-on path"),
        )
    }

    fn query(record: &InstalledAddon) -> DlssFixAvailability {
        let root = tempdir().expect("db root");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");
        let game_id = record.game_id().clone();
        context
            .storage()
            .upsert_installed_addon(record)
            .expect("record");
        availability(&context, &game_id).expect("availability")
    }

    fn prepare_pending(context: &Context, game_id: &GameId, game_root: &Path, feature: &str) {
        let guard = crate::game_mutation_lock::try_lock(game_id).expect("test lock");
        let scope =
            crate::file_mutation::MutationScope::new([game_root.to_path_buf()]).expect("scope");
        let _pending = crate::file_mutation::RetryableFileMutationV2::prepare(
            context,
            &guard,
            &scope,
            feature,
            Some(game_id.as_str()),
            &crate::file_mutation::RetryableFilePlan {
                operations: vec![crate::file_mutation::RetryableFileOperation::Write {
                    path: game_root.join("pending-dlss-fix.addon64"),
                    bytes: b"pending".to_vec(),
                    expected: crate::file_mutation::V2DiskObservation::Absent,
                }],
            },
        )
        .expect("prepared pending mutation");
    }

    #[test]
    fn partial_bound_missing_and_invalid_records_expose_explicit_actions() {
        let root = tempdir().expect("game root");
        let game_id = GameId::new("manual:dlss-availability").expect("game id");
        let target = root.path().join("renodx-dlssfix.addon64");

        let source_only_record = record(root.path(), &game_id).with_tracked_source(source("old"));
        let source_only = query(&source_only_record);
        assert_binding(
            source_only,
            DlssFixBindingState::SourceOnly,
            &[DlssFixAction::Repair, DlssFixAction::Remove],
        );

        fs::write(&target, b"live").expect("companion");
        let bound_record = record(root.path(), &game_id)
            .with_created_file(PathRef::new(target.to_string_lossy().into_owned()).expect("target"))
            .with_tracked_source(source("old"));
        let bound = query(&bound_record);
        assert_binding(
            bound,
            DlssFixBindingState::Bound,
            &[DlssFixAction::Update, DlssFixAction::Remove],
        );

        fs::remove_file(&target).expect("missing companion");
        let missing_record = record(root.path(), &game_id)
            .with_created_file(PathRef::new(target.to_string_lossy().into_owned()).expect("target"))
            .with_tracked_source(source("old"));
        let missing = query(&missing_record);
        assert_binding(
            missing,
            DlssFixBindingState::Bound,
            &[DlssFixAction::Repair, DlssFixAction::Remove],
        );

        let invalid_record = record(root.path(), &game_id)
            .with_tracked_source(source("one"))
            .with_tracked_source(source("two"));
        let invalid = query(&invalid_record);
        assert_binding(
            invalid,
            DlssFixBindingState::Invalid,
            &[DlssFixAction::ValidationRequired],
        );
    }

    #[test]
    fn pending_recovery_is_actionable_even_without_a_renodx_row() {
        let root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:dlss-pending-recovery").expect("game id");
        prepare_pending(
            &context,
            &game_id,
            game_root.path(),
            renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        );

        let availability = availability(&context, &game_id).expect("availability");
        assert_recovery_pending(availability);
    }

    #[test]
    fn unrelated_pending_mutation_does_not_become_a_dlss_recovery_action() {
        let root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:unrelated-pending").expect("game id");
        prepare_pending(
            &context,
            &game_id,
            game_root.path(),
            renderpilot_domain::mutation_features::RENODX_UPDATE,
        );

        assert_binding(
            availability(&context, &game_id).expect("availability"),
            DlssFixBindingState::None,
            &[],
        );
    }

    fn assert_binding(
        availability: DlssFixAvailability,
        expected_state: DlssFixBindingState,
        expected_actions: &[DlssFixAction],
    ) {
        match availability {
            DlssFixAvailability::Binding { state, actions } => {
                assert_eq!(state, expected_state);
                assert_eq!(actions, expected_actions);
            }
            DlssFixAvailability::RecoveryPending { .. } => {
                panic!("expected binding availability")
            }
        }
    }

    fn assert_recovery_pending(availability: DlssFixAvailability) {
        match availability {
            DlssFixAvailability::RecoveryPending { actions } => {
                assert_eq!(actions, vec![DlssFixAction::RetryRecovery]);
            }
            DlssFixAvailability::Binding { .. } => panic!("expected pending recovery"),
        }
    }
}
