use renderpilot_domain::{
    NormalizedPathRelation, PathRef, PeerEndpointRole, normalized_path_relation,
};

use super::error::RenoDxPeerEffectError;
use super::model::{EndpointBundle, RenoDxPeerEffectGroup};
use crate::peer_mutation_executor::VerifiedPeerFile;

pub(super) fn ensure_bytes_match_image(
    path: &PathRef,
    bytes: &[u8],
    image: &VerifiedPeerFile,
) -> Result<(), RenoDxPeerEffectError> {
    if !image.matches_content_bytes(bytes) {
        return Err(RenoDxPeerEffectError::BeforeImageMismatch(path.clone()));
    }
    Ok(())
}

pub(super) fn ensure_distinct_pair_paths(
    live_path: &PathRef,
    sidecar_path: &PathRef,
) -> Result<(), RenoDxPeerEffectError> {
    if matches!(
        normalized_path_relation(live_path.as_str(), sidecar_path.as_str()),
        NormalizedPathRelation::Equal
    ) {
        return Err(RenoDxPeerEffectError::SamePairPath(live_path.clone()));
    }
    if normalized_path_relation(live_path.as_str(), sidecar_path.as_str()).overlaps() {
        return Err(RenoDxPeerEffectError::OverlappingPaths(
            live_path.clone(),
            sidecar_path.clone(),
        ));
    }
    Ok(())
}

pub(super) fn bundle_overlaps(left: &EndpointBundle, right: &EndpointBundle) -> bool {
    let mut overlaps = false;
    left.visit(|left_endpoint| {
        right.visit(|right_endpoint| {
            overlaps |=
                normalized_path_relation(left_endpoint.path.as_str(), right_endpoint.path.as_str())
                    .overlaps();
        });
    });
    overlaps
}

pub(super) fn group_rank(group: RenoDxPeerEffectGroup) -> u8 {
    match group {
        RenoDxPeerEffectGroup::Addon => 0,
        RenoDxPeerEffectGroup::Host => 1,
        RenoDxPeerEffectGroup::Config => 2,
    }
}

pub(super) fn role_for(group: RenoDxPeerEffectGroup) -> PeerEndpointRole {
    match group {
        RenoDxPeerEffectGroup::Addon => PeerEndpointRole::Disjoint,
        RenoDxPeerEffectGroup::Host => PeerEndpointRole::TopologyDownstream,
        RenoDxPeerEffectGroup::Config => PeerEndpointRole::RenoDxReshadeIni,
    }
}
