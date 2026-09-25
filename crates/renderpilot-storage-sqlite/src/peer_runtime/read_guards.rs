//! Storage binding for process-local peer read-guard evidence.
//!
//! The domain owns which paths must be guarded and what each observation means.
//! This module only binds that closed derivation to the sealed peer roots,
//! rejects lexical escapes, and enforces exact ordered evidence at the storage
//! authority boundary.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{
    ExactOptiConfigProjection, GameProxyTopology, InstalledAddon, NormalizedPathRelation, PathRef,
    PeerCatalogPhysicalContract, PeerCatalogRollbackClaim, PeerEndpointIntent, PeerFileImage,
    PeerReadGuardEvidence, PeerReadGuardRequirement, PeerReusedClaimMembershipContract,
    PeerTransitionContext, PlannedGameProxyTopology, ProxyPeerRoute, RenoDxDlssProjection,
    RenoDxReshadeIniAuthority, normalized_path_key, normalized_path_relation,
    required_read_guards_with_catalog, required_read_guards_with_renodx_reshade_ini,
    required_read_guards_with_renodx_reshade_ini_and_dlss,
    required_read_guards_with_renodx_reshade_ini_and_optiscaler_config, validate_read_guards,
};

use super::permit::domain_error;
use super::shared_roots::BoundSharedPeerRoots;

mod dlss;

/// The immutable read-guard projection retained by a file permit.
#[derive(Debug)]
pub(super) struct ReadGuardProjection {
    pub(super) canonical_game_root: PathRef,
    pub(super) requirements: Vec<PeerReadGuardRequirement>,
    pub(super) initial_evidence: Vec<PeerReadGuardEvidence>,
    pub(super) catalog_claim: Option<PeerCatalogRollbackClaim>,
}

/// Closed permit state. Shared-Vulkan permits retain the exact parsed root
/// authority while file permits carry their complete read-guard projection.
#[derive(Debug)]
pub(super) enum ReadGuardPermitState {
    File {
        canonical_game_root: PathRef,
        requirements: Vec<PeerReadGuardRequirement>,
        initial_evidence: Vec<PeerReadGuardEvidence>,
        catalog_claim: Option<PeerCatalogRollbackClaim>,
    },
    Shared {
        roots: BoundSharedPeerRoots,
    },
}

struct BoundRoots {
    canonical_game_root: PathRef,
    sealed_roots: Vec<PathRef>,
}

/// Sealed transition inputs shared by initial and final read-guard checks.
///
/// This owns no state. It groups the aggregate images, endpoint program, and
/// root authority that must always be derived together.
#[derive(Clone, Copy)]
pub(super) struct ReadGuardTransition<'a> {
    pub(super) sealed_roots: &'a [String],
    pub(super) before_peer: Option<&'a InstalledAddon>,
    pub(super) after_peer: Option<&'a InstalledAddon>,
    pub(super) before_topology: Option<&'a GameProxyTopology>,
    pub(super) planned_after_topology: Option<&'a PlannedGameProxyTopology>,
    pub(super) route: ProxyPeerRoute,
    pub(super) intents: &'a [PeerEndpointIntent],
    pub(super) program_before: &'a [Option<PeerFileImage>],
}

/// Optional typed companions to the ordinary peer transition.
///
/// Their combinations are checked at the derivation boundary; this structure
/// only prevents them from being threaded as unrelated positional arguments.
#[derive(Clone, Copy, Default)]
pub(super) struct ReadGuardCompanions<'a> {
    pub(super) catalog: Option<&'a PeerCatalogPhysicalContract>,
    pub(super) catalog_claim: Option<&'a PeerCatalogRollbackClaim>,
    pub(super) renodx_reshade_ini: Option<&'a RenoDxReshadeIniAuthority>,
    pub(super) dlss: Option<&'a RenoDxDlssProjection>,
    pub(super) optiscaler_config: Option<&'a ExactOptiConfigProjection>,
}

/// Inputs captured while minting a file-peer permit.
#[derive(Clone, Copy)]
pub(super) struct InitialReadGuardInput<'a> {
    pub(super) canonical_game_root: &'a str,
    pub(super) transition: ReadGuardTransition<'a>,
    pub(super) initial_evidence: &'a [PeerReadGuardEvidence],
    pub(super) companions: ReadGuardCompanions<'a>,
}

