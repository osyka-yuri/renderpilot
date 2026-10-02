use std::{collections::BTreeMap, path::Path};

use renderpilot_domain::{GameInstallation, InstalledAddon};
use renderpilot_platform_windows::vulkan_layer::AppUnregisterOutcome;
use renderpilot_storage_sqlite::{EngineConfigJournalOwner, PendingFileMutationRow};

use crate::context::RetiredLeftoversObservation;

use super::super::external_cleanup::{
    EnginePendingClass, VulkanPendingClass, classify_engine_pending,
};
use super::{super::dto::*, insert_item, issue, item};

pub(super) fn append_engine_item(
    items: &mut BTreeMap<String, LeftoverItem>,
    game: &GameInstallation,
    owner: &EngineConfigJournalOwner,
    observation: &RetiredLeftoversObservation,
) {
    let receipt = owner.journal.stable.as_ref().or_else(|| {
        owner
            .journal
            .pending
            .as_ref()
            .and_then(|pending| pending.prior.as_ref())
    });
    let path = receipt.map(|receipt| receipt.path.as_str());
    let has_file_fence = observation
        .pending_files
        .iter()
        .any(|row| row.feature.contains("engine_config"));
    let (disposition, issue_value) = if has_file_fence {
        (
            LeftoverDisposition::Blocked,
            Some(issue(
                LeftoverIssueCode::PendingConflict,
                Some("An Engine.ini filesystem operation is still pending.".to_owned()),
            )),
        )
    } else {
        match classify_engine_pending(game, owner) {
            EnginePendingClass::OwnRelease => (LeftoverDisposition::Retryable, None),
            EnginePendingClass::Foreign => (
                LeftoverDisposition::Blocked,
                Some(issue(
                    LeftoverIssueCode::ForeignPending,
                    Some(
                        "The Engine.ini receipt has an unsupported pending transition.".to_owned(),
                    ),
                )),
            ),
            EnginePendingClass::None if owner.journal.stable.is_some() => {
                (LeftoverDisposition::Cleanable, None)
            }
            EnginePendingClass::None => (
                LeftoverDisposition::Blocked,
                Some(issue(
                    LeftoverIssueCode::UnsupportedOwner,
                    Some("The Engine.ini owner has no stable release receipt.".to_owned()),
                )),
            ),
        }
    };
    insert_item(
        items,
        item(
            game.id(),
            LeftoverCategory::EngineConfig,
            path,
            &format!("engine-config:{}", owner.kind.as_str()),
            disposition,
            issue_value,
        ),
    );
}

pub(super) fn append_vulkan_item(
    items: &mut BTreeMap<String, LeftoverItem>,
    game: &GameInstallation,
    addon: &InstalledAddon,
    observation: &RetiredLeftoversObservation,
    pending_class: VulkanPendingClass,
) {
    let exe = addon.registered_exe_path();
    let (disposition, issue_value) = match pending_class {
        VulkanPendingClass::OwnPrepared | VulkanPendingClass::OwnCommitted => {
            (LeftoverDisposition::Retryable, None)
        }
        VulkanPendingClass::Foreign => (
            LeftoverDisposition::Blocked,
            Some(issue(
                LeftoverIssueCode::ForeignPending,
                Some("A foreign shared Vulkan operation is pending.".to_owned()),
            )),
        ),
        VulkanPendingClass::None => match (observation.native_shared_vulkan.as_ref(), exe) {
            (Some(Ok(native)), Some(exe)) => {
                match native.unregister_app_outcome(Path::new(exe.as_str())) {
                    Ok(
                        AppUnregisterOutcome::TargetAbsent
                        | AppUnregisterOutcome::RemovedOthersRemain
                        | AppUnregisterOutcome::RemovedLast,
                    ) => (LeftoverDisposition::Cleanable, None),
                    Ok(_) => (
                        LeftoverDisposition::Blocked,
                        Some(issue(
                            LeftoverIssueCode::UnprovenOwnership,
                            Some(
                                "The app list does not prove this game's registration.".to_owned(),
                            ),
                        )),
                    ),
                    Err(error) => (
                        LeftoverDisposition::Blocked,
                        Some(issue(
                            LeftoverIssueCode::ModifiedFile,
                            Some(error.to_string()),
                        )),
                    ),
                }
            }
            (Some(Err(error)), _) => (
                LeftoverDisposition::Blocked,
                Some(issue(
                    LeftoverIssueCode::NativeAuthorityUnavailable,
                    Some(error.clone()),
                )),
            ),
            (_, None) => (
                LeftoverDisposition::Blocked,
                Some(issue(
                    LeftoverIssueCode::UnprovenOwnership,
                    Some("The shared Vulkan owner has no executable binding.".to_owned()),
                )),
            ),
            (None, _) => (
                LeftoverDisposition::Blocked,
                Some(issue(
                    LeftoverIssueCode::NativeAuthorityUnavailable,
                    Some("The shared Vulkan registration could not be observed.".to_owned()),
                )),
            ),
        },
    };
    insert_item(
        items,
        item(
            game.id(),
            LeftoverCategory::VulkanRegistration,
            exe.map(|path| path.as_str()),
            "shared-vulkan-registration",
            disposition,
            issue_value,
        ),
    );
}

