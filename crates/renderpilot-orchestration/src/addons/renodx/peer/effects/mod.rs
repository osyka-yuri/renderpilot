//! Pure RenoDX endpoint lowering.
//!
//! The accumulator receives only sealed preimages and already-prepared bytes.
//! It performs no filesystem, storage, context, or reread operation, and keeps
//! payloads aligned with the immutable endpoint program by ordinal.

mod error;
mod model;
mod validation;

use std::path::PathBuf;

use renderpilot_domain::{PathRef, PeerEndpointRole};

use crate::addons::shared_vulkan_mutation::FileIntent;
use crate::peer_mutation_executor::{
    EndpointExpectation, EndpointPostcondition, ExactEndpoint, ExactEndpointProgram,
    VerifiedPeerFile,
};

use self::model::{EndpointBundle, EndpointSpec};
use self::validation::{
    bundle_overlaps, ensure_bytes_match_image, ensure_distinct_pair_paths, group_rank, role_for,
};

pub(crate) use error::RenoDxPeerEffectError;
pub(crate) use model::RenoDxPeerEffectGroup;

#[derive(Debug)]
pub(crate) struct RenoDxPeerEffectAccumulator {
    bundles: Vec<EndpointBundle>,
}

impl RenoDxPeerEffectAccumulator {
    pub(crate) fn new() -> Self {
        Self {
            bundles: Vec::new(),
        }
    }

    pub(crate) fn create(
        &mut self,
        group: RenoDxPeerEffectGroup,
        path: PathRef,
        bytes: Vec<u8>,
    ) -> Result<(), RenoDxPeerEffectError> {
        let role = role_for(group);
        self.push(EndpointBundle::Single {
            group,
            endpoint: EndpointSpec {
                path,
                role,
                before: EndpointExpectation::Absent,
                before_bytes: None,
                after: EndpointPostcondition::for_file_bytes(&bytes)
                    .map_err(|_| RenoDxPeerEffectError::InvalidPayloadDigest)?,
                payload: Some(bytes),
            },
        })
    }

    pub(crate) fn replace(
        &mut self,
        group: RenoDxPeerEffectGroup,
        path: PathRef,
        before: &VerifiedPeerFile,
        before_bytes: Vec<u8>,
        bytes: Vec<u8>,
    ) -> Result<(), RenoDxPeerEffectError> {
        ensure_bytes_match_image(&path, &before_bytes, before)?;
        let role = role_for(group);
        self.push(EndpointBundle::Single {
            group,
            endpoint: EndpointSpec {
                path,
                role,
                before: EndpointExpectation::File(before.clone()),
                before_bytes: Some(before_bytes),
                after: EndpointPostcondition::for_file_bytes(&bytes)
                    .map_err(|_| RenoDxPeerEffectError::InvalidPayloadDigest)?,
                payload: Some(bytes),
            },
        })
    }

    pub(crate) fn acquire_host(
        &mut self,
        live_path: PathRef,
        sidecar_path: PathRef,
        live_before: &VerifiedPeerFile,
        original_bytes: Vec<u8>,
        prepared_bytes: Vec<u8>,
    ) -> Result<(), RenoDxPeerEffectError> {
        ensure_distinct_pair_paths(&live_path, &sidecar_path)?;
        ensure_bytes_match_image(&live_path, &original_bytes, live_before)?;
        let live_before_bytes = original_bytes.clone();
        self.push(EndpointBundle::HostAcquisition {
            sidecar: EndpointSpec {
                path: sidecar_path,
                role: PeerEndpointRole::Disjoint,
                before: EndpointExpectation::Absent,
                before_bytes: None,
                after: EndpointPostcondition::for_file_bytes(&original_bytes)
                    .map_err(|_| RenoDxPeerEffectError::InvalidPayloadDigest)?,
                payload: Some(original_bytes),
            },
            live: EndpointSpec {
                path: live_path,
                role: PeerEndpointRole::TopologyDownstream,
                before: EndpointExpectation::File(live_before.clone()),
                before_bytes: Some(live_before_bytes),
                after: EndpointPostcondition::for_file_bytes(&prepared_bytes)
                    .map_err(|_| RenoDxPeerEffectError::InvalidPayloadDigest)?,
                payload: Some(prepared_bytes),
            },
        })
    }