/// Inputs retained by a file-peer permit for the final CAS fence.
#[derive(Clone, Copy)]
pub(super) struct FinalReadGuardInput<'a> {
    pub(super) canonical_game_root: &'a PathRef,
    pub(super) transition: ReadGuardTransition<'a>,
    pub(super) stored_requirements: &'a [PeerReadGuardRequirement],
    pub(super) initial_evidence: &'a [PeerReadGuardEvidence],
    pub(super) final_evidence: &'a [PeerReadGuardEvidence],
    pub(super) companions: ReadGuardCompanions<'a>,
}

/// Derives and validates the initial read-guard proof for a file preparation.
pub(super) fn bind_initial(input: InitialReadGuardInput<'_>) -> AppResult<ReadGuardProjection> {
    let bound_roots = bind_roots(input.canonical_game_root, input.transition.sealed_roots)?;
    let requirements = derive_requirements(
        &bound_roots.canonical_game_root,
        &bound_roots.sealed_roots,
        input.transition,
        input.companions,
    )?;
    validate_exact_evidence(&requirements, input.initial_evidence)?;
    validate_semantics(&requirements, input.initial_evidence)?;
    if let Some(projection) = input.companions.dlss {
        dlss::validate_projection_binding(
            projection,
            input.transition.intents,
            input.transition.program_before,
            &requirements,
            input.initial_evidence,
        )?;
    }

    Ok(ReadGuardProjection {
        canonical_game_root: bound_roots.canonical_game_root,
        requirements,
        initial_evidence: input.initial_evidence.to_vec(),
        catalog_claim: input.companions.catalog_claim.cloned(),
    })
}

/// Re-derives and validates final read evidence before a SQLite CAS begins.
///
/// The stored requirement projection is compared to a fresh derivation from
/// the permit's sealed inputs. This is an internal integrity check, not a
/// caller-facing replacement path.
pub(super) fn validate_final(input: FinalReadGuardInput<'_>) -> AppResult<()> {
    let bound_roots = bind_roots(
        input.canonical_game_root.as_str(),
        input.transition.sealed_roots,
    )?;
    if bound_roots.canonical_game_root != *input.canonical_game_root {
        return Err(AppError::storage_failed(
            "canonical game root changed inside permit",
        ));
    }
    if input.companions.catalog_claim.is_some()
        && (input.companions.renodx_reshade_ini.is_some()
            || input.companions.optiscaler_config.is_some())
    {
        return Err(AppError::invalid_input(
            "catalog projection cannot accompany RenoDX ReShade.ini authority",
        ));
    }
    let catalog = derive_catalog_physical(
        input.companions.catalog_claim,
        input.transition.before_peer,
        input.transition.after_peer,
        input.transition.intents,
        input.transition.program_before,
    )?;
    let companions = ReadGuardCompanions {
        catalog: catalog.as_ref(),
        ..input.companions
    };
    let derived = derive_requirements(
        &bound_roots.canonical_game_root,
        &bound_roots.sealed_roots,
        input.transition,
        companions,
    )?;
    if derived != input.stored_requirements {
        return Err(AppError::storage_failed(
            "peer read-guard requirement projection changed inside permit",
        ));
    }
    validate_exact_evidence(input.stored_requirements, input.initial_evidence)?;
    validate_semantics(input.stored_requirements, input.initial_evidence)?;
    if let Some(projection) = input.companions.dlss {
        dlss::validate_projection_binding(
            projection,
            input.transition.intents,
            input.transition.program_before,
            input.stored_requirements,
            input.initial_evidence,
        )?;
    }
    validate_exact_evidence(input.stored_requirements, input.final_evidence)?;
    validate_semantics(input.stored_requirements, input.final_evidence)?;
    if let Some(projection) = input.companions.dlss {
        dlss::validate_projection_binding(
            projection,
            input.transition.intents,
            input.transition.program_before,
            input.stored_requirements,
            input.final_evidence,
        )?;
    }
    Ok(())
}