pub(super) fn append_pending_items(
    items: &mut BTreeMap<String, LeftoverItem>,
    game: &GameInstallation,
    observation: &RetiredLeftoversObservation,
    vulkan_pending: VulkanPendingClass,
) {
    let already_has_trace = !items.is_empty();
    let ownerless_committed_retry =
        vulkan_pending == VulkanPendingClass::OwnCommitted && observation.owners.addon().is_none();
    if !already_has_trace && !ownerless_committed_retry {
        return;
    }
    for row in &observation.pending_files {
        if row.feature.contains("engine_config") && observation.owners.engine_journal().is_some() {
            continue;
        }
        insert_item(items, pending_file_item(game, row));
    }
    if let Some(row) = observation.pending_shared_vulkan.as_ref() {
        match vulkan_pending {
            VulkanPendingClass::OwnCommitted if observation.owners.addon().is_none() => {
                insert_item(
                    items,
                    item(
                        game.id(),
                        LeftoverCategory::VulkanRegistration,
                        None,
                        &format!("pending-shared:{}", row.id),
                        LeftoverDisposition::Retryable,
                        None,
                    ),
                );
            }
            VulkanPendingClass::Foreign => {
                insert_item(
                    items,
                    item(
                        game.id(),
                        LeftoverCategory::VulkanRegistration,
                        None,
                        &format!("pending-shared:{}", row.id),
                        LeftoverDisposition::Blocked,
                        Some(issue(
                            LeftoverIssueCode::ForeignPending,
                            Some("A shared Vulkan transaction must be resolved first.".to_owned()),
                        )),
                    ),
                );
            }
            VulkanPendingClass::OwnPrepared
            | VulkanPendingClass::OwnCommitted
            | VulkanPendingClass::None => {}
        }
    }
}

fn pending_file_item(game: &GameInstallation, row: &PendingFileMutationRow) -> LeftoverItem {
    let category = match row.feature.as_str() {
        "engine_config_release" | "engine_config_install" => LeftoverCategory::EngineConfig,
        "optiscaler_uninstall" => LeftoverCategory::OptiscalerFile,
        value if value.contains("component") => LeftoverCategory::ComponentFile,
        _ => LeftoverCategory::AddonFile,
    };
    item(
        game.id(),
        category,
        None,
        &format!("pending-file:{}", row.id),
        LeftoverDisposition::Blocked,
        Some(issue(
            LeftoverIssueCode::PendingConflict,
            Some(
                "A filesystem operation is pending and must use its existing recovery path."
                    .to_owned(),
            ),
        )),
    )
}