    pub(crate) fn finalize(mut self) -> Result<RenoDxPeerEffects, RenoDxPeerEffectError> {
        if self.bundles.is_empty() {
            return Err(RenoDxPeerEffectError::Program(
                crate::peer_mutation_executor::PeerRouteError::EmptyProgram,
            ));
        }
        let host_count = self
            .bundles
            .iter()
            .flat_map(|bundle| {
                let mut roles = Vec::new();
                bundle.visit(|endpoint| {
                    if endpoint.role == PeerEndpointRole::TopologyDownstream {
                        roles.push(());
                    }
                });
                roles
            })
            .count();
        if host_count > 1 {
            return Err(RenoDxPeerEffectError::InvalidHostCardinality(host_count));
        }
        self.bundles.sort_by(|left, right| {
            group_rank(left.group())
                .cmp(&group_rank(right.group()))
                .then_with(|| {
                    renderpilot_domain::normalized_path_key(left.logical_path().as_str()).cmp(
                        &renderpilot_domain::normalized_path_key(right.logical_path().as_str()),
                    )
                })
        });

        let endpoint_count = self.bundles.iter().map(EndpointBundle::len).sum();
        let mut endpoints = Vec::with_capacity(endpoint_count);
        let mut payloads = Vec::with_capacity(endpoint_count);
        let mut game_intents = Vec::with_capacity(endpoint_count);
        for bundle in self.bundles {
            bundle.visit_owned(|spec| {
                let EndpointSpec {
                    path,
                    role,
                    before,
                    before_bytes,
                    after,
                    payload,
                } = spec;
                game_intents.push(FileIntent {
                    live_path: PathBuf::from(path.as_str()),
                    before: before_bytes,
                    after: payload.clone(),
                });
                endpoints.push(ExactEndpoint::new(path, role, before, after));
                payloads.push(payload);
            });
        }
        let program =
            ExactEndpointProgram::new(endpoints).map_err(RenoDxPeerEffectError::Program)?;
        Ok(RenoDxPeerEffects {
            program,
            payloads,
            game_intents,
        })
    }