/// Binds the typed companion preimage to the exact endpoint or read-guard
/// observation which carries it. The domain guard projection intentionally
/// checks identity and digest; storage also binds the declared byte length and
/// retained bytes so those three fields cannot be substituted independently.
pub(super) fn validate_dlss_projection_manifest_binding(
    sealed_roots: &[String],
    intents: &[PeerEndpointIntent],
    program_before: &[Option<PeerFileImage>],
    projection: &RenoDxDlssProjection,
) -> AppResult<()> {
    dlss::validate_projection_manifest_binding(sealed_roots, intents, program_before, projection)
}

fn derive_requirements(
    canonical_game_root: &PathRef,
    sealed_roots: &[PathRef],
    transition: ReadGuardTransition<'_>,
    companions: ReadGuardCompanions<'_>,
) -> AppResult<Vec<PeerReadGuardRequirement>> {
    if companions.catalog.is_some()
        && (companions.renodx_reshade_ini.is_some() || companions.optiscaler_config.is_some())
    {
        return Err(AppError::invalid_input(
            "catalog projection cannot accompany RenoDX ReShade.ini authority",
        ));
    }
    let requirements = if let Some(opti) = companions.optiscaler_config {
        if companions.dlss.is_some() {
            return Err(AppError::invalid_input(
                "OptiScaler configuration companion cannot carry a DLSS projection",
            ));
        }
        required_read_guards_with_renodx_reshade_ini_and_optiscaler_config(
            PeerTransitionContext::new(
                transition.before_peer,
                transition.after_peer,
                transition.before_topology,
                transition.planned_after_topology,
                transition.route,
            ),
            transition.intents,
            companions.renodx_reshade_ini,
            opti,
        )
    } else if let Some(projection) = companions.dlss {
        required_read_guards_with_renodx_reshade_ini_and_dlss(
            PeerTransitionContext::new(
                transition.before_peer,
                transition.after_peer,
                transition.before_topology,
                transition.planned_after_topology,
                transition.route,
            ),
            transition.intents,
            companions.renodx_reshade_ini,
            projection,
        )
    } else {
        match companions.renodx_reshade_ini {
            Some(authority) => required_read_guards_with_renodx_reshade_ini(
                transition.before_peer,
                transition.after_peer,
                transition.before_topology,
                transition.planned_after_topology,
                transition.route,
                transition.intents,
                authority,
            ),
            None => required_read_guards_with_catalog(
                transition.before_peer,
                transition.after_peer,
                transition.before_topology,
                transition.planned_after_topology,
                transition.route,
                transition.intents,
                companions.catalog,
            ),
        }
    }
    .map_err(domain_error)?;

    for requirement in &requirements {
        let path = strict_absolute_path(requirement.path().as_str(), "peer read guard")?;
        if requirement.is_topology_sourced() {
            if !is_strict_descendant(canonical_game_root.as_str(), path.as_str()) {
                return Err(AppError::storage_failed(
                    "topology-sourced peer read guard is not a strict descendant of the canonical game root",
                ));
            }
        } else if !sealed_roots
            .iter()
            .any(|root| is_strict_descendant(root.as_str(), path.as_str()))
        {
            return Err(AppError::storage_failed(
                "non-topology peer read guard is not a strict descendant of any sealed peer root",
            ));
        }
    }
    Ok(requirements)
}

fn derive_catalog_physical(
    claim: Option<&PeerCatalogRollbackClaim>,
    before_peer: Option<&InstalledAddon>,
    after_peer: Option<&InstalledAddon>,
    intents: &[PeerEndpointIntent],
    program_before: &[Option<PeerFileImage>],
) -> AppResult<Option<PeerCatalogPhysicalContract>> {
    claim
        .map(|claim| {
            PeerCatalogPhysicalContract::derive(
                claim,
                before_peer,
                after_peer,
                intents,
                program_before,
            )
            .map_err(domain_error)
        })
        .transpose()
}

fn bind_roots(canonical_game_root: &str, sealed_roots: &[String]) -> AppResult<BoundRoots> {
    if !(1..=2).contains(&sealed_roots.len()) {
        return Err(AppError::storage_failed(
            "peer program must seal exactly one or two peer roots",
        ));
    }
    let canonical = strict_absolute_path(canonical_game_root, "canonical game root")?;
    let sealed = sealed_roots
        .iter()
        .map(|root| strict_absolute_path(root, "sealed game root"))
        .collect::<AppResult<Vec<_>>>()?;
    if canonical != sealed[0] {
        return Err(AppError::storage_failed(
            "canonical game root differs from the first sealed peer-program root",
        ));
    }
    if sealed
        .windows(2)
        .any(|pair| normalized_path_key(pair[0].as_str()) == normalized_path_key(pair[1].as_str()))
    {
        return Err(AppError::storage_failed(
            "peer program cannot seal duplicate game roots",
        ));
    }
    Ok(BoundRoots {
        canonical_game_root: canonical,
        sealed_roots: sealed,
    })
}

