//! Pure semantic Xiph member topologies and stable vendor discriminators.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::naming::XiphMember;

/// One directed importer-to-dependency edge in a Xiph topology.
pub type XiphEdge = (XiphMember, XiphMember);

/// A validated semantic member set and its exact directed import graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XiphTopology {
    members: BTreeSet<XiphMember>,
    edges: BTreeSet<XiphEdge>,
}

impl XiphTopology {
    /// Maximum number of semantic Xiph members in one deployment.
    pub const MAX_MEMBERS: usize = 4;

    /// Creates and validates a semantic topology.
    pub fn new(
        members: impl IntoIterator<Item = XiphMember>,
        edges: impl IntoIterator<Item = XiphEdge>,
    ) -> Result<Self, XiphTopologyError> {
        let members = members.into_iter().collect::<BTreeSet<_>>();
        if members.is_empty() {
            return Err(XiphTopologyError::Empty);
        }
        if members.len() > Self::MAX_MEMBERS {
            return Err(XiphTopologyError::TooManyMembers);
        }

        let mut collected_edges = BTreeSet::new();
        for (source, target) in edges {
            if source == target {
                return Err(XiphTopologyError::SelfEdge(source));
            }
            if !members.contains(&source) || !members.contains(&target) {
                return Err(XiphTopologyError::EndpointMissing { source, target });
            }
            if !is_allowed_edge(source, target) {
                return Err(XiphTopologyError::DisallowedEdge { source, target });
            }
            if !collected_edges.insert((source, target)) {
                return Err(XiphTopologyError::DuplicateEdge { source, target });
            }
        }

        if members.len() > 1 && !is_connected(&members, &collected_edges) {
            return Err(XiphTopologyError::Disconnected);
        }

        Ok(Self {
            members,
            edges: collected_edges,
        })
    }

    /// Returns all semantic members in stable code order.
    pub fn members(&self) -> impl Iterator<Item = XiphMember> + '_ {
        XiphMember::ALL
            .into_iter()
            .filter(|member| self.members.contains(member))
    }

    /// Returns all directed edges in stable source/target order.
    pub fn edges(&self) -> impl Iterator<Item = XiphEdge> + '_ {
        XiphMember::ALL.into_iter().flat_map(move |source| {
            XiphMember::ALL
                .into_iter()
                .map(move |target| (source, target))
                .filter(|edge| self.edges.contains(edge))
        })
    }

    /// Returns whether this topology contains a semantic member.
    #[must_use]
    pub fn contains(&self, member: XiphMember) -> bool {
        self.members.contains(&member)
    }

    /// Returns the direct dependencies imported by one member.
    pub fn dependencies(&self, member: XiphMember) -> impl Iterator<Item = XiphMember> + '_ {
        self.edges()
            .filter_map(move |(source, target)| (source == member).then_some(target))
    }

    /// Returns the exact stable bytes hashed for the vendor discriminator.
    #[must_use]
    pub fn vendor_discriminator_preimage(&self) -> Vec<u8> {
        let mut preimage = Vec::with_capacity(64);
        preimage.extend_from_slice(b"renderpilot:xiph-vendor-topology:v1");
        preimage.push(0);
        preimage.push(self.members.len() as u8);
        for member in self.members() {
            preimage.push(b'M');
            preimage.push(member.code());
        }
        preimage.extend_from_slice(&(self.edges.len() as u16).to_le_bytes());
        for (source, target) in self.edges() {
            preimage.push(b'E');
            preimage.push(source.code());
            preimage.push(target.code());
        }
        preimage
    }

    /// Returns the stable discriminator for this semantic vendor topology.
    #[must_use]
    pub fn vendor_discriminator(&self) -> String {
        let digest = Sha256::digest(self.vendor_discriminator_preimage());
        let mut output = String::from("vendor-topology-v1-");
        for byte in digest {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

/// Returns the stable discriminator for a topology.
#[must_use]
pub fn vendor_topology_discriminator(topology: &XiphTopology) -> String {
    topology.vendor_discriminator()
}

/// Returns the exact bytes used by [`vendor_topology_discriminator`].
#[must_use]
pub fn vendor_topology_preimage(topology: &XiphTopology) -> Vec<u8> {
    topology.vendor_discriminator_preimage()
}

/// Why a proposed semantic topology is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XiphTopologyError {
    /// No semantic members were supplied.
    Empty,
    /// More than the four reviewed members were supplied.
    TooManyMembers,
    /// An edge points to a member absent from the set.
    EndpointMissing {
        /// Importing member.
        source: XiphMember,
        /// Dependency member.
        target: XiphMember,
    },
    /// A member cannot import itself.
    SelfEdge(XiphMember),
    /// The edge is not part of the reviewed Xiph ABI graph.
    DisallowedEdge {
        /// Importing member.
        source: XiphMember,
        /// Dependency member.
        target: XiphMember,
    },
    /// The same semantic edge was supplied twice.
    DuplicateEdge {
        /// Importing member.
        source: XiphMember,
        /// Dependency member.
        target: XiphMember,
    },
    /// A multi-member topology has disconnected components.
    Disconnected,
}

impl XiphMember {
    /// Stable byte code used by topology discriminators.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::VorbisFile => 0x01,
            Self::VorbisEnc => 0x02,
            Self::Vorbis => 0x03,
            Self::Ogg => 0x04,
        }
    }
}

pub(super) const fn is_allowed_edge(source: XiphMember, target: XiphMember) -> bool {
    matches!(
        (source, target),
        (XiphMember::Vorbis, XiphMember::Ogg)
            | (XiphMember::VorbisFile, XiphMember::Vorbis | XiphMember::Ogg)
            | (XiphMember::VorbisEnc, XiphMember::Vorbis | XiphMember::Ogg)
    )
}

fn is_connected(members: &BTreeSet<XiphMember>, edges: &BTreeSet<XiphEdge>) -> bool {
    let Some(start) = members.iter().next().copied() else {
        return false;
    };
    let mut visited = BTreeSet::from([start]);
    let mut pending = vec![start];
    while let Some(current) = pending.pop() {
        for candidate in members {
            let adjacent =
                edges.contains(&(current, *candidate)) || edges.contains(&(*candidate, current));
            if adjacent && visited.insert(*candidate) {
                pending.push(*candidate);
            }
        }
    }
    visited.len() == members.len()
}
