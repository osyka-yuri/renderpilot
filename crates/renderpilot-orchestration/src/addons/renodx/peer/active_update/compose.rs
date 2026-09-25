use renderpilot_domain::{PeerEndpointRole, PlannedGameProxyTopology};

use super::error::RenoDxActiveUpdateError;
use super::model::{
    RenoDxActiveUpdateComposition, RenoDxActiveUpdateInput, RenoDxActiveUpdateMetadata,
    RenoDxActiveUpdatePhysical,
};
use super::validation::{self, ValidatedUpdate};
use crate::addons::renodx::peer::effects::{RenoDxPeerEffectAccumulator, RenoDxPeerEffectGroup};

/// Composes one active RenoDX update from sealed evidence and prepared bytes.
pub(crate) fn compose_active_update(
    input: RenoDxActiveUpdateInput<'_>,
) -> Result<RenoDxActiveUpdateComposition, RenoDxActiveUpdateError> {
    let validated = validation::validate_input(input)?;
    if !validated.has_physical_change {
        return Ok(if validated.before == validated.after {
            RenoDxActiveUpdateComposition::Noop
        } else {
            RenoDxActiveUpdateComposition::Metadata(Box::new(RenoDxActiveUpdateMetadata::new(
                validated.after.clone(),
            )))
        });
    }

    let ValidatedUpdate {
        before: _,
        after,
        topology,
        addon_path,
        addon_before,
        addon_before_bytes,
        addon_bytes,
        host,
        config,
        ..
    } = validated;
    let mut accumulator = RenoDxPeerEffectAccumulator::new();
    if !validation::same_image(addon_before, &addon_bytes)? {
        accumulator.replace(
            RenoDxPeerEffectGroup::Addon,
            addon_path,
            addon_before,
            addon_before_bytes.to_vec(),
            addon_bytes,
        )?;
    }
    if let Some(host) = host
        && !validation::same_image(&host.before, &host.bytes)?
    {
        accumulator.replace(
            RenoDxPeerEffectGroup::Host,
            host.path,
            &host.before,
            host.before_bytes,
            host.bytes,
        )?;
    }
    if let Some(config) = config {
        let (path, before, before_bytes, after) = config.into_parts();
        let after = after.ok_or(RenoDxActiveUpdateError::InvalidInput(
            "active config physical change has no postimage",
        ))?;
        match (before, before_bytes) {
            (Some(before), Some(before_bytes)) if before_bytes != after => {
                accumulator.replace(
                    RenoDxPeerEffectGroup::Config,
                    path,
                    &before,
                    before_bytes,
                    after,
                )?;
            }
            (None, _) => {
                accumulator.create(RenoDxPeerEffectGroup::Config, path, after)?;
            }
            _ => {}
        }
    }
    let effects = accumulator.finalize()?;
    let (program, payloads, game_intents) = effects.into_parts();
    if !program
        .endpoints()
        .iter()
        .any(|endpoint| endpoint.role() == PeerEndpointRole::Disjoint)
        && !program
            .endpoints()
            .iter()
            .any(|endpoint| endpoint.role() == PeerEndpointRole::TopologyDownstream)
    {
        return Err(RenoDxActiveUpdateError::InvalidInput(
            "physical active update has no endpoint transition",
        ));
    }
    Ok(RenoDxActiveUpdateComposition::Physical(Box::new(
        RenoDxActiveUpdatePhysical::new(
            after.clone(),
            program,
            payloads,
            game_intents,
            PlannedGameProxyTopology::Exact(topology.clone()),
        ),
    )))
}