/// Binds root authority for an aggregate-only reused-claim membership change.
///
/// Membership guards are storage-derived from the opaque domain contract. The
/// caller may provide observations only; it cannot provide or reorder the
/// requirement set. Every managed-live guard must be below at least one
/// sealed root.
pub(super) fn bind_roots_for_membership(
    canonical_game_root: &str,
    sealed_roots: &[String],
) -> AppResult<(PathRef, Vec<PathRef>)> {
    let bound = bind_roots(canonical_game_root, sealed_roots)?;
    Ok((bound.canonical_game_root, bound.sealed_roots))
}

pub(super) fn bind_canonical_game_root(
    canonical_game_root: &str,
    sealed_roots: &[String],
) -> AppResult<PathRef> {
    Ok(bind_roots(canonical_game_root, sealed_roots)?.canonical_game_root)
}

pub(super) fn validate_membership_evidence(
    sealed_roots: &[PathRef],
    contract: &PeerReusedClaimMembershipContract,
    evidence: &[PeerReadGuardEvidence],
) -> AppResult<()> {
    let requirements = contract.read_guards();
    for requirement in requirements {
        let path = strict_absolute_path(requirement.path().as_str(), "reused claim guard")?;
        if path != *requirement.path() {
            return Err(AppError::storage_failed(
                "reused claim guard path is not canonically normalized",
            ));
        }
        if !sealed_roots
            .iter()
            .any(|root| is_strict_descendant(root.as_str(), path.as_str()))
        {
            return Err(AppError::storage_failed(
                "reused claim guard is not a strict descendant of any sealed peer root",
            ));
        }
    }
    validate_exact_evidence(requirements, evidence)?;
    validate_semantics(requirements, evidence)
}

/// Validates one topology participant against the first sealed root.
///
/// Both values are reparsed through the same strict lexical parser used for
/// peer read guards. This rejects dot segments, duplicate separators, and
/// component-prefix siblings before the component-wise descendant check.
pub(super) fn validate_topology_participant(
    canonical_game_root: &PathRef,
    participant: &PathRef,
) -> AppResult<()> {
    let canonical = strict_absolute_path(canonical_game_root.as_str(), "canonical game root")?;
    let participant = strict_absolute_path(participant.as_str(), "topology participant")?;
    if !is_strict_descendant(canonical.as_str(), participant.as_str()) {
        return Err(AppError::storage_failed(
            "OptiScaler topology participant must be a strict descendant of the canonical game root",
        ));
    }
    Ok(())
}

fn validate_exact_evidence(
    requirements: &[PeerReadGuardRequirement],
    evidence: &[PeerReadGuardEvidence],
) -> AppResult<()> {
    if requirements.len() != evidence.len() {
        return Err(AppError::storage_failed(
            "peer read-guard evidence cardinality differs from derived requirements",
        ));
    }
    for (requirement, observed) in requirements.iter().zip(evidence) {
        if observed.path() != requirement.path() {
            return Err(AppError::storage_failed(
                "peer read-guard evidence path/order differs from derived requirements",
            ));
        }
    }
    Ok(())
}

fn validate_semantics(
    requirements: &[PeerReadGuardRequirement],
    evidence: &[PeerReadGuardEvidence],
) -> AppResult<()> {
    validate_read_guards(requirements, evidence).map_err(domain_error)
}

