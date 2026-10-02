use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    ServiceError,
    catalog::{
        installation_lifecycle::residue::{
            LocalCleanupClaim, RetiredRootObservation, local_cleanup_claims,
        },
        leftovers::dto::LeftoverItem,
    },
    context::RetiredLeftoversObservation,
};

/// Stable semantic identity for Leave suppression and one-shot actions. It
/// covers current cleanup owners and the selected native app association, but
/// ignores volatile authority epochs, timestamps, random tokens, and other
/// games' Vulkan registrations.
pub(super) fn semantic_revision(
    observation: &RetiredLeftoversObservation,
    items: &[LeftoverItem],
) -> Result<String, ServiceError> {
    let mut items = items.iter().collect::<Vec<_>>();
    items.sort_by(|left, right| left.item_id.cmp(&right.item_id));
    let claims = local_cleanup_claims(&observation.owners)
        .iter()
        .map(claim_value)
        .collect::<Vec<_>>();
    let mut pending_files = observation
        .pending_files
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "feature": row.feature,
                "state": format!("{:?}", row.state),
                "manifest": digest(&row.manifest_json),
            })
        })
        .collect::<Vec<_>>();
    pending_files.sort_by_key(|value| value["id"].as_str().unwrap_or_default().to_owned());
    let pending_shared = observation.pending_shared_vulkan.as_ref().map(|row| {
        json!({
            "id": row.id,
            "scope": format!("{:?}", row.scope),
            "gameId": row.game_id.as_ref().map(|id| id.as_str()),
            "feature": row.feature,
            "state": format!("{:?}", row.state),
            "manifest": digest(&row.manifest_json),
            "roots": digest(&row.root_capabilities_json),
        })
    });
    let addon = observation
        .owners
        .addon()
        .map(|addon| serde_json::to_value(addon.clone().with_timestamps(None, None)))
        .transpose()
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let game = serde_json::to_value(observation.owners.game())
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let components = serde_json::to_value(observation.owners.components())
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let baselines = serde_json::to_value(observation.owners.baselines())
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let optiscaler = serde_json::to_value(observation.owners.optiscaler_state())
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let topology = serde_json::to_value(observation.owners.topology())
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let engine = observation.owners.engine_journal().map(|owner| {
        json!({
            "kind": owner.kind.as_str(),
            "journal": owner.journal,
        })
    });

    let value = json!({
        "contract": "game-leftovers-leave-v2",
        "game": game,
        "gameId": observation.owners.game().id().as_str(),
        "installPath": observation.owners.game().install_path().as_str(),
        "availabilityRevision": observation.owners.availability().revision(),
        "root": root_state(observation),
        "claims": claims,
        "components": components,
        "baselines": baselines,
        "optiscaler": optiscaler,
        "topology": topology,
        "addon": addon,
        "engine": engine,
        "nativeAppAssociation": native_app_association(observation),
        "items": items,
        "pendingFiles": pending_files,
        "pendingSharedVulkan": pending_shared,
        "hasPendingNvapi": !observation.pending_nvapi.is_empty(),
    });
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    Ok(format!("leave-v2-{}", hex::encode(Sha256::digest(encoded))))
}

fn claim_value(claim: &LocalCleanupClaim) -> Value {
    json!({
        "path": claim.path().as_str(),
        "category": format!("{:?}", claim.category()),
        "digest": claim.sha256().map(|digest| digest.as_str()),
        "nativeIdentity": claim.native_identity(),
        "removable": claim.removable(),
        "blockedReason": claim.blocked_reason().map(|reason| format!("{reason:?}")),
    })
}

fn root_state(observation: &RetiredLeftoversObservation) -> Value {
    match &observation.root {
        RetiredRootObservation::Missing(_) => json!({"state": "missing"}),
        RetiredRootObservation::ResidueOnly(root) => json!({
            "state": "residueOnly",
            "nativeIdentity": root.native_root_identity(),
            "snapshotDigest": root.snapshot_digest().as_str(),
            "remnants": root.remnants().iter().map(|remnant| json!({
                "path": remnant.path().as_str(),
                "nativeIdentity": remnant.native_identity(),
                "digest": remnant.digest().map(|digest| digest.as_str()),
                "category": format!("{:?}", remnant.category()),
                "removable": remnant.removable(),
            })).collect::<Vec<_>>(),
        }),
        RetiredRootObservation::PresentNonResidue {
            native_root_identity,
        } => {
            json!({"state": "presentNonResidue", "nativeIdentity": native_root_identity})
        }
        RetiredRootObservation::Indeterminate { reason } => {
            json!({"state": "indeterminate", "reason": reason})
        }
    }
}

fn native_app_association(observation: &RetiredLeftoversObservation) -> Value {
    let Some(exe) = observation
        .owners
        .addon()
        .filter(|addon| {
            addon.host_kind() == Some(renderpilot_domain::InstalledAddonHostKind::SharedVulkanLayer)
        })
        .and_then(|addon| addon.registered_exe_path())
    else {
        return Value::Null;
    };
    match observation.native_shared_vulkan.as_ref() {
        Some(Ok(native)) => {
            let bytes = match &native.apps {
                renderpilot_platform_windows::vulkan_layer::FileObservation::Absent => {
                    return json!({"available": true, "matchingEntries": []});
                }
                renderpilot_platform_windows::vulkan_layer::FileObservation::Present(bytes) => {
                    bytes
                }
            };
            match renderpilot_platform_windows::vulkan_layer::parse_app_list(bytes) {
                Ok(paths) => {
                    let mut entries = paths
                        .into_iter()
                        .filter(|path| {
                            crate::paths::same_path(path, std::path::Path::new(exe.as_str()))
                        })
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    entries.sort_by_key(|path| renderpilot_domain::normalized_path_key(path));
                    json!({"available": true, "matchingEntries": entries})
                }
                Err(_) => json!({"available": false}),
            }
        }
        Some(Err(_)) | None => json!({"available": false}),
    }
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