    fn push(&mut self, bundle: EndpointBundle) -> Result<(), RenoDxPeerEffectError> {
        if self.bundles.iter().any(|existing| existing == &bundle) {
            return Err(RenoDxPeerEffectError::DuplicatePath(
                bundle.logical_path().clone(),
            ));
        }
        if self.bundles.iter().any(|existing| {
            renderpilot_domain::normalized_path_key(existing.logical_path().as_str())
                == renderpilot_domain::normalized_path_key(bundle.logical_path().as_str())
        }) {
            return Err(RenoDxPeerEffectError::ConflictingPath(
                bundle.logical_path().clone(),
            ));
        }
        for existing in &self.bundles {
            if bundle_overlaps(existing, &bundle) {
                return Err(RenoDxPeerEffectError::OverlappingBundle(
                    existing.logical_path().clone(),
                    bundle.logical_path().clone(),
                ));
            }
        }
        self.bundles.push(bundle);
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RenoDxPeerEffects {
    program: ExactEndpointProgram,
    payloads: Vec<Option<Vec<u8>>>,
    game_intents: Vec<FileIntent>,
}

impl RenoDxPeerEffects {
    #[cfg(test)]
    pub(crate) fn program(&self) -> &ExactEndpointProgram {
        &self.program
    }

    #[cfg(test)]
    pub(crate) fn payloads(&self) -> &[Option<Vec<u8>>] {
        &self.payloads
    }

    #[cfg(test)]
    pub(crate) fn game_intents(&self) -> &[FileIntent] {
        &self.game_intents
    }

    pub(crate) fn into_parts(
        self,
    ) -> (ExactEndpointProgram, Vec<Option<Vec<u8>>>, Vec<FileIntent>) {
        (self.program, self.payloads, self.game_intents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::{PeerEndpointRole, Sha256Hash};
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("valid path")
    }

    fn file(identity: &str, digest: char, length: u64) -> VerifiedPeerFile {
        VerifiedPeerFile::new_with_length(
            identity.to_owned(),
            Sha256Hash::new(digest.to_string().repeat(Sha256Hash::HEX_LENGTH)).expect("digest"),
            length,
        )
        .expect("file")
    }

    fn file_for_bytes(identity: &str, bytes: &[u8]) -> VerifiedPeerFile {
        VerifiedPeerFile::new_with_length(
            identity.to_owned(),
            Sha256Hash::new(hex::encode(Sha256::digest(bytes))).expect("digest"),
            bytes.len() as u64,
        )
        .expect("file")
    }

    #[test]
    fn final_program_has_addon_sidecar_host_and_typed_ini_order() {
        let mut accumulator = RenoDxPeerEffectAccumulator::new();
        accumulator
            .create(
                RenoDxPeerEffectGroup::Config,
                path("C:/Game/ReShade.ini"),
                b"ini".to_vec(),
            )
            .expect("config");
        accumulator
            .acquire_host(
                path("C:/Game/ReShade64.dll"),
                path("C:/Game/ReShade64.dll.bak"),
                &file_for_bytes("host", b"old"),
                b"old".to_vec(),
                b"new".to_vec(),
            )
            .expect("host");
        accumulator
            .create(
                RenoDxPeerEffectGroup::Addon,
                path("C:/Game/renodx.addon64"),
                b"addon".to_vec(),
            )
            .expect("addon");

        let effects = accumulator.finalize().expect("effects");
        let endpoints = effects.program().endpoints();
        assert_eq!(endpoints.len(), 4);
        assert_eq!(endpoints[0].role(), PeerEndpointRole::Disjoint);
        assert_eq!(endpoints[0].path().as_str(), "C:/Game/renodx.addon64");
        assert_eq!(endpoints[1].role(), PeerEndpointRole::Disjoint);
        assert_eq!(endpoints[1].path().as_str(), "C:/Game/ReShade64.dll.bak");
        assert_eq!(endpoints[2].role(), PeerEndpointRole::TopologyDownstream);
        assert_eq!(endpoints[2].path().as_str(), "C:/Game/ReShade64.dll");
        assert_eq!(endpoints[3].role(), PeerEndpointRole::RenoDxReshadeIni);
        assert_eq!(endpoints[3].path().as_str(), "C:/Game/ReShade.ini");
        assert_eq!(effects.payloads().len(), endpoints.len());
        assert_eq!(effects.game_intents().len(), endpoints.len());
        assert_eq!(
            effects.game_intents()[0],
            FileIntent {
                live_path: PathBuf::from("C:/Game/renodx.addon64"),
                before: None,
                after: Some(b"addon".to_vec()),
            }
        );
        assert_eq!(
            effects.game_intents()[1],
            FileIntent {
                live_path: PathBuf::from("C:/Game/ReShade64.dll.bak"),
                before: None,
                after: Some(b"old".to_vec()),
            }
        );
        assert_eq!(
            effects.game_intents()[2],
            FileIntent {
                live_path: PathBuf::from("C:/Game/ReShade64.dll"),
                before: Some(b"old".to_vec()),
                after: Some(b"new".to_vec()),
            }
        );
        assert_eq!(
            effects.game_intents()[3],
            FileIntent {
                live_path: PathBuf::from("C:/Game/ReShade.ini"),
                before: None,
                after: Some(b"ini".to_vec()),
            }
        );
    }

    #[test]
    fn replace_rejects_before_bytes_that_do_not_match_the_sealed_file() {
        let mut accumulator = RenoDxPeerEffectAccumulator::new();
        let target = path("C:/Game/renodx.addon64");
        let error = accumulator
            .replace(
                RenoDxPeerEffectGroup::Addon,
                target.clone(),
                &file_for_bytes("addon", b"sealed"),
                b"different".to_vec(),
                b"new".to_vec(),
            )
            .expect_err("mismatched before bytes must fail closed");
        assert_eq!(error, RenoDxPeerEffectError::BeforeImageMismatch(target));
    }

    #[test]
    fn host_acquisition_rejects_a_retained_image_that_differs_from_bytes() {
        let mut accumulator = RenoDxPeerEffectAccumulator::new();
        let error = accumulator
            .acquire_host(
                path("C:/Game/ReShade64.dll"),
                path("C:/Game/ReShade64.dll.bak"),
                &file("host", 'a', 3),
                b"different".to_vec(),
                b"new".to_vec(),
            )
            .expect_err("image mismatch must fail closed");
        assert!(matches!(
            error,
            RenoDxPeerEffectError::BeforeImageMismatch(_)
        ));
    }

    #[test]
    fn duplicate_effect_is_rejected_instead_of_silently_coalesced() {
        let mut accumulator = RenoDxPeerEffectAccumulator::new();
        let target = path("C:/Game/renodx.addon64");
        accumulator
            .create(
                RenoDxPeerEffectGroup::Addon,
                target.clone(),
                b"addon".to_vec(),
            )
            .expect("first effect");

        let error = accumulator
            .create(
                RenoDxPeerEffectGroup::Addon,
                target.clone(),
                b"addon".to_vec(),
            )
            .expect_err("duplicate effect must fail closed");

        assert_eq!(error, RenoDxPeerEffectError::DuplicatePath(target));
    }
}