/// Validates one normalized absolute lexical path without touching the
/// filesystem. Backslashes are accepted as native input and normalized to
/// forward slashes; the resulting form must contain no empty interior
/// components or dot segments.
pub(super) fn strict_absolute_path(value: &str, context: &str) -> AppResult<PathRef> {
    if value.is_empty() || value.trim() != value || value.contains('\0') {
        return Err(AppError::storage_failed(format!(
            "{context} must be one normalized absolute lexical path"
        )));
    }
    let mut normalized = value.replace('\\', "/");
    if normalized.starts_with("//./") {
        return Err(AppError::storage_failed(format!(
            "{context} cannot use a device path"
        )));
    }
    if let Some(verbatim) = normalized.strip_prefix("//?/") {
        if let Some(unc) = verbatim.strip_prefix("UNC/") {
            normalized = format!("//{unc}");
        } else if verbatim.len() >= 3
            && verbatim.as_bytes()[1] == b':'
            && verbatim.as_bytes()[2] == b'/'
        {
            normalized.drain(..4);
        } else {
            return Err(AppError::storage_failed(format!(
                "{context} has a malformed verbatim path"
            )));
        }
    }

    let is_drive = normalized.len() >= 2 && normalized.as_bytes()[1] == b':';
    if is_drive {
        let bytes = normalized.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[2] != b'/'
            || normalized[3..].contains(':')
        {
            return Err(AppError::storage_failed(format!(
                "{context} has a malformed drive path"
            )));
        }
        validate_components(&normalized[3..], context)?;
    } else if let Some(unc) = normalized.strip_prefix("//") {
        let mut parts = unc.split('/');
        let (Some(server), Some(share)) = (parts.next(), parts.next()) else {
            return Err(AppError::storage_failed(format!(
                "{context} has a malformed UNC path"
            )));
        };
        if server.is_empty() || share.is_empty() {
            return Err(AppError::storage_failed(format!(
                "{context} has a malformed UNC path"
            )));
        }
        for component in unc.split('/') {
            if component.is_empty() {
                return Err(AppError::storage_failed(format!(
                    "{context} has a malformed UNC path"
                )));
            }
        }
        validate_component_values(unc.split('/'), context)?;
    } else if let Some(root_relative) = normalized.strip_prefix('/') {
        validate_components(root_relative, context)?;
    } else {
        return Err(AppError::storage_failed(format!(
            "{context} must be absolute"
        )));
    }

    let is_root = normalized == "/"
        || (normalized.len() == 3 && normalized.as_bytes()[1] == b':' && normalized.ends_with('/'));
    if !is_root && normalized.ends_with('/') {
        return Err(AppError::storage_failed(format!(
            "{context} must not have a trailing separator"
        )));
    }
    PathRef::new(normalized)
        .map_err(|error| AppError::storage_failed(format!("{context} is invalid: {error}")))
}

fn validate_components(value: &str, context: &str) -> AppResult<()> {
    if value.is_empty() {
        return Ok(());
    }
    for component in value.split('/') {
        if component.is_empty() {
            return Err(AppError::storage_failed(format!(
                "{context} contains duplicate separators"
            )));
        }
    }
    validate_component_values(value.split('/'), context)
}

fn validate_component_values<'a>(
    components: impl IntoIterator<Item = &'a str>,
    context: &str,
) -> AppResult<()> {
    for component in components {
        if component.is_empty() || component == "." || component == ".." || component.contains(':')
        {
            return Err(AppError::storage_failed(format!(
                "{context} contains an invalid path component"
            )));
        }
    }
    Ok(())
}

pub(super) fn is_strict_descendant(root: &str, path: &str) -> bool {
    normalized_path_relation(root, path) == NormalizedPathRelation::LeftAncestor
}

#[cfg(test)]
pub(super) fn test_strict_absolute_path(value: &str) -> AppResult<PathRef> {
    strict_absolute_path(value, "test path")
}

#[cfg(test)]
pub(super) fn test_is_strict_descendant(root: &str, path: &str) -> bool {
    is_strict_descendant(root, path)
}

#[cfg(test)]
pub(super) fn test_bind_initial(
    input: InitialReadGuardInput<'_>,
) -> AppResult<ReadGuardProjection> {
    bind_initial(input)
}

#[cfg(test)]
pub(super) fn test_validate_final(input: FinalReadGuardInput<'_>) -> AppResult<()> {
    validate_final(input)
}

#[cfg(test)]
pub(super) fn test_bind_game_root(root: &str, sealed_roots: &[String]) -> AppResult<PathRef> {
    Ok(bind_roots(root, sealed_roots)?.canonical_game_root)
}
