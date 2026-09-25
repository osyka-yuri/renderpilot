use renderpilot_domain::{PathRef, PeerEndpointRole};

use crate::peer_mutation_executor::{EndpointExpectation, EndpointPostcondition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenoDxPeerEffectGroup {
    Addon,
    Host,
    Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EndpointSpec {
    pub(super) path: PathRef,
    pub(super) role: PeerEndpointRole,
    pub(super) before: EndpointExpectation,
    pub(super) before_bytes: Option<Vec<u8>>,
    pub(super) after: EndpointPostcondition,
    pub(super) payload: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EndpointBundle {
    Single {
        group: RenoDxPeerEffectGroup,
        endpoint: EndpointSpec,
    },
    /// The retained foreign host is published to its sidecar before the new
    /// ReShade downstream is written.  This is the only ordered pair emitted
    /// by an active RenoDX install.
    HostAcquisition {
        sidecar: EndpointSpec,
        live: EndpointSpec,
    },
}

impl EndpointBundle {
    pub(super) fn group(&self) -> RenoDxPeerEffectGroup {
        match self {
            Self::Single { group, .. } => *group,
            Self::HostAcquisition { .. } => RenoDxPeerEffectGroup::Host,
        }
    }

    pub(super) fn logical_path(&self) -> &PathRef {
        match self {
            Self::Single { endpoint, .. } => &endpoint.path,
            Self::HostAcquisition { live, .. } => &live.path,
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Single { .. } => 1,
            Self::HostAcquisition { .. } => 2,
        }
    }

    pub(super) fn visit(&self, mut visitor: impl FnMut(&EndpointSpec)) {
        match self {
            Self::Single { endpoint, .. } => visitor(endpoint),
            Self::HostAcquisition { sidecar, live } => {
                visitor(sidecar);
                visitor(live);
            }
        }
    }

    pub(super) fn visit_owned(self, mut visitor: impl FnMut(EndpointSpec)) {
        match self {
            Self::Single { endpoint, .. } => visitor(endpoint),
            Self::HostAcquisition { sidecar, live } => {
                visitor(sidecar);
                visitor(live);
            }
        }
    }
}
