use std::path::Path;

use renderpilot_domain::{GameId, GameProxyTopology, ProxyImplementation};

use crate::ServiceError;
use crate::addons::renodx::errors;

/// Validates the topology shape and its relationship to the analyzed game root.
pub(super) fn validate_topology(
    game_id: &GameId,
    game_root: &Path,
    topology: &GameProxyTopology,
) -> Result<(), ServiceError> {
    topology.validate().map_err(|error| {
        errors::invalid(format!("active RenoDX proxy topology is invalid: {error}"))
    })?;
    if &topology.game_id != game_id {
        return Err(errors::invalid(
            "active RenoDX proxy topology does not belong to this game".to_owned(),
        ));
    }
    if topology.outer.implementation != ProxyImplementation::OptiScaler {
        return Err(errors::invalid(
            "active RenoDX proxy topology outer implementation is not OptiScaler".to_owned(),
        ));
    }
    let Some(parent) = Path::new(topology.root_slot.as_str()).parent() else {
        return Err(errors::invalid(
            "active RenoDX proxy topology root slot has no parent".to_owned(),
        ));
    };
    if !crate::paths::same_path(parent, game_root) {
        return Err(errors::invalid(
            "active RenoDX proxy topology root slot is not directly under the game root".to_owned(),
        ));
    }

    for participant in topology.participant_paths() {
        let participant_path = Path::new(participant.as_str());
        let canonical_participant =
            crate::paths::canonical_candidate(participant_path).map_err(|error| {
                errors::invalid(format!(
                    "active RenoDX proxy topology participant path cannot be resolved: {error}"
                ))
            })?;
        if crate::paths::same_path(&canonical_participant, game_root)
            || !crate::paths::is_within(&canonical_participant, game_root)
        {
            return Err(errors::invalid(format!(
                "active RenoDX proxy topology participant path escapes the game root: {participant}"
            )));
        }
    }
    Ok(())
}

pub(super) fn catalog_plan(
    resolution: crate::addons::renodx::matcher::RenoDxResolution,
) -> Result<crate::addons::renodx::matcher::ResolvedInstall, ServiceError> {
    use crate::addons::renodx::matcher::RenoDxResolution;

    match resolution {
        RenoDxResolution::Installable(plan) => Ok(*plan),
        RenoDxResolution::External { .. } => Err(errors::invalid(
            "RenoDX for this game is distributed externally; install it manually".to_owned(),
        )),
        RenoDxResolution::NativeHdr => Err(errors::invalid(
            "this game has native HDR; RenoDX is not needed".to_owned(),
        )),
        RenoDxResolution::UnsupportedSettings => Err(errors::unsupported_settings()),
        RenoDxResolution::Incompatible { reason } => Err(errors::invalid(format!(
            "RenoDX is not compatible with this game: {reason:?}"
        ))),
        RenoDxResolution::Blacklisted { message } => Err(errors::invalid(format!(
            "RenoDX is not supported for this game: {}",
            message.fallback_text
        ))),
        RenoDxResolution::NoMatch => Err(errors::invalid(
            "RenoDX has no profile for this game".to_owned(),
        )),
    }
}

pub(super) fn ensure_file_architecture(
    file_arch: renderpilot_domain::Architecture,
    plan_arch: renderpilot_domain::Architecture,
) -> Result<(), ServiceError> {
    if file_arch == plan_arch {
        return Ok(());
    }
    Err(errors::invalid(format!(
        "this add-on is {} but RenoDX for this game needs the {} build — download the matching add-on",
        architecture_label(file_arch),
        architecture_label(plan_arch),
    )))
}

fn architecture_label(architecture: renderpilot_domain::Architecture) -> &'static str {
    match architecture {
        renderpilot_domain::Architecture::X64 => "64-bit",
        renderpilot_domain::Architecture::X86 => "32-bit",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use renderpilot_domain::{
        FileReceipt, GameId, GameProxyTopology, PathRef, ProxyImplementation, ProxyLink,
        ProxyRootPrestate, Sha256Hash,
    };
    use tempfile::tempdir;

    use super::validate_topology;

    fn topology(game_id: &GameId, game_root: &Path) -> GameProxyTopology {
        let root_slot = game_root.join("ReShade64.dll");
        let root_slot_ref =
            PathRef::new(root_slot.to_str().expect("UTF-8 root slot")).expect("root slot");
        GameProxyTopology {
            id: "optiscaler:test".to_owned(),
            game_id: game_id.clone(),
            root_slot: root_slot_ref.clone(),
            outer: ProxyLink {
                implementation: ProxyImplementation::OptiScaler,
                path: root_slot_ref,
                receipt: FileReceipt::owned(
                    "outer",
                    Sha256Hash::new("a".repeat(64)).expect("digest"),
                )
                .expect("receipt"),
            },
            downstream: None,
            downstream_origin: None,
            root_prestate: ProxyRootPrestate::Absent,
        }
    }

    #[test]
    fn accepts_a_valid_topology_within_the_game_root() {
        let directory = tempdir().expect("game root");
        let game_id = GameId::new("manual:validation").expect("game id");
        let topology = topology(&game_id, directory.path());

        validate_topology(&game_id, directory.path(), &topology).expect("valid topology");
    }

    #[test]
    fn rejects_a_topology_owned_by_another_game() {
        let directory = tempdir().expect("game root");
        let game_id = GameId::new("manual:validation").expect("game id");
        let other_game = GameId::new("manual:other").expect("other game id");
        let topology = topology(&other_game, directory.path());

        assert!(validate_topology(&game_id, directory.path(), &topology).is_err());
    }

    #[test]
    fn rejects_a_participant_that_escapes_the_game_root() {
        let directory = tempdir().expect("game root");
        let outside = tempdir().expect("outside root");
        let game_id = GameId::new("manual:validation").expect("game id");
        let mut topology = topology(&game_id, directory.path());
        let outside_path = PathRef::new(
            outside
                .path()
                .join("ReShade64.dll")
                .to_str()
                .expect("UTF-8 outside path"),
        )
        .expect("outside path");
        topology.downstream = Some(ProxyLink {
            implementation: ProxyImplementation::ReShade,
            path: outside_path,
            receipt: FileReceipt::reused(
                "downstream",
                Sha256Hash::new("b".repeat(64)).expect("digest"),
            )
            .expect("receipt"),
        });
        topology.downstream_origin = Some(topology.root_slot.clone());

        assert!(validate_topology(&game_id, directory.path(), &topology).is_err());
    }
}
